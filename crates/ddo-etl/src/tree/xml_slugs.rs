use super::*;

#[derive(serde::Deserialize)]
pub(super) struct XmlIdentityCatalog {
    pub(super) entity: Vec<XmlIdentity>,
}

#[derive(serde::Deserialize)]
pub(super) struct XmlIdentity {
    pub(super) table: String,
    pub(super) upstream_id: i64,
    pub(super) name: String,
    pub(super) description: Option<String>,
    pub(super) variant: Option<String>,
    #[serde(default)]
    pub(super) removed: Vec<RemovedEntry>,
    #[serde(default)]
    pub(super) note: Vec<EntityNote>,
}

pub(super) fn xml_identities() -> &'static XmlIdentityCatalog {
    static CATALOG: std::sync::LazyLock<XmlIdentityCatalog> = std::sync::LazyLock::new(|| {
        toml::from_str(include_str!("../../data/xml_identity.toml")).expect("embedded XML identities are valid")
    });
    &CATALOG
}

pub(crate) fn xml_entity_slug(table: &str, upstream_id: Option<i64>, name: &str, description: Option<&str>) -> String {
    let matches = xml_identities()
        .entity
        .iter()
        .filter(|identity| identity.table == table && identity.name == name)
        .collect::<Vec<_>>();
    let identity = matches.iter().find(|identity| Some(identity.upstream_id) == upstream_id).copied().or_else(|| {
        let descriptions =
            matches.iter().filter(|identity| identity.description.as_deref() == description).collect::<Vec<_>>();
        (descriptions.len() == 1).then(|| *descriptions[0])
    });
    let base = if table == "augments" { augment_name_slug(name) } else { slug(name) };
    identity
        .and_then(|identity| identity.variant.as_ref())
        .map_or(base.clone(), |variant| format!("{base}-{}", slug(variant)))
}

pub(crate) fn augment_slug_candidates(
    name: &str,
    description: &str,
    level: Option<i64>,
    family: &str,
) -> Vec<(String, Option<String>)> {
    let base = augment_name_slug(name);
    let mut variants = vec![None];
    if let Some(variant) = augment_variant(description) {
        variants.push(Some(variant.split(':').next().unwrap_or_default().to_string()));
    }
    if let Some(level) = level {
        variants.push(Some(format!("ml-{level}")));
    }
    variants.push(Some(family.to_string()));
    variants
        .into_iter()
        .map(|variant| {
            let identity = variant.as_ref().map_or(base.clone(), |variant| format!("{base}-{}", slug(variant)));
            (identity, variant)
        })
        .collect()
}

pub(super) const NAMED_ENTITY_TABLES: &[&str] = &[
    "adventure_packs",
    "augments",
    "augment_slot_types",
    "bonus_types",
    "classes",
    "clickies",
    "crafting_systems",
    "crafting_ingredients",
    "damage_types",
    "effects",
    "enhancement_trees",
    "enhancements",
    "enhancement_selections",
    "equipment_slots",
    "events",
    "feats",
    "filigrees",
    "guild_buffs",
    "items",
    "optional_buffs",
    "patrons",
    "quest_chains",
    "quests",
    "races",
    "sagas",
    "sentient_gems",
    "set_bonuses",
    "spells",
    "stances",
    "vendors",
    "vendor_currencies",
    "weapon_types",
];

pub(crate) fn insert_entity_identities(db: &Connection) -> Result<()> {
    for table in NAMED_ENTITY_TABLES {
        if !table_columns(db, table)?.iter().any(|column| column == "slug") {
            continue;
        }
        let name_column = if *table == "augment_slot_types" { "label" } else { "name" };
        let mut select = db.prepare(&format!("SELECT id, {name_column}, slug FROM {table} WHERE slug IS NOT NULL"))?;
        for row in select.query_map([], |row| {
            Ok(annotations::EntityIdentity {
                kind: table.to_string(),
                entity_id: row.get(0)?,
                name: row.get(1)?,
                slug: row.get(2)?,
            })
        })? {
            row?.insert(db)?;
        }
    }
    Ok(())
}

pub(crate) fn store_xml_annotations(db: &Connection, wiki: &WikiOverrides) -> Result<()> {
    let mut metadata = annotations::EntityAnnotations::from_xml_inputs(db)?;
    metadata.add_wiki_citations(db, wiki)?;
    metadata.store(db)?;
    annotations::store_xml_pack_exceptions(db)
}

pub(crate) fn assign_xml_entity_slugs(db: &Connection) -> Result<()> {
    let metadata = annotations::EntityAnnotations::from_xml_inputs(db)?;
    let qualified_rows: BTreeSet<_> = metadata
        .entities
        .iter()
        .filter(|(_, annotation)| annotation.variant.is_some())
        .map(|(key, _)| key.clone())
        .collect();
    for (key, annotation) in metadata.entities {
        if annotation.variant.is_none() {
            continue;
        }
        let (table, id) = key.split_once('/').context("XML identity annotation has no table and id")?;
        if !NAMED_ENTITY_TABLES.contains(&table) {
            continue;
        }
        let id = id.parse::<i64>()?;
        let name: String = db.query_row(&format!("SELECT name FROM {table} WHERE id = ?1"), [id], |row| row.get(0))?;
        let base = if table == "augments" { augment_name_slug(&name) } else { slug(&name) };
        let identity = annotation.variant.as_ref().map_or(base.clone(), |variant| format!("{base}-{}", slug(variant)));
        db.execute(&format!("UPDATE {table} SET slug = ?1 WHERE id = ?2"), rusqlite::params![identity, id])?;
    }
    for table in NAMED_ENTITY_TABLES {
        if matches!(*table, "enhancements" | "enhancement_selections" | "crafting_ingredients") {
            continue;
        }
        if *table == "augments" {
            let mut statement = db.prepare("SELECT id, name, description, min_level, family, slug FROM augments")?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let mut counts_by_base = BTreeMap::<String, usize>::new();
            for (_, name, _, _, _, _) in &rows {
                *counts_by_base.entry(augment_name_slug(name)).or_default() += 1;
            }
            let mut occupied_slugs: BTreeSet<String> = rows.iter().map(|(_, _, _, _, _, slug)| slug.clone()).collect();
            for (id, name, description, level, family, previous_slug) in rows {
                if qualified_rows.contains(&format!("augments/{id}")) {
                    continue;
                }
                occupied_slugs.remove(&previous_slug);
                let base = augment_name_slug(&name);
                let (identity, variant) = augment_slug_candidates(&name, &description, level, &family)
                    .into_iter()
                    .filter(|(_, variant)| variant.is_some() || counts_by_base[&base] == 1)
                    .find(|(identity, _)| !occupied_slugs.contains(identity))
                    .with_context(|| format!("augment {name:?} has no unique name variant"))?;
                db.execute(
                    "UPDATE augments SET slug = ?1, variant = ?2 WHERE id = ?3",
                    rusqlite::params![identity, variant, id],
                )?;
                occupied_slugs.insert(identity);
            }
            continue;
        }
        let name_column = if *table == "augment_slot_types" { "label" } else { "name" };
        let mut statement = db.prepare(&format!("SELECT id, {name_column} FROM {table}"))?;
        let rows = statement
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (id, name) in rows {
            if qualified_rows.contains(&format!("{table}/{id}")) {
                continue;
            }
            if *table == "stances" {
                let owner_kind: String =
                    db.query_row("SELECT owner_kind FROM stances WHERE id = ?1", [id], |row| row.get(0))?;
                if owner_kind != "standalone" {
                    continue;
                }
            }
            let identity = slug(&name);
            let colliding_name: Option<String> = db
                .query_row(
                    &format!("SELECT {name_column} FROM {table} WHERE slug = ?1 AND id <> ?2 LIMIT 1"),
                    rusqlite::params![identity, id],
                    |row| row.get(0),
                )
                .optional()?;
            ensure!(
                colliding_name.is_none(),
                "{table} slug collision {identity:?}: {:?} and {name:?}",
                colliding_name.unwrap_or_default()
            );
            db.execute(&format!("UPDATE {table} SET slug = ?1 WHERE id = ?2"), rusqlite::params![identity, id])?;
        }
    }
    let mut enhancements = db.prepare(
        "SELECT e.id, e.internal_name, t.version,
                (SELECT COUNT(*) FROM enhancements other WHERE other.internal_name = e.internal_name)
           FROM enhancements e JOIN enhancement_trees t ON t.id = e.tree_id ORDER BY e.id",
    )?;
    let enhancement_rows = enhancements
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, i64>(3)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, internal_name, version, name_count) in enhancement_rows {
        let identity =
            if name_count > 1 { format!("{}-v{version}", slug(&internal_name)) } else { slug(&internal_name) };
        let variant = (name_count > 1).then(|| format!("v{version}"));
        db.execute(
            "UPDATE enhancements SET slug = ?1, variant = ?2 WHERE id = ?3",
            rusqlite::params![identity, variant, id],
        )?;
    }
    let mut selections = db.prepare(
        "SELECT s.id, e.slug, s.name FROM enhancement_selections s
         JOIN enhancements e ON e.id = s.enhancement_id ORDER BY s.id",
    )?;
    let selection_rows = selections
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, enhancement_slug, name) in selection_rows {
        let identity = format!("{enhancement_slug}-{}", slug(&name));
        db.execute("UPDATE enhancement_selections SET slug = ?1 WHERE id = ?2", rusqlite::params![identity, id])?;
    }
    let mut ingredients = db.prepare(
        "SELECT i.id, s.slug, i.name, i.tier FROM crafting_ingredients i
         JOIN crafting_systems s ON s.id = i.system_id ORDER BY i.id",
    )?;
    let ingredient_rows = ingredients
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, system_slug, name, tier) in ingredient_rows {
        let identity = format!("{system_slug}-{}-{}", slug(&name), slug(&tier));
        db.execute("UPDATE crafting_ingredients SET slug = ?1 WHERE id = ?2", rusqlite::params![identity, id])?;
    }
    for table in NAMED_ENTITY_TABLES {
        let missing: i64 =
            db.query_row(&format!("SELECT COUNT(*) FROM {table} WHERE slug IS NULL"), [], |row| row.get(0))?;
        ensure!(missing == 0, "{table} has {missing} named rows without slugs");
    }
    insert_entity_identities(db)
}
