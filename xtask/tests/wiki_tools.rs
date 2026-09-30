use ddo_etl::wiki::DescriptionKind;
use serde_json::Value;
use std::path::{Path, PathBuf};
use xtask::wiki_tools::{
    blank_description_page_url, likely_wiki_page_url, socket_label_spelling_key, socket_label_spelling_warnings,
    wiki_check_report, write_wiki_batch,
};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/ddo-etl/tests/fixtures")
}

fn fixture_corrections_dir() -> PathBuf {
    fixtures_dir().join("corrections")
}

fn line_count(path: &Path) -> usize {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display())).lines().count()
}

#[test]
fn wiki_check_counts_corrections_and_warns_about_each_stale_one() {
    let report = wiki_check_report(
        &fixtures_dir().join("DataFiles"),
        Some(&fixtures_dir().join("wiki")),
        Some(&fixture_corrections_dir()),
    )
    .unwrap();
    for expected_line in ["correction_applied_count: 3", "correction_stale_count: 1"] {
        assert!(report.lines().any(|line| line == expected_line), "missing {expected_line:?} in\n{report}");
    }
    let (_, warnings) = report.split_once("warnings:\n").unwrap();
    assert!(
        warnings.lines().any(|line| line
            == "warning: correction augment \"Ruby of Acid Damage\".min_level expects 7 but Maetrim now has null; delete it from corrections.toml"),
        "{warnings}"
    );
}

#[test]
fn wiki_check_reports_the_wiki_counts_for_a_valid_wiki_dir() {
    let report = wiki_check_report(
        &fixtures_dir().join("DataFiles"),
        Some(&fixtures_dir().join("wiki")),
        Some(&fixture_corrections_dir()),
    )
    .unwrap();

    for expected_line in [
        "wiki_quest_loot_entry_count: 1",
        "wiki_quest_entry_count: 3",
        "wiki_crafting_system_count: 2",
        "wiki_crafting_recipe_count: 5",
        "wiki_crafting_ingredient_count: 5",
        "wiki_description_entry_count: 4",
        "wiki_description_filled_count: 2",
        "wiki_description_skipped_count: 3",
        "wiki_item_written_count: 1",
        "wiki_item_superseded_count: 1",
        "wiki_item_probable_duplicate_count: 0",
    ] {
        assert!(report.lines().any(|line| line == expected_line), "missing {expected_line:?} in\n{report}");
    }
    assert!(!report.contains("written_item_count"), "only wiki lines: {report}");
}

#[test]
fn wiki_check_warns_about_superseded_and_probably_duplicate_wiki_items() {
    let fixture_items = std::fs::read_to_string(fixtures_dir().join("wiki/items.toml")).unwrap();
    let draft_dir = tempfile::tempdir().unwrap();
    std::fs::write(
        draft_dir.path().join("items_draft.toml"),
        fixture_items.replace("Battle Axe of the Oozing Hunger", "Argentis Armor (Level 12)"),
    )
    .unwrap();

    let report =
        wiki_check_report(&fixtures_dir().join("DataFiles"), Some(draft_dir.path()), Some(&fixture_corrections_dir()))
            .unwrap();

    let warnings_start = report.find("\nwarnings:").unwrap_or_else(|| panic!("no warnings section in\n{report}"));
    for expected_warning in [
        "warning: wiki item \"Five Rings\" is now in Maetrim's files; delete it from items_draft.toml",
        "warning: wiki item \"Argentis Armor (Level 12)\" may duplicate Maetrim's \"Argenti's Armor\"",
    ] {
        assert!(
            report[warnings_start..].lines().any(|line| line == expected_warning),
            "missing {expected_warning:?} in\n{report}"
        );
    }
    assert!(report[..warnings_start].lines().any(|line| line == "wiki_item_probable_duplicate_count: 1"), "{report}");
}

#[test]
fn wiki_check_warns_about_a_wiki_item_that_looks_like_a_variant_of_a_maetrim_item() {
    let fixture_items = std::fs::read_to_string(fixtures_dir().join("wiki/items.toml")).unwrap();
    let variant_items = fixture_items.replace("name = \"Five Rings\"", "name = \"Five Rings (plain)\"").replace(
        "drop_location = \"Test source: a wiki copy of an item Maetrim already carries\"",
        "drop_location = \"Secret of the Slavers' Stockade, Small chest\"",
    );
    let other_level_items = variant_items.replace("minimum_level = 8", "minimum_level = 9");
    let variant_warning = "warning: wiki item \"Five Rings (plain)\" looks like a variant of Maetrim's \"Five Rings\" (same level and drop location)";

    let report_for = |items_toml: &str| {
        let draft_dir = tempfile::tempdir().unwrap();
        std::fs::write(draft_dir.path().join("items_draft.toml"), items_toml).unwrap();
        wiki_check_report(&fixtures_dir().join("DataFiles"), Some(draft_dir.path()), Some(&fixture_corrections_dir()))
            .unwrap()
    };

    let report = report_for(&variant_items);
    let warnings_start = report.find("\nwarnings:").unwrap_or_else(|| panic!("no warnings section in\n{report}"));
    assert!(
        report[warnings_start..].lines().any(|line| line == variant_warning),
        "missing {variant_warning:?} in\n{report}"
    );
    assert!(!report_for(&other_level_items).contains("looks like a variant"), "a different level is no variant");
    assert!(!report_for(&fixture_items).contains("looks like a variant"));
}

#[test]
fn wiki_check_warns_about_a_wiki_effect_spelled_like_a_maetrim_effect_apart_from_punctuation() {
    let fixture_items = std::fs::read_to_string(fixtures_dir().join("wiki/items.toml")).unwrap();
    let items_with_effect = |effect_name: &str| {
        fixture_items.replacen(
            "{ name = \"Ethereal\" },",
            &format!("{{ name = \"Ethereal\" }},\n  {{ name = \"{effect_name}\" }},"),
            1,
        )
    };
    let report_for = |items_toml: &str| {
        let draft_dir = tempfile::tempdir().unwrap();
        std::fs::write(draft_dir.path().join("items_draft.toml"), items_toml).unwrap();
        wiki_check_report(&fixtures_dir().join("DataFiles"), Some(draft_dir.path()), Some(&fixture_corrections_dir()))
            .unwrap()
    };

    let report = report_for(&items_with_effect("Maximum Charge Tier / III"));
    let effect_warning =
        "warning: wiki effect \"Maximum Charge Tier / III\" may be Maetrim's \"MaximumChargeTierIII\" spelled differently";
    let warnings_start = report.find("\nwarnings:").unwrap_or_else(|| panic!("no warnings section in\n{report}"));
    assert!(
        report[warnings_start..].lines().any(|line| line == effect_warning),
        "missing {effect_warning:?} in\n{report}"
    );
    assert!(
        !report_for(&items_with_effect("Maximum Charge Tier: III")).contains("wiki effect"),
        "the build reuses his"
    );
    assert!(!report_for(&fixture_items).contains("wiki effect"), "Test Oozing Hunger is no one's spelling");
}

#[test]
fn wiki_check_warns_about_family_augments_no_recipe_yields() {
    let fixture_crafting = std::fs::read_to_string(fixtures_dir().join("wiki/crafting.toml")).unwrap();
    let minor_fire_guard_recipe = "[[system.recipe]]\ntier = \"heroic\"\nslot = \"crafting: accessory invasion\"\noption = \"Minor Fire Guard\"\naugments = [\"Minor Fire Guard\"]\n";
    let recipe_start = fixture_crafting.find(minor_fire_guard_recipe).expect("the fixture Minor Fire Guard recipe");
    let recipe_end = recipe_start + fixture_crafting[recipe_start..].find("\n\n").unwrap() + 2;
    let draft_dir = tempfile::tempdir().unwrap();
    std::fs::write(
        draft_dir.path().join("crafting.toml"),
        format!("{}{}", &fixture_crafting[..recipe_start], &fixture_crafting[recipe_end..]),
    )
    .unwrap();

    let full_report = wiki_check_report(
        &fixtures_dir().join("DataFiles"),
        Some(&fixtures_dir().join("wiki")),
        Some(&fixture_corrections_dir()),
    )
    .unwrap();
    let draft_report =
        wiki_check_report(&fixtures_dir().join("DataFiles"), Some(draft_dir.path()), Some(&fixture_corrections_dir()))
            .unwrap();

    let unused_augment_warning =
        "warning: Heroic Green Steel: Greensteel_Heroic augment \"Minor Fire Guard\" has no recipe";
    assert!(!full_report.contains(unused_augment_warning), "{full_report}");
    let warnings_start =
        draft_report.find("\nwarnings:\n").unwrap_or_else(|| panic!("no warnings section in\n{draft_report}"));
    assert!(
        draft_report[..warnings_start].lines().any(|line| line == "wiki_crafting_recipe_count: 4"),
        "{draft_report}"
    );
    assert!(draft_report[warnings_start..].lines().any(|line| line == unused_augment_warning), "{draft_report}");
    assert!(!draft_report.contains("\"+5 Fortitude Save\" has no recipe"), "{draft_report}");
}

#[test]
fn socket_label_spelling_key_ignores_case_spacing_punctuation_and_known_misspellings() {
    assert_eq!(
        socket_label_spelling_key("crafting: Zentarim Attuned"),
        socket_label_spelling_key("crafting: zhentarim attuned")
    );
    assert_eq!(
        socket_label_spelling_key("upgrade: Upgradable Item"),
        socket_label_spelling_key("upgrade: upgradeable-item")
    );
    assert_eq!(
        socket_label_spelling_key("crafting: Suppressed power"),
        socket_label_spelling_key("crafting: suppressed Power")
    );
    assert_ne!(
        socket_label_spelling_key("crafting: reaper helmet"),
        socket_label_spelling_key("crafting: reaper belt")
    );
}

#[test]
fn socket_label_spelling_warnings_name_each_group_of_colliding_labels() {
    let labels = ["crafting: Zentarim Attuned", "colorless", "crafting: Zhentarim Attuned", "crafting: reaper belt"]
        .map(String::from);

    assert_eq!(
        socket_label_spelling_warnings(&labels),
        ["warning: socket labels differ only by spelling: \"crafting: Zentarim Attuned\" / \"crafting: Zhentarim Attuned\""]
    );
}

#[test]
fn wiki_check_prints_no_socket_label_warning_for_the_fixture_labels() {
    let report = wiki_check_report(
        &fixtures_dir().join("DataFiles"),
        Some(&fixtures_dir().join("wiki")),
        Some(&fixture_corrections_dir()),
    )
    .unwrap();

    assert!(!report.contains("socket labels differ only by spelling"), "{report}");
}

#[test]
fn wiki_check_fails_naming_an_unknown_quest() {
    let draft_dir = tempfile::tempdir().unwrap();
    std::fs::write(
        draft_dir.path().join("quests.toml"),
        "[[quest]]\nname = \"No Such Quest\"\npage = \"https://ddowiki.com/page/No_Such_Quest\"\nread = \"2026-09-29\"\nfree_to_play = true\n",
    )
    .unwrap();

    let error =
        wiki_check_report(&fixtures_dir().join("DataFiles"), Some(draft_dir.path()), Some(&fixture_corrections_dir()))
            .unwrap_err();

    assert!(format!("{error:#}").contains("No Such Quest"), "{error:#}");
}

#[test]
fn wiki_batch_writes_the_reading_agent_inputs() {
    let out_dir = tempfile::tempdir().unwrap();

    write_wiki_batch(
        &fixtures_dir().join("DataFiles"),
        Some(&fixtures_dir().join("wiki")),
        Some(&fixture_corrections_dir()),
        out_dir.path(),
    )
    .unwrap();

    assert_eq!(line_count(&out_dir.path().join("item_names.txt")), 15);
    assert_eq!(
        std::fs::read_to_string(out_dir.path().join("wiki_source_items.txt")).unwrap(),
        "Battle Axe of the Oozing Hunger\n"
    );
    let augment_lines = std::fs::read_to_string(out_dir.path().join("augment_names.txt")).unwrap();
    assert_eq!(augment_lines.lines().count(), 11);
    assert!(augment_lines.lines().any(|line| line == "Alchemical\tFire I: Combustion\t29"), "{augment_lines}");
    let quest_pages: Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.path().join("quest_pages.json")).unwrap()).unwrap();
    assert_eq!(quest_pages.as_object().unwrap().len(), 13);
    assert_eq!(
        quest_pages["Dr. Rushmore's Mansion - Behind the Door"],
        likely_wiki_page_url("Dr. Rushmore's Mansion - Behind the Door")
    );
    let crafting_systems: Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.path().join("crafting_systems.json")).unwrap()).unwrap();
    assert_eq!(crafting_systems.as_object().unwrap().len(), 41);
}

#[test]
fn wiki_batch_lists_every_blank_description_the_wiki_files_have_not_filled() {
    let empty_wiki_dir = tempfile::tempdir().unwrap();
    let unfilled_out_dir = tempfile::tempdir().unwrap();
    let filled_out_dir = tempfile::tempdir().unwrap();

    write_wiki_batch(
        &fixtures_dir().join("DataFiles"),
        Some(empty_wiki_dir.path()),
        Some(&fixture_corrections_dir()),
        unfilled_out_dir.path(),
    )
    .unwrap();
    write_wiki_batch(
        &fixtures_dir().join("DataFiles"),
        Some(&fixtures_dir().join("wiki")),
        Some(&fixture_corrections_dir()),
        filled_out_dir.path(),
    )
    .unwrap();

    assert_eq!(
        std::fs::read_to_string(unfilled_out_dir.path().join("blank_descriptions.txt")).unwrap(),
        "item\t+1 Ember Repeating Light Crossbow\thttps://ddowiki.com/page/Item:+1_Ember_Repeating_Light_Crossbow\n\
         item\t+1 Starter Heavy Steel Shield\thttps://ddowiki.com/page/Item:+1_Starter_Heavy_Steel_Shield\n\
         augment\tLunar Gem of Evocation (Heroic)\thttps://ddowiki.com/page/Item:Lunar_Gem_of_Evocation_(Heroic)\n"
    );
    assert_eq!(
        std::fs::read_to_string(filled_out_dir.path().join("blank_descriptions.txt")).unwrap(),
        "item\t+1 Starter Heavy Steel Shield\thttps://ddowiki.com/page/Item:+1_Starter_Heavy_Steel_Shield\n"
    );
}

#[test]
fn blank_description_page_urls_prefix_items_and_augments_only() {
    assert_eq!(
        blank_description_page_url(DescriptionKind::Item, "Royal Guard Mask"),
        "https://ddowiki.com/page/Item:Royal_Guard_Mask"
    );
    assert_eq!(
        blank_description_page_url(DescriptionKind::Augment, "Lunar Gem of Evocation (Heroic)"),
        "https://ddowiki.com/page/Item:Lunar_Gem_of_Evocation_(Heroic)"
    );
    for kind in [DescriptionKind::Race, DescriptionKind::Feat, DescriptionKind::Enhancement] {
        assert_eq!(
            blank_description_page_url(kind, "Dhampir Dark Bargainer"),
            "https://ddowiki.com/page/Dhampir_Dark_Bargainer",
            "{kind:?}"
        );
    }
}

#[test]
fn likely_wiki_page_url_underscores_spaces_escapes_apostrophes_and_drops_the_difficulty() {
    assert_eq!(likely_wiki_page_url("The Chronoscope"), "https://ddowiki.com/page/The_Chronoscope");
    assert_eq!(
        likely_wiki_page_url("Dr. Rushmore's Mansion - Behind the Door"),
        "https://ddowiki.com/page/Dr._Rushmore%27s_Mansion_-_Behind_the_Door"
    );
    for difficulty in ["Casual", "Normal", "Hard", "Elite"] {
        assert_eq!(
            likely_wiki_page_url(&format!("The Pit ({difficulty})")),
            "https://ddowiki.com/page/The_Pit",
            "{difficulty}"
        );
    }
    assert_eq!(likely_wiki_page_url("The Pit (Reaper)"), "https://ddowiki.com/page/The_Pit_(Reaper)");
}
