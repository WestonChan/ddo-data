use anyhow::{Context, Result};
use ddo_api::entity_slugs::resolve_entity_slug;
use ddo_model::enums::EntityKind;
use rusqlite::Connection;
use std::collections::BTreeMap;

fn slugs_for_unique_names(db: &Connection) -> Result<BTreeMap<(String, String), String>> {
    db.prepare("SELECT kind,name,MIN(slug) FROM entity_identity GROUP BY kind,name HAVING COUNT(*)=1")?
        .query_map([], |row| Ok(((row.get(0)?, row.get(1)?), row.get(2)?)))?
        .collect::<rusqlite::Result<_>>()
        .map_err(Into::into)
}

pub fn redirect_check_violations(
    db: &Connection,
    deployed: Option<&Connection>,
    today: Option<&str>,
) -> Result<Vec<String>> {
    let mut violations = ddo_etl::tree::redirects::redirect_expiry_violations(db, today)?;
    let Some(deployed) = deployed else {
        return Ok(violations);
    };
    let current = slugs_for_unique_names(db)?;
    for ((table, name), previous_slug) in slugs_for_unique_names(deployed)? {
        let Some(current_slug) = current.get(&(table.clone(), name.clone())) else {
            continue;
        };
        if current_slug == &previous_slug {
            continue;
        }
        let kind = EntityKind::from_table_name(&table)
            .with_context(|| format!("unknown entity identity kind {table:?} for {previous_slug:?}"))?;
        if resolve_entity_slug(db, kind, &previous_slug)?.is_none_or(|resolved| resolved.slug != *current_slug) {
            violations.push(format!("{} {name:?} kept its name but changed its served slug from {previous_slug:?} to {current_slug:?}; declare a redirect in data/redirects.toml", kind.as_str()));
        }
    }
    Ok(violations)
}
