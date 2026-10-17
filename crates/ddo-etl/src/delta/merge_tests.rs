use super::tests::{fixture_database, rebuilt_database};
use super::{database_delta, merge_delta_into, ConflictSubject, RemovalDeclineReason, TableMerge, TABLE_MERGES};
use crate::tree::{annotated_entities, rederive_data_build_tables};
use rusqlite::Connection;
use std::collections::BTreeSet;

fn edited_database(sql: &str) -> Connection {
    let original = fixture_database();
    original.execute_batch(sql).unwrap();
    rebuilt_database(&original)
}

fn assert_foreign_keys(db: &Connection) {
    assert!(!db.prepare("PRAGMA foreign_key_check").unwrap().exists([]).unwrap());
}

fn ordinary_fixture_database(sql: &str) -> Connection {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut original = Connection::open_in_memory().unwrap();
    crate::build::build_database(
        &root.join("DataFiles"),
        &crate::wiki::WikiOverrides::from_dir(&root.join("wiki")).unwrap(),
        &crate::corrections::Corrections::from_dir(&root.join("data-corrections")).unwrap(),
        &mut original,
        &ddo_model::DatasetVersion { upstream_sha: "fixture".into(), built_at: "2026-10-17T00:00:00Z".into() },
    )
    .unwrap();
    original.execute_batch(sql).unwrap();
    rebuilt_ordinary_database(&original)
}

fn rebuilt_ordinary_database(original: &Connection) -> Connection {
    let root = std::env::temp_dir().join(format!("ddo-merge-{}-{:?}", std::process::id(), std::thread::current().id()));
    crate::tree::export_database(original, &root).unwrap();
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataFiles");
    crate::icons::export_icons(&fixtures, &root.join("icons")).unwrap();
    std::fs::create_dir_all(root.join("icons/ui")).unwrap();
    std::fs::copy(root.join("icons/feats/PowerAttack.png"), root.join("icons/ui/Unknown.png")).unwrap();
    let completion = crate::icons::complete_referenced_icons(&root).unwrap();
    assert!(completion.unresolved_case_aliases.is_empty());
    for missing in completion.missing_icons {
        let destination = root.join("icons").join(missing);
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::copy(root.join("icons/ui/Unknown.png"), destination).unwrap();
    }
    crate::icons::place_enhancement_icons_beside_trees(&root).unwrap();
    let mut rebuilt = Connection::open_in_memory().unwrap();
    crate::tree::build_from_data(&root, &mut rebuilt).unwrap();
    std::fs::remove_dir_all(root).unwrap();
    rebuilt
}

#[test]
fn merge_columns_preserves_ours_and_accepts_unedited_fields() {
    let previous = fixture_database();
    let new = edited_database("UPDATE enhancements SET cost_per_rank = '[1]', description = 'His description' WHERE internal_name = 'SDDefenseBoost'");
    let mut ours =
        edited_database("UPDATE enhancements SET cost_per_rank = '[3]' WHERE internal_name = 'SDDefenseBoost'");
    let outcome = merge_delta_into(&mut ours, &previous, &new, &BTreeSet::new()).unwrap();
    assert_eq!(outcome.conflicts.len(), 1);
    assert_eq!(outcome.conflicts[0].subject, ConflictSubject::Column("cost_per_rank".into()));
    assert_eq!(outcome.conflicts[0].previous, "\"[2]\"");
    assert_eq!(outcome.conflicts[0].new, "\"[1]\"");
    assert_eq!(outcome.conflicts[0].ours, "\"[3]\"");
    let fields: (String, String) = ours
        .query_row(
            "SELECT cost_per_rank, description FROM enhancements WHERE internal_name = 'SDDefenseBoost'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(fields, ("[3]".into(), "His description".into()));
    assert_foreign_keys(&ours);
}

#[test]
fn merge_of_identical_changes_has_no_conflict() {
    let previous = fixture_database();
    let new = edited_database("UPDATE enhancements SET cost_per_rank = '[1]' WHERE internal_name = 'SDDefenseBoost'");
    let mut ours =
        edited_database("UPDATE enhancements SET cost_per_rank = '[1]' WHERE internal_name = 'SDDefenseBoost'");
    let outcome = merge_delta_into(&mut ours, &previous, &new, &BTreeSet::new()).unwrap();
    assert!(outcome.conflicts.is_empty());
    assert!(outcome.applied.values().all(|counts| counts.changed == 0));
    assert!(database_delta(&ours, &new).unwrap().is_empty());
}

#[test]
fn both_added_entities_report_absent_previous_and_keep_our_arrays() {
    let previous = edited_database("DELETE FROM modifiers WHERE source_kind = 'feat' AND source_id IN (SELECT id FROM feats WHERE name = 'Alertness'); DELETE FROM feats WHERE name = 'Alertness'");
    let new = fixture_database();
    let mut ours = edited_database("UPDATE feats SET description = 'Our addition' WHERE name = 'Alertness'; DELETE FROM modifiers WHERE source_kind = 'feat' AND source_id IN (SELECT id FROM feats WHERE name = 'Alertness')");
    let outcome = merge_delta_into(&mut ours, &previous, &new, &BTreeSet::new()).unwrap();
    assert!(outcome
        .conflicts
        .iter()
        .any(|conflict| conflict.subject == ConflictSubject::Column("description".into())
            && conflict.previous == "absent"));
    assert!(outcome
        .conflicts
        .iter()
        .any(|conflict| conflict.subject == ConflictSubject::Array("modifier") && conflict.previous == "absent"));
    assert!(outcome.inserted.is_empty());
}

#[test]
fn merge_inserts_translate_references_and_pass_the_foreign_key_check() {
    let new = fixture_database();
    let previous = edited_database("DELETE FROM modifiers WHERE source_kind = 'feat' AND source_id IN (SELECT id FROM feats WHERE name = 'Alertness'); DELETE FROM feats WHERE name = 'Alertness'");
    let mut ours = edited_database("DELETE FROM modifiers WHERE source_kind = 'feat' AND source_id IN (SELECT id FROM feats WHERE name = 'Alertness'); DELETE FROM feats WHERE name = 'Alertness'");
    let outcome = merge_delta_into(&mut ours, &previous, &new, &BTreeSet::new()).unwrap();
    assert_eq!(outcome.inserted.len(), 1);
    assert!(outcome.conflicts.is_empty());
    assert!(database_delta(&ours, &new).unwrap().is_empty());
    assert_foreign_keys(&ours);
}

#[test]
fn merge_lists_changes_to_entities_missing_in_ours() {
    let previous = fixture_database();
    let new = edited_database("UPDATE feats SET description = 'His description' WHERE name = 'Alertness'");
    let mut ours = edited_database("DELETE FROM modifiers WHERE source_kind = 'feat' AND source_id IN (SELECT id FROM feats WHERE name = 'Alertness'); DELETE FROM feats WHERE name = 'Alertness'");
    let outcome = merge_delta_into(&mut ours, &previous, &new, &BTreeSet::new()).unwrap();
    assert_eq!(outcome.absent_in_ours.len(), 1);
    assert!(outcome.inserted.is_empty());
}

#[test]
fn removal_already_absent_in_ours_takes_no_action() {
    let previous = fixture_database();
    let new = edited_database("DELETE FROM modifiers WHERE source_kind = 'feat' AND source_id IN (SELECT id FROM feats WHERE name = 'Alertness'); DELETE FROM feats WHERE name = 'Alertness'");
    let mut ours = edited_database("DELETE FROM modifiers WHERE source_kind = 'feat' AND source_id IN (SELECT id FROM feats WHERE name = 'Alertness'); DELETE FROM feats WHERE name = 'Alertness'");
    let outcome = merge_delta_into(&mut ours, &previous, &new, &BTreeSet::new()).unwrap();
    assert!(outcome.absent_in_ours.is_empty());
    assert!(outcome.applied.is_empty());
    assert!(outcome.conflicts.is_empty());
    assert!(outcome.declined_removals.is_empty());
}

#[test]
fn arrays_merge_as_one_unit() {
    let previous = fixture_database();
    let new = edited_database("UPDATE dcs SET description = 'His check' WHERE icon = 'Knock'");
    let mut unchanged = fixture_database();
    let outcome = merge_delta_into(&mut unchanged, &previous, &new, &BTreeSet::new()).unwrap();
    assert!(outcome.conflicts.is_empty());
    assert!(database_delta(&unchanged, &new).unwrap().is_empty());
    let mut ours = edited_database("UPDATE dcs SET description = 'Our check' WHERE icon = 'Knock'");
    let outcome = merge_delta_into(&mut ours, &previous, &new, &BTreeSet::new()).unwrap();
    assert_eq!(outcome.conflicts.len(), 1);
    assert_eq!(outcome.conflicts[0].subject, ConflictSubject::Array("dc"));
    assert!(outcome.conflicts[0].ours.contains("Our check"));
}

#[test]
fn socket_slots_and_options_merge_as_one_array() {
    let previous = ordinary_fixture_database("");
    let new = ordinary_fixture_database("UPDATE item_augment_slot_options SET description = 'His socket choice' WHERE id = (SELECT min(id) FROM item_augment_slot_options)");
    let mut ours = ordinary_fixture_database("");
    let outcome = merge_delta_into(&mut ours, &previous, &new, &BTreeSet::new()).unwrap();
    assert!(outcome.conflicts.is_empty());
    assert!(database_delta(&ours, &new).unwrap().is_empty());
    assert_foreign_keys(&ours);
    let mut ours = ordinary_fixture_database("UPDATE item_augment_slot_options SET description = 'Our socket choice' WHERE id = (SELECT min(id) FROM item_augment_slot_options)");
    let outcome = merge_delta_into(&mut ours, &previous, &new, &BTreeSet::new()).unwrap();
    assert_eq!(outcome.conflicts.len(), 1);
    assert_eq!(outcome.conflicts[0].subject, ConflictSubject::Array("socket"));
    assert!(outcome.conflicts[0].ours.contains("Our socket choice"));
    assert_foreign_keys(&ours);
}

#[test]
fn removal_is_declined_when_ours_edited_it() {
    let previous = fixture_database();
    let new = edited_database("DELETE FROM modifiers WHERE source_kind = 'feat' AND source_id IN (SELECT id FROM feats WHERE name = 'Alertness'); DELETE FROM feats WHERE name = 'Alertness'");
    let mut ours = edited_database("UPDATE feats SET description = 'Our description' WHERE name = 'Alertness'");
    let outcome = merge_delta_into(&mut ours, &previous, &new, &BTreeSet::new()).unwrap();
    assert_eq!(outcome.declined_removals.len(), 1);
    assert_eq!(outcome.declined_removals[0].reason, RemovalDeclineReason::EditedByUs);
}

#[test]
fn annotated_removal_is_declined() {
    let previous = fixture_database();
    let new = edited_database("DELETE FROM modifiers WHERE source_kind = 'feat' AND source_id IN (SELECT id FROM feats WHERE name = 'Alertness'); DELETE FROM feats WHERE name = 'Alertness'");
    let mut ours = edited_database("INSERT INTO citations (owner_kind, owner_id, field, url, read) SELECT 'feats', id, 'description', 'https://ddowiki.com/page/Alertness', '2026-10-17' FROM feats WHERE name = 'Alertness'");
    let annotated = annotated_entities(&ours).unwrap();
    let outcome = merge_delta_into(&mut ours, &previous, &new, &annotated).unwrap();
    assert_eq!(outcome.declined_removals.len(), 1);
    assert_eq!(outcome.declined_removals[0].reason, RemovalDeclineReason::Annotated);
}

#[test]
fn removal_is_declined_when_a_surviving_row_references_it() {
    let previous = fixture_database();
    let new = edited_database("DELETE FROM quests; DELETE FROM patrons");
    let mut ours =
        edited_database("UPDATE quests SET flagging = 'Our research' WHERE id = (SELECT min(id) FROM quests)");
    let outcome = merge_delta_into(&mut ours, &previous, &new, &BTreeSet::new()).unwrap();
    assert!(outcome
        .declined_removals
        .iter()
        .any(|removal| removal.table == "patrons" && matches!(removal.reason, RemovalDeclineReason::ReferencedBy(_))));
    assert!(outcome
        .declined_removals
        .iter()
        .any(|removal| removal.table == "quests" && removal.reason == RemovalDeclineReason::EditedByUs));
    assert_foreign_keys(&ours);
    rederive_data_build_tables(&ours).unwrap();
    assert!(database_delta(&ours, &rebuilt_database(&ours)).unwrap().is_empty());
}

#[test]
fn references_replaced_by_his_change_do_not_protect_a_removal() {
    let previous = fixture_database();
    let new = edited_database("UPDATE quests SET patron_id = NULL; DELETE FROM patrons");
    let mut ours = fixture_database();
    let outcome = merge_delta_into(&mut ours, &previous, &new, &BTreeSet::new()).unwrap();
    assert!(outcome.declined_removals.is_empty());
    assert!(database_delta(&ours, &new).unwrap().is_empty());
    assert_foreign_keys(&ours);
}

#[test]
fn owned_entities_and_their_arrays_follow_a_declined_owner_removal() {
    let previous = fixture_database();
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import_ddobuilder/ba7e18e/DataFiles");
    let root =
        std::env::temp_dir().join(format!("ddo-remove-tree-{}-{:?}", std::process::id(), std::thread::current().id()));
    crate::tree::export_ddobuilder_tree(&previous, &source, &root).unwrap();
    std::fs::remove_dir_all(root.join("characters/enhancements/bombardier")).unwrap();
    let mut new = Connection::open_in_memory().unwrap();
    crate::tree::build_from_data(&root, &mut new).unwrap();
    std::fs::remove_dir_all(root).unwrap();
    let mut ours = fixture_database();
    let tree_id: i64 =
        ours.query_row("SELECT id FROM enhancement_trees WHERE name = 'Bombardier'", [], |row| row.get(0)).unwrap();
    let annotated = BTreeSet::from([("enhancement_trees".into(), tree_id)]);
    let outcome = merge_delta_into(&mut ours, &previous, &new, &annotated).unwrap();
    assert!(
        outcome
            .declined_removals
            .iter()
            .any(|removal| removal.table == "enhancements"
                && matches!(removal.reason, RemovalDeclineReason::OwnerKept(_)))
    );
    assert!(outcome.declined_removals.iter().any(|removal| removal.table == "enhancement_selections"
        && matches!(removal.reason, RemovalDeclineReason::OwnerKept(_))));
    assert!(database_delta(&previous, &ours).unwrap().is_empty());
    assert_foreign_keys(&ours);
}

#[test]
fn unreferenced_removal_clears_derived_rows_before_foreign_key_check() {
    let previous = fixture_database();
    let new = edited_database("DELETE FROM bonus_type_aliases WHERE bonus_type_id IN (SELECT id FROM bonus_types WHERE name = 'Insight'); DELETE FROM bonus_types WHERE name = 'Insight'");
    let mut ours = fixture_database();
    let outcome = merge_delta_into(&mut ours, &previous, &new, &BTreeSet::new()).unwrap();
    assert!(outcome.declined_removals.is_empty());
    assert!(outcome.deleted.iter().any(|(table, _)| table == "bonus_types"));
    for (table, rule) in TABLE_MERGES {
        if matches!(rule, TableMerge::Rederived) {
            let count: i64 = ours.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| row.get(0)).unwrap();
            assert_eq!(count, 0, "{table}");
        }
    }
    assert_foreign_keys(&ours);
    rederive_data_build_tables(&ours).unwrap();
    assert!(database_delta(&ours, &rebuilt_database(&ours)).unwrap().is_empty());
    assert_foreign_keys(&ours);
}

#[test]
fn dangling_reference_stops_the_merge() {
    let previous = edited_database("UPDATE quests SET patron_id = NULL");
    let new = fixture_database();
    let mut ours = edited_database("UPDATE quests SET patron_id = NULL; DELETE FROM patrons");
    let before = crate::database_tables::table_rows(&ours, "entity_identity").unwrap();
    let error = merge_delta_into(&mut ours, &previous, &new, &BTreeSet::new()).unwrap_err();
    assert!(error.to_string().contains("which the data files no longer have"), "{error:#}");
    assert!(database_delta(&ours, &edited_database("UPDATE quests SET patron_id = NULL; DELETE FROM patrons"))
        .unwrap()
        .is_empty());
    assert_eq!(before, crate::database_tables::table_rows(&ours, "entity_identity").unwrap());
    assert!(ours.pragma_query_value(None, "foreign_keys", |row| row.get::<_, bool>(0)).unwrap());
    assert!(ours.is_autocommit());
}

#[test]
fn renamed_annotated_selection_survives_beside_the_addition() {
    let previous = fixture_database();
    let new = edited_database("UPDATE enhancement_selections SET name = 'SLA: Vial Smash Acid', slug = 'bomspellvialselection-sla-vial-smash-acid' WHERE slug = 'bomspellvialselection-sla-vial-of-acid'");
    let mut ours = edited_database("INSERT INTO corrections (kind, name, qualifier, field, from_value, to_value, reason, source, read) SELECT 'enhancement', e.name, 'selection \"SLA: Vial of Acid\"', 'description', '\"Upstream prose\"', json_quote(s.description), 'Wiki research', 'https://ddowiki.com/page/Bombardier', '2026-10-17' FROM enhancement_selections s JOIN enhancements e ON e.id = s.enhancement_id WHERE s.slug = 'bomspellvialselection-sla-vial-of-acid'");
    let annotated = annotated_entities(&ours).unwrap();
    let old_id: i64 = ours
        .query_row(
            "SELECT id FROM enhancement_selections WHERE slug = 'bomspellvialselection-sla-vial-of-acid'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(annotated.contains(&("enhancement_selections".into(), old_id)));
    let outcome = merge_delta_into(&mut ours, &previous, &new, &annotated).unwrap();
    assert!(outcome
        .declined_removals
        .iter()
        .any(|removal| removal.ours_id == old_id && removal.table == "enhancement_selections"));
    assert!(outcome.inserted.iter().any(|(table, _)| table == "enhancement_selections"));
    assert!(outcome.declined_removals.iter().any(|removal| removal.ours_id == old_id && removal.position_renumbered));
    let selections = ours.prepare("SELECT name, sort_order FROM enhancement_selections WHERE enhancement_id = (SELECT id FROM enhancements WHERE internal_name = 'BomSpellVialSelection') ORDER BY sort_order").unwrap().query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(selections.iter().map(|(_, position)| *position).collect::<Vec<_>>(), (0..6).collect::<Vec<_>>());
    assert_eq!(selections[1].0, "SLA: Vial Smash Acid");
    assert_eq!(selections[2].0, "SLA: Vial of Acid");
    rederive_data_build_tables(&ours).unwrap();
    assert!(database_delta(&ours, &rebuilt_database(&ours)).unwrap().is_empty());
    assert_foreign_keys(&ours);
}

#[test]
fn selection_reordering_takes_his_order_without_unique_position_collisions() {
    let previous = fixture_database();
    let new = edited_database("UPDATE enhancement_selections SET sort_order = -sort_order - 1 WHERE enhancement_id = (SELECT id FROM enhancements WHERE internal_name = 'BomSpellVialSelection'); UPDATE enhancement_selections SET sort_order = sort_order + 5 WHERE enhancement_id = (SELECT id FROM enhancements WHERE internal_name = 'BomSpellVialSelection')");
    let mut ours = fixture_database();
    let outcome = merge_delta_into(&mut ours, &previous, &new, &BTreeSet::new()).unwrap();
    assert!(outcome.conflicts.is_empty());
    assert!(database_delta(&ours, &new).unwrap().is_empty());
    assert_foreign_keys(&ours);
}

fn selection_names(db: &Connection) -> Vec<String> {
    db.prepare("SELECT name FROM enhancement_selections WHERE enhancement_id = (SELECT id FROM enhancements WHERE internal_name = 'BomSpellVialSelection') ORDER BY sort_order")
        .unwrap().query_map([], |row| row.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap()
}

#[test]
fn selection_rename_preserves_our_unrelated_reordering() {
    let previous = fixture_database();
    let new = edited_database("UPDATE enhancement_selections SET name = 'SLA: Vial Smash Acid', slug = 'bomspellvialselection-sla-vial-smash-acid' WHERE slug = 'bomspellvialselection-sla-vial-of-acid'");
    let mut ours = edited_database("UPDATE enhancement_selections SET sort_order = -sort_order - 1 WHERE enhancement_id = (SELECT id FROM enhancements WHERE internal_name = 'BomSpellVialSelection'); UPDATE enhancement_selections SET sort_order = CASE sort_order WHEN -1 THEN 4 WHEN -5 THEN 0 ELSE -sort_order - 1 END WHERE enhancement_id = (SELECT id FROM enhancements WHERE internal_name = 'BomSpellVialSelection')");
    let mut expected = selection_names(&ours);
    expected.remove(1);
    expected.push("SLA: Vial Smash Acid".into());
    let outcome = merge_delta_into(&mut ours, &previous, &new, &BTreeSet::new()).unwrap();
    assert_eq!(selection_names(&ours), expected);
    assert!(outcome.conflicts.is_empty(), "{:?}", outcome.conflicts);
}

#[test]
fn selection_two_sided_reordering_keeps_ours_with_one_order_conflict() {
    let previous = fixture_database();
    let new = edited_database("UPDATE enhancement_selections SET sort_order = -sort_order - 1 WHERE enhancement_id = (SELECT id FROM enhancements WHERE internal_name = 'BomSpellVialSelection'); UPDATE enhancement_selections SET sort_order = CASE sort_order WHEN -1 THEN 2 WHEN -3 THEN 0 ELSE -sort_order - 1 END WHERE enhancement_id = (SELECT id FROM enhancements WHERE internal_name = 'BomSpellVialSelection')");
    let mut ours = edited_database("UPDATE enhancement_selections SET sort_order = -sort_order - 1 WHERE enhancement_id = (SELECT id FROM enhancements WHERE internal_name = 'BomSpellVialSelection'); UPDATE enhancement_selections SET sort_order = CASE sort_order WHEN -1 THEN 4 WHEN -5 THEN 0 ELSE -sort_order - 1 END WHERE enhancement_id = (SELECT id FROM enhancements WHERE internal_name = 'BomSpellVialSelection')");
    let expected = selection_names(&ours);
    let outcome = merge_delta_into(&mut ours, &previous, &new, &BTreeSet::new()).unwrap();
    assert_eq!(selection_names(&ours), expected);
    assert_eq!(outcome.conflicts.len(), 1);
    assert_eq!(outcome.conflicts[0].identity, "enhancements/bomspellvialselection");
}

#[test]
fn selection_addition_follows_his_neighbour_without_losing_our_order() {
    let previous = edited_database("DELETE FROM modifiers WHERE source_kind = 'enhancement_selection' AND source_id = (SELECT id FROM enhancement_selections WHERE slug = 'bomspellvialselection-sla-vial-of-acid'); DELETE FROM enhancement_selections WHERE slug = 'bomspellvialselection-sla-vial-of-acid'");
    let new = fixture_database();
    let mut ours = edited_database("DELETE FROM modifiers WHERE source_kind = 'enhancement_selection' AND source_id = (SELECT id FROM enhancement_selections WHERE slug = 'bomspellvialselection-sla-vial-of-acid'); DELETE FROM enhancement_selections WHERE slug = 'bomspellvialselection-sla-vial-of-acid'; UPDATE enhancement_selections SET sort_order = -sort_order - 1 WHERE enhancement_id = (SELECT id FROM enhancements WHERE internal_name = 'BomSpellVialSelection'); UPDATE enhancement_selections SET sort_order = CASE sort_order WHEN -1 THEN 4 WHEN -5 THEN 0 ELSE -sort_order - 1 END WHERE enhancement_id = (SELECT id FROM enhancements WHERE internal_name = 'BomSpellVialSelection')");
    let mut expected = selection_names(&ours);
    expected.push("SLA: Vial of Acid".into());
    let outcome = merge_delta_into(&mut ours, &previous, &new, &BTreeSet::new()).unwrap();
    assert_eq!(selection_names(&ours), expected);
    assert!(outcome.conflicts.is_empty(), "{:?}", outcome.conflicts);
}

#[test]
fn merge_preserves_correction_notes_removed_upstream() {
    let previous = ordinary_fixture_database("INSERT INTO corrections (kind, name, field, from_value, to_value, reason, source, read) SELECT 'feat', name, 'description', '\"Upstream prose\"', json_quote(description), 'Wiki research', 'https://ddowiki.com/page/Power_Attack', '2026-10-17' FROM feats WHERE name = 'Power Attack'");
    let new = ordinary_fixture_database("");
    let mut ours = ordinary_fixture_database("INSERT INTO corrections (kind, name, field, from_value, to_value, reason, source, read) SELECT 'feat', name, 'description', '\"Upstream prose\"', json_quote(description), 'Wiki research', 'https://ddowiki.com/page/Power_Attack', '2026-10-17' FROM feats WHERE name = 'Power Attack'");
    let outcome = merge_delta_into(&mut ours, &previous, &new, &BTreeSet::new()).unwrap();
    assert!(!outcome.kept_correction_notes.is_empty());
    assert!(database_delta(&previous, &ours).unwrap().is_empty());
}

#[test]
fn superseded_wiki_row_takes_his_columns_and_drops_its_primary_citation() {
    let previous = ordinary_fixture_database("");
    let new = ordinary_fixture_database("DELETE FROM citations WHERE owner_kind = 'items' AND field = '' AND owner_id IN (SELECT id FROM items WHERE name = 'Battle Axe of the Oozing Hunger'); UPDATE items SET description = 'His newly supplied description', provenance = 'maetrim' WHERE name = 'Battle Axe of the Oozing Hunger'");
    let mut ours = ordinary_fixture_database("");
    let outcome = merge_delta_into(&mut ours, &previous, &new, &BTreeSet::new()).unwrap();
    assert!(outcome.conflicts.is_empty());
    rederive_data_build_tables(&ours).unwrap();
    let rebuilt = rebuilt_ordinary_database(&ours);
    let (description, provenance): (String, String) = rebuilt
        .query_row(
            "SELECT description, provenance FROM items WHERE name = 'Battle Axe of the Oozing Hunger'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(description, "His newly supplied description");
    assert_eq!(provenance, "maetrim");
    assert!(database_delta(&ours, &rebuilt).unwrap().is_empty());
}

#[test]
fn verification_accepts_recomputed_source_provenance() {
    let previous = ordinary_fixture_database("");
    let new = ordinary_fixture_database("UPDATE items SET drop_location = ''");
    let mut ours = ordinary_fixture_database("UPDATE quest_drops SET is_rare = 1 - is_rare");
    let outcome = merge_delta_into(&mut ours, &previous, &new, &BTreeSet::new()).unwrap();
    assert!(outcome.conflicts.is_empty());
    rederive_data_build_tables(&ours).unwrap();
    let rebuilt = rebuilt_ordinary_database(&ours);
    let source_provenance = |db: &Connection| {
        db.prepare("SELECT drop_text_segment, source_match FROM quest_drops ORDER BY id")
            .unwrap()
            .query_map([], |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    };
    assert_ne!(source_provenance(&ours), source_provenance(&rebuilt));
    assert!(
        database_delta(&ours, &rebuilt).unwrap().is_empty(),
        "V2 compares authored rows while source provenance is recomputed"
    );
}

#[test]
fn rederivation_is_idempotent_and_preserves_authored_rows() {
    let db = fixture_database();
    let before = TABLE_MERGES
        .iter()
        .map(|(table, _)| ((*table).to_string(), crate::database_tables::table_rows(&db, table).unwrap()))
        .collect::<std::collections::BTreeMap<_, _>>();
    rederive_data_build_tables(&db).unwrap();
    rederive_data_build_tables(&db).unwrap();
    for (table, rows) in before {
        assert_eq!(rows, crate::database_tables::table_rows(&db, &table).unwrap(), "{table}");
    }
}

#[test]
fn merge_read_query_count_does_not_grow_with_rows() {
    use rusqlite::trace::{TraceEvent, TraceEventCodes};
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SELECT_COUNT: AtomicUsize = AtomicUsize::new(0);
    fn trace(event: TraceEvent<'_>) {
        if let TraceEvent::Stmt(statement, _) = event {
            if statement.sql().trim_start().to_uppercase().starts_with("SELECT") {
                SELECT_COUNT.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
    let mut counts = Vec::new();
    for is_committed in [false, true] {
        let build_side = |incoming: bool| {
            let scratch = tempfile::tempdir().unwrap();
            let root = scratch.path().join("data");
            let mut original = if is_committed {
                let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
                for entry in walkdir::WalkDir::new(&source).into_iter().map(Result::unwrap) {
                    if entry.file_type().is_file()
                        && entry.path().extension().is_some_and(|extension| extension == "png")
                    {
                        let destination = root.join(entry.path().strip_prefix(&source).unwrap());
                        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
                        std::fs::copy(entry.path(), destination).unwrap();
                    }
                }
                let mut db = Connection::open_in_memory().unwrap();
                crate::tree::build_from_data(&source, &mut db).unwrap();
                db
            } else {
                let db = fixture_database();
                let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/import_ddobuilder/ba7e18e/DataFiles");
                crate::tree::export_ddobuilder_tree(&db, &source, &root).unwrap();
                db
            };
            let columns = crate::database_tables::table_columns(&original, "feats").unwrap();
            let selected = columns
                .iter()
                .map(|column| match column.as_str() {
                    "id" => "-1".to_string(),
                    "name" => if incoming { "'Query count added'" } else { "'Query count removed'" }.into(),
                    "slug" => if incoming { "'query-count-added'" } else { "'query-count-removed'" }.into(),
                    _ => format!("[{column}]"),
                })
                .collect::<Vec<_>>()
                .join(", ");
            original
                .execute(&format!("INSERT INTO feats SELECT {selected} FROM feats WHERE name = 'Alertness'"), [])
                .unwrap();
            if incoming {
                original.execute("UPDATE enhancements SET description = 'A measured column edit' WHERE internal_name = 'SDDefenseBoost'", []).unwrap();
                original.execute("UPDATE modifiers SET amounts = '[7]' WHERE source_kind = 'enhancement' AND source_id IN (SELECT id FROM enhancements WHERE internal_name = 'RavBarbarianPowerAttack')", []).unwrap();
            }
            crate::tree::export_database(&original, &root).unwrap();
            original = Connection::open_in_memory().unwrap();
            crate::tree::build_from_data(&root, &mut original).unwrap();
            original
        };
        let previous = build_side(false);
        let new = build_side(true);
        let mut ours = build_side(false);
        new.trace_v2(TraceEventCodes::SQLITE_TRACE_STMT, Some(trace));
        previous.trace_v2(TraceEventCodes::SQLITE_TRACE_STMT, Some(trace));
        ours.trace_v2(TraceEventCodes::SQLITE_TRACE_STMT, Some(trace));
        SELECT_COUNT.store(0, Ordering::SeqCst);
        let outcome = merge_delta_into(&mut ours, &previous, &new, &BTreeSet::new()).unwrap();
        assert!(outcome.conflicts.is_empty());
        assert_eq!(outcome.applied["feats"].added, 1);
        assert_eq!(outcome.applied["feats"].removed, 1);
        assert!(outcome.applied["enhancements"].changed >= 1);
        assert!(outcome.applied["enhancements"].arrays_replaced >= 1);
        counts.push(SELECT_COUNT.load(Ordering::SeqCst));
    }
    assert_eq!(counts[0], counts[1]);
    assert_eq!(counts[0], 3 * (1 + TABLE_MERGES.len()));
}

#[test]
fn unique_positions_follow_entity_or_array_merge_policy() {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(ddo_model::ddl()).unwrap();
    for (table, rule) in TABLE_MERGES {
        let mut indexes = db.prepare(&format!("PRAGMA index_list('{table}')")).unwrap();
        let indexes = indexes
            .query_map([], |row| Ok((row.get::<_, String>(1)?, row.get::<_, bool>(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        for (index, is_unique) in indexes {
            if !is_unique {
                continue;
            }
            let mut columns = db.prepare(&format!("PRAGMA index_info('{index}')")).unwrap();
            let columns = columns
                .query_map([], |row| row.get::<_, Option<String>>(2))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            let mut metadata = db.prepare(&format!("PRAGMA table_info('{table}')")).unwrap();
            let scalar_integers = metadata
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, bool>(3)?,
                        row.get::<_, i64>(5)?,
                    ))
                })
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            let mut foreign_keys = db.prepare(&format!("PRAGMA foreign_key_list('{table}')")).unwrap();
            let references = foreign_keys
                .query_map([], |row| row.get::<_, String>(3))
                .unwrap()
                .collect::<rusqlite::Result<BTreeSet<_>>>()
                .unwrap();
            for position in columns.iter().flatten().filter(|column| {
                !references.contains(*column)
                    && scalar_integers.iter().any(|(name, kind, required, primary)| {
                        name == *column && kind == "INTEGER" && *required && (*primary == 0 || name != "id")
                    })
            }) {
                match rule {
                    TableMerge::Array { .. } | TableMerge::DatasetArray { .. } | TableMerge::Rederived => {}
                    TableMerge::Entity { owner_column: Some(owner), position_column: Some(declared) } => {
                        assert_eq!(position, declared, "{table} position has no declared group policy");
                        assert!(columns.contains(&Some((*owner).into())), "{table}");
                    }
                    _ => panic!("{table}.{position} has no position merge policy"),
                }
            }
        }
    }
}
