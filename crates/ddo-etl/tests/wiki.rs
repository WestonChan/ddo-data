use ddo_etl::build::build;
use ddo_etl::wiki::WikiOverrides;
use ddo_model::DatasetVersion;
use rusqlite::Connection;
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn version() -> DatasetVersion {
    DatasetVersion { upstream_sha: "31ef0201".into(), built_at: "2026-09-20T00:00:00Z".into() }
}

fn quest(name: &str, rare: &[&str]) -> String {
    let rare: Vec<String> = rare.iter().map(|r| format!("{r:?}")).collect();
    format!(
        "[[quest]]\nname = {name:?}\npage = \"https://ddowiki.com/page/{}\"\nread = \"2026-09-27\"\nrare = [{}]\n",
        name.replace(' ', "_"),
        rare.join(", ")
    )
}

fn load(files: &[(&str, &str)]) -> Result<WikiOverrides, String> {
    WikiOverrides::from_sources(files).map_err(|e| format!("{e:#}"))
}

fn build_with(wiki: &WikiOverrides) -> Result<ddo_etl::build::BuildReport, String> {
    let mut conn = Connection::open_in_memory().unwrap();
    build(&fixtures().join("DataFiles"), wiki, &mut conn, &version()).map_err(|e| format!("{e:#}"))
}

#[test]
fn reads_quest_loot_from_every_toml_file_in_the_directory() {
    let wiki = WikiOverrides::from_dir(&fixtures().join("wiki")).unwrap();
    assert_eq!(wiki.quest_loot.len(), 1);
    let entry = &wiki.quest_loot[0];
    assert_eq!(entry.name, "Book Burning");
    assert_eq!(entry.page, "https://ddowiki.com/page/Book_Burning");
    assert_eq!(entry.read, "2026-09-27");
    assert_eq!(entry.rare, vec!["Buckler of the Golden Age".to_string()]);
}

#[test]
fn embedded_wiki_files_load() {
    let wiki = WikiOverrides::embedded().unwrap();
    assert!(wiki.quest_loot.iter().any(|q| q.name == "Book Burning"));
}

#[test]
fn rejects_a_page_outside_ddowiki() {
    let src = quest("The Grotto", &[]).replace("https://ddowiki.com/page/", "https://example.com/");
    let err = load(&[("a.toml", &src)]).unwrap_err();
    assert!(err.contains("The Grotto") && err.contains("https://ddowiki.com/page/"), "{err}");
}

#[test]
fn rejects_a_read_date_that_is_not_iso() {
    for bad in ["27/09/2026", "2026-9-27", "2026-13-01", "2026-02-30", "yesterday"] {
        let src = quest("The Grotto", &[]).replace("2026-09-27", bad);
        let err = load(&[("a.toml", &src)]).unwrap_err();
        assert!(err.contains("The Grotto") && err.contains(bad), "{bad}: {err}");
    }
}

#[test]
fn rejects_a_quest_listed_twice_across_files() {
    let err = load(&[("a.toml", &quest("The Grotto", &[])), ("b.toml", &quest("The Grotto", &[]))]).unwrap_err();
    assert!(err.contains("The Grotto") && err.contains("a.toml") && err.contains("b.toml"), "{err}");
}

#[test]
fn rejects_unknown_fields() {
    let src = quest("The Grotto", &[]) + "common = [\"Docent of Defiance\"]\n";
    let err = load(&[("a.toml", &src)]).unwrap_err();
    assert!(err.contains("a.toml") && err.contains("common"), "{err}");
}

#[test]
fn build_fails_naming_a_quest_absent_from_the_quests_table() {
    let wiki = load(&[("a.toml", &quest("The Missing Quest", &[]))]).unwrap();
    let err = build_with(&wiki).unwrap_err();
    assert!(err.contains("The Missing Quest"), "{err}");
}

#[test]
fn build_fails_naming_an_item_absent_from_the_items_table() {
    let wiki = load(&[("a.toml", &quest("Book Burning", &["Buckler of the Missing Age"]))]).unwrap();
    let err = build_with(&wiki).unwrap_err();
    assert!(err.contains("Buckler of the Missing Age") && err.contains("Book Burning"), "{err}");
}

#[test]
fn build_reports_the_wiki_entries_it_applied() {
    let wiki = WikiOverrides::from_dir(&fixtures().join("wiki")).unwrap();
    let report = build_with(&wiki).unwrap();
    assert_eq!(report.wiki_quest_loot_entries, 1);
    assert_eq!(report.wiki_rare_drops, 1);
}
