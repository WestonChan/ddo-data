use ddo_etl::build::{build_database, BuildReport};
use ddo_etl::corrections::Corrections;
use ddo_etl::wiki::WikiOverrides;
use ddo_model::DatasetVersion;
use rusqlite::Connection;
use std::path::PathBuf;

const LOST_SEEKERS_CITATION: &str =
    "[[chain]]\nname = \"The Lost Seekers\"\npage = \"https://ddowiki.com/page/The_Lost_Seekers\"\nread = \"2026-10-02\"\n";
const SHARN_SAGA_CITATION: &str = "[[saga]]\nname = \"Masterminds of Sharn\"\n\
     page = \"https://ddowiki.com/page/Masterminds_of_Sharn_(saga)\"\nread = \"2026-10-02\"\n";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fixture_wiki() -> WikiOverrides {
    WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap()
}

fn parsed_wiki(files: &[(&str, &str)]) -> Result<WikiOverrides, String> {
    WikiOverrides::from_toml_files(files).map_err(|e| format!("{e:#}"))
}

fn built_with(wiki: &WikiOverrides) -> Result<(Connection, BuildReport), String> {
    let mut db = Connection::open_in_memory().unwrap();
    let dataset_version = DatasetVersion { upstream_sha: "31ef0201".into(), built_at: "2026-10-02T00:00:00Z".into() };
    let report =
        build_database(&fixtures_dir().join("DataFiles"), wiki, &Corrections::default(), &mut db, &dataset_version)
            .map_err(|e| format!("{e:#}"))?;
    Ok((db, report))
}

fn built_with_files(files: &[(&str, &str)]) -> Result<(Connection, BuildReport), String> {
    let mut toml_files = vec![("quests.toml", include_str!("fixtures/wiki/quests.toml"))];
    toml_files.extend_from_slice(files);
    built_with(&parsed_wiki(&toml_files)?)
}

fn strings(db: &Connection, sql: &str) -> Vec<String> {
    let mut statement = db.prepare(sql).unwrap();
    statement.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
}

#[test]
fn writes_a_wiki_quest_chain_with_its_pack_quests_in_order_and_rewards() {
    let (db, report) = built_with(&fixture_wiki()).unwrap();
    let (pack, source, wiki_url): (String, String, String) = db
        .query_row(
            "SELECT p.name, c.source, c.wiki_url FROM quest_chains c JOIN adventure_packs p ON p.id = c.pack_id
              WHERE c.name = 'The Lost Seekers'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        (pack.as_str(), source.as_str(), wiki_url.as_str()),
        ("Free to Play", "wiki", "https://ddowiki.com/page/The_Lost_Seekers")
    );
    assert_eq!(
        strings(
            &db,
            "SELECT q.name FROM quest_chain_quests cq JOIN quests q ON q.id = cq.quest_id ORDER BY cq.sort_order"
        ),
        ["The Grotto", "Redemption"]
    );
    assert_eq!(
        strings(
            &db,
            "SELECT i.name || ' ' || cr.is_rare FROM quest_chain_rewards cr JOIN items i ON i.id = cr.item_id
              ORDER BY i.name"
        ),
        ["Docent of Defiance 0", "Kundarak Delving Boots 1"]
    );
    assert_eq!(
        (report.wiki_quest_chain_count, report.quest_chain_quest_link_count, report.quest_chain_reward_count),
        (1, 2, 2)
    );
}

#[test]
fn writes_a_wiki_saga_whose_rewards_carry_their_tier() {
    let (db, report) = built_with(&fixture_wiki()).unwrap();
    assert_eq!(
        strings(&db, "SELECT q.name FROM saga_quests sq JOIN quests q ON q.id = sq.quest_id ORDER BY sq.sort_order"),
        ["Project Nemesis", "Ghosts of Perdition"],
        "a saga may name a quest the wiki creates"
    );
    assert_eq!(
        strings(
            &db,
            "SELECT i.name || ' ' || COALESCE(sr.tier, '-') || ' ' || sr.is_rare FROM saga_rewards sr
               JOIN items i ON i.id = sr.item_id ORDER BY i.name, sr.tier"
        ),
        ["Alabaster of the Twelve - 0", "Five Rings epic 0", "Five Rings legendary 1"]
    );
    assert_eq!(strings(&db, "SELECT source FROM sagas"), ["wiki"]);
    assert_eq!((report.wiki_saga_count, report.saga_quest_link_count, report.saga_reward_count), (1, 2, 3));
}

#[test]
fn reads_chains_and_sagas_only_from_files_named_for_them() {
    let wiki = fixture_wiki();
    assert_eq!(wiki.quest_chains.len(), 1);
    assert_eq!(wiki.sagas.len(), 1);
    assert_eq!(wiki.quest_chains[0].file_name, "quest_chains.toml");
    let error = parsed_wiki(&[("quest_chains.toml", SHARN_SAGA_CITATION)]).unwrap_err();
    assert!(error.contains("[[chain]]"), "{error}");
}

#[test]
fn rejects_a_chain_named_twice_across_files() {
    let error =
        parsed_wiki(&[("quest_chains.toml", LOST_SEEKERS_CITATION), ("quest_chains_b.toml", LOST_SEEKERS_CITATION)])
            .unwrap_err();
    assert!(error.contains("The Lost Seekers") && error.contains("quest_chains.toml"), "{error}");
}

#[test]
fn rejects_a_quest_listed_twice_in_one_chain() {
    let toml_text = format!("{LOST_SEEKERS_CITATION}quests = [\"The Grotto\", \"The Grotto\"]\n");
    let error = parsed_wiki(&[("quest_chains.toml", &toml_text)]).unwrap_err();
    assert!(error.contains("The Grotto") && error.contains("twice"), "{error}");
}

#[test]
fn rejects_a_saga_reward_tier_outside_heroic_epic_and_legendary() {
    let toml_text = format!("{SHARN_SAGA_CITATION}rewards = [{{ name = \"Five Rings\", tier = \"mythic\" }}]\n");
    let error = parsed_wiki(&[("sagas.toml", &toml_text)]).unwrap_err();
    assert!(error.contains("mythic") && error.contains("heroic, epic, legendary"), "{error}");
}

#[test]
fn rejects_a_saga_reward_listed_twice_at_one_tier() {
    let toml_text = format!(
        "{SHARN_SAGA_CITATION}rewards = [{{ name = \"Five Rings\", tier = \"epic\" }}, \
         {{ name = \"Five Rings\", tier = \"epic\", rare = true }}]\n"
    );
    let error = parsed_wiki(&[("sagas.toml", &toml_text)]).unwrap_err();
    assert!(error.contains("Five Rings") && error.contains("twice"), "{error}");
}

#[test]
fn rejects_a_tier_on_a_quest_chain_reward() {
    let toml_text = format!("{LOST_SEEKERS_CITATION}rewards = [{{ name = \"Five Rings\", tier = \"epic\" }}]\n");
    assert!(parsed_wiki(&[("quest_chains.toml", &toml_text)]).is_err(), "a chain reward takes name and rare");
}

#[test]
fn build_fails_naming_a_chain_quest_item_or_pack_his_files_lack() {
    for (field_line, missing_name) in [
        ("quests = [\"No Such Quest\"]\n", "No Such Quest"),
        ("rewards = [\"No Such Ring\"]\n", "No Such Ring"),
        ("pack = \"No Such Pack\"\n", "No Such Pack"),
    ] {
        let toml_text = format!("{LOST_SEEKERS_CITATION}{field_line}");
        let error = built_with_files(&[("quest_chains.toml", &toml_text)]).map(|_| ()).unwrap_err();
        assert!(error.contains(missing_name) && error.contains("The Lost Seekers"), "{error}");
    }
    let toml_text = format!("{SHARN_SAGA_CITATION}rewards = [{{ name = \"No Such Ring\", tier = \"epic\" }}]\n");
    let error = built_with_files(&[("sagas.toml", &toml_text)]).map(|_| ()).unwrap_err();
    assert!(error.contains("No Such Ring") && error.contains("Masterminds of Sharn"), "{error}");
}
