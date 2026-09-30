use ddo_etl::build::{build_database, BuildReport};
use ddo_etl::corrections::Corrections;
use ddo_etl::map::drop_location::{
    chest_following, chest_label, drop_text_in_description, marks_rare_loot, quest_name_spans,
};
use ddo_etl::wiki::WikiOverrides;
use ddo_model::DatasetVersion;
use rusqlite::Connection;
use std::path::PathBuf;

fn built_without_wiki() -> (Connection, BuildReport) {
    let mut db = Connection::open_in_memory().unwrap();
    let data_files_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataFiles");
    let dataset_version = DatasetVersion { upstream_sha: "31ef0201".into(), built_at: "2026-09-20T00:00:00Z".into() };
    let report =
        build_database(&data_files_dir, &WikiOverrides::default(), &Corrections::default(), &mut db, &dataset_version)
            .unwrap();
    (db, report)
}

fn is_rare(db: &Connection, quest: &str, item: &str) -> Option<bool> {
    db.query_row(
        "SELECT ql.is_rare FROM quest_loot ql JOIN quests q ON q.id = ql.quest_id JOIN items i ON i.id = ql.item_id
          WHERE q.name = ?1 AND i.name = ?2",
        [quest, item],
        |r| r.get(0),
    )
    .ok()
}

#[test]
fn marks_a_rare_chest_drop_from_maetrims_text_alone() {
    let (db, report) = built_without_wiki();
    assert_eq!(is_rare(&db, "Book Burning", "Buckler of the Golden Age"), Some(true), "'end chest (rare)'");
    assert_eq!(report.drop_text_rare_link_count, 1);
}

#[test]
fn leaves_unmarked_and_rare_encounter_drops_common() {
    let (db, _) = built_without_wiki();
    assert_eq!(is_rare(&db, "The Cursed Crypt", "Docent of Defiance"), Some(false));
    assert_eq!(
        is_rare(&db, "Land of Lamordia", "Gravekeeper's Docent"),
        Some(false),
        "'red-named rare encounter chests' names a rare monster, not rare loot"
    );
}

#[test]
fn classifies_drop_text_segments_by_their_rarity_marker() {
    for rare in [
        "Book Burning, end chest (rare)",
        "The Final Draw, end chest (rare )",
        "Ruins of Myth Drannor, Yegora 's chest (RARE)",
        "Magic of Myth Drannor, rare drop in any end chest",
        "also Rare Drop in any Sharn quest",
    ] {
        assert!(marks_rare_loot(rare), "{rare}");
    }
    for common in [
        "The Cursed Crypt, End Chest",
        "Land of Lamordia, red-named rare encounter chests",
        "The Borderlands, chest (Rare Encounters)",
        "Three-Barrel Cove (heroic), Brine 's Chest (Rare Encounter)",
        "Korthos Island, Any of the Rares",
    ] {
        assert!(!marks_rare_loot(common), "{common}");
    }
}

#[test]
fn labels_the_chest_named_after_a_quest_without_rarity_or_asides() {
    for (text_after_quest_name, expected_chest) in [
        (", end chest", Some("end chest")),
        (", End Chest", Some("end chest")),
        (", Althea's chest", Some("althea's chest")),
        (", Vornir Frosthelm's chest (rare)", Some("vornir frosthelm's chest")),
        (", end chest (rare )", Some("end chest")),
        (", rare drop in any end chest", Some("any end chest")),
        (" (heroic), Brine 's Chest (Rare Encounter)", Some("brine 's chest")),
        (", red-named rare encounter chests", Some("red-named rare encounter chests")),
        (": Any Feywild rare encounter chest.", Some("any feywild rare encounter chest")),
        ("", None),
        (" (rare)", None),
        (",  ", None),
        (" and ", None),
        (", Rantha 's chest &amp", Some("rantha 's chest")),
        (" (wilderness), Rare Chests, and ", Some("rare chests")),
        (", any legendary end chest or ", Some("any legendary end chest")),
        (", end chest, and, ", Some("end chest")),
        (" and visits to the Cerulean Hills and ", Some("visits to the cerulean hills")),
        (", or ", None),
    ] {
        assert_eq!(chest_label(text_after_quest_name).as_deref(), expected_chest, "{text_after_quest_name:?}");
    }
}

#[test]
fn gives_quests_listed_together_the_chest_named_after_the_last_of_them() {
    let drop_location =
        "Lines of Supply, Breaking the Ranks and A Break in the Ice, end chest; Cold Snap, Althea's chest";
    let quest_name_spans: Vec<std::ops::Range<usize>> =
        ["Lines of Supply", "Breaking the Ranks", "A Break in the Ice", "Cold Snap"]
            .iter()
            .map(|quest_name| {
                let start = drop_location.find(quest_name).unwrap();
                start..start + quest_name.len()
            })
            .collect();
    let chests: Vec<Option<String>> =
        quest_name_spans.iter().map(|span| chest_following(drop_location, span.end, &quest_name_spans)).collect();
    assert_eq!(
        chests.iter().map(Option::as_deref).collect::<Vec<_>>(),
        [Some("end chest"), Some("end chest"), Some("end chest"), Some("althea's chest")]
    );
}

#[test]
fn finds_a_quest_name_spelt_with_other_capitals_or_spacing_only_as_whole_words() {
    let spans_of = |drop_text: &str, quest_name: &str| -> Vec<String> {
        quest_name_spans(drop_text, quest_name).into_iter().map(|span| drop_text[span].to_string()).collect()
    };
    assert_eq!(
        spans_of("Lines of Supply, A Break In the Ice, end chest", "A Break in the Ice"),
        ["A Break In the Ice"]
    );
    assert_eq!(spans_of("Quest: Wake me up  Inside", "Wake Me Up Inside"), ["Wake me up  Inside"]);
    assert!(
        spans_of("Fables of the Feywild, any chest", "The Feywild").is_empty(),
        "a lower-case first letter is prose"
    );
    assert!(spans_of("The pitiless ice", "The Pit").is_empty(), "a folded match must end at a word boundary");
    assert_eq!(spans_of("The Pitiless", "The Pit"), ["The Pit"], "an exact match needs no boundary, as before");
    assert_eq!(
        spans_of("and ToEE: Lower\nTemple Complex, rare encounter chests", "ToEE: Lower Temple Complex"),
        ["ToEE: Lower\nTemple Complex"],
        "a name his text wraps onto the next line"
    );
}

#[test]
fn gives_each_quest_its_own_chest_when_the_text_names_one_after_each() {
    let drop_location = "Isle of Dread (wilderness), Rare Chests, and All Hail the King end chest";
    let quest_name_spans: Vec<std::ops::Range<usize>> = ["Isle of Dread", "All Hail the King"]
        .iter()
        .flat_map(|quest_name| quest_name_spans(drop_location, quest_name))
        .collect();
    let chests: Vec<Option<String>> =
        quest_name_spans.iter().map(|span| chest_following(drop_location, span.end, &quest_name_spans)).collect();
    assert_eq!(chests.iter().map(Option::as_deref).collect::<Vec<_>>(), [Some("rare chests"), Some("end chest")]);
}

#[test]
fn reads_the_drop_text_after_drops_in_in_an_augment_description() {
    for (description, expected_drop_text) in [
        (
            "+2 Artifact Bonus to Strength\nDrops in: The House of Gems, end chest\nDinosaur Crisis, optional chest",
            Some("The House of Gems, end chest\nDinosaur Crisis, optional chest"),
        ),
        (
            "Wards against Slows.\nDrops in U48 Quest: The Knight who Cried Windmill",
            Some("The Knight who Cried Windmill"),
        ),
        (
            "Grants the feat \"Quick Draw\"\nDrops in U48: Any Feywild rare encounter chest.",
            Some("Any Feywild rare encounter chest."),
        ),
        ("Drops in; Isle of Dread, any chest", Some("Isle of Dread, any chest")),
        ("Drops in Isle of Dread, any chest", Some("Isle of Dread, any chest")),
        (
            "Acid on crits.\nDrops in: Drop Location: ToEE: First Level and Earth Temple",
            Some("ToEE: First Level and Earth Temple"),
        ),
        ("+2 Profane Bonus to Evocation DCs\nDrops in: ?", Some("?")),
        (
            "Crushing Wave.\nDrop Location: Ghost of a Chance (epic), puzzle chest",
            Some("Ghost of a Chance (epic), puzzle chest"),
        ),
        ("+2 Artifact Bonus to Strength", None),
    ] {
        assert_eq!(drop_text_in_description(description), expected_drop_text, "{description:?}");
    }
}
