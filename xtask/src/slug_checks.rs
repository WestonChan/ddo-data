use anyhow::{Context, Result};
use ddo_model::enums::EntityKind;
use rusqlite::Connection;
use std::collections::BTreeSet;

fn current_slugs(db: &Connection) -> Result<BTreeSet<(String, String)>> {
    let mut slugs = BTreeSet::new();
    let mut statement = db.prepare("SELECT kind, slug FROM entity_identity")?;
    for row in statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))? {
        let (table, slug) = row?;
        let kind = EntityKind::ALL
            .iter()
            .find(|kind| kind.table_name() == table)
            .with_context(|| format!("unknown entity identity kind {table:?} for {slug:?}"))?;
        slugs.insert((kind.as_str().to_string(), slug));
    }
    Ok(slugs)
}

pub fn redirect_check_violations(
    db: &Connection,
    deployed: Option<&Connection>,
    today: Option<&str>,
) -> Result<Vec<String>> {
    let mut violations = ddo_etl::tree::slug_lists::redirect_expiry_violations(db, today)?;
    let Some(deployed) = deployed else {
        return Ok(violations);
    };
    let current = current_slugs(db)?;
    let declared = db
        .prepare("SELECT kind, from_slug FROM served_redirects UNION SELECT kind, slug FROM removed_slugs")?
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<BTreeSet<_>>>()?;
    let mut exempt = declared.clone();
    for (child_kind, owner_kind, foreign_key) in [
        (EntityKind::EnhancementSelection, EntityKind::Enhancement, "enhancement_id"),
        (EntityKind::CraftingIngredient, EntityKind::CraftingSystem, "system_id"),
    ] {
        let mut statement = deployed.prepare(&format!("SELECT child.slug, owner.slug FROM {} child JOIN entity_identity owner ON owner.kind = ?1 AND owner.entity_id = child.{foreign_key}", child_kind.table_name()))?;
        for row in statement
            .query_map([owner_kind.table_name()], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
        {
            let (child, owner) = row?;
            if declared.contains(&(owner_kind.as_str().to_string(), owner)) {
                exempt.insert((child_kind.as_str().to_string(), child));
            }
        }
    }
    for slug in deployed
        .prepare("SELECT slug FROM stances WHERE owner_kind <> 'standalone'")?
        .query_map([], |row| row.get::<_, String>(0))?
    {
        exempt.insert((EntityKind::Stance.as_str().to_string(), slug?));
    }
    for (kind, slug) in current_slugs(deployed)?.difference(&current) {
        if !exempt.contains(&(kind.clone(), slug.clone())) {
            violations.push(format!("{kind} {slug:?} was served by the deployed database but is missing; declare a redirect or removed row in data/redirects.toml"));
        }
    }
    Ok(violations)
}
