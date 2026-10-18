use crate::tree::{schemas, slug};
use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub(super) const PARAMETERIZED_ENTITY_KINDS: &[(&str, &str)] = &[
    ("QuestChain", "quest_chains"),
    ("Saga", "sagas"),
    ("ItemMaterials", "item_materials"),
    ("Triggers", "triggers"),
    ("WeaponProficiencies", "weapon_proficiencies"),
];

struct NamedEntity {
    table: String,
    name: String,
    slug: String,
    path: PathBuf,
}

struct Reference {
    table: String,
    name: String,
    path: PathBuf,
    location: String,
}

struct DocumentContext<'a> {
    path: &'a Path,
    schema: &'a serde_json::Value,
    fallback: Option<&'a str>,
}

#[derive(Default)]
pub(in crate::tree) struct Catalog {
    entities: Vec<NamedEntity>,
    references: Vec<Reference>,
    errors: Vec<String>,
}

fn singular_kind(table: &str) -> String {
    match table {
        "adventure_packs" => "pack".into(),
        "set_bonuses" => "set".into(),
        "bonus_types" => "bonus type".into(),
        "crafting_systems" => "crafting system".into(),
        "quest_chains" => "chain".into(),
        "classes" => "class".into(),
        "vendor_currencies" => "vendor currency".into(),
        "weapon_proficiencies" => "weapon proficiency".into(),
        _ => table.trim_end_matches('s').replace('_', " "),
    }
}

fn edit_distance(left: &str, right: &str) -> usize {
    let right = right.chars().collect::<Vec<_>>();
    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    for (row, left_character) in left.chars().enumerate() {
        let mut current = vec![row + 1; right.len() + 1];
        for (column, right_character) in right.iter().enumerate() {
            current[column + 1] = (current[column] + 1)
                .min(previous[column + 1] + 1)
                .min(previous[column] + usize::from(left_character != *right_character));
        }
        previous = current;
    }
    previous[right.len()]
}

fn definition<'a>(schema: &'a serde_json::Value, node: &'a serde_json::Value) -> &'a serde_json::Value {
    node["$ref"]
        .as_str()
        .and_then(|reference| reference.strip_prefix("#/$defs/"))
        .map_or(node, |name| &schema["$defs"][name])
}

impl Catalog {
    pub(in crate::tree) fn add_contents(&mut self, path: &Path, kind: &str, contents: &str) -> Result<()> {
        let document = toml::from_str(contents).with_context(|| format!("parsing {}", path.display()))?;
        let schema = schemas::file_schema(kind);
        let fallback =
            PARAMETERIZED_ENTITY_KINDS.iter().find_map(|(file_kind, table)| (*file_kind == kind).then_some(*table));
        self.visit(&DocumentContext { path, schema, fallback }, schema, &document, "", "");
        Ok(())
    }

    fn visit(
        &mut self,
        context: &DocumentContext<'_>,
        node: &serde_json::Value,
        value: &toml::Value,
        location: &str,
        owner_slug: &str,
    ) {
        let DocumentContext { path, schema, fallback } = *context;
        let node = definition(schema, node);
        if let Some(table) = node["x-reference-kind"].as_str() {
            if let Some(name) = value.as_str() {
                self.references.push(Reference {
                    table: table.into(),
                    name: name.into(),
                    path: path.into(),
                    location: location.into(),
                });
            } else if let Some(names) = value.as_array() {
                for (index, name) in names.iter().enumerate() {
                    if let Some(name) = name.as_str() {
                        self.references.push(Reference {
                            table: table.into(),
                            name: name.into(),
                            path: path.into(),
                            location: format!("{location} {}", index + 1),
                        });
                    }
                }
            }
        }
        if let Some(fields) = value.as_table() {
            if node["x-source-reference"].as_bool() == Some(true) {
                if let (Some(kind), Some(name)) =
                    (fields.get("kind").and_then(toml::Value::as_str), fields.get("name").and_then(toml::Value::as_str))
                {
                    if let Some(kind) =
                        ddo_model::enums::SourceKind::ALL.iter().copied().find(|candidate| candidate.as_str() == kind)
                    {
                        let table = crate::tree::sources::source_table(kind);
                        if !table.is_empty() {
                            self.references.push(Reference {
                                table: table.into(),
                                name: name.into(),
                                path: path.into(),
                                location: location.into(),
                            });
                        }
                    } else {
                        self.errors.push(format!("{}: {location}: unknown source kind {kind:?}", path.display()));
                    }
                }
            }
            let table = node["x-table"].as_str().or(fallback);
            let mut child_owner = owner_slug.to_string();
            if let Some(table) = table.filter(|table| {
                crate::tree::NAMED_ENTITY_TABLES.contains(table)
                    || matches!(*table, "effect_tier_groups" | "item_materials" | "triggers" | "weapon_proficiencies")
            }) {
                if let Some(name) = fields.get("name").or_else(|| fields.get("label")).and_then(toml::Value::as_str) {
                    let variant = fields.get("variant").and_then(toml::Value::as_str);
                    let identity = match table {
                        "enhancements" => crate::tree::qualified_slug(
                            fields.get("internal_name").and_then(toml::Value::as_str).unwrap_or(name),
                            variant,
                        ),
                        "enhancement_selections" | "stances" => ddo_model::identity::nested_slug(owner_slug, name),
                        "crafting_ingredients" => format!(
                            "{owner_slug}-{}-{}",
                            slug(name),
                            slug(fields.get("tier").and_then(toml::Value::as_str).unwrap_or_default())
                        ),
                        "augment_slot_types" => slug(name),
                        "augments" => {
                            let base = crate::tree::augment_name_slug(name);
                            variant.map_or(base.clone(), |variant| format!("{base}-{}", slug(variant)))
                        }
                        _ => crate::tree::qualified_slug(name, variant),
                    };
                    child_owner.clone_from(&identity);
                    self.entities.push(NamedEntity {
                        table: table.into(),
                        name: name.into(),
                        slug: identity,
                        path: path.into(),
                    });
                }
            }
            for (field, child) in fields {
                if let Some(property) = node["properties"].get(field) {
                    let child_location = if child.as_array().is_some_and(|rows| rows.iter().any(toml::Value::is_table))
                    {
                        if location.is_empty() {
                            format!("[[{field}]]")
                        } else {
                            format!("{location}: [[{field}]]")
                        }
                    } else if child.is_array() || location.is_empty() {
                        field.to_string()
                    } else {
                        location.to_string()
                    };
                    self.visit(context, property, child, &child_location, &child_owner);
                }
            }
        } else if let Some(rows) = value.as_array() {
            for (index, row) in rows.iter().enumerate() {
                if row.is_table() {
                    self.visit(context, &node["items"], row, &format!("{location} {}", index + 1), owner_slug);
                }
            }
        }
    }

    fn nearest<'a>(&'a self, query: &str, table: Option<&str>) -> Vec<&'a NamedEntity> {
        let query = query.to_lowercase();
        let mut ranked = self
            .entities
            .iter()
            .filter(|entity| table.is_none_or(|table| table.split('|').any(|kind| kind == entity.table)))
            .map(|entity| (edit_distance(&query, &entity.name.to_lowercase()), entity))
            .collect::<Vec<_>>();
        ranked.sort_by(|(first_distance, first), (second_distance, second)| {
            (first_distance, &first.name, &first.table, &first.path, &first.slug).cmp(&(
                second_distance,
                &second.name,
                &second.table,
                &second.path,
                &second.slug,
            ))
        });
        ranked.into_iter().map(|(_, entity)| entity).collect()
    }

    pub(in crate::tree) fn reference_errors(&self) -> Result<Vec<String>> {
        let names =
            self.entities.iter().map(|entity| (entity.table.as_str(), entity.name.as_str())).collect::<BTreeSet<_>>();
        let mut errors = self.errors.clone();
        for reference in &self.references {
            if reference.table.split('|').any(|table| names.contains(&(table, reference.name.as_str()))) {
                continue;
            }
            let mut seen = BTreeSet::new();
            let nearest = self
                .nearest(&reference.name, Some(&reference.table))
                .into_iter()
                .filter(|entity| seen.insert(entity.name.as_str()))
                .take(3)
                .map(|entity| format!("{} ({})", entity.name, entity.path.display()))
                .collect::<Vec<_>>();
            let expected = reference
                .table
                .split('|')
                .map(|table| {
                    crate::tree::layout::expected_reference_path(table, &reference.name, &reference.path)
                        .map(|path| path.display().to_string())
                })
                .collect::<Result<Vec<_>>>()?
                .join("\n  expected ");
            errors.push(format!(
                "{}: {}: unknown {} {:?}, expected {}; nearest: {}",
                reference.path.display(),
                reference.location,
                reference.table.split('|').map(singular_kind).collect::<Vec<_>>().join(" or "),
                reference.name,
                expected,
                nearest.join(", ")
            ));
        }
        Ok(errors)
    }
}

pub fn find(root: &Path, query: &str) -> Result<String> {
    let mut catalog = Catalog::default();
    for entry in walkdir::WalkDir::new(root).sort_by_file_name() {
        let entry = entry?;
        if !entry.file_type().is_file() || entry.path().extension().is_none_or(|extension| extension != "toml") {
            continue;
        }
        let contents = std::fs::read_to_string(entry.path())?;
        catalog.add_contents(entry.path().strip_prefix(root)?, schemas::file_kind(root, entry.path())?, &contents)?;
    }
    let query_lower = query.to_lowercase();
    let mut matches = catalog
        .entities
        .iter()
        .filter(|entity| entity.name.to_lowercase().contains(&query_lower) || entity.slug.contains(&query_lower))
        .collect::<Vec<_>>();
    if matches.is_empty() {
        matches = catalog.nearest(query, None).into_iter().take(5).collect();
    } else {
        matches.sort_by_key(|entity| (&entity.table, &entity.name, &entity.path, &entity.slug));
    }
    Ok(matches
        .into_iter()
        .map(|entity| format!("{}  {}  {}\n", singular_kind(&entity.table), entity.name, entity.path.display()))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_names_and_slugs_match_the_real_export() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::build::build_database(
            &fixtures.join("DataFiles"),
            &crate::wiki::WikiOverrides::from_dir(&fixtures.join("wiki")).unwrap(),
            &crate::corrections::Corrections::from_dir(&fixtures.join("data-corrections")).unwrap(),
            &mut db,
            &ddo_model::DatasetVersion { upstream_sha: "fixture".into(), built_at: "2026-10-06T00:00:00Z".into() },
        )
        .unwrap();
        let offer = db.query_row("SELECT offer.id FROM vendor_offers offer JOIN items item ON item.id = offer.item_id WHERE item.name = 'Ethereal Great Crossbow'", [], |row| row.get(0)).unwrap();
        crate::vendor_prices::insert_prices(
            &db,
            offer,
            &[crate::vendor_prices::VendorPrice { ingredient: "Silver Ingot".into(), quantity: 1 }],
        )
        .unwrap();
        let root = std::env::temp_dir().join(format!("ddo-catalog-test-{}", std::process::id()));
        crate::tree::export_database(&db, &root).unwrap();
        let mut catalog = Catalog::default();
        for entry in walkdir::WalkDir::new(&root).into_iter().map(Result::unwrap) {
            if entry.file_type().is_file() && entry.path().extension().is_some_and(|extension| extension == "toml") {
                catalog
                    .add_contents(
                        entry.path().strip_prefix(&root).unwrap(),
                        schemas::file_kind(&root, entry.path()).unwrap(),
                        &std::fs::read_to_string(entry.path()).unwrap(),
                    )
                    .unwrap();
            }
        }
        for table in crate::tree::NAMED_ENTITY_TABLES {
            let name = if *table == "augment_slot_types" { "label" } else { "name" };
            let mut select = db.prepare(&format!("SELECT {name}, slug FROM {table} WHERE slug IS NOT NULL")).unwrap();
            let expected = select
                .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
                .unwrap()
                .collect::<rusqlite::Result<BTreeSet<_>>>()
                .unwrap();
            let actual = catalog
                .entities
                .iter()
                .filter(|entity| entity.table == *table)
                .map(|entity| (entity.name.clone(), entity.slug.clone()))
                .collect::<BTreeSet<_>>();
            assert_eq!(actual, expected, "{table}");
        }
        assert!(catalog.reference_errors().unwrap().is_empty());
        let mut broken = Catalog::default();
        for entry in walkdir::WalkDir::new(&root).into_iter().map(Result::unwrap) {
            if !entry.file_type().is_file() || entry.path().extension().is_none_or(|extension| extension != "toml") {
                continue;
            }
            let relative = entry.path().strip_prefix(&root).unwrap();
            let mut contents = std::fs::read_to_string(entry.path()).unwrap();
            if relative == Path::new("characters/classes/dark-apostate.toml") {
                contents = contents.replace("feat = \"Heavy Armor Proficiency\"", "feat = \"Heavy Armor Proficienc\"");
            }
            if relative == Path::new("characters/classes/paladin.toml") {
                contents = contents.replace("spell = \"Resistance\"", "spell = \"Resistanc\"");
            }
            if relative.ends_with("gear/ethereal-great-crossbow.toml") {
                contents = contents.replace("ingredient = \"Silver Ingot\"", "ingredient = \"Silver Ingo\"");
            }
            broken.add_contents(relative, schemas::file_kind(&root, entry.path()).unwrap(), &contents).unwrap();
        }
        let errors = broken.reference_errors().unwrap().join("\n");
        for expected in [
            "unknown feat \"Heavy Armor Proficienc\", expected characters/feats/<category>.toml",
            "unknown spell \"Resistanc\", expected characters/spells/<class>.toml",
            "expected packs/shared/vendors/currencies.toml\n  expected packs/free-to-play/crafting/<system>/crafting.toml",
            "nearest: Resistance (characters/spells/paladin.toml)",
            "Silver Ingot (packs/shared/vendors/currencies.toml)",
        ] {
            assert!(errors.contains(expected), "{expected}: {errors}");
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
