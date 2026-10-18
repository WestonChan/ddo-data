use crate::database_tables::{table_columns, table_names};
use crate::map::source_alias::SourceAliases;
use crate::wiki::WikiOverrides;
use anyhow::{bail, ensure, Context, Result};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Display};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[macro_use]
mod records;
mod annotations;
mod build;
mod correction_notes;
mod crafting;
mod export;
mod file_types;
mod icon_references;
mod ids;
mod kinds;
mod layout;
mod pack_layout;
mod tables;
mod xml_slugs;
use annotations::wiki_url_for_name;
pub(crate) use annotations::AppliedCorrection;
pub(crate) use build::referenced_data_icons;
pub use build::{build_from_data, rederive_data_build_tables};
pub(crate) use crafting::recipe_focus_path;
pub use ddo_model::identity::slug;
use export::*;
pub use export::{
    annotated_entities, data_file_bytes, export_database, export_ddobuilder_tree, replace_data_files, DataFileChanges,
};
pub(crate) use kinds::insert_xml_vendor_currencies;
use kinds::UpstreamCommitFile;
pub use kinds::{ExceptionsFile, PackException, QuestPackException};
pub(crate) use layout::qualified_slug;
use layout::*;
pub use layout::{pack_exceptions, DataFilePaths};
pub(crate) use schemas::file_kind;
pub use schemas::find;
use tables::*;
use xml_slugs::*;
pub(crate) use xml_slugs::{assign_xml_entity_slugs, augment_slug_candidates, store_xml_annotations, xml_entity_slug};
pub mod redirects;
pub mod schemas;
mod sources;

database_record! {
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Citation in "citations" as "citation" {
    keys { owner_kind: &str, owner_id: i64 }
    fields {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    field: String => field,
    url: String => url,
    read: String => read,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    note: Option<String> => note,
    }
}
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("x-table" = annotations::AppliedCorrection::TABLE, "x-array" = annotations::AppliedCorrection::ARRAY))]
pub struct DdobuilderCorrection {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_name: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub field: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added_as_name: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Omitted when Maetrim has no value for this field.")]
    pub upstream: Option<serde_json::Value>,
    pub reason: String,
    pub url: String,
    pub read: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub qualifier: String,
}

database_record! {
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemovedEntry in "removed_entities" as "removed" {
    keys { owner_kind: &str, owner_id: i64 }
    fields {
    kind: String => kind,
    name: String => name,
    reason: String => reason,
    url: String => url,
    read: String => read,
    }
}
}

#[derive(Debug, Default)]
pub struct DatasetFileReport {
    pub file_count_by_folder: BTreeMap<String, usize>,
    pub row_count: usize,
    pub source_note_count: usize,
    pub missing_folder_quest_sources: Vec<PathBuf>,
    pub icon_actions: Vec<String>,
    pub layout_warnings: Vec<String>,
    written_files: BTreeSet<PathBuf>,
}

impl Display for DatasetFileReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            formatter,
            "{} rows in {} TOML files",
            self.row_count,
            self.file_count_by_folder.values().sum::<usize>()
        )?;
        for (folder, count) in &self.file_count_by_folder {
            writeln!(formatter, "{folder}: {count}")?;
        }
        writeln!(formatter, "sources needing a note: {}", self.source_note_count)?;
        for action in &self.icon_actions {
            writeln!(formatter, "HARD enhancement icon layout: {action}")?;
        }
        for path in &self.missing_folder_quest_sources {
            writeln!(formatter, "WARN folder quest source missing: {}", path.display())?;
        }
        for warning in &self.layout_warnings {
            writeln!(formatter, "WARN data layout: {warning}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{build_from_data, export_database, slug, table_names, NAMED_ENTITY_TABLES};
    use crate::corrections::Corrections;
    use crate::wiki::WikiOverrides;
    use ddo_model::DatasetVersion;
    use rusqlite::Connection;
    use std::collections::BTreeMap;
    use std::path::Path;

    fn table_rows(db: &Connection, table: &str) -> anyhow::Result<Vec<Vec<rusqlite::types::Value>>> {
        let columns = super::table_columns(db, table)?;
        let order = (1..=columns.len()).map(|index| index.to_string()).collect::<Vec<_>>().join(", ");
        let mut select = db.prepare(&format!("SELECT * FROM {table} ORDER BY {order}"))?;
        let rows = select
            .query_map([], |row| (0..columns.len()).map(|index| row.get(index)).collect::<rusqlite::Result<Vec<_>>>())?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    #[test]
    fn trailing_spaces_inside_source_text_do_not_end_toml_lines() {
        let source_text = "first line; \n second line";
        let encoded = super::typed_file_text(&BTreeMap::from([("description", source_text)])).unwrap();
        assert!(!encoded.contains("; \n"));
        let parsed: toml::Value = toml::from_str(&encoded).unwrap();
        assert_eq!(parsed["description"].as_str(), Some(source_text));
    }

    #[test]
    fn grouped_source_text_with_trailing_spaces_keeps_its_value() {
        let original = "[[row]]\nname = 'Example'\ndescription = \"\"\"\nfirst line; \nsecond line\"\"\"\n";
        let clean = super::escape_trailing_space_multiline_literals(original).unwrap();
        assert!(!clean.lines().any(|line| line.ends_with([' ', '\t'])));
        assert_eq!(toml::from_str::<toml::Value>(original).unwrap(), toml::from_str(&clean).unwrap());
    }

    #[test]
    fn slug_drops_apostrophes_and_marks_level_tiers() {
        assert_eq!(slug("The Fury's Rage (level 4)"), "the-furys-rage-level-4");
        assert_eq!(super::augment_name_slug("Spell Lore: Fire"), "spell-lore--fire");
        assert_eq!(super::augment_name_slug("Spell Lore Fire"), "spell-lore-fire");
        assert_eq!(
            super::augment_variant("Fortitude Save +1\nEarth:Opposition:Ethereal"),
            Some("Earth:Opposition:Ethereal")
        );
        assert_eq!(super::item_level_family("The Fury's Rage (level 4)"), Some(("The Fury's Rage", 4)));
        assert_eq!(super::item_level_family("The Fury's Rage (Level 24)"), Some(("The Fury's Rage", 24)));
        assert_eq!(super::item_level_family("The Fury's Rage"), None);
        assert_eq!(super::item_family_tier("The Fury's Rage"), ("The Fury's Rage", "heroic".to_string()));
        assert_eq!(super::item_family_tier("Epic The Fury's Rage"), ("The Fury's Rage", "epic".to_string()));
        assert_eq!(super::item_family_tier("Legendary The Fury's Rage"), ("The Fury's Rage", "legendary".to_string()));
    }

    #[test]
    fn quest_folder_identifies_a_missing_source_without_adding_one() {
        let root = std::env::temp_dir().join(format!("ddo-data-quest-folder-{}", std::process::id()));
        let quest_folder = root.join("packs/test-pack/test-quest");
        std::fs::create_dir_all(&quest_folder).unwrap();
        std::fs::write(quest_folder.join("quest.toml"), "name = \"Test Quest\"\n").unwrap();
        let item_path = quest_folder.join("test-item.toml");
        assert_eq!(super::folder_quest_name(&root, &item_path).unwrap().as_deref(), Some("Test Quest"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn correction_notes_follow_a_renamed_entity() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        db.execute(
            "INSERT INTO corrections (kind, name, field, from_value, to_value, reason, source, read)
             VALUES ('item', 'Old Name', 'name', ?1, ?2, 'wiki spelling', 'https://ddowiki.com/page/Old_Name', '2026-10-06')",
            ["\"Old Name\"", "\"New Name\""],
        )
        .unwrap();
        let notes = super::correction_notes(&db, "items", "New Name").unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].upstream, Some(serde_json::json!("Old Name")));
    }

    #[test]
    fn explicit_variant_qualifies_a_named_identity() {
        assert_eq!(super::qualified_slug("Underwater Action", Some("2")), "underwater-action-2");
        assert_eq!(super::qualified_slug("AasimarCore2", Some("v1")), "aasimarcore2-v1");
    }

    #[test]
    fn crafting_augments_require_their_own_family() {
        let root = Path::new("data");
        let path = root.join("packs/shared/crafting/nearly-complete/augments/charisma-6.toml");
        let error = super::schemas::read_file(root, &path, "name = '+6 Charisma'\ndescription = ''\n").unwrap_err();
        assert!(error.to_string().contains("family"), "{error}");
        let file = super::schemas::read_file(
            root,
            &path,
            "name = '+6 Charisma'\ndescription = ''\nfamily = 'NearlyComplete'\n",
        )
        .unwrap();
        assert_eq!(file.as_value().unwrap()["family"].as_str(), Some("NearlyComplete"));
    }

    #[test]
    fn recipe_focus_paths_preserve_element_order_and_gem_and_essence() {
        let paired = "Additional effect for an Invasion focus of Air and a Subjugation focus of Earth; no cost.";
        let reversed = "Additional effect for an Invasion focus of Earth and a Subjugation focus of Air; no cost.";
        assert_eq!(super::recipe_focus_path(paired).as_deref(), Some("Invasion Air / Subjugation Earth"));
        assert_eq!(super::recipe_focus_path(reversed).as_deref(), Some("Invasion Earth / Subjugation Air"));
        let imbued = "Imbued Shard made from focus type Fire, gem type Escalation, essence type Material.";
        assert_eq!(super::recipe_focus_path(imbued).as_deref(), Some("Fire:Escalation:Material"));
        let legendary = "Tier 1/2 foci Fire and Water (page code FWEa WFEa FWAe WFAe) give no bonus.";
        assert_eq!(super::recipe_focus_path(legendary).as_deref(), Some("FWEa WFEa FWAe WFAe"));
    }

    #[test]
    fn every_false_default_is_omitted_from_data_files() {
        fn assert_no_false(value: &toml::Value, path: &str) {
            match value {
                toml::Value::Boolean(false) => panic!("{path}: false default was serialized"),
                toml::Value::Table(fields) => {
                    for (field, value) in fields {
                        if !matches!(field.as_str(), "upstream" | "correction") {
                            assert_no_false(value, &format!("{path}.{field}"));
                        }
                    }
                }
                toml::Value::Array(elements) => {
                    for (index, value) in elements.iter().enumerate() {
                        assert_no_false(value, &format!("{path}[{index}]"));
                    }
                }
                _ => {}
            }
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        for entry in walkdir::WalkDir::new(&root) {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().is_some_and(|extension| extension == "toml") {
                let contents = std::fs::read_to_string(path).unwrap();
                let file = super::schemas::read_file(&root, path, &contents).unwrap();
                assert_no_false(&file.as_value().unwrap(), &path.display().to_string());
            }
        }
    }

    #[test]
    fn complete_export_query_count_does_not_grow_with_entities() {
        use rusqlite::trace::{TraceEvent, TraceEventCodes};
        use std::sync::atomic::{AtomicUsize, Ordering};
        static QUERY_COUNT: AtomicUsize = AtomicUsize::new(0);
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let wiki = WikiOverrides::from_dir(&fixtures.join("wiki")).unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        crate::build::build_database(
            &fixtures.join("DataFiles"),
            &wiki,
            &Corrections::default(),
            &mut db,
            &DatasetVersion { upstream_sha: "fixture".to_string(), built_at: "2026-10-08T00:00:00Z".to_string() },
        )
        .unwrap();
        let root = std::env::temp_dir().join(format!("ddo-etl-export-scale-{}", std::process::id()));
        db.trace_v2(
            TraceEventCodes::SQLITE_TRACE_STMT,
            Some(|event| {
                if let TraceEvent::Stmt(_, _) = event {
                    QUERY_COUNT.fetch_add(1, Ordering::SeqCst);
                }
            }),
        );
        QUERY_COUNT.store(0, Ordering::SeqCst);
        let first = super::export_database(&db, &root.join("first")).unwrap();
        let first_count = QUERY_COUNT.load(Ordering::SeqCst);
        let annotations = super::annotations::EntityAnnotations::from_database(&db).unwrap();
        let notes = super::CorrectionNoteIndex::from_database(&db).unwrap();
        let mut modifiers = super::modifier_rows_by_owner(&db).unwrap();
        let mut clickies = super::kinds::ClickiesFile::from_rows(
            &db,
            &annotations,
            &mut modifiers,
            &super::export::CorrectionNoteIndex::from_database(&db).unwrap(),
        )
        .unwrap();
        let mut guild_buffs = super::kinds::GuildBuffsFile::from_rows(&db, &annotations, &mut modifiers).unwrap();
        let mut optional_buffs =
            super::kinds::OptionalBuffsFile::from_rows(&db, &annotations, &mut modifiers, &notes).unwrap();
        let trees =
            super::kinds::EnhancementTreeFile::all_from_rows(&db, &annotations, &notes, &mut modifiers).unwrap();
        let systems = super::kinds::CraftingSystemFile::all_from_rows(&db, &annotations).unwrap();
        let mut identities = super::ids::StableIds::default();
        for row in &mut clickies.row {
            row.name.push_str(" Copy");
        }
        for row in &mut guild_buffs.row {
            row.name.push_str(" Copy");
        }
        for row in &mut optional_buffs.row {
            row.name.push_str(" Copy");
        }
        clickies.write_clickies(&db, Path::new("effects/clickies.toml"), &mut identities).unwrap();
        guild_buffs.write_guild_buffs(&db, Path::new("characters/guild-buffs.toml"), &mut identities).unwrap();
        optional_buffs.write_optional_buffs(&db, Path::new("characters/optional-buffs.toml"), &mut identities).unwrap();
        for mut tree in trees.into_values() {
            tree.name.push_str(" Copy");
            for enhancement in &mut tree.enhancement {
                enhancement.internal_name.push_str("Copy");
            }
            let identity = slug(&tree.name);
            let id = identities.issue("enhancement_trees", &identity, Path::new("tree.toml")).unwrap();
            tree.write_enhancement_tree(&db, Path::new("tree.toml"), id, &identity).unwrap();
        }
        let mut added_annotations = super::annotations::EntityAnnotations::default();
        for mut system in systems.into_values() {
            system.name.push_str(" Copy");
            system.family.clear();
            system.recipe.clear();
            system.write_crafting_system(&db, Path::new("crafting.toml"), &mut identities).unwrap();
            added_annotations
                .add_data_file(
                    Path::new("data"),
                    Path::new("data/packs/shared/crafting/copy/crafting.toml"),
                    &super::schemas::SourceFile::CraftingSystem(system),
                )
                .unwrap();
        }
        for index in 0..25 {
            let name = format!("Scale Set {index}");
            let set: super::kinds::SetFile =
                toml::from_str(&format!("name = {name:?}\n[[item]]\nname = 'Legendary Cloak of Winter'\n")).unwrap();
            let identity = slug(&name);
            let id = super::ids::stable_hash(&["set_bonuses", &identity]);
            set.write_set(&db, Path::new("packs/shared/sets/test.toml"), id, &identity).unwrap();
            set.insert_members(&db, Path::new("packs/shared/sets/test.toml"), id).unwrap();
            let quest_name = format!("Scale Quest {index}");
            let quest: super::kinds::QuestFile =
                toml::from_str(&format!("name = {quest_name:?}\npack = 'Free to Play'\n")).unwrap();
            let identity = slug(&quest_name);
            quest
                .write_quest(&db, Path::new("quest.toml"), super::ids::stable_hash(&["quests", &identity]), &identity)
                .unwrap();
            let chain: super::kinds::QuestSeriesFile = toml::from_str(&format!(
                "name = 'Scale Chain {index}'\nquests = [{quest_name:?}]\n[[citation]]\nurl = 'https://ddowiki.com/page/Example'\nread = '2026-10-14'\n"
            )).unwrap();
            let id = chain.write_quest_series(&db, Path::new("chain.toml"), &mut identities, "quest_chains").unwrap();
            chain.insert_quests(&db, Path::new("chain.toml"), id, "quest_chain_quests", "chain_id").unwrap();
            added_annotations
                .add_data_file(
                    Path::new("data"),
                    Path::new("data/packs/shared/chains/scale.toml"),
                    &super::schemas::SourceFile::QuestChain(chain),
                )
                .unwrap();
        }
        added_annotations.store(&db).unwrap();
        db.execute("DELETE FROM entity_identity", []).unwrap();
        super::insert_entity_identities(&db).unwrap();
        QUERY_COUNT.store(0, Ordering::SeqCst);
        let second = super::export_database(&db, &root.join("second")).unwrap();
        let second_count = QUERY_COUNT.load(Ordering::SeqCst);
        assert!(second.row_count > first.row_count);
        assert!(second.written_files.len() > first.written_files.len());
        assert!(first_count > 0);
        assert_eq!(second_count, first_count);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn export_places_authored_packs_and_member_sets_together() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let mut db = Connection::open_in_memory().unwrap();
        crate::build::build_database(
            &fixtures.join("DataFiles"),
            &WikiOverrides::from_dir(&fixtures.join("wiki")).unwrap(),
            &Corrections::default(),
            &mut db,
            &DatasetVersion { upstream_sha: "fixture".into(), built_at: "2026-10-14T00:00:00Z".into() },
        )
        .unwrap();
        let layout = super::ExportLayout::from_database(&db).unwrap();
        for (table, name, expected) in [
            ("vendors", "Morten Edgewright", "packs/free-to-play/vendors/morten-edgewright/vendor.toml"),
            ("quest_chains", "The Lost Seekers", "packs/free-to-play/chains/the-lost-seekers.toml"),
            (
                "sagas",
                "The Haunting of Saltmarsh",
                "packs/sinister-secret-of-saltmarsh/sagas/the-haunting-of-saltmarsh.toml",
            ),
            ("set_bonuses", "Cooking By the Book", "packs/shared/sets/cooking-by-the-book.toml"),
        ] {
            let id =
                db.query_row(&format!("SELECT id FROM {table} WHERE name = ?1"), [name], |row| row.get(0)).unwrap();
            assert_eq!(layout.path(table, id).unwrap(), Path::new(expected), "{name}");
        }
    }

    #[test]
    fn fixture_export_round_trips_every_table_and_is_idempotent() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let wiki = WikiOverrides::from_dir(&fixtures.join("wiki")).unwrap();
        let mut baseline = Connection::open_in_memory().unwrap();
        crate::build::build_database(
            &fixtures.join("DataFiles"),
            &wiki,
            &Corrections::from_dir(&fixtures.join("data-corrections")).unwrap(),
            &mut baseline,
            &DatasetVersion { upstream_sha: "fixture".to_string(), built_at: "2026-10-06T00:00:00Z".to_string() },
        )
        .unwrap();
        let root = std::env::temp_dir().join(format!("ddo-etl-data-roundtrip-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let stale_file = root.join("packs/unsourced/gear/obsolete-item.toml");
        std::fs::create_dir_all(stale_file.parent().unwrap()).unwrap();
        std::fs::write(&stale_file, "id = 1\n").unwrap();
        let stale_schema = root.join(".schemas/obsolete.schema.json");
        std::fs::create_dir_all(stale_schema.parent().unwrap()).unwrap();
        std::fs::write(&stale_schema, "{}\n").unwrap();
        let stale_icon = root.join("characters/enhancements/obsolete/icons/Unused.png");
        std::fs::create_dir_all(stale_icon.parent().unwrap()).unwrap();
        std::fs::write(&stale_icon, b"unowned icon").unwrap();
        let first_report = export_database(&baseline, &root).unwrap();
        assert_eq!(first_report.icon_actions.len(), 1, "{first_report}");
        assert!(first_report.icon_actions[0].contains(&stale_icon.display().to_string()));
        assert!(first_report.icon_actions[0].contains("delete"));
        assert_eq!(std::fs::read(&stale_icon).unwrap(), b"unowned icon");
        let mut stale_icon_db = Connection::open_in_memory().unwrap();
        let error = format!("{:#}", build_from_data(&root, &mut stale_icon_db).unwrap_err());
        assert!(error.contains(&first_report.icon_actions[0]), "{error}");
        std::fs::remove_file(&stale_icon).unwrap();
        let aliases_path = root.join("upstream/source-aliases.toml");
        let aliases_contents = std::fs::read(&aliases_path).unwrap();
        std::fs::remove_file(&aliases_path).unwrap();
        let mut missing_file_db = Connection::open_in_memory().unwrap();
        let missing_file_error = format!("{:#}", build_from_data(&root, &mut missing_file_db).unwrap_err());
        assert!(missing_file_error.contains(&aliases_path.display().to_string()), "{missing_file_error}");
        std::fs::write(&aliases_path, aliases_contents).unwrap();

        let fresh_exports = std::env::temp_dir().join(format!("ddo-etl-fresh-exports-{}", std::process::id()));
        let fresh_a = fresh_exports.join("fresh-a");
        let fresh_b = fresh_exports.join("fresh-b");
        super::export_database(&baseline, &fresh_a).unwrap();
        super::export_database(&baseline, &fresh_b).unwrap();
        let toml_bytes = |folder: &Path| -> BTreeMap<std::path::PathBuf, Vec<u8>> {
            walkdir::WalkDir::new(folder)
                .into_iter()
                .map(Result::unwrap)
                .filter(|entry| {
                    entry.file_type().is_file() && entry.path().extension().is_some_and(|ext| ext == "toml")
                })
                .map(|entry| {
                    (entry.path().strip_prefix(folder).unwrap().to_path_buf(), std::fs::read(entry.path()).unwrap())
                })
                .collect()
        };
        assert_eq!(
            toml_bytes(&root),
            toml_bytes(&fixtures.join("data")),
            "export must match the committed fixture data"
        );
        assert_eq!(toml_bytes(&root), toml_bytes(&fresh_a));
        assert_eq!(toml_bytes(&fresh_a), toml_bytes(&fresh_b));
        std::fs::remove_dir_all(fresh_exports).unwrap();
        assert!(!stale_file.exists());
        assert!(!stale_schema.exists());
        assert!(root.join("effects/bonus-types.toml").is_file());
        assert!(root.join("effects/augment-slot-types.toml").is_file());
        assert!(!root.join("rules").exists());
        for folder in ["characters", "effects", "packs", "upstream"] {
            assert!(!root.join(folder).join("rows").exists());
        }
        assert!(walkdir::WalkDir::new(&root)
            .into_iter()
            .map(Result::unwrap)
            .filter(|entry| entry.file_type().is_file())
            .any(|entry| std::fs::read_to_string(entry.path()).is_ok_and(|contents| contents.contains("[[weapon]]"))));
        assert!(walkdir::WalkDir::new(&root)
            .into_iter()
            .map(Result::unwrap)
            .filter(|entry| entry.file_type().is_file())
            .any(|entry| std::fs::read_to_string(entry.path())
                .is_ok_and(|contents| contents.contains("[[socket.option]]"))));
        assert!(
            walkdir::WalkDir::new(&root)
                .into_iter()
                .map(Result::unwrap)
                .filter(|entry| entry.file_type().is_file())
                .any(|entry| std::fs::read_to_string(entry.path())
                    .is_ok_and(|contents| contents.contains("[[modifier]]")))
        );
        for entry in walkdir::WalkDir::new(&root).into_iter().map(Result::unwrap) {
            if entry.path().extension().is_some_and(|extension| extension == "toml") {
                let contents = std::fs::read_to_string(entry.path()).unwrap();
                assert!(!contents.contains("provenance = "), "{}", entry.path().display());
                assert!(!contents.contains("table = "), "{}", entry.path().display());
                assert!(!contents.contains("owner_id = "), "{}", entry.path().display());
                assert!(!contents.contains("feat_id = "), "{}", entry.path().display());
                assert!(!contents.contains("group_index = "), "{}", entry.path().display());
                assert!(!contents.contains("slot_order = "), "{}", entry.path().display());
                assert!(!contents.contains("option_order = "), "{}", entry.path().display());
                if entry.path().file_name().is_some_and(|name| name == "quest.toml") {
                    assert!(!contents.contains("pack_id = "), "{}", entry.path().display());
                    assert!(!contents.contains("patron_id = "), "{}", entry.path().display());
                }
                if entry.path().components().any(|component| component.as_os_str() == "feats") {
                    assert!(!contents.contains("source_id = "), "{}", entry.path().display());
                }
            }
        }
        crate::icons::export_icons(&fixtures.join("DataFiles"), &root.join("icons")).unwrap();
        std::fs::create_dir_all(root.join("icons/ui")).unwrap();
        std::fs::copy(root.join("icons/feats/PowerAttack.png"), root.join("icons/ui/Unknown.png")).unwrap();
        let icon_report = crate::icons::complete_referenced_icons(&root).unwrap();
        for missing_icon in icon_report.missing_icons {
            let icon_path = root.join("icons").join(missing_icon);
            std::fs::create_dir_all(icon_path.parent().unwrap()).unwrap();
            std::fs::copy(root.join("icons/ui/Unknown.png"), icon_path).unwrap();
        }
        for entry in walkdir::WalkDir::new(&root).into_iter().map(Result::unwrap) {
            let path = entry.path();
            if !entry.file_type().is_file() || path.extension().is_none_or(|extension| extension != "toml") {
                continue;
            }
            let Some((_, kind)) = crate::icons::icon_kind_for_data_path(path.strip_prefix(&root).unwrap()) else {
                continue;
            };
            let contents = std::fs::read_to_string(path).unwrap();
            let file = super::schemas::read_file(&root, path, &contents).unwrap();
            for (icon_kind, icon, _) in file.icons(kind) {
                let icon_path =
                    root.join("icons").join(if icon_kind == "*" { "" } else { icon_kind }).join(format!("{icon}.png"));
                if !icon_path.is_file() {
                    std::fs::create_dir_all(icon_path.parent().unwrap()).unwrap();
                    std::fs::copy(root.join("icons/ui/Unknown.png"), icon_path).unwrap();
                }
            }
        }
        crate::icons::place_enhancement_icons_beside_trees(&root).unwrap();
        let missing_icon = root.join("icons/items/Quarterstaff_6a.png");
        let icon_bytes = std::fs::read(&missing_icon).unwrap();
        std::fs::remove_file(&missing_icon).unwrap();
        let mut missing_icon_db = Connection::open_in_memory().unwrap();
        let missing_icon_error = format!("{:#}", build_from_data(&root, &mut missing_icon_db).unwrap_err());
        assert!(missing_icon_error.contains("Quarterstaff_6a"), "{missing_icon_error}");
        std::fs::write(&missing_icon, icon_bytes).unwrap();
        let broken_references = [
            (
                "packs/magic-of-myth-drannor/quests/book-burning/gear/buckler-of-the-golden-age.toml",
                "source",
                "Book Burnin",
            ),
            ("packs/free-to-play/chains/the-lost-seekers.toml", "quests", "Redemptio"),
            ("packs/shared/sets/eminence-of-winter.toml", "item", "Legendary Cloak of Winte"),
        ];
        let mut originals = Vec::new();
        for (relative, array, unknown) in broken_references {
            let path = root.join(relative);
            let original = std::fs::read_to_string(&path).unwrap();
            let mut document: toml::Value = toml::from_str(&original).unwrap();
            if array == "quests" {
                document[array][0] = toml::Value::String(unknown.into());
            } else {
                document[array][0]["name"] = toml::Value::String(unknown.into());
            }
            if array == "source" {
                document["effect"][0]["name"] = toml::Value::String("Guardbreakin".into());
            }
            std::fs::write(&path, toml::to_string(&document).unwrap()).unwrap();
            originals.push((path, original));
        }
        let aliases_path = root.join("upstream/source-aliases.toml");
        let aliases_original = std::fs::read_to_string(&aliases_path).unwrap();
        let mut aliases: toml::Value = toml::from_str(&aliases_original).unwrap();
        aliases["adventure_pack"][0]["reason"] = toml::Value::String(String::new());
        std::fs::write(&aliases_path, toml::to_string(&aliases).unwrap()).unwrap();
        let error = format!("{:#}", build_from_data(&root, &mut Connection::open_in_memory().unwrap()).unwrap_err());
        assert!(error.contains("needs a name, target, reason, wiki page and read date"), "{error}");
        std::fs::write(&aliases_path, aliases_original).unwrap();
        for (relative, _, unknown) in broken_references {
            assert!(error.contains(relative) && error.contains(unknown), "{error}");
        }
        assert!(error.contains("[[source]] 1: unknown quest \"Book Burnin\", expected packs/magic-of-myth-drannor/quests/book-burnin/quest.toml; nearest: Book Burning"), "{error}");
        assert!(error.contains("[[effect]] 1: unknown effect \"Guardbreakin\", expected effects/guardbreakin.toml; nearest: Guardbreaking"), "{error}");
        println!("{error}");
        for (path, original) in originals {
            std::fs::write(path, original).unwrap();
        }
        let priced_item_path = walkdir::WalkDir::new(&root)
            .into_iter()
            .map(Result::unwrap)
            .filter(|entry| entry.path().extension().is_some_and(|extension| extension == "toml"))
            .find(|entry| std::fs::read_to_string(entry.path()).unwrap().contains("[[source.price]]"))
            .unwrap()
            .into_path();
        let valid_item = std::fs::read_to_string(&priced_item_path).unwrap();
        for (invalid, expected) in [
            (valid_item.replacen("kind = \"vendor\"", "kind = \"vendr\"", 1), "unknown source kind"),
            (
                {
                    let mut document: toml::Value = toml::from_str(&valid_item).unwrap();
                    let source = document["source"]
                        .as_array_mut()
                        .unwrap()
                        .iter_mut()
                        .find(|source| source.get("price").is_some())
                        .unwrap();
                    source["price"][0]["quantity"] = toml::Value::Integer(0);
                    toml::to_string(&document).unwrap()
                },
                "quantity",
            ),
        ] {
            std::fs::write(&priced_item_path, invalid).unwrap();
            let mut invalid_db = Connection::open_in_memory().unwrap();
            let error = format!("{:#}", build_from_data(&root, &mut invalid_db).unwrap_err());
            assert!(error.contains(&priced_item_path.strip_prefix(&root).unwrap().display().to_string()), "{error}");
            assert!(error.contains(expected), "{error}");
            std::fs::write(&priced_item_path, &valid_item).unwrap();
        }
        let bad_note_path = root.join("packs/unsourced/gear/bad-note.toml");
        std::fs::write(&bad_note_path, "name = 'Bad Note'\nslot = 'Feet'\nitem_category = 'Clothing'\nminimum_level = 1\n[[ddobuilder_correction]]\nkind = 'item'\nfield = 'name'\nreason = 'A rename without its upstream value'\nurl = 'https://ddowiki.com/page/Item:Bad_Note'\nread = '2026-10-08'\n").unwrap();
        let mut invalid_db = Connection::open_in_memory().unwrap();
        let error = format!("{:#}", build_from_data(&root, &mut invalid_db).unwrap_err());
        assert!(error.contains(&bad_note_path.display().to_string()), "{error}");
        assert!(error.contains("upstream name"), "{error}");
        for retired_kind in ["release", "slug_collision"] {
            std::fs::write(&bad_note_path, format!("name = 'Bad Note'\nslot = 'Feet'\nitem_category = 'Clothing'\nminimum_level = 1\n[[ddobuilder_correction]]\nkind = '{retired_kind}'\nfield = 'name'\nupstream = 'Bad note'\nreason = 'A review, not a correction'\nurl = 'https://ddowiki.com/page/Item:Bad_Note'\nread = '2026-10-08'\n")).unwrap();
            let error =
                format!("{:#}", build_from_data(&root, &mut Connection::open_in_memory().unwrap()).unwrap_err());
            assert!(error.contains(&bad_note_path.display().to_string()), "{error}");
            assert!(
                error.contains(&format!("{retired_kind:?} is not a DDOBuilder correction"))
                    && error.contains("[[citation]]"),
                "{error}"
            );
        }
        std::fs::remove_file(&bad_note_path).unwrap();
        let mut from_files = Connection::open_in_memory().unwrap();
        let built_report = build_from_data(&root, &mut from_files).unwrap();
        assert!(first_report.row_count >= built_report.row_count);
        assert!(built_report.layout_warnings.is_empty(), "{built_report}");
        let shared_chain_path = root.join("packs/free-to-play/chains/the-lost-seekers.toml");
        let original_chain = std::fs::read_to_string(&shared_chain_path).unwrap();
        let mut chain: super::kinds::QuestSeriesFile = toml::from_str(&original_chain).unwrap();
        chain.quests.retain(|quest| quest == "Redemption");
        std::fs::write(&shared_chain_path, chain.file_text().unwrap()).unwrap();
        let mut shortened_chain_db = Connection::open_in_memory().unwrap();
        let shortened = build_from_data(&root, &mut shortened_chain_db).unwrap();
        let expected = "quest_chains \"The Lost Seekers\": move packs/free-to-play/chains/the-lost-seekers.toml -> packs/free-to-play/the-coin-lords/chains/the-lost-seekers.toml";
        assert!(shortened.layout_warnings.iter().any(|warning| warning == expected), "{shortened}");
        export_database(&shortened_chain_db, &root).unwrap();
        let nested_chain_path = root.join("packs/free-to-play/the-coin-lords/chains/the-lost-seekers.toml");
        std::fs::write(&nested_chain_path, &original_chain).unwrap();
        let mut restored_membership_db = Connection::open_in_memory().unwrap();
        let restored = build_from_data(&root, &mut restored_membership_db).unwrap();
        let expected = "quest_chains \"The Lost Seekers\": move packs/free-to-play/the-coin-lords/chains/the-lost-seekers.toml -> packs/free-to-play/chains/the-lost-seekers.toml";
        assert!(restored.layout_warnings.iter().any(|warning| warning == expected), "{restored}");
        export_database(&restored_membership_db, &root).unwrap();
        assert_eq!(toml_bytes(&root), toml_bytes(&fixtures.join("data")));
        let mut moved_kinds = std::collections::BTreeSet::new();
        let mut moved_files = Vec::new();
        for entry in walkdir::WalkDir::new(&root).into_iter().map(Result::unwrap) {
            let path = entry.path();
            if !path.extension().is_some_and(|extension| extension == "toml") {
                continue;
            }
            let file = super::schemas::read_file(&root, path, &std::fs::read_to_string(path).unwrap()).unwrap();
            let Some((kind, name)) = file.layout_entity() else {
                continue;
            };
            if kind == "quests" && !matches!(&file, super::schemas::SourceFile::Quest(quest) if quest.is_challenge) {
                continue;
            }
            if matches!(kind, "adventure_packs" | "classes" | "races" | "enhancement_trees") {
                continue;
            }
            let file_kind = super::schemas::file_kind(&root, path).unwrap();
            if !moved_kinds.insert(file_kind) {
                continue;
            }
            let relative = path.strip_prefix(&root).unwrap();
            let misplaced = if relative.starts_with("packs") {
                let suffix = relative.components().skip(2).collect::<std::path::PathBuf>();
                root.join("packs/misplaced").join(suffix)
            } else {
                path.parent().unwrap().join("misplaced").join(path.file_name().unwrap())
            };
            moved_files.push((path.to_path_buf(), misplaced, kind, name.to_string()));
        }
        assert_eq!(moved_kinds.len(), 15);
        for (original, misplaced, _, _) in &moved_files {
            std::fs::create_dir_all(misplaced.parent().unwrap()).unwrap();
            std::fs::rename(original, misplaced).unwrap();
        }
        let mut relocated = Connection::open_in_memory().unwrap();
        let warnings = build_from_data(&root, &mut relocated).unwrap();
        for (original, misplaced, kind, name) in &moved_files {
            let expected = format!(
                "{kind} {name:?}: move {} -> {}",
                misplaced.strip_prefix(&root).unwrap().display(),
                original.strip_prefix(&root).unwrap().display()
            );
            assert!(warnings.layout_warnings.contains(&expected), "missing {expected}: {warnings}");
            assert!(warnings.to_string().contains(&format!("WARN data layout: {expected}")));
            std::fs::rename(misplaced, original).unwrap();
        }

        for table in NAMED_ENTITY_TABLES {
            let duplicates: i64 = from_files
                .query_row(
                    &format!("SELECT COUNT(*) FROM (SELECT slug FROM {table} WHERE slug IS NOT NULL GROUP BY slug HAVING COUNT(*) > 1)"),
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(duplicates, 0, "{table} contains a duplicate slug");
            for db in [&baseline, &from_files] {
                let missing: i64 = db
                    .query_row(&format!("SELECT COUNT(*) FROM {table} WHERE slug IS NULL OR slug = ''"), [], |row| {
                        row.get(0)
                    })
                    .unwrap();
                assert_eq!(missing, 0, "{table} contains an unnamed identity");
            }
        }
        for table in table_names(&baseline).unwrap() {
            let baseline_count: i64 =
                baseline.query_row(&format!("SELECT COUNT(*) FROM \"{table}\""), [], |row| row.get(0)).unwrap();
            let file_count: i64 =
                from_files.query_row(&format!("SELECT COUNT(*) FROM \"{table}\""), [], |row| row.get(0)).unwrap();
            assert_eq!(baseline_count, file_count, "{table}");
        }
        let first_files: BTreeMap<_, _> = walkdir::WalkDir::new(&root)
            .into_iter()
            .map(Result::unwrap)
            .filter(|entry| entry.file_type().is_file())
            .map(|entry| (entry.path().to_path_buf(), std::fs::read(entry.path()).unwrap()))
            .collect();
        let second_report = export_database(&from_files, &root).unwrap();
        let second_files: BTreeMap<_, _> = walkdir::WalkDir::new(&root)
            .into_iter()
            .map(Result::unwrap)
            .filter(|entry| entry.file_type().is_file())
            .map(|entry| (entry.path().to_path_buf(), std::fs::read(entry.path()).unwrap()))
            .collect();
        assert_eq!(first_report.file_count_by_folder, second_report.file_count_by_folder);
        let first_difference = first_files.keys().find(|path| first_files.get(*path) != second_files.get(*path));
        if let Some(path) = first_difference {
            let before = String::from_utf8_lossy(first_files.get(path).unwrap());
            let after = String::from_utf8_lossy(second_files.get(path).unwrap());
            let changed = before.lines().zip(after.lines()).enumerate().find(|(_, (old, new))| old != new);
            let line = changed.map_or(0, |(index, _)| index);
            let old_context = before.lines().skip(line.saturating_sub(8)).take(18).collect::<Vec<_>>();
            let new_context = after.lines().skip(line.saturating_sub(8)).take(18).collect::<Vec<_>>();
            panic!("first changed file: {} line {}: {old_context:?} vs {new_context:?}", path.display(), line + 1);
        }
        baseline.execute("UPDATE dataset_version SET built_at = '2026-10-07T00:00:00Z'", []).unwrap();
        baseline.execute("UPDATE schema_version SET applied_at = '2026-10-07 00:00:00'", []).unwrap();
        export_database(&baseline, &root).unwrap();
        let third_files: BTreeMap<_, _> = walkdir::WalkDir::new(&root)
            .into_iter()
            .map(Result::unwrap)
            .filter(|entry| entry.file_type().is_file())
            .map(|entry| (entry.path().to_path_buf(), std::fs::read(entry.path()).unwrap()))
            .collect();
        let second_difference = second_files.keys().find(|path| second_files.get(*path) != third_files.get(*path));
        assert!(second_difference.is_none(), "second changed file: {second_difference:?}");
        let new_item_path = root.join("packs/unsourced/gear/new-test-item.toml");
        std::fs::create_dir_all(new_item_path.parent().unwrap()).unwrap();
        std::fs::write(
            &new_item_path,
            "name = \"New Test Item\"\nslot = \"Feet\"\nitem_category = \"Clothing\"\nwiki_url = \"https://ddowiki.com/page/Item:New_Test_Item\"\nminimum_level = 1\n[[effect]]\nname = \"Ethereal\"\n[[source]]\nkind = \"quest\"\nname = \"Book Burning\"\nloot_type = \"chest\"\n[[modifier]]\neffect_type = \"Strength\"\nbonus_type = \"Enhancement\"\namount_type = \"Simple\"\namounts = [6]\n[[modifier.requirement]]\nreq_type = \"Feat\"\nitems = [\"Power Attack\"]\n",
        )
        .unwrap();
        let file_bytes_before_build = walkdir::WalkDir::new(&root)
            .into_iter()
            .map(Result::unwrap)
            .filter(|entry| entry.file_type().is_file())
            .map(|entry| {
                let path = entry.path().to_path_buf();
                let bytes = std::fs::read(&path).unwrap();
                let original_permissions = std::fs::metadata(&path).unwrap().permissions();
                let mut readonly_permissions = original_permissions.clone();
                readonly_permissions.set_readonly(true);
                std::fs::set_permissions(&path, readonly_permissions).unwrap();
                (path, (bytes, original_permissions))
            })
            .collect::<BTreeMap<_, _>>();
        let mut first_with_new_item = Connection::open_in_memory().unwrap();
        build_from_data(&root, &mut first_with_new_item).unwrap();
        let new_item_id: i64 = first_with_new_item
            .query_row("SELECT id FROM items WHERE name = 'New Test Item'", [], |row| row.get(0))
            .unwrap();
        for (table, owner_column) in [("sources", "item_id"), ("modifiers", "source_id")] {
            let count: i64 = first_with_new_item
                .query_row(&format!("SELECT COUNT(*) FROM {table} WHERE {owner_column} = ?1"), [new_item_id], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 1, "{table}");
        }
        let modifier_id: i64 = first_with_new_item
            .query_row("SELECT id FROM modifiers WHERE source_kind = 'item' AND source_id = ?1", [new_item_id], |row| {
                row.get(0)
            })
            .unwrap();
        let requirement_count: i64 = first_with_new_item
            .query_row(
                "SELECT COUNT(*) FROM requirements WHERE owner_kind = 'modifier' AND owner_id = ?1",
                [modifier_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(requirement_count, 1);
        let allocated_file = std::fs::read_to_string(&new_item_path).unwrap();
        assert!(!allocated_file.contains("id = "));
        let mut second_with_new_item = Connection::open_in_memory().unwrap();
        build_from_data(&root, &mut second_with_new_item).unwrap();
        assert_eq!(allocated_file, std::fs::read_to_string(&new_item_path).unwrap());
        for (path, (bytes, original_permissions)) in &file_bytes_before_build {
            assert_eq!(&std::fs::read(path).unwrap(), bytes, "{} changed during build", path.display());
            std::fs::set_permissions(path, original_permissions.clone()).unwrap();
        }
        let moved_item_path = root.join("packs/general/gear/new-test-item.toml");
        std::fs::create_dir_all(moved_item_path.parent().unwrap()).unwrap();
        std::fs::rename(&new_item_path, &moved_item_path).unwrap();
        let mut moved_item_database = Connection::open_in_memory().unwrap();
        build_from_data(&root, &mut moved_item_database).unwrap();
        let moved_identity: (i64, String) = moved_item_database
            .query_row("SELECT id, slug FROM items WHERE name = 'New Test Item'", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(moved_identity, (new_item_id, "new-test-item".to_string()));
        let renamed_item_path = root.join("packs/general/gear/renamed-test-item.toml");
        let moved_contents = std::fs::read_to_string(&moved_item_path).unwrap();
        std::fs::write(
            &moved_item_path,
            moved_contents.replace("name = \"New Test Item\"", "name = \"Renamed Test Item\""),
        )
        .unwrap();
        std::fs::rename(&moved_item_path, &renamed_item_path).unwrap();
        let mut renamed_item_database = Connection::open_in_memory().unwrap();
        build_from_data(&root, &mut renamed_item_database).unwrap();
        let renamed_identity: (i64, String) = renamed_item_database
            .query_row("SELECT id, slug FROM items WHERE name = 'Renamed Test Item'", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_ne!(renamed_identity.0, new_item_id);
        assert_eq!(renamed_identity.1, "renamed-test-item");
        let other_item_ids = |db: &Connection| {
            db.prepare(
                "SELECT name, id FROM items WHERE name NOT IN ('New Test Item', 'Renamed Test Item') ORDER BY name",
            )
            .unwrap()
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
        };
        assert_eq!(other_item_ids(&moved_item_database), other_item_ids(&renamed_item_database));
        std::fs::write(&renamed_item_path, moved_contents).unwrap();
        std::fs::rename(&renamed_item_path, &new_item_path).unwrap();
        assert_eq!(
            table_rows(&first_with_new_item, "items").unwrap(),
            table_rows(&second_with_new_item, "items").unwrap()
        );
        export_database(&first_with_new_item, &root).unwrap();
        let exported_item = walkdir::WalkDir::new(&root)
            .into_iter()
            .map(Result::unwrap)
            .filter(|entry| {
                entry.file_type().is_file() && entry.path().extension().is_some_and(|extension| extension == "toml")
            })
            .map(|entry| entry.into_path())
            .find(|path| {
                std::fs::read_to_string(path).is_ok_and(|contents| contents.starts_with("name = \"New Test Item\""))
            })
            .unwrap();
        let first_exported_item = std::fs::read_to_string(&exported_item).unwrap();
        export_database(&second_with_new_item, &root).unwrap();
        assert_eq!(first_exported_item, std::fs::read_to_string(&exported_item).unwrap());
        let copied_root = std::env::temp_dir().join(format!("ddo-etl-data-item-removal-{}", std::process::id()));
        std::fs::create_dir_all(&copied_root).unwrap();
        for entry in walkdir::WalkDir::new(&root).into_iter().map(Result::unwrap) {
            if entry.file_type().is_file() {
                let destination = copied_root.join(entry.path().strip_prefix(&root).unwrap());
                std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
                std::fs::copy(entry.path(), destination).unwrap();
            }
        }
        std::fs::remove_file(copied_root.join(exported_item.strip_prefix(&root).unwrap())).unwrap();
        let mut without_new_item = Connection::open_in_memory().unwrap();
        build_from_data(&copied_root, &mut without_new_item).unwrap();
        let vocabulary_count = |db: &Connection| {
            db.query_row(
                "SELECT count.item_count FROM effect_vocabulary_counts count JOIN effects e ON e.id = count.id WHERE count.kind = 'effect' AND e.name = 'Ethereal'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
        };
        assert_eq!(vocabulary_count(&second_with_new_item), vocabulary_count(&without_new_item) + 1);
        let priced_item_path = walkdir::WalkDir::new(&copied_root)
            .into_iter()
            .map(Result::unwrap)
            .filter(|entry| {
                entry.file_type().is_file() && entry.path().extension().is_some_and(|extension| extension == "toml")
            })
            .map(|entry| entry.into_path())
            .find(|path| {
                std::fs::read_to_string(path)
                    .is_ok_and(|contents| contents.starts_with("name = \"Alabaster of the Twelve\""))
            })
            .unwrap();
        let mut priced_item: super::kinds::ItemFile =
            toml::from_str(&std::fs::read_to_string(&priced_item_path).unwrap()).unwrap();
        let offer = priced_item.source.iter_mut().find(|source| source.kind == "vendor").unwrap();
        assert_eq!(
            offer.price,
            vec![crate::vendor_prices::VendorPrice { ingredient: "Planar Shard".to_string(), quantity: 10 }]
        );
        offer.price.clear();
        std::fs::write(&priced_item_path, priced_item.file_text().unwrap()).unwrap();
        let mut without_vendor_cost = Connection::open_in_memory().unwrap();
        build_from_data(&copied_root, &mut without_vendor_cost).unwrap();
        let no_cost_export = copied_root.join("export-without-cost");
        super::export_database(&without_vendor_cost, &no_cost_export).unwrap();
        let restored_item: super::kinds::ItemFile = toml::from_str(
            &std::fs::read_to_string(no_cost_export.join(priced_item_path.strip_prefix(&copied_root).unwrap()))
                .unwrap(),
        )
        .unwrap();
        assert!(restored_item
            .source
            .iter()
            .filter(|source| source.kind == "vendor")
            .all(|source| source.price.is_empty()));
        let corrected_item = walkdir::WalkDir::new(&root)
            .into_iter()
            .map(Result::unwrap)
            .filter(|entry| {
                entry.file_type().is_file() && entry.path().extension().is_some_and(|extension| extension == "toml")
            })
            .map(|entry| entry.into_path())
            .find(|path| {
                std::fs::read_to_string(path)
                    .is_ok_and(|contents| contents.starts_with("name = \"Epic Ethereal Bracers\""))
            })
            .unwrap();
        let mut altered_note = std::fs::read_to_string(&corrected_item).unwrap();
        altered_note.push_str("\n[[ddobuilder_correction]]\nkind = \"item\"\nfield = \"minimum_level\"\nupstream = 999\nreason = \"Wiki records the corrected level.\"\nurl = \"https://ddowiki.com/page/Item:Epic_Ethereal_Bracers\"\nread = \"2026-10-07\"\n");
        std::fs::write(&corrected_item, altered_note).unwrap();
        let corrections = Corrections::from_data(&root).unwrap();
        let mut stale_database = Connection::open_in_memory().unwrap();
        let stale_report = crate::build::build_database(
            &fixtures.join("DataFiles"),
            &wiki,
            &corrections,
            &mut stale_database,
            &DatasetVersion { upstream_sha: "fixture".to_string(), built_at: "2026-10-07T00:00:00Z".to_string() },
        )
        .unwrap();
        assert_eq!(stale_report.correction_stale_count, 1);
        std::fs::remove_dir_all(copied_root).unwrap();
        std::fs::remove_dir_all(&root).unwrap();
    }
}
