use ddo_etl::{build, corrections::Corrections, tree, wiki::WikiOverrides};
use ddo_model::DatasetVersion;
use rusqlite::Connection;
use std::path::Path;

#[test]
fn exported_ddobuilder_tree_builds_with_dc_icons_from_other_kinds() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import_ddobuilder/ba7e18e/DataFiles");
    let root = std::env::temp_dir().join(format!("ddo-import-dc-icons-{}", std::process::id()));
    let mut db = Connection::open_in_memory().unwrap();
    build::build_database(
        &source,
        &WikiOverrides::default(),
        &Corrections::default(),
        &mut db,
        &DatasetVersion {
            upstream_sha: "ba7e18eecafb1bb5fd6b6cf52cc0b6395bbeb20c".to_string(),
            built_at: "2026-10-17T00:00:00Z".to_string(),
        },
    )
    .unwrap();
    tree::export_ddobuilder_tree(&db, &source, &root).unwrap();
    let mut rebuilt = Connection::open_in_memory().unwrap();
    tree::build_from_data(&root, &mut rebuilt).unwrap();
    for (destination, original) in [
        ("characters/enhancements/half-orc/icons/Knock.png", "FeatImages/Knock.png"),
        ("icons/feats/Intimidate.png", "UIImages/Intimidate.png"),
        ("icons/feats/Diplomacy.png", "UIImages/Diplomacy.png"),
        ("icons/feats/Bluff.png", "UIImages/Bluff.png"),
    ] {
        assert_eq!(std::fs::read(root.join(destination)).unwrap(), std::fs::read(source.join(original)).unwrap());
    }
    let completed = ddo_etl::icons::complete_referenced_icons(&root).unwrap();
    assert_eq!(completed, ddo_etl::icons::IconCompletionReport::default());
    tree::export_ddobuilder_tree(&db, &source, &root).unwrap();
    tree::build_from_data(&root, &mut Connection::open_in_memory().unwrap()).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

use ddo_etl::import_ddobuilder::{import_ddobuilder, DdobuilderImportOptions};
use std::path::PathBuf;

const PREVIOUS_SHA: &str = "ba7e18eecafb1bb5fd6b6cf52cc0b6395bbeb20c";
const NEW_SHA: &str = "d1350a0c6ec396a65dc02680fa9801a14a79ebf3";

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import_ddobuilder")
}

fn copy_tree(source: &Path, destination: &Path) {
    for entry in walkdir::WalkDir::new(source) {
        let entry = entry.unwrap();
        if entry.file_type().is_file() {
            let target = destination.join(entry.path().strip_prefix(source).unwrap());
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn fieldless_citation_does_not_make_an_upstream_item_wiki_only() {
    let scratch = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataFiles");
    let mut db = Connection::open_in_memory().unwrap();
    let corrections = Corrections::from_dir(&source.parent().unwrap().join("data-corrections")).unwrap();
    build::build_database(
        &source,
        &WikiOverrides::default(),
        &corrections,
        &mut db,
        &DatasetVersion { upstream_sha: PREVIOUS_SHA.into(), built_at: "2026-10-18T00:00:00Z".into() },
    )
    .unwrap();
    db.execute("INSERT INTO citations (owner_kind, owner_id, field, url, read) SELECT 'items', id, '', 'https://ddowiki.com/page/Item:Reviewed', '2026-10-18' FROM items WHERE id = (SELECT MIN(id) FROM items)", []).unwrap();
    export_fixture_with_icons(&db, &source, scratch.path());
    let mut rebuilt = Connection::open_in_memory().unwrap();
    tree::build_from_data(scratch.path(), &mut rebuilt).unwrap();
    let provenance: String = rebuilt.query_row(
        "SELECT provenance FROM items JOIN citations ON owner_kind = 'items' AND owner_id = items.id WHERE citations.url = 'https://ddowiki.com/page/Item:Reviewed'", [], |row| row.get(0)
    ).unwrap();
    assert_eq!(provenance, "maetrim");
}

fn export_fixture_with_icons(db: &Connection, source: &Path, root: &Path) {
    tree::export_database(db, root).unwrap();
    ddo_etl::icons::export_icons(source, &root.join("icons")).unwrap();
    let icons = ddo_etl::icons::complete_referenced_icons(root).unwrap();
    for missing in icons.missing_icons {
        let path = root.join("icons").join(missing);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::copy(source.join("FeatImages/PowerAttack.png"), path).unwrap();
    }
    ddo_etl::icons::place_enhancement_icons_beside_trees(root).unwrap();
}

fn assert_overlay_tables_match(normalized: &Connection, ours: &Connection) {
    let mut delta = ddo_etl::delta::database_delta(normalized, ours).unwrap();
    delta.arrays.retain(|key, change| {
        if key.array != "citation" {
            return true;
        }
        assert!(
            change.previous.iter().all(|citation| change.new.contains(citation)),
            "XML citations must be a subset of C: {key:?}: {change:?}"
        );
        false
    });
    assert!(delta.is_empty(), "{delta:#?}");
    for table in ddo_etl::database_tables::table_names(ours).unwrap() {
        if table == "citations" {
            continue;
        }
        let columns = ddo_etl::database_tables::table_columns(ours, &table).unwrap();
        let stable_rows = |db: &Connection| {
            ddo_etl::database_tables::table_rows(db, &table)
                .unwrap()
                .into_iter()
                .map(|row| {
                    row.into_iter()
                        .zip(&columns)
                        .filter_map(|(value, column)| {
                            (column != "built_at" && !(table == "schema_version" && column == "applied_at"))
                                .then_some(value)
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(stable_rows(normalized), stable_rows(ours), "{table}");
    }
}

#[test]
fn converging_overlay_socket_renames_follow_the_writer_and_have_no_import_delta() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let scratch = tempfile::tempdir().unwrap();
    let mut fixture = Connection::open_in_memory().unwrap();
    tree::build_from_data(&fixtures.join("data"), &mut fixture).unwrap();
    let mut wiki = WikiOverrides::from_database(&fixture).unwrap();
    let mut corrections = Corrections::from_database(&fixture).unwrap();
    for (original, target) in [
        ("crafting: attuned to heroism 1", "crafting: reviewed tier 1"),
        ("crafting: attuned to heroism 2", "crafting: reviewed tier 1"),
        ("crafting: attuned to heroism 3", "crafting: reviewed tier 2"),
        ("crafting: attuned to heroism 4", "crafting: reviewed tier 2"),
    ] {
        let mut correction = corrections.entries[0].clone();
        correction.kind = ddo_model::enums::CorrectionKind::SocketLabel;
        correction.name = original.into();
        correction.field = "name".into();
        correction.from = ddo_etl::corrections::CorrectionValue::Text(original.into());
        correction.to = ddo_etl::corrections::CorrectionValue::Text(target.into());
        correction.family = None;
        correction.selection = None;
        corrections.entries.push(correction);
    }
    let item = wiki.items.iter_mut().find(|item| item.name == "Battle Axe of the Oozing Hunger").unwrap();
    item.augment_slots = vec!["crafting: reviewed tier 1".into(), "crafting: reviewed tier 2".into()];
    for recipe in &mut wiki.crafting_systems[0].recipes {
        recipe.slot = Some("crafting: reviewed tier 1".into());
        recipe.grants_slot = Some("crafting: reviewed tier 2".into());
    }
    let source = scratch.path().join("DataFiles");
    copy_tree(&fixtures.join("DataFiles"), &source);
    let root = scratch.path().join("data");
    let mut previous = Connection::open_in_memory().unwrap();
    let report = build::build_database(
        &source,
        &wiki,
        &corrections,
        &mut previous,
        &DatasetVersion { upstream_sha: PREVIOUS_SHA.into(), built_at: "2026-10-18T00:00:00Z".into() },
    )
    .unwrap();
    assert!(report.stale_corrections.is_empty(), "{:?}", report.stale_corrections);
    export_fixture_with_icons(&previous, &source, &root);
    let icons = scratch.path().join("icons");
    ddo_etl::icons::copy_data_icons(&root, &icons).unwrap();
    for (kind, folder) in ddo_etl::icons::ICON_KIND_SOURCE_FOLDERS {
        if icons.join(kind).is_dir() {
            copy_tree(&icons.join(kind), &source.join(folder));
        }
    }
    let mut ours = Connection::open_in_memory().unwrap();
    tree::build_from_data(&root, &mut ours).unwrap();
    let report = import_ddobuilder(&DdobuilderImportOptions {
        previous_dir: source.clone(),
        source_dir: source,
        data_dir: root.clone(),
        previous_sha: Some(PREVIOUS_SHA.into()),
        sha: Some(NEW_SHA.into()),
        wiki: WikiOverrides::from_database(&ours).unwrap(),
        corrections: Corrections::from_database(&ours).unwrap(),
    })
    .unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert!(!report.to_string().contains("CORRECTION"), "{report}");
    assert!(!report.to_string().contains("reviewed tier"), "{report}");
    let mut rebuilt = Connection::open_in_memory().unwrap();
    tree::build_from_data(&root, &mut rebuilt).unwrap();
    for db in [&previous, &rebuilt] {
        let labels = db.prepare("SELECT s.label FROM item_augment_slots l JOIN items i ON i.id=l.item_id JOIN augment_slot_types s ON s.id=l.slot_id WHERE i.name='Battle Axe of the Oozing Hunger' ORDER BY l.sort_order").unwrap().query_map([], |row| row.get::<_, String>(0)).unwrap().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(labels, ["crafting: reviewed tier 1", "crafting: reviewed tier 2"]);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM crafting_recipes r JOIN augment_slot_types s ON s.id=r.slot_id JOIN augment_slot_types g ON g.id=r.grants_slot_id WHERE s.label='crafting: reviewed tier 1' AND g.label='crafting: reviewed tier 2'", [], |row| row.get::<_, i64>(0)).unwrap(), wiki.crafting_systems[0].recipes.len() as i64);
    }
}

#[test]
fn converging_non_merging_overlay_renames_name_both_corrections() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut fixture = Connection::open_in_memory().unwrap();
    tree::build_from_data(&fixtures.join("data"), &mut fixture).unwrap();
    let mut corrections = Corrections::from_database(&fixture).unwrap();
    for original in ["Masterminds of Sharn", "Chill of Ravenloft"] {
        let mut correction = corrections.entries[0].clone();
        correction.kind = ddo_model::enums::CorrectionKind::AdventurePack;
        correction.name = original.into();
        correction.field = "name".into();
        correction.from = ddo_etl::corrections::CorrectionValue::Text(original.into());
        correction.to = ddo_etl::corrections::CorrectionValue::Text("Converging pack".into());
        correction.family = None;
        correction.selection = None;
        corrections.entries.push(correction);
    }
    let error = build::build_database(
        &fixtures.join("DataFiles"),
        &WikiOverrides::default(),
        &corrections,
        &mut Connection::open_in_memory().unwrap(),
        &DatasetVersion { upstream_sha: PREVIOUS_SHA.into(), built_at: "2026-10-18T00:00:00Z".into() },
    )
    .unwrap_err();
    let message = format!("{error:#}");
    for name in ["Masterminds of Sharn", "Chill of Ravenloft", "Converging pack"] {
        assert!(message.contains(name), "{message}");
    }
}

#[test]
fn overlay_references_follow_only_renames_active_in_each_xml_build() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let scratch = tempfile::tempdir().unwrap();
    let mut initial = Connection::open_in_memory().unwrap();
    tree::build_from_data(&fixtures.join("data"), &mut initial).unwrap();
    initial.execute("UPDATE adventure_packs SET name = 'Reviewed Masterminds of Sharn', slug = 'reviewed-masterminds-of-sharn' WHERE name = 'Masterminds of Sharn'", []).unwrap();
    initial.execute("INSERT INTO corrections (kind,name,field,from_value,to_value,reason,source,read) VALUES ('adventure_pack','Masterminds of Sharn','name','\"Masterminds of Sharn\"','\"Reviewed Masterminds of Sharn\"','Exercise the overlay reference contract','https://ddowiki.com/page/Masterminds_of_Sharn','2026-10-18')", []).unwrap();
    initial.execute("UPDATE vendors SET pack_id = (SELECT id FROM adventure_packs WHERE name = 'Reviewed Masterminds of Sharn') WHERE id = (SELECT MIN(id) FROM vendors)", []).unwrap();
    tree::rederive_data_build_tables(&initial).unwrap();
    let data_root = scratch.path().join("data");
    export_fixture_with_icons(&initial, &fixtures.join("DataFiles"), &data_root);
    let mut ours = Connection::open_in_memory().unwrap();
    tree::build_from_data(&data_root, &mut ours).unwrap();
    let wiki = WikiOverrides::from_database(&ours).unwrap();
    let corrections = Corrections::from_database(&ours).unwrap();
    let version = DatasetVersion { upstream_sha: "fixture".into(), built_at: "2026-10-18T00:00:00Z".into() };
    for (state, adopted) in [("previous", false), ("new-unchanged", false), ("new-adopted", true)] {
        let source = scratch.path().join(state);
        copy_tree(&fixtures.join("DataFiles"), &source);
        if adopted {
            let quests = source.join("Quests.xml");
            let contents = std::fs::read_to_string(&quests)
                .unwrap()
                .replace("Masterminds of Sharn", "Reviewed Masterminds of Sharn");
            std::fs::write(quests, contents).unwrap();
        }
        let mut built = Connection::open_in_memory().unwrap();
        let report = build::build_database(&source, &wiki, &corrections, &mut built, &version).unwrap();
        let resolved = report
            .stale_corrections
            .iter()
            .filter(|correction| correction.kind == "adventure_pack")
            .collect::<Vec<_>>();
        assert_eq!(resolved.len(), usize::from(adopted), "{state}: {resolved:?}");
        if adopted {
            assert!(matches!(resolved[0].cause, build::StaleCorrectionCause::RenameDoneUpstream { .. }));
        }
        let pack: String = built.query_row("SELECT p.name FROM sagas s JOIN adventure_packs p ON p.id=s.pack_id WHERE s.name='Masterminds of Sharn'", [], |row| row.get(0)).unwrap();
        assert_eq!(pack, "Reviewed Masterminds of Sharn", "{state}");
        if !adopted {
            let export = scratch.path().join(format!("{state}-data"));
            export_fixture_with_icons(&built, &source, &export);
            let mut normalized = Connection::open_in_memory().unwrap();
            tree::build_from_data(&export, &mut normalized).unwrap();
            assert_overlay_tables_match(&normalized, &ours);
        }
    }
    let mut unknown = wiki;
    let saga = unknown.sagas.iter_mut().find(|saga| saga.name == "Masterminds of Sharn").unwrap();
    saga.pack = Some("A name neither XML nor a correction carries".into());
    let owner_file = saga.file_name.clone();
    let error = build::build_database(
        &fixtures.join("DataFiles"),
        &unknown,
        &corrections,
        &mut Connection::open_in_memory().unwrap(),
        &version,
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains(&owner_file), "{error:#}");
}

#[test]
fn canonical_fixture_reproduces_all_tables_with_xml_citations_a_subset_of_ours() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut ours = Connection::open_in_memory().unwrap();
    tree::build_from_data(&fixtures.join("data"), &mut ours).unwrap();
    let corrections = Corrections::from_database(&ours).unwrap();
    assert!(corrections.entries.iter().any(|correction| correction.family.is_some() && correction.field == "name"));
    assert!(corrections.entries.iter().any(|correction| correction.selection.is_some() && correction.field == "name"));
    for wiki in [WikiOverrides::from_dir(&fixtures.join("wiki")).unwrap(), WikiOverrides::from_database(&ours).unwrap()]
    {
        let mut upstream = Connection::open_in_memory().unwrap();
        let report = build::build_database(
            &fixtures.join("DataFiles"),
            &wiki,
            &corrections,
            &mut upstream,
            &DatasetVersion { upstream_sha: "fixture".into(), built_at: "2026-10-06T00:00:00Z".into() },
        )
        .unwrap();
        assert!(report.stale_corrections.is_empty(), "{:?}", report.stale_corrections);
        let ordered_corrections = |mut corrections: Corrections| {
            corrections.entries.sort_by_key(|correction| {
                (
                    correction.kind.as_str().to_string(),
                    correction.name.clone(),
                    correction.field.clone(),
                    correction.qualifier(),
                )
            });
            corrections
        };
        assert_eq!(
            ordered_corrections(Corrections::from_database(&upstream).unwrap()),
            ordered_corrections(corrections.clone())
        );
        let scratch = tempfile::tempdir().unwrap();
        export_fixture_with_icons(&upstream, &fixtures.join("DataFiles"), scratch.path());
        let mut normalized = Connection::open_in_memory().unwrap();
        tree::build_from_data(scratch.path(), &mut normalized).unwrap();
        assert_overlay_tables_match(&normalized, &ours);
    }
}

#[test]
fn source_citations_do_not_change_drop_provenance() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data");
    let scratch = tempfile::tempdir().unwrap();
    copy_tree(&fixtures, scratch.path());
    let item_path = scratch.path().join("packs/shared/gear/battle-axe-of-the-oozing-hunger.toml");
    let mut item: toml::Table = toml::from_str(&std::fs::read_to_string(&item_path).unwrap()).unwrap();
    let provenance = |root: &Path| {
        let mut db = Connection::open_in_memory().unwrap();
        tree::build_from_data(root, &mut db).unwrap();
        db.query_row("SELECT d.drop_text_segment, d.source_match FROM quest_drops d JOIN items i ON d.owner_kind = 'item' AND d.owner_id = i.id JOIN quests q ON q.id = d.quest_id WHERE i.name = 'Battle Axe of the Oozing Hunger' AND q.name = 'The Grotto'", [], |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?))).unwrap()
    };
    let with_citation = provenance(scratch.path());
    item.get_mut("citation")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .retain(|citation| citation.get("field").and_then(toml::Value::as_str) != Some("sources"));
    std::fs::write(&item_path, toml::to_string(&item).unwrap()).unwrap();
    let without_citation = provenance(scratch.path());
    assert_eq!(without_citation, (Some("Test source: The Grotto, end chest".into()), Some("The Grotto".into())));
    assert_eq!(with_citation, without_citation, "citations only record checks");
}

#[test]
fn database_wiki_facts_ignore_item_citation_sources_and_use_constant_queries() {
    use rusqlite::trace::{TraceEvent, TraceEventCodes};
    use std::sync::atomic::{AtomicUsize, Ordering};
    static QUERY_COUNT: AtomicUsize = AtomicUsize::new(0);
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let scratch = tempfile::tempdir().unwrap();
    copy_tree(&fixtures.join("data"), scratch.path());
    let item_path = scratch.path().join("packs/shared/gear/battle-axe-of-the-oozing-hunger.toml");
    let original: toml::Table = toml::from_str(&std::fs::read_to_string(&item_path).unwrap()).unwrap();
    let mut previous_facts = None;
    let mut previous_queries = None;
    for annotation in
        [None, Some("url = \"https://ddowiki.com/page/Item:Battle_Axe_of_the_Oozing_Hunger\""), Some("in_game = true")]
    {
        let mut item = original.clone();
        item.remove("citation");
        if let Some(annotation) = annotation {
            let citation: toml::Table =
                toml::from_str(&format!("{annotation}\nread = \"2026-10-18\"\nnote = \"Checked in the source\""))
                    .unwrap();
            item.insert("citation".into(), toml::Value::Array(vec![toml::Value::Table(citation)]));
        }
        std::fs::write(&item_path, toml::to_string(&item).unwrap()).unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        tree::build_from_data(scratch.path(), &mut db).unwrap();
        db.trace_v2(
            TraceEventCodes::SQLITE_TRACE_STMT,
            Some(|event| {
                if let TraceEvent::Stmt(_, _) = event {
                    QUERY_COUNT.fetch_add(1, Ordering::SeqCst);
                }
            }),
        );
        QUERY_COUNT.store(0, Ordering::SeqCst);
        let wiki = WikiOverrides::from_database(&db).unwrap();
        let queries = QUERY_COUNT.load(Ordering::SeqCst);
        if let Some(previous) = &previous_facts {
            assert_eq!(&wiki.items, previous);
        }
        if let Some(previous) = previous_queries {
            assert_eq!(queries, previous);
        }
        previous_facts = Some(wiki.items);
        previous_queries = Some(queries);
    }
    let mut added_item = original;
    added_item.insert("name".into(), "Additional Wiki Axe".into());
    added_item.remove("citation");
    std::fs::write(
        scratch.path().join("packs/shared/gear/additional-wiki-axe.toml"),
        toml::to_string(&added_item).unwrap(),
    )
    .unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    tree::build_from_data(scratch.path(), &mut db).unwrap();
    db.trace_v2(
        TraceEventCodes::SQLITE_TRACE_STMT,
        Some(|event| {
            if let TraceEvent::Stmt(_, _) = event {
                QUERY_COUNT.fetch_add(1, Ordering::SeqCst);
            }
        }),
    );
    QUERY_COUNT.store(0, Ordering::SeqCst);
    let wiki = WikiOverrides::from_database(&db).unwrap();
    assert_eq!(wiki.items.len(), previous_facts.unwrap().len() + 1);
    assert_eq!(
        QUERY_COUNT.load(Ordering::SeqCst),
        previous_queries.unwrap(),
        "wiki reconstruction must not query per entity"
    );
}

#[test]
fn database_wiki_reference_failure_names_the_owning_data_file() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut ours = Connection::open_in_memory().unwrap();
    tree::build_from_data(&fixtures.join("data"), &mut ours).unwrap();
    let wiki = WikiOverrides::from_database(&ours).unwrap();
    let corrections = Corrections::from_database(&ours).unwrap();
    let scratch = tempfile::tempdir().unwrap();
    copy_tree(&fixtures.join("DataFiles"), scratch.path());
    std::fs::remove_file(scratch.path().join("Items/Buckler of the Golden Age.item")).unwrap();
    let error = build::build_database(
        scratch.path(),
        &wiki,
        &corrections,
        &mut Connection::open_in_memory().unwrap(),
        &DatasetVersion { upstream_sha: "fixture".into(), built_at: "2026-10-18T00:00:00Z".into() },
    )
    .unwrap_err();
    let message = format!("{error:#}");
    assert!(message.contains("Buckler of the Golden Age"), "{message}");
    assert!(
        message.contains("packs/magic-of-myth-drannor/quests/book-burning/gear/buckler-of-the-golden-age.toml"),
        "{message}"
    );
}

#[test]
fn xml_overlay_does_not_recreate_citations_owned_by_the_data_tree() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut db = Connection::open_in_memory().unwrap();
    build::build_database(
        &fixtures.join("DataFiles"),
        &WikiOverrides::from_dir(&fixtures.join("wiki")).unwrap(),
        &Corrections::default(),
        &mut db,
        &DatasetVersion { upstream_sha: "fixture".into(), built_at: "2026-10-18T00:00:00Z".into() },
    )
    .unwrap();
    let count: i64 = db.query_row("SELECT COUNT(*) FROM citations WHERE url = 'https://ddowiki.com/page/Item:Battle_Axe_of_the_Oozing_Hunger'", [], |row| row.get(0)).unwrap();
    assert_eq!(count, 0, "wiki facts must not recreate C's citations");
}

#[test]
fn every_entity_preserves_citations_with_notes_through_the_data_tree() {
    let scratch = tempfile::tempdir().unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let source = fixtures.join("DataFiles");
    let mut db = Connection::open_in_memory().unwrap();
    build::build_database(
        &source,
        &WikiOverrides::from_dir(&fixtures.join("wiki")).unwrap(),
        &Corrections::from_dir(&fixtures.join("data-corrections")).unwrap(),
        &mut db,
        &DatasetVersion { upstream_sha: PREVIOUS_SHA.into(), built_at: "2026-10-18T00:00:00Z".into() },
    )
    .unwrap();
    let mut expected = Vec::new();
    for (table, merge) in ddo_etl::delta::TABLE_MERGES {
        if !matches!(merge, ddo_etl::delta::TableMerge::Entity { .. }) {
            continue;
        }
        let columns = db
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        let identity_column = if columns.iter().any(|column| column == "slug") { "slug" } else { "name" };
        let limit = if matches!(*table, "stances" | "effects") { "" } else { " LIMIT 1" };
        let rows = db
            .prepare(&format!("SELECT id, {identity_column} FROM {table} ORDER BY {identity_column}{limit}"))
            .unwrap()
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert!(!rows.is_empty(), "fixture has no {table}");
        for (id, slug) in rows {
            for in_game in [false, true] {
                let note = format!("Checked {table}/{slug}; in_game={in_game}.");
                let url = (!in_game).then_some("https://ddowiki.com/page/Review");
                db.execute("INSERT INTO citations (owner_kind, owner_id, field, url, in_game, read, note) VALUES (?1, ?2, 'review', ?3, ?4, '2026-10-18', ?5)", rusqlite::params![table, id, url, in_game, note]).unwrap();
                expected.push((table, identity_column, slug.clone(), note, url, in_game));
            }
        }
    }
    let stance_requirements = |db: &Connection| {
        db.prepare("SELECT s.slug, r.group_kind, r.group_index, r.sort_order, r.req_type, r.items, r.value FROM requirements r JOIN stances s ON r.owner_kind = 'stance' AND r.owner_id = s.id ORDER BY s.slug, r.group_index, r.sort_order").unwrap()
            .query_map([], |row| (0..7).map(|column| row.get::<_, rusqlite::types::Value>(column)).collect::<rusqlite::Result<Vec<_>>>()).unwrap()
            .collect::<rusqlite::Result<Vec<_>>>().unwrap()
    };
    let expected_requirements = stance_requirements(&db);
    assert!(!expected_requirements.is_empty());
    export_fixture_with_icons(&db, &source, scratch.path());
    let before = tree::data_file_bytes(scratch.path()).unwrap();
    let mut rebuilt = Connection::open_in_memory().unwrap();
    tree::build_from_data(scratch.path(), &mut rebuilt).unwrap();
    assert_eq!(expected_requirements, stance_requirements(&rebuilt));
    let annotated = tree::annotated_entities(&rebuilt).unwrap();
    for (table, identity_column, slug, note, url, in_game) in expected {
        let id: i64 = rebuilt
            .query_row(&format!("SELECT id FROM {table} WHERE {identity_column} = ?1"), [&slug], |row| row.get(0))
            .unwrap();
        let count: i64 = rebuilt
            .query_row(
                "SELECT COUNT(*) FROM citations WHERE owner_kind = ?1 AND owner_id = ?2 AND note = ?3 AND url IS ?4 AND in_game = ?5",
                rusqlite::params![table, id, note, url, in_game],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "{table}/{slug}");
        assert!(annotated.contains(&(table.to_string(), id)), "{table}/{slug}");
    }
    tree::export_database(&rebuilt, scratch.path()).unwrap();
    assert_eq!(before, tree::data_file_bytes(scratch.path()).unwrap());
}

struct ImportWorld {
    scratch: tempfile::TempDir,
    options: DdobuilderImportOptions,
}

impl ImportWorld {
    fn new(corrections: Corrections) -> Self {
        Self::with_wiki(corrections, WikiOverrides::default())
    }

    fn with_wiki(corrections: Corrections, wiki: WikiOverrides) -> Self {
        let scratch = tempfile::tempdir().unwrap();
        let previous_dir = scratch.path().join("previous");
        let source_dir = scratch.path().join("new");
        let data_dir = scratch.path().join("data");
        copy_tree(&fixture_root().join("ba7e18e/DataFiles"), &previous_dir);
        copy_tree(&previous_dir, &source_dir);
        copy_tree(&fixture_root().join("d1350a0c/DataFiles"), &source_dir);
        let mut db = Connection::open_in_memory().unwrap();
        build::build_database(
            &previous_dir,
            &wiki,
            &corrections,
            &mut db,
            &DatasetVersion { upstream_sha: PREVIOUS_SHA.into(), built_at: "2026-10-17T00:00:00Z".into() },
        )
        .unwrap();
        tree::export_ddobuilder_tree(&db, &previous_dir, &data_dir).unwrap();
        Self {
            scratch,
            options: DdobuilderImportOptions {
                source_dir,
                previous_dir,
                data_dir,
                previous_sha: Some(PREVIOUS_SHA.into()),
                sha: Some(NEW_SHA.into()),
                wiki,
                corrections,
            },
        }
    }

    fn database(&self) -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        tree::build_from_data(&self.options.data_dir, &mut db).unwrap();
        db
    }

    fn edit(&self, sql: &str) {
        let db = self.database();
        db.execute_batch(sql).unwrap();
        tree::rederive_data_build_tables(&db).unwrap();
        tree::export_database(&db, &self.options.data_dir).unwrap();
        ddo_etl::icons::place_enhancement_icons_beside_trees(&self.options.data_dir).unwrap();
    }

    fn cost(&self, internal_name: &str) -> String {
        self.database()
            .query_row("SELECT cost_per_rank FROM enhancements WHERE internal_name = ?1", [internal_name], |row| {
                row.get(0)
            })
            .unwrap()
    }

    fn bytes(&self) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
        walkdir::WalkDir::new(&self.options.data_dir)
            .into_iter()
            .map(Result::unwrap)
            .filter(|entry| entry.file_type().is_file())
            .map(|entry| {
                (
                    entry.path().strip_prefix(&self.options.data_dir).unwrap().to_path_buf(),
                    std::fs::read(entry.path()).unwrap(),
                )
            })
            .collect()
    }
}

#[test]
fn import_reports_an_adopted_pack_rename_and_keeps_an_unchanged_one() {
    for adopted in [false, true] {
        let mut world = ImportWorld::new(Corrections::default());
        let db = world.database();
        db.execute("UPDATE adventure_packs SET name='Reviewed Terror of Demogorgon', slug='reviewed-terror-of-demogorgon' WHERE name='Terror of Demogorgon'", []).unwrap();
        db.execute("INSERT INTO corrections (kind,name,field,from_value,to_value,reason,source,read) VALUES ('adventure_pack','Terror of Demogorgon','name','\"Terror of Demogorgon\"','\"Reviewed Terror of Demogorgon\"','Exercise the rename lifecycle','https://ddowiki.com/page/Terror_of_Demogorgon','2026-10-18')", []).unwrap();
        db.execute("INSERT INTO vendors (name,slug,location,pack_id,provenance,wiki_url) SELECT 'Reviewed Vendor','reviewed-vendor','Fixture location',id,'wiki','https://ddowiki.com/page/Vendor' FROM adventure_packs WHERE name='Reviewed Terror of Demogorgon'", []).unwrap();
        tree::rederive_data_build_tables(&db).unwrap();
        world.options.data_dir = world.scratch.path().join("reviewed-data");
        tree::export_ddobuilder_tree(&db, &world.options.previous_dir, &world.options.data_dir).unwrap();
        let ours = world.database();
        world.options.wiki = WikiOverrides::from_database(&ours).unwrap();
        world.options.corrections = Corrections::from_database(&ours).unwrap();
        let source = world.scratch.path().join("rename-source");
        copy_tree(&world.options.previous_dir, &source);
        if adopted {
            let quests = source.join("Quests.xml");
            let contents = std::fs::read_to_string(&quests)
                .unwrap()
                .replace("Terror of Demogorgon", "Reviewed Terror of Demogorgon");
            std::fs::write(quests, contents).unwrap();
        }
        world.options.source_dir = source;
        let report = import_ddobuilder(&world.options).unwrap();
        assert!(!report.has_conflicts(), "{report}");
        let text = report.to_string();
        assert_eq!(text.contains("RESOLVED CORRECTION"), adopted, "{text}");
        assert_eq!(text.contains("renamed upstream"), adopted, "{text}");
        let result = world.database();
        let pack: String = result
            .query_row(
                "SELECT p.name FROM vendors v JOIN adventure_packs p ON p.id=v.pack_id WHERE v.name='Reviewed Vendor'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pack, "Reviewed Terror of Demogorgon");
    }
}

#[test]
fn import_of_the_recorded_commit_changes_nothing() {
    let mut world = ImportWorld::new(Corrections::default());
    world.options.source_dir = world.options.previous_dir.clone();
    world.options.sha = world.options.previous_sha.clone();
    let before = world.bytes();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert_eq!(before, world.bytes());
}

#[test]
fn import_supersedes_wiki_only_item_without_a_conflict() {
    let wiki = WikiOverrides::from_toml_files(&[(
        "items.toml",
        r#"
[[item]]
name = "Reviewed Trinket"
page = "https://ddowiki.com/page/Item:Reviewed_Trinket"
read = "2026-10-18"
slot = "Trinket"
category = "Jewelry"
minimum_level = 1
description = "Wiki description"
drop_location = "Unknown"
"#,
    )])
    .unwrap();
    let world = ImportWorld::with_wiki(Corrections::default(), wiki);
    let path = world.options.data_dir.join("packs/unsourced/gear/reviewed-trinket.toml");
    let mut document = std::fs::read_to_string(&path).unwrap();
    document
        .push_str("\n[[citation]]\nurl = \"https://ddowiki.com/page/Item:Reviewed_Trinket\"\nread = \"2026-10-18\"\n");
    std::fs::write(&path, document).unwrap();
    let provenance = |db: &Connection| -> String {
        db.query_row("SELECT provenance FROM items WHERE name = 'Reviewed Trinket'", [], |row| row.get(0)).unwrap()
    };
    assert_eq!(provenance(&world.database()), "wiki");
    let items = world.options.source_dir.join("Items");
    std::fs::create_dir_all(&items).unwrap();
    std::fs::write(items.join("Reviewed Trinket.item"), "<Items><Item><Name>Reviewed Trinket</Name><MinLevel>1</MinLevel><EquipmentSlot><Trinket/></EquipmentSlot><Description>His description</Description></Item></Items>").unwrap();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    let db = world.database();
    assert_eq!(provenance(&db), "maetrim");
    let citations: i64 = db.query_row("SELECT COUNT(*) FROM citations WHERE owner_kind = 'items' AND owner_id = (SELECT id FROM items WHERE name = 'Reviewed Trinket') AND field = ''", [], |row| row.get(0)).unwrap();
    assert_eq!(citations, 1);
    let path = world.options.data_dir.join("packs/unsourced/gear/reviewed-trinket.toml");
    assert!(!std::fs::read_to_string(path).unwrap().contains("is_wiki_only"));
}

#[test]
fn import_takes_his_value_on_a_field_we_did_not_change() {
    let world = ImportWorld::new(Corrections::default());
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert_eq!(world.cost("ATShockGauntlet"), "[1]");
}

#[test]
fn import_keeps_our_value_and_lists_the_conflict() {
    let world = ImportWorld::new(Corrections::default());
    world.edit("UPDATE enhancements SET cost_per_rank = '[3]' WHERE internal_name = 'SDDefenseBoost'");
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(report.has_conflicts());
    assert_eq!(world.cost("SDDefenseBoost"), "[3]");
    let text = report.to_string();
    for expected in
        ["CONFLICT", "characters/enhancements/stalwart-defender/tree.toml", "cost_per_rank", "[2]", "[1]", "[3]"]
    {
        assert!(text.contains(expected), "{text}");
    }
}

#[test]
fn import_takes_no_action_when_we_already_made_his_change() {
    let world = ImportWorld::new(Corrections::default());
    world.edit("UPDATE enhancements SET cost_per_rank = '[1]' WHERE internal_name = 'SDDefenseBoost'");
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert_eq!(world.cost("SDDefenseBoost"), "[1]");
}

#[test]
fn import_adds_his_new_file_where_the_layout_puts_it() {
    let world = ImportWorld::new(Corrections::default());
    let report = import_ddobuilder(&world.options).unwrap();
    assert_eq!(report.tables["filigrees"].added, 8);
    assert_eq!(report.tables["filigrees"].changed, 0);
    assert_eq!(report.tables["set_bonuses"].added, 1);
    assert_eq!(report.tables["set_bonuses"].changed, 0);
    assert!(report.to_string().contains("characters/enhancements/bombardier/tree.toml: REMOVED"));
    assert!(world.options.data_dir.join("packs/shared/filigrees/night-revels-gifts.toml").is_file());
    assert_eq!(
        std::fs::read(world.options.data_dir.join("icons/filigrees/NightRevelsGifts.png")).unwrap(),
        std::fs::read(world.options.source_dir.join("FiligreeImages/NightRevelsGifts.png")).unwrap()
    );
}

#[test]
fn import_refuses_a_previous_that_is_not_the_recorded_commit() {
    let mut world = ImportWorld::new(Corrections::default());
    world.options.previous_sha = Some("wrong".into());
    let before = world.bytes();
    let error = import_ddobuilder(&world.options).unwrap_err().to_string();
    assert!(error.contains("wrong") && error.contains(PREVIOUS_SHA), "{error}");
    assert_eq!(before, world.bytes());
}

#[test]
fn import_twice_changes_nothing_the_second_time() {
    let mut world = ImportWorld::new(Corrections::default());
    import_ddobuilder(&world.options).unwrap();
    world.options.previous_dir = world.options.source_dir.clone();
    world.options.previous_sha = world.options.sha.clone();
    let before = world.bytes();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert_eq!(before, world.bytes());
}

#[test]
fn augment_family_rename_preserves_the_upstream_correction_name() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let source = fixtures.join("DataFiles");
    let wiki = WikiOverrides::from_dir(&fixtures.join("wiki")).unwrap();
    let mut corrections = Corrections::from_dir(&fixtures.join("data-corrections")).unwrap();
    let version = DatasetVersion { upstream_sha: PREVIOUS_SHA.into(), built_at: "2026-10-18T00:00:00Z".into() };
    let mut baseline = Connection::open_in_memory().unwrap();
    build::build_database(&source, &wiki, &corrections, &mut baseline, &version).unwrap();
    let (name, family): (String, String) = baseline
        .query_row("SELECT name, family FROM augments ORDER BY id LIMIT 1", [], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap();
    let renamed = format!("{name} reviewed");
    let text = format!(
        r#"[[correction]]
kind = "augment"
name = {name:?}
family = {family:?}
field = "name"
from = {name:?}
to = {renamed:?}
reason = "Reviewed family rename."
source = "https://ddowiki.com/page/Test"
read = "2026-10-18"
"#
    );
    corrections.entries.extend(Corrections::from_toml_files(&[("rename.toml", &text)]).unwrap().entries);
    let mut corrected = Connection::open_in_memory().unwrap();
    build::build_database(&source, &wiki, &corrections, &mut corrected, &version).unwrap();
    let scratch = tempfile::tempdir().unwrap();
    export_fixture_with_icons(&corrected, &source, scratch.path());
    let mut rebuilt = Connection::open_in_memory().unwrap();
    tree::build_from_data(scratch.path(), &mut rebuilt).unwrap();
    let recorded_name: String = rebuilt
        .query_row("SELECT name FROM corrections WHERE reason = 'Reviewed family rename.'", [], |row| row.get(0))
        .unwrap();
    assert_eq!(recorded_name, name);
}

#[test]
fn selection_and_parent_renames_keep_the_upstream_parent_in_both_builders() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let source = fixtures.join("DataFiles");
    let wiki = WikiOverrides::from_dir(&fixtures.join("wiki")).unwrap();
    let version = DatasetVersion { upstream_sha: PREVIOUS_SHA.into(), built_at: "2026-10-18T00:00:00Z".into() };
    for parent_first in [true, false] {
        let mut corrections = Corrections::from_dir(&fixtures.join("data-corrections")).unwrap();
        let mut renames = [
            ("", "Kensei: Action Boost", "Kensei: Action Boost Reviewed"),
            ("selection = \"Attack Boost\"\n", "Attack Boost", "Attack Boost Reviewed"),
        ];
        if !parent_first {
            renames.reverse();
        }
        for (qualifier, before, after) in renames {
            let text = format!(
                r#"[[correction]]
kind = "enhancement"
name = "Kensei: Action Boost"
field = "name"
from = {before:?}
to = {after:?}
{qualifier}reason = "Paired rename."
source = "https://ddowiki.com/page/Test"
read = "2026-10-18"
"#
            );
            corrections.entries.extend(Corrections::from_toml_files(&[("paired.toml", &text)]).unwrap().entries);
        }
        let mut corrected = Connection::open_in_memory().unwrap();
        let report = build::build_database(&source, &wiki, &corrections, &mut corrected, &version).unwrap();
        assert!(report.stale_corrections.is_empty());
        let scratch = tempfile::tempdir().unwrap();
        export_fixture_with_icons(&corrected, &source, scratch.path());
        let mut rebuilt = Connection::open_in_memory().unwrap();
        tree::build_from_data(scratch.path(), &mut rebuilt).unwrap();
        for db in [&corrected, &rebuilt] {
            let mut query = db
                .prepare("SELECT name, qualifier FROM corrections WHERE reason = 'Paired rename.' ORDER BY qualifier")
                .unwrap();
            let keys = query
                .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            assert_eq!(
                keys,
                vec![
                    ("Kensei: Action Boost".into(), "".into()),
                    ("Kensei: Action Boost".into(), "selection \"Attack Boost\"".into())
                ]
            );
            let names: (String, String) = db.query_row("SELECT e.name, s.name FROM enhancements e JOIN enhancement_selections s ON s.enhancement_id = e.id WHERE s.name = 'Attack Boost Reviewed'", [], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
            assert_eq!(names, ("Kensei: Action Boost Reviewed".into(), "Attack Boost Reviewed".into()));
        }
    }
}

#[test]
fn every_declared_enhancement_field_agrees_between_builders() {
    use ddo_etl::corrections::{correctable_fields, CorrectionValue};
    use ddo_model::enums::CorrectionKind;
    let expected = [
        "arrows",
        "cooldown_seconds",
        "cost_per_rank",
        "description",
        "duration_seconds",
        "icon",
        "is_clickie",
        "is_tier5",
        "min_spent",
        "name",
        "ranks",
        "x",
        "y",
    ];
    let mut declared =
        correctable_fields(CorrectionKind::Enhancement).iter().map(|field| field.name).collect::<Vec<_>>();
    declared.sort();
    assert_eq!(declared, expected);
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let source = fixtures.join("DataFiles");
    let wiki = WikiOverrides::from_dir(&fixtures.join("wiki")).unwrap();
    let baseline_corrections = Corrections::from_dir(&fixtures.join("data-corrections")).unwrap();
    let version = DatasetVersion { upstream_sha: PREVIOUS_SHA.into(), built_at: "2026-10-18T00:00:00Z".into() };
    let mut baseline = Connection::open_in_memory().unwrap();
    build::build_database(&source, &wiki, &baseline_corrections, &mut baseline, &version).unwrap();
    for selection in [false, true] {
        for field in expected {
            for empty_list in [false, true] {
                if empty_list && !["cost_per_rank", "arrows"].contains(&field) {
                    continue;
                }
                if selection && ["arrows", "is_tier5", "x", "y"].contains(&field) {
                    continue;
                }
                let (table, row_name) = if selection {
                    ("enhancement_selections", "Attack Boost")
                } else {
                    ("enhancements", "Kensei: Action Boost")
                };
                let original: rusqlite::types::Value = baseline
                    .query_row(&format!("SELECT {field} FROM {table} WHERE name = ?1"), [row_name], |row| row.get(0))
                    .unwrap();
                let original = match original {
                    rusqlite::types::Value::Text(encoded) if ["cost_per_rank", "arrows"].contains(&field) => {
                        CorrectionValue::from_json(&encoded).unwrap()
                    }
                    value => CorrectionValue::from_sql(value),
                };
                let mut replacement = match field {
                    "name" => serde_json::json!(format!("{row_name} reviewed")),
                    "description" => serde_json::json!("Reviewed enhancement description."),
                    "icon" => serde_json::json!("PowerAttack"),
                    "arrows" => serde_json::json!(["ArrowUp", "ArrowRight"]),
                    "cost_per_rank" => serde_json::json!([2, 3]),
                    "is_clickie" => serde_json::json!(0),
                    "is_tier5" => serde_json::json!(1),
                    _ => serde_json::json!(7),
                };
                if empty_list {
                    replacement = serde_json::json!([]);
                }
                let from = if original == CorrectionValue::Null { "\"null\"".into() } else { original.to_json() };
                let qualifier = if selection { "selection = \"Attack Boost\"\n" } else { "" };
                let text = format!(
                    r#"[[correction]]
kind = "enhancement"
name = "Kensei: Action Boost"
field = {field:?}
from = {from}
to = {replacement}
{qualifier}reason = "Reviewed in test."
source = "https://ddowiki.com/page/Test"
read = "2026-10-18"
"#
                );
                let mut corrections = baseline_corrections.clone();
                corrections
                    .entries
                    .extend(Corrections::from_toml_files(&[("enhancement.toml", text.as_str())]).unwrap().entries);
                if selection && field == "name" {
                    corrections.entries.extend(
                        Corrections::from_toml_files(&[(
                            "sibling.toml",
                            r#"[[correction]]
kind = "enhancement"
name = "Kensei: Action Boost"
selection = "Attack Boost"
field = "cost_per_rank"
from = [1]
to = [4]
reason = "Sibling of renamed selection."
source = "https://ddowiki.com/page/Test"
read = "2026-10-18"
"#,
                        )])
                        .unwrap()
                        .entries,
                    );
                }
                let mut corrected = Connection::open_in_memory().unwrap();
                build::build_database(&source, &wiki, &corrections, &mut corrected, &version)
                    .unwrap_or_else(|error| panic!("{table}.{field}: {error:#}"));
                let scratch = tempfile::tempdir().unwrap();
                export_fixture_with_icons(&corrected, &source, scratch.path());
                let mut rebuilt = Connection::open_in_memory().unwrap();
                tree::build_from_data(scratch.path(), &mut rebuilt)
                    .unwrap_or_else(|error| panic!("{table}.{field}: {error:#}"));
                for db in [&corrected, &rebuilt] {
                    let actual: String = db.query_row(
                    "SELECT to_value FROM corrections WHERE kind = 'enhancement' AND field = ?1 AND reason = 'Reviewed in test.'",
                    [field], |row| row.get(0),
                ).unwrap_or_else(|error| panic!("{table}.{field}: {error}"));
                    assert_eq!(actual, replacement.to_string(), "{table}.{field}");
                    if selection && field == "name" {
                        let sibling: (String, String) = db.query_row(
                        "SELECT qualifier, to_value FROM corrections WHERE reason = 'Sibling of renamed selection.'",
                        [], |row| Ok((row.get(0)?, row.get(1)?)),
                    ).unwrap();
                        assert_eq!(sibling, ("selection \"Attack Boost\"".into(), "[4]".into()));
                    }
                }
            }
        }
    }
}

#[test]
fn enhancement_notes_reject_identity_fields_and_fractional_upstream_costs() {
    for (field, upstream, expected) in [
        ("internal_name", toml::Value::String("OldIdentity".into()), "allowed fields"),
        ("cost_per_rank", toml::Value::Array(vec![toml::Value::Float(1.5)]), "integer"),
    ] {
        let world = ImportWorld::new(Corrections::default());
        let path = world.options.data_dir.join("characters/enhancements/ravager-fighter/tree.toml");
        let mut tree: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let note = toml::Table::from_iter([
            ("kind".into(), toml::Value::String("enhancement".into())),
            ("field".into(), toml::Value::String(field.into())),
            ("upstream".into(), upstream),
            ("reason".into(), toml::Value::String("Invalid correction test.".into())),
            ("url".into(), toml::Value::String("https://ddowiki.com/page/Test".into())),
            ("read".into(), toml::Value::String("2026-10-18".into())),
        ]);
        tree["enhancement"][0]
            .as_table_mut()
            .unwrap()
            .insert("ddobuilder_correction".into(), toml::Value::Array(vec![toml::Value::Table(note)]));
        std::fs::write(&path, toml::to_string_pretty(&tree).unwrap()).unwrap();
        let error =
            tree::build_from_data(&world.options.data_dir, &mut Connection::open_in_memory().unwrap()).unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("ravager-fighter/tree.toml") && message.contains(expected), "{message}");
    }
}

#[test]
fn annotation_sources_and_correction_kinds_are_typed() {
    let source_shape = serde_json::json!([
        {"required": ["url"], "not": {"required": ["in_game"]}},
        {"required": ["in_game"], "not": {"required": ["url"]}},
    ]);
    for schema in [
        serde_json::to_value(schemars::schema_for!(tree::Citation)).unwrap(),
        serde_json::to_value(schemars::schema_for!(tree::DdobuilderCorrection)).unwrap(),
    ] {
        assert_eq!(schema["oneOf"], source_shape);
        assert_eq!(schema["properties"]["in_game"]["const"], true);
    }
    let schema = serde_json::to_value(schemars::schema_for!(tree::DdobuilderCorrection)).unwrap();
    let expected_kinds = ddo_model::enums::CorrectionKind::ALL.iter().map(|kind| kind.as_str()).collect::<Vec<_>>();
    assert_eq!(schema["$defs"]["CorrectionKind"]["enum"], serde_json::json!(expected_kinds));
    for source in ["url = \"https://ddowiki.com/page/Test\"", "in_game = true"] {
        let text = format!("{source}\nread = \"2026-10-18\"\nnote = \"Observed cost.\"\n");
        let citation: tree::Citation = toml::from_str(&text).unwrap();
        let encoded = toml::to_string(&citation).unwrap();
        let decoded: tree::Citation = toml::from_str(&encoded).unwrap();
        assert_eq!(toml::to_string(&decoded).unwrap(), encoded);
        let text = format!("kind = \"enhancement\"\nfield = \"cost_per_rank\"\nreason = \"Observed cost.\"\n{source}\nread = \"2026-10-18\"\n");
        let correction: tree::DdobuilderCorrection = toml::from_str(&text).unwrap();
        let encoded = toml::to_string(&correction).unwrap();
        assert_eq!(toml::to_string(&toml::from_str::<tree::DdobuilderCorrection>(&encoded).unwrap()).unwrap(), encoded);
        for kind in ["release", "slug_collision"] {
            assert!(toml::from_str::<tree::DdobuilderCorrection>(
                &text.replace("\"enhancement\"", &format!("{kind:?}"))
            )
            .is_err());
        }
    }
    for source in ["", "in_game = false", "url = \"https://ddowiki.com/page/Test\"\nin_game = true"] {
        assert!(toml::from_str::<tree::Citation>(&format!("{source}\nread = \"2026-10-18\"\n")).is_err());
        for (array, fields) in [
            ("citation", ""),
            (
                "enhancement.ddobuilder_correction",
                "kind = \"enhancement\"\nfield = \"cost_per_rank\"\nreason = \"Observed cost.\"\nupstream = [2]\n",
            ),
        ] {
            let world = ImportWorld::new(Corrections::default());
            let path = world.options.data_dir.join("characters/enhancements/ravager-fighter/tree.toml");
            let original = std::fs::read_to_string(&path).unwrap();
            std::fs::write(&path, format!("{original}\n[[{array}]]\n{fields}{source}\nread = \"2026-10-18\"\n"))
                .unwrap();
            let error =
                tree::build_from_data(&world.options.data_dir, &mut Connection::open_in_memory().unwrap()).unwrap_err();
            assert!(format!("{error:#}").contains("ravager-fighter/tree.toml"), "{error:#}");
        }
        assert!(toml::from_str::<tree::DdobuilderCorrection>(&format!("kind = \"enhancement\"\nfield = \"cost_per_rank\"\nreason = \"Observed cost.\"\n{source}\nread = \"2026-10-18\"\n")).is_err());
    }
}

#[test]
fn import_reports_enhancement_cost_resolution_conflict_and_unchanged_values() {
    let correction = r#"[[correction]]
kind = "enhancement"
name = "Ravager: Draconic Power Attack"
field = "cost_per_rank"
from = [1]
to = [2]
reason = "Reviewed enhancement cost."
source = "https://ddowiki.com/page/Test"
read = "2026-10-18"
"#;
    for source in ["source = \"https://ddowiki.com/page/Test\"", "in_game = true"] {
        let correction = correction.replace("source = \"https://ddowiki.com/page/Test\"", source);
        for (new_cost, expected) in [(1, None), (2, Some("RESOLVED CORRECTION")), (3, Some("CONFLICT"))] {
            let world = ImportWorld::new(Corrections::from_toml_files(&[("cost.toml", correction.as_str())]).unwrap());
            let path = world.options.source_dir.join("EnhancementTrees/Fighter_Ravager.tree.xml");
            let xml = std::fs::read_to_string(&path).unwrap();
            assert!(xml.contains("<CostPerRank size=\"1\">1</CostPerRank>"));
            std::fs::write(
                path,
                xml.replace(
                    "<CostPerRank size=\"1\">1</CostPerRank>",
                    &format!("<CostPerRank size=\"1\">{new_cost}</CostPerRank>"),
                ),
            )
            .unwrap();
            let report = import_ddobuilder(&world.options).unwrap().to_string();
            let lines = report.lines().filter(|line| line.contains("cost_per_rank")).collect::<Vec<_>>();
            if let Some(expected) = expected {
                assert!(lines.iter().any(|line| line.contains(expected)), "{new_cost}: {report}");
            } else {
                assert!(lines.is_empty(), "{report}");
            }
            let cost: String = world
                .database()
                .query_row(
                    "SELECT cost_per_rank FROM enhancements WHERE name = 'Ravager: Draconic Power Attack'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(cost, "[2]");
        }
    }
}

fn ravager_corrections() -> Corrections {
    Corrections::from_dir(&fixture_root().join("corrections")).unwrap()
}

#[test]
fn import_reports_a_note_his_value_now_matches() {
    let mut corrections = ravager_corrections();
    corrections.entries.retain(|entry| entry.name.contains("Draconic"));
    let world = ImportWorld::new(corrections.clone());
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    let text = report.to_string();
    assert!(text.contains("RESOLVED CORRECTION") && text.contains("ravager-fighter/tree.toml"), "{text}");
    let mut expected = corrections;
    expected.entries[0].file_name = "data".into();
    assert_eq!(Corrections::from_database(&world.database()).unwrap(), expected);
}

#[test]
fn import_keeps_a_corrected_value_he_changed_again() {
    let mut corrections = ravager_corrections();
    corrections.entries.retain(|entry| entry.name.contains("Barbarian"));
    let world = ImportWorld::new(corrections.clone());
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(report.has_conflicts(), "{report}");
    let result = Corrections::from_database(&world.database()).unwrap();
    assert_eq!(result.entries[0].to, corrections.entries[0].to);
    let parsed =
        ddo_etl::xml::trees::parse(&world.options.source_dir.join("EnhancementTrees/Barbarian_Ravager.tree.xml"))
            .unwrap();
    assert_eq!(
        result.entries[0].from,
        ddo_etl::corrections::CorrectionValue::Text(parsed.enhancements[0].description.clone().unwrap())
    );
    assert!(report.to_string().contains("ravager-barbarian/tree.toml"));
}

#[test]
fn import_places_his_new_tree_icons_beside_the_tree() {
    let world = ImportWorld::new(Corrections::default());
    import_ddobuilder(&world.options).unwrap();
    let overlay = fixture_root().join("d1350a0c/DataFiles/EnhancementImages");
    let mut checked = 0;
    for entry in std::fs::read_dir(overlay).unwrap() {
        let entry = entry.unwrap();
        let destination =
            world.options.data_dir.join("characters/enhancements/bombardier/icons").join(entry.file_name());
        assert_eq!(std::fs::read(destination).unwrap(), std::fs::read(entry.path()).unwrap());
        checked += 1;
    }
    assert_eq!(checked, 10);
}

#[test]
fn import_follows_his_icon_reference_swap() {
    let world = ImportWorld::new(Corrections::default());
    import_ddobuilder(&world.options).unwrap();
    for (internal, expected) in [("BomBurningAmbition", "BomAugmentation"), ("BomAugmentation", "BomBurningAmbition")] {
        let icon: String = world
            .database()
            .query_row("SELECT icon FROM enhancements WHERE internal_name = ?1", [internal], |row| row.get(0))
            .unwrap();
        assert_eq!(icon, expected);
        assert!(world
            .options
            .data_dir
            .join(format!("characters/enhancements/bombardier/icons/{expected}.png"))
            .exists());
    }
}

#[test]
fn import_removes_an_entity_we_left_alone() {
    let world = ImportWorld::new(Corrections::default());
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert!(report.to_string().contains("characters/enhancements/bombardier/tree.toml: REMOVED enhancement_selections/bomspellvialselection-sla-vial-of-acid"), "{report}");
    let count: i64 = world
        .database()
        .query_row("SELECT count(*) FROM enhancements WHERE internal_name = 'BomSpellvialSelectionII'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn import_declines_removing_an_annotated_entity() {
    let world = ImportWorld::new(Corrections::default());
    world.edit("INSERT INTO citations (owner_kind, owner_id, field, url, read) SELECT 'enhancements', id, '', 'https://ddowiki.com/page/Bombardier', '2026-10-17' FROM enhancements WHERE internal_name = 'BomSpellvialSelectionII'");
    let report = import_ddobuilder(&world.options).unwrap();
    let count: i64 = world.database().query_row("SELECT count(*) FROM enhancement_selections WHERE enhancement_id IN (SELECT id FROM enhancements WHERE internal_name = 'BomSpellvialSelectionII')", [], |row| row.get(0)).unwrap();
    assert_eq!(count, 5);
    assert!(report.to_string().contains("DECLINED REMOVAL"));
}

#[test]
fn import_treats_a_renamed_selection_as_a_removal_and_an_addition() {
    let world = ImportWorld::new(Corrections::default());
    world.edit("INSERT INTO corrections (kind, name, qualifier, field, from_value, to_value, reason, source, read) SELECT 'enhancement', e.name, 'selection \"SLA: Vial of Acid\"', 'icon', '\"OldIcon\"', json_quote(s.icon), 'Reviewed icon', 'https://ddowiki.com/page/Bombardier', '2026-10-17' FROM enhancement_selections s JOIN enhancements e ON e.id = s.enhancement_id WHERE s.slug = 'bomspellvialselection-sla-vial-of-acid'");
    let report = import_ddobuilder(&world.options).unwrap();
    let db = world.database();
    let mut query = db.prepare("SELECT slug, sort_order FROM enhancement_selections WHERE enhancement_id IN (SELECT id FROM enhancements WHERE internal_name = 'BomSpellVialSelection') ORDER BY sort_order").unwrap();
    let selections = query
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    let old_position =
        selections.iter().position(|(slug, _)| slug == "bomspellvialselection-sla-vial-of-acid").unwrap();
    assert!(old_position > 0);
    assert!(selections.iter().any(|(slug, _)| slug == "bomspellvialselection-sla-vial-smash-acid"));
    assert_eq!(
        selections.iter().map(|(_, position)| *position).collect::<Vec<_>>(),
        (0..selections.len() as i64).collect::<Vec<_>>()
    );
    assert!(report.to_string().contains("position was renumbered"), "{report}");
}

#[test]
fn import_takes_his_new_icon_bytes_on_a_file_we_left_alone() {
    let world = ImportWorld::new(Corrections::default());
    import_ddobuilder(&world.options).unwrap();
    assert_eq!(
        std::fs::read(world.options.data_dir.join("icons/feats/Alertness.png")).unwrap(),
        std::fs::read(world.options.source_dir.join("FeatImages/Alertness.png")).unwrap()
    );
}

#[test]
fn import_keeps_our_icon_bytes_and_lists_the_conflict() {
    let world = ImportWorld::new(Corrections::default());
    let ours = std::fs::read(world.options.previous_dir.join("FeatImages/Knock.png")).unwrap();
    std::fs::write(world.options.data_dir.join("icons/feats/Alertness.png"), &ours).unwrap();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(report.has_conflicts(), "{report}");
    assert!(report.to_string().contains("icons/feats/Alertness.png: ICON CONFLICT"));
    assert_eq!(std::fs::read(world.options.data_dir.join("icons/feats/Alertness.png")).unwrap(), ours);
}

#[test]
fn import_fails_without_writing_when_his_build_fails() {
    let mut world = ImportWorld::new(Corrections::default());
    let mut correction = ravager_corrections().entries.remove(0);
    correction.name = "Bombardier: Spellvial Selection II".into();
    correction.from = ddo_etl::corrections::CorrectionValue::Text("Previous prose".into());
    correction.to = ddo_etl::corrections::CorrectionValue::Text("Our prose".into());
    world.options.corrections.entries.push(correction);
    let before = world.bytes();
    let error = format!("{:#}", import_ddobuilder(&world.options).unwrap_err());
    assert!(error.contains("building the new DataFiles") && error.contains("Spellvial Selection II"), "{error}");
    assert_eq!(before, world.bytes());
}

#[test]
fn import_fails_without_writing_when_his_image_is_missing() {
    let world = ImportWorld::new(Corrections::default());
    std::fs::remove_file(world.options.source_dir.join("FeatImages/Alertness.png")).unwrap();
    let before = world.bytes();
    let error = format!("{:#}", import_ddobuilder(&world.options).unwrap_err());
    assert!(error.contains("missing image files"), "{error}");
    assert_eq!(before, world.bytes());
}

#[test]
fn import_removes_a_file_he_removed() {
    let mut world = ImportWorld::new(Corrections::default());
    import_ddobuilder(&world.options).unwrap();
    std::mem::swap(&mut world.options.previous_dir, &mut world.options.source_dir);
    std::mem::swap(&mut world.options.previous_sha, &mut world.options.sha);
    import_ddobuilder(&world.options).unwrap();
    assert!(!world.options.data_dir.join("packs/shared/filigrees/night-revels-gifts.toml").exists());
    assert!(!world.options.data_dir.join("icons/filigrees/NightRevelsGifts.png").exists());
}

#[test]
fn import_lists_his_change_to_an_entity_we_removed() {
    let world = ImportWorld::new(Corrections::default());
    world.edit("PRAGMA foreign_keys = OFF; DELETE FROM enhancements WHERE internal_name = 'ATShockGauntlet'; DELETE FROM requirements WHERE owner_kind = 'enhancement' AND owner_id NOT IN (SELECT id FROM enhancements); DELETE FROM modifiers WHERE source_kind = 'enhancement' AND source_id NOT IN (SELECT id FROM enhancements); DELETE FROM entity_identity");
    std::fs::remove_file(
        world.options.data_dir.join("characters/enhancements/arcanotechnician/icons/ATShockGauntlet.png"),
    )
    .unwrap();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(report.to_string().contains("CHANGED UPSTREAM, ABSENT IN OURS"), "{report}");
    let count: i64 = world
        .database()
        .query_row("SELECT count(*) FROM enhancements WHERE internal_name = 'ATShockGauntlet'", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

fn cli(world: &ImportWorld) -> std::process::Command {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_ddo-etl"));
    command
        .arg("import-ddobuilder")
        .arg("--previous")
        .arg(&world.options.previous_dir)
        .arg("--source")
        .arg(&world.options.source_dir)
        .arg("--data")
        .arg(&world.options.data_dir)
        .arg("--previous-sha")
        .arg(world.options.previous_sha.as_ref().unwrap())
        .arg("--sha")
        .arg(world.options.sha.as_ref().unwrap())
        .arg("--wiki")
        .arg(fixture_root().join("wiki"));
    command
}

#[test]
fn import_cli_exits_two_on_a_conflict_and_records_the_new_commit() {
    let world = ImportWorld::new(Corrections::default());
    world.edit("UPDATE enhancements SET cost_per_rank = '[3]' WHERE internal_name = 'SDDefenseBoost'");
    let output = cli(&world).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("CONFLICT"));
    let sha: String =
        world.database().query_row("SELECT upstream_sha FROM dataset_version", [], |row| row.get(0)).unwrap();
    assert_eq!(sha, NEW_SHA);
}

#[test]
fn import_cli_uses_the_data_files_corrections_by_default() {
    let mut corrections = ravager_corrections();
    corrections.entries.retain(|entry| entry.name.contains("Barbarian"));
    let world = ImportWorld::new(corrections);
    let output = cli(&world).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Our researched Barbarian"));
}

#[test]
fn import_cli_accepts_a_corrections_override() {
    let world = ImportWorld::new(ravager_corrections());
    let override_dir = world.scratch.path().join("corrections");
    std::fs::create_dir(&override_dir).unwrap();
    let original = std::fs::read_to_string(fixture_root().join("corrections/ravager.toml")).unwrap();
    let draconic = original.split("[[correction]]").nth(1).unwrap();
    std::fs::write(override_dir.join("ravager.toml"), format!("[[correction]]{draconic}")).unwrap();
    let output = cli(&world).arg("--corrections").arg(override_dir).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", String::from_utf8_lossy(&output.stderr));
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(report.contains("RESOLVED CORRECTION") && report.contains("Our researched Barbarian"), "{report}");
    assert!(!report.contains("CONFLICT enhancement \"Ravager: Barbarian Power Attack\""), "{report}");
    assert!(report.contains("enhancements/ravbarbarianpowerattack description"), "{report}");
}

#[test]
fn import_refuses_missing_and_empty_data_directories() {
    let mut world = ImportWorld::new(Corrections::default());
    for path in [world.scratch.path().join("absent"), world.scratch.path().join("empty")] {
        if path.ends_with("empty") {
            std::fs::create_dir(&path).unwrap();
        }
        world.options.data_dir = path;
        assert!(import_ddobuilder(&world.options).is_err());
    }
}

#[test]
fn import_lists_one_conflict_for_an_array_both_sides_changed() {
    let world = ImportWorld::new(Corrections::default());
    world.edit("UPDATE modifiers SET amounts = '[7]' WHERE source_kind = 'enhancement' AND source_id IN (SELECT id FROM enhancements WHERE internal_name = 'RavBarbarianPowerAttack')");
    let report = import_ddobuilder(&world.options).unwrap();
    let conflicts: Vec<_> = report.entries.iter().filter(|entry| entry.category == "CONFLICT").collect();
    assert_eq!(conflicts.len(), 1, "{report}");
    assert!(conflicts[0].detail.contains("modifier"));
    let db = world.database();
    let amounts: Vec<String> = db.prepare("SELECT amounts FROM modifiers WHERE source_kind = 'enhancement' AND source_id IN (SELECT id FROM enhancements WHERE internal_name = 'RavBarbarianPowerAttack') ORDER BY sort_order").unwrap().query_map([], |row| row.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    assert_eq!(amounts, vec!["[7]", "[7]"]);
}

#[test]
fn import_replaces_an_array_he_changed_and_we_did_not() {
    let world = ImportWorld::new(Corrections::default());
    let parsed =
        ddo_etl::xml::trees::parse(&world.options.source_dir.join("EnhancementTrees/Fighter_Ravager.tree.xml"))
            .unwrap();
    let expected: Vec<_> = parsed.enhancements[0].effects.iter().map(|effect| effect.amounts.clone()).collect();
    import_ddobuilder(&world.options).unwrap();
    let db = world.database();
    let amounts: Vec<String> = db.prepare("SELECT amounts FROM modifiers WHERE source_kind = 'enhancement' AND source_id IN (SELECT id FROM enhancements WHERE internal_name = 'DLRavDraconicPowerAttack') ORDER BY sort_order").unwrap().query_map([], |row| row.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    let actual: Vec<Vec<f64>> = amounts.iter().map(|encoded| serde_json::from_str(encoded).unwrap()).collect();
    assert_eq!(actual, expected);
}

#[test]
fn import_lists_an_added_pack_as_needing_research() {
    let mut world = ImportWorld::new(Corrections::default());
    std::fs::copy(
        fixture_root().join("quests-without-terror-of-demogorgon/Quests.xml"),
        world.options.previous_dir.join("Quests.xml"),
    )
    .unwrap();
    let empty_data = world.scratch.path().join("without-pack");
    let mut db = Connection::open_in_memory().unwrap();
    build::build_database(
        &world.options.previous_dir,
        &WikiOverrides::default(),
        &Corrections::default(),
        &mut db,
        &DatasetVersion { upstream_sha: PREVIOUS_SHA.into(), built_at: "2026-10-17T00:00:00Z".into() },
    )
    .unwrap();
    tree::export_ddobuilder_tree(&db, &world.options.previous_dir, &empty_data).unwrap();
    world.options.data_dir = empty_data;
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(report.to_string().contains("packs/terror-of-demogorgon/pack.toml: NEEDS RESEARCH"), "{report}");
}

#[test]
fn import_preserves_unmanaged_files_and_removes_stale_managed_files() {
    let world = ImportWorld::new(Corrections::default());
    std::fs::write(world.options.data_dir.join("README.md"), "Our local guide").unwrap();
    std::fs::write(world.options.data_dir.join(".schemas/stale.json"), "{}").unwrap();
    import_ddobuilder(&world.options).unwrap();
    assert_eq!(std::fs::read_to_string(world.options.data_dir.join("README.md")).unwrap(), "Our local guide");
    assert!(!world.options.data_dir.join(".schemas/stale.json").exists());
}

#[test]
fn import_cli_exits_two_on_an_icon_conflict() {
    let world = ImportWorld::new(Corrections::default());
    std::fs::copy(
        world.options.previous_dir.join("FeatImages/Knock.png"),
        world.options.data_dir.join("icons/feats/Alertness.png"),
    )
    .unwrap();
    let output = cli(&world).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("ICON CONFLICT"));
}

#[test]
fn import_rebases_both_sides_without_mutating_supplied_corrections() {
    let mut corrections = ravager_corrections();
    corrections.entries.retain(|entry| entry.name.contains("Barbarian"));
    let mut world = ImportWorld::new(corrections.clone());
    import_ddobuilder(&world.options).unwrap();
    assert_eq!(world.options.corrections, corrections);
    world.options.previous_dir = world.options.source_dir.clone();
    world.options.previous_sha = world.options.sha.clone();
    let before = world.bytes();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert_eq!(before, world.bytes());
    assert_eq!(world.options.corrections, corrections);
}

#[test]
fn import_icon_conflicts_name_the_final_tree_local_path() {
    let world = ImportWorld::new(Corrections::default());
    let old_icon = world.options.previous_dir.join("EnhancementImages/BomBurningAmbition.png");
    let ours = world.options.data_dir.join("characters/enhancements/bombardier/icons/BomBurningAmbition.png");
    let incoming = world.options.source_dir.join("EnhancementImages/BomBurningAmbition.png");
    let ours_bytes = std::fs::read(world.options.previous_dir.join("FeatImages/Knock.png")).unwrap();
    let incoming_bytes = std::fs::read(world.options.source_dir.join("FeatImages/Alertness.png")).unwrap();
    assert_ne!(ours_bytes, incoming_bytes);
    assert_ne!(std::fs::read(old_icon).unwrap(), incoming_bytes);
    std::fs::write(&ours, &ours_bytes).unwrap();
    std::fs::write(incoming, incoming_bytes).unwrap();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(
        report.to_string().contains("characters/enhancements/bombardier/icons/BomBurningAmbition.png: ICON CONFLICT"),
        "{report}"
    );
    assert_eq!(std::fs::read(ours).unwrap(), ours_bytes);
}
