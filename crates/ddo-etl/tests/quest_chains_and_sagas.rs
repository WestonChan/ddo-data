mod fixture;

use ddo_etl::build::{build_database, BuildReport};
use ddo_etl::corrections::Corrections;
use ddo_etl::wiki::WikiOverrides;
use ddo_model::DatasetVersion;
use rusqlite::Connection;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fixture_wiki() -> WikiOverrides {
    WikiOverrides::from_database(&fixture::database(|_| {})).unwrap()
}

fn built_with(wiki: &WikiOverrides) -> Result<(Connection, BuildReport), String> {
    let mut db = Connection::open_in_memory().unwrap();
    let dataset_version = DatasetVersion { upstream_sha: "31ef0201".into(), built_at: "2026-10-02T00:00:00Z".into() };
    let report =
        build_database(&fixtures_dir().join("DataFiles"), wiki, &Corrections::default(), &mut db, &dataset_version)
            .map_err(|e| format!("{e:#}"))?;
    Ok((db, report))
}

fn strings(db: &Connection, sql: &str) -> Vec<String> {
    let mut statement = db.prepare(sql).unwrap();
    statement.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
}

#[test]
fn writes_a_wiki_quest_chain_with_its_pack_quests_in_order_and_rewards() {
    let (db, report) = built_with(&fixture_wiki()).unwrap();
    let (pack, provenance, wiki_url): (String, String, String) = db
        .query_row(
            "SELECT p.name, c.provenance, c.wiki_url FROM quest_chains c JOIN adventure_packs p ON p.id = c.pack_id
              WHERE c.name = 'The Lost Seekers'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        (pack.as_str(), provenance.as_str(), wiki_url.as_str()),
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
            "SELECT i.name || ' ' || cr.is_rare FROM sources cr JOIN items i ON i.id = cr.item_id
              WHERE cr.kind = 'quest_chain' ORDER BY i.name"
        ),
        ["Acrobat's Ring 0", "Docent of Defiance 0", "Kundarak Delving Boots 1"]
    );
    assert_eq!(
        (report.wiki_quest_chain_count, report.quest_chain_quest_link_count, report.quest_chain_reward_count),
        (1, 2, 3)
    );
}

#[test]
fn writes_a_wiki_saga_whose_rewards_carry_their_tier() {
    let (db, report) = built_with(&fixture_wiki()).unwrap();
    assert_eq!(
        strings(
            &db,
            "SELECT q.name FROM saga_quests sq JOIN quests q ON q.id = sq.quest_id JOIN sagas s ON s.id = sq.saga_id
              WHERE s.name = 'Masterminds of Sharn' ORDER BY sq.sort_order"
        ),
        ["Project Nemesis", "Ghosts of Perdition"],
        "a saga may name a quest the wiki creates"
    );
    assert_eq!(
        strings(
            &db,
            "SELECT i.name || ' ' || sr.tier || ' ' || sr.is_rare FROM sources sr
               JOIN items i ON i.id = sr.item_id JOIN sagas s ON s.id = sr.saga_id
              WHERE s.name = 'Masterminds of Sharn' AND i.provenance = 'maetrim'
              ORDER BY i.name, sr.tier"
        ),
        [
            "Alabaster of the Twelve any 0",
            "Band of Diani ir'Wynarn epic 0",
            "Five Rings epic 0",
            "Five Rings legendary 1"
        ]
    );
    assert_eq!(strings(&db, "SELECT provenance FROM sagas"), ["wiki", "wiki"]);
    assert_eq!((report.wiki_saga_count, report.saga_quest_link_count, report.saga_reward_count), (2, 4, 5));
}

#[test]
fn build_fails_naming_a_chain_quest_item_or_pack_his_files_lack() {
    for (kind, owner, missing_name) in [
        ("quest", "The Lost Seekers", "No Such Quest"),
        ("quest_chain", "The Lost Seekers", "No Such Ring"),
        ("pack", "The Lost Seekers", "No Such Pack"),
        ("saga", "Masterminds of Sharn", "No Such Ring"),
    ] {
        let ours = fixture::database(|root| {
            let chain_path = root.join("packs/free-to-play/chains/the-lost-seekers.toml");
            let mut chain: toml::Table = toml::from_str(&std::fs::read_to_string(&chain_path).unwrap()).unwrap();
            match kind {
                "quest" => {
                    let path = root.join("packs/free-to-play/quests/no-such-quest/quest.toml");
                    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                    std::fs::write(path, "name = \"No Such Quest\"\npack = \"Free to Play\"\nfavor = 0\ndifficulties = [\"normal\"]\n[[run]]\ntier = \"heroic\"\nlevel = 1\n").unwrap();
                    chain.insert("quests".into(), toml::Value::Array(vec![toml::Value::String(missing_name.into())]));
                }
                "pack" => {
                    let path = root.join("packs/no-such-pack/pack.toml");
                    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                    std::fs::write(path, "name = \"No Such Pack\"\n").unwrap();
                    chain.insert("pack".into(), toml::Value::String(missing_name.into()));
                }
                _ => {
                    let path = root.join("packs/unsourced/gear/no-such-ring.toml");
                    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                    let tier = if kind == "saga" { "tier = \"epic\"\n" } else { "" };
                    std::fs::write(path, format!(
                        "name = \"No Such Ring\"\nslot = \"Ring\"\nitem_category = \"Jewelry\"\nminimum_level = 1\n[[source]]\nkind = {kind:?}\nname = {owner:?}\n{tier}"
                    )).unwrap();
                }
            }
            std::fs::write(chain_path, toml::to_string(&chain).unwrap()).unwrap();
        });
        let wiki = WikiOverrides::from_database(&ours).unwrap();
        let owner_file = if kind == "saga" {
            &wiki.sagas.iter().find(|saga| saga.name == owner).unwrap().file_name
        } else {
            &wiki.quest_chains.iter().find(|chain| chain.name == owner).unwrap().file_name
        };
        let error = built_with(&wiki).map(|_| ()).unwrap_err();
        assert!(error.contains(missing_name) && error.contains(owner) && error.contains(owner_file), "{error}");
    }
}

fn reward_rows(db: &Connection, sql: &str) -> Vec<String> {
    strings(db, sql)
}

#[test]
fn links_his_items_to_the_quest_chain_their_drop_text_credits_with_its_end_reward() {
    let (db, report) = built_with(&fixture_wiki()).unwrap();
    assert_eq!(
        reward_rows(
            &db,
            "SELECT c.name || ' / ' || i.name || ' ' || cr.is_rare FROM sources cr
               JOIN quest_chains c ON c.id = cr.chain_id JOIN items i ON i.id = cr.item_id
              WHERE i.name = 'Acrobat''s Ring'"
        ),
        ["The Lost Seekers / Acrobat's Ring 0"],
        "'The Lost Seekers, End reward'"
    );
    assert_eq!(report.drop_text_quest_chain_reward_count, 1);
}

#[test]
fn links_his_items_to_the_saga_and_tier_their_drop_text_credits() {
    let (db, report) = built_with(&fixture_wiki()).unwrap();
    assert_eq!(
        reward_rows(
            &db,
            "SELECT s.name || ' / ' || i.name || ' ' || sr.tier FROM sources sr
               JOIN sagas s ON s.id = sr.saga_id JOIN items i ON i.id = sr.item_id
              WHERE i.name IN ('Band of Diani ir''Wynarn', 'Ring of the Kraken') ORDER BY i.name"
        ),
        ["Masterminds of Sharn / Band of Diani ir'Wynarn epic", "The Haunting of Saltmarsh / Ring of the Kraken epic"],
        "'Masterminds of Sharn saga: Epic end reward' and 'The Haunting of Saltmarsh (Epic) saga end reward'"
    );
    assert_eq!(report.drop_text_saga_reward_count, 2);
    assert_eq!(
        reward_rows(
            &db,
            "SELECT q.name || ' ' || ql.loot_type FROM sources ql JOIN quests q ON q.id = ql.quest_id
               JOIN items i ON i.id = ql.item_id WHERE i.name = 'Ring of the Kraken'"
        ),
        ["Saltmarsh chest"],
        "the quest a saga's name holds keeps only its own chest link"
    );
}

#[test]
fn leaves_reward_text_naming_no_recorded_chain_or_saga_unlinked() {
    let (db, report) = built_with(&WikiOverrides::default()).unwrap();
    assert_eq!(strings(&db, "SELECT COUNT(*) || '' FROM sources WHERE kind = 'quest_chain'"), ["0"]);
    assert_eq!(strings(&db, "SELECT COUNT(*) || '' FROM sources WHERE kind = 'saga'"), ["0"]);
    assert_eq!((report.drop_text_quest_chain_reward_count, report.drop_text_saga_reward_count), (0, 0));
}

#[test]
fn gives_a_quest_chain_reward_no_tier_when_its_segment_names_one() {
    let ours = fixture::database(|root| {
        let path = root.join("packs/shared/chains/masterminds-of-sharn.toml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            r#"
name = "Masterminds of Sharn"
wiki_url = "https://ddowiki.com/page/Masterminds_of_Sharn"
quests = ["The Grotto"]
[[citation]]
url = "https://ddowiki.com/page/Masterminds_of_Sharn"
read = "2026-10-02"
"#,
        )
        .unwrap();
    });
    let mut wiki = WikiOverrides::from_database(&ours).unwrap();
    wiki.quest_chains.retain(|chain| chain.name == "Masterminds of Sharn");
    let (db, report) =
        built_with(&WikiOverrides { quest_chains: wiki.quest_chains, quests: wiki.quests, ..WikiOverrides::default() })
            .unwrap();
    assert_eq!(
        strings(
            &db,
            "SELECT i.name || ' ' || COALESCE(cr.tier, '-') FROM sources cr JOIN items i ON i.id = cr.item_id
              WHERE cr.kind = 'quest_chain'"
        ),
        ["Band of Diani ir'Wynarn -"],
        "'Masterminds of Sharn saga: Epic end reward' names a tier only a saga reward carries"
    );
    assert_eq!(report.drop_text_quest_chain_reward_count, 1);
}
