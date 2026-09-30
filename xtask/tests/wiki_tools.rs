use serde_json::Value;
use std::path::{Path, PathBuf};
use xtask::wiki_tools::{likely_wiki_page_url, wiki_check_report, write_wiki_batch};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/ddo-etl/tests/fixtures")
}

fn line_count(path: &Path) -> usize {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display())).lines().count()
}

#[test]
fn wiki_check_reports_the_wiki_counts_for_a_valid_wiki_dir() {
    let report = wiki_check_report(&fixtures_dir().join("DataFiles"), Some(&fixtures_dir().join("wiki"))).unwrap();

    for expected_line in [
        "wiki_quest_loot_entry_count: 1",
        "wiki_quest_entry_count: 3",
        "wiki_crafting_system_count: 2",
        "wiki_crafting_recipe_count: 5",
        "wiki_crafting_ingredient_count: 5",
    ] {
        assert!(report.lines().any(|line| line == expected_line), "missing {expected_line:?} in\n{report}");
    }
    assert!(!report.contains("written_item_count"), "only wiki lines: {report}");
}

#[test]
fn wiki_check_fails_naming_an_unknown_quest() {
    let draft_dir = tempfile::tempdir().unwrap();
    std::fs::write(
        draft_dir.path().join("quests.toml"),
        "[[quest]]\nname = \"No Such Quest\"\npage = \"https://ddowiki.com/page/No_Such_Quest\"\nread = \"2026-09-29\"\nfree_to_play = true\n",
    )
    .unwrap();

    let error = wiki_check_report(&fixtures_dir().join("DataFiles"), Some(draft_dir.path())).unwrap_err();

    assert!(format!("{error:#}").contains("No Such Quest"), "{error:#}");
}

#[test]
fn wiki_batch_writes_the_reading_agent_inputs() {
    let out_dir = tempfile::tempdir().unwrap();

    write_wiki_batch(&fixtures_dir().join("DataFiles"), Some(&fixtures_dir().join("wiki")), out_dir.path()).unwrap();

    assert_eq!(line_count(&out_dir.path().join("item_names.txt")), 15);
    let augment_lines = std::fs::read_to_string(out_dir.path().join("augment_names.txt")).unwrap();
    assert_eq!(augment_lines.lines().count(), 10);
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
