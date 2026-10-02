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

fn draft_dir_with_fixture_quests() -> tempfile::TempDir {
    let draft_dir = tempfile::tempdir().unwrap();
    std::fs::copy(fixtures_dir().join("wiki/quests.toml"), draft_dir.path().join("quests.toml")).unwrap();
    draft_dir
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
fn wiki_check_warns_to_delete_a_rename_done_upstream() {
    let corrections_dir = tempfile::tempdir().unwrap();
    std::fs::write(
        corrections_dir.path().join("corrections_items.toml"),
        "[[correction]]\nkind = \"item\"\nname = \"Docent of Defiant\"\nfield = \"name\"\n\
         from = \"Docent of Defiant\"\nto = \"Docent of Defiance\"\nreason = \"Test rename.\"\n\
         source = \"https://ddowiki.com/page/Test\"\nread = \"2026-10-01\"\n",
    )
    .unwrap();
    let report = wiki_check_report(
        &fixtures_dir().join("DataFiles"),
        Some(&fixtures_dir().join("wiki")),
        Some(corrections_dir.path()),
    )
    .unwrap();
    assert!(report.lines().any(|line| line == "correction_stale_count: 1"), "{report}");
    let (_, warnings) = report.split_once("warnings:\n").unwrap();
    assert!(
        warnings.lines().any(|line| line
            == "warning: correction item \"Docent of Defiant\".name is done upstream: \"Docent of Defiance\" exists; delete it from corrections_items.toml"),
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
        "quest_augment_loot_link_count: 6",
        "drop_text_rare_augment_link_count: 1",
        "wiki_rare_augment_drop_count: 1",
        "wiki_added_quest_augment_loot_link_count: 0",
        "wiki_loot_drop_count: 0",
        "wiki_loot_augment_drop_count: 0",
        "wiki_quest_entry_count: 4",
        "wiki_quest_created_count: 1",
        "drop_text_wiki_quest_link_count: 1",
        "wiki_quest_superseded_count: 1",
        "wiki_quest_probable_duplicate_count: 0",
        "wiki_crafting_system_count: 2",
        "wiki_crafting_recipe_count: 5",
        "wiki_crafting_ingredient_count: 5",
        "wiki_description_entry_count: 4",
        "wiki_description_filled_count: 2",
        "wiki_description_skipped_count: 3",
        "wiki_description_augment_link_count: 1",
        "wiki_item_written_count: 1",
        "wiki_item_superseded_count: 1",
        "wiki_item_probable_duplicate_count: 0",
        "wiki_augment_written_count: 1",
        "wiki_augment_superseded_count: 1",
        "wiki_augment_probable_duplicate_count: 0",
        "wiki_quest_chain_count: 1",
        "quest_chain_quest_link_count: 2",
        "quest_chain_reward_count: 2",
        "wiki_saga_count: 1",
        "saga_quest_link_count: 2",
        "saga_reward_count: 3",
    ] {
        assert!(report.lines().any(|line| line == expected_line), "missing {expected_line:?} in\n{report}");
    }
    assert!(!report.contains("written_item_count"), "only wiki lines: {report}");
}

#[test]
fn wiki_check_warns_about_superseded_and_probably_duplicate_wiki_items() {
    let fixture_items = std::fs::read_to_string(fixtures_dir().join("wiki/items.toml")).unwrap();
    let draft_dir = draft_dir_with_fixture_quests();
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
fn wiki_check_warns_about_superseded_and_probably_duplicate_wiki_augments() {
    let fixture_augments = std::fs::read_to_string(fixtures_dir().join("wiki/augments.toml")).unwrap();
    let draft_dir = tempfile::tempdir().unwrap();
    std::fs::write(
        draft_dir.path().join("augments_draft.toml"),
        fixture_augments.replace("name = \"Test Gem of Oozing Resistance\"", "name = \"Storms Bulwark\""),
    )
    .unwrap();

    let report =
        wiki_check_report(&fixtures_dir().join("DataFiles"), Some(draft_dir.path()), Some(&fixture_corrections_dir()))
            .unwrap();

    let warnings_start = report.find("\nwarnings:").unwrap_or_else(|| panic!("no warnings section in\n{report}"));
    for expected_warning in [
        "warning: wiki augment \"Storm's Bulwark\" is now in Maetrim's files; delete it from augments_draft.toml",
        "warning: wiki augment \"Storms Bulwark\" may duplicate Maetrim's \"Storm's Bulwark\"",
    ] {
        assert!(
            report[warnings_start..].lines().any(|line| line == expected_warning),
            "missing {expected_warning:?} in\n{report}"
        );
    }
    assert!(
        report[..warnings_start].lines().any(|line| line == "wiki_augment_probable_duplicate_count: 1"),
        "{report}"
    );
}

#[test]
fn wiki_check_warns_about_superseded_and_probably_duplicate_wiki_quests() {
    let fixture_quests = std::fs::read_to_string(fixtures_dir().join("wiki/quests.toml")).unwrap();
    let draft_dir = tempfile::tempdir().unwrap();
    std::fs::write(
        draft_dir.path().join("quests_draft.toml"),
        fixture_quests.replace("name = \"Ghosts of Perdition\"", "name = \"The Chrono-scope\""),
    )
    .unwrap();

    let report =
        wiki_check_report(&fixtures_dir().join("DataFiles"), Some(draft_dir.path()), Some(&fixture_corrections_dir()))
            .unwrap();

    let warnings_start = report.find("\nwarnings:").unwrap_or_else(|| panic!("no warnings section in\n{report}"));
    for expected_warning in [
        "warning: wiki quest \"The Grotto\" is now in Maetrim's files; delete its quest fields from quests_draft.toml",
        "warning: wiki quest \"The Chrono-scope\" may duplicate Maetrim's \"The Chronoscope\"",
    ] {
        assert!(
            report[warnings_start..].lines().any(|line| line == expected_warning),
            "missing {expected_warning:?} in\n{report}"
        );
    }
    assert!(report[..warnings_start].lines().any(|line| line == "wiki_quest_probable_duplicate_count: 1"), "{report}");
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
        let draft_dir = draft_dir_with_fixture_quests();
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
        let draft_dir = draft_dir_with_fixture_quests();
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
fn wiki_check_warns_about_each_crafting_note_that_is_a_correction_candidate() {
    let fixture_crafting = std::fs::read_to_string(fixtures_dir().join("wiki/crafting.toml")).unwrap();
    let cleanse_note = "note = \"Returns a crafted item to its blank state; no augment counterpart.\"";
    let test_altar_note = "note = \"Test note: adds a socket, never an augment.\"";
    let report_with_notes = |cleanse_replacement: &str, test_altar_replacement: &str| {
        let draft_dir = tempfile::tempdir().unwrap();
        std::fs::write(
            draft_dir.path().join("crafting.toml"),
            fixture_crafting
                .replace(cleanse_note, cleanse_replacement)
                .replace(test_altar_note, test_altar_replacement),
        )
        .unwrap();
        wiki_check_report(&fixtures_dir().join("DataFiles"), Some(draft_dir.path()), Some(&fixture_corrections_dir()))
            .unwrap()
    };
    assert!(!report_with_notes(cleanse_note, test_altar_note).contains("correction candidate"));
    for (cleanse_text, test_altar_text) in [
        ("Test note: his value stands until a page says otherwise.", "Test note: MAETRIM SPELLS it Colourless."),
        ("Test note: Maetrim names the socket differently.", "Test note: His Augment carries a +4 the page reads +5."),
    ] {
        let report = report_with_notes(&format!("note = {cleanse_text:?}"), &format!("note = {test_altar_text:?}"));
        let (_, warnings) = report.split_once("\nwarnings:\n").unwrap_or_else(|| panic!("no warnings in\n{report}"));
        let candidate_warnings: Vec<&str> =
            warnings.lines().filter(|line| line.contains("correction candidate")).collect();
        assert_eq!(
            candidate_warnings,
            [
                format!("warning: crafting note is a correction candidate: Heroic Green Steel / Cleanse an item: {cleanse_text}"),
                format!(
                    "warning: crafting note is a correction candidate: Test Upgrade Altar / Test Colorless Augment Slot: {test_altar_text}"
                ),
            ]
        );
    }
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

    assert_eq!(line_count(&out_dir.path().join("item_names.txt")), 21);
    assert_eq!(
        std::fs::read_to_string(out_dir.path().join("wiki_source_items.txt")).unwrap(),
        "Battle Axe of the Oozing Hunger\n"
    );
    assert_eq!(
        std::fs::read_to_string(out_dir.path().join("wiki_source_quests.txt")).unwrap(),
        "Ghosts of Perdition\n"
    );
    assert_eq!(
        std::fs::read_to_string(out_dir.path().join("wiki_source_augments.txt")).unwrap(),
        "Named\tTest Gem of Oozing Resistance\t29\n"
    );
    assert_eq!(
        std::fs::read_to_string(out_dir.path().join("quest_chain_names.txt")).unwrap(),
        "The Lost Seekers\t1\n",
        "his reward segments that are no quest's own end reward and say no saga, with how many of his items name each"
    );
    assert_eq!(
        std::fs::read_to_string(out_dir.path().join("saga_names.txt")).unwrap(),
        "Masterminds of Sharn\t1\nThe Haunting of Saltmarsh\t1\n",
        "a saga's reward is listed though the saga's name holds a quest's"
    );
    let augment_lines = std::fs::read_to_string(out_dir.path().join("augment_names.txt")).unwrap();
    assert_eq!(augment_lines.lines().count(), 16);
    assert!(augment_lines.lines().any(|line| line == "Alchemical\tFire I: Combustion\t29"), "{augment_lines}");
    let quest_pages: Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.path().join("quest_pages.json")).unwrap()).unwrap();
    assert_eq!(quest_pages.as_object().unwrap().len(), 26, "Maetrim's 25 quests and challenges and the wiki quest");
    assert_eq!(quest_pages["Ghosts of Perdition"], likely_wiki_page_url("Ghosts of Perdition"));
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
