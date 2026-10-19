mod fixture;

use ddo_etl::build::{build_database, BuildReport};
use ddo_etl::corrections::Corrections;
use ddo_etl::map::drop_location::names_store_purchase;
use ddo_etl::wiki::WikiOverrides;
use ddo_model::DatasetVersion;
use rusqlite::Connection;
use std::path::PathBuf;

const GOLDEN_AGE_CROSSBOW: &str = "Light Crossbow of the Golden Age";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn built_with(wiki: &WikiOverrides) -> (Connection, BuildReport) {
    let mut db = Connection::open_in_memory().unwrap();
    let dataset_version = DatasetVersion { upstream_sha: "31ef0201".into(), built_at: "2026-10-02T00:00:00Z".into() };
    let report =
        build_database(&fixtures_dir().join("DataFiles"), wiki, &Corrections::default(), &mut db, &dataset_version)
            .unwrap();
    (db, report)
}

fn wiki_with_augment(name: &str, drop_line: &str, edit: impl FnOnce(&std::path::Path)) -> WikiOverrides {
    let ours = fixture::database(|root| {
        let path = root.join(format!("packs/shared/augments/named/{}.toml", ddo_etl::tree::slug(name)));
        let description = format!("Test description.\nDrops in: {drop_line}");
        std::fs::write(path, format!(
            "name = {name:?}\nis_wiki_only = true\nfamily = \"Named\"\nmin_level = 20\ndescription = {description:?}\n[[slot]]\nslot = \"green\"\n"
        )).unwrap();
        edit(root);
    });
    let mut wiki = WikiOverrides::from_database(&ours).unwrap();
    wiki.augments.retain(|augment| augment.name == name);
    WikiOverrides { augments: wiki.augments, quests: wiki.quests, sagas: wiki.sagas, ..WikiOverrides::default() }
}

fn drop_rows(db: &Connection, loot_name: &str) -> Vec<String> {
    let mut statement = db
        .prepare(
            "SELECT d.kind || ' ' || COALESCE(q.name, c.name, s.name, p.name) || ' ' || COALESCE(d.loot_type, '-')
                    || ' ' || COALESCE(d.chest, '-') || ' ' || d.is_rare
               FROM sources d LEFT JOIN quests q ON q.id = d.quest_id LEFT JOIN quest_chains c ON c.id = d.chain_id
               LEFT JOIN sagas s ON s.id = d.saga_id LEFT JOIN adventure_packs p ON p.id = d.pack_id
               LEFT JOIN items i ON i.id = d.item_id LEFT JOIN augments a ON a.id = d.augment_id
              WHERE COALESCE(i.name, a.name) = ?1 ORDER BY 1",
        )
        .unwrap();
    statement.query_map([loot_name], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
}

#[test]
fn links_an_item_whose_drop_text_names_a_pack_and_no_quest_to_the_pack() {
    let (db, report) = built_with(&WikiOverrides::default());
    assert_eq!(
        drop_rows(&db, GOLDEN_AGE_CROSSBOW),
        ["adventure_pack Magic of Myth Drannor chest any end chest 1"],
        "'Magic of Myth Drannor, rare drop in any end chest'"
    );
    assert_eq!((report.pack_loot_link_count, report.pack_augment_loot_link_count), (1, 0));
}

#[test]
fn gives_a_segment_to_the_pack_whose_longer_name_holds_a_quest_name() {
    let ours = fixture::database(|root| {
        let path = root.join("packs/magic-of-myth-drannor/quests/myth-drannor/quest.toml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            r#"
name = "Myth Drannor"
is_wiki_only = true
pack = "Magic of Myth Drannor"
favor = 0
difficulties = ["normal"]
[[run]]
tier = "heroic"
level = 30
"#,
        )
        .unwrap();
    });
    let mut wiki = WikiOverrides::from_database(&ours).unwrap();
    wiki.quests.retain(|quest| quest.name == "Myth Drannor");
    let (db, report) = built_with(&WikiOverrides { quests: wiki.quests, ..WikiOverrides::default() });
    assert_eq!(drop_rows(&db, GOLDEN_AGE_CROSSBOW), ["adventure_pack Magic of Myth Drannor chest any end chest 1"]);
    assert_eq!(report.pack_loot_link_count, 1);
}

#[test]
fn leaves_a_saga_end_reward_naming_a_pack_to_the_saga() {
    let (db, _) = built_with(&WikiOverrides::from_database(&fixture::database(|_| {})).unwrap());
    assert!(
        drop_rows(&db, "Band of Diani ir'Wynarn").iter().all(|row| !row.starts_with("adventure_pack")),
        "'Masterminds of Sharn saga: Epic end reward' names the pack Masterminds of Sharn too: {:?}",
        drop_rows(&db, "Band of Diani ir'Wynarn")
    );
}

#[test]
fn links_an_augment_whose_drop_text_names_a_pack_to_the_pack() {
    let (db, report) =
        built_with_augment_dropping_in("Test Gem of Myth Drannor", "Magic of Myth Drannor, any end chest");
    assert_eq!(
        drop_rows(&db, "Test Gem of Myth Drannor"),
        ["adventure_pack Magic of Myth Drannor chest any end chest 0"]
    );
    assert_eq!(report.pack_augment_loot_link_count, 1);
}

#[test]
fn leaves_a_segment_naming_a_saga_reward_tier_unlinked_to_the_pack() {
    let (db, report) =
        built_with_augment_dropping_in("Test Gem of the Saga", "Magic of Myth Drannor (Heroic), Elite and True Elite");
    assert!(drop_rows(&db, "Test Gem of the Saga").is_empty(), "True Elite is a saga's reward list");
    assert_eq!(report.pack_augment_loot_link_count, 0);
}

fn built_with_augment_dropping_in(augment_name: &str, drop_line: &str) -> (Connection, BuildReport) {
    built_with(&wiki_with_augment(augment_name, drop_line, |_| {}))
}

#[test]
fn leaves_a_saga_tier_after_a_name_both_a_saga_and_a_pack_carry_to_the_saga() {
    let (db, report) =
        built_with_augment_dropping_in("Test Gem of Sharn", "Masterminds of Sharn (Epic), any difficulty");
    assert!(drop_rows(&db, "Test Gem of Sharn").is_empty(), "his wording for a saga's epic reward list");
    assert_eq!(report.pack_augment_loot_link_count, 0);
    let (db, _) = built_with_augment_dropping_in("Test Gem of the Cove", "Magic of Myth Drannor (Heroic), end chest");
    assert_eq!(
        drop_rows(&db, "Test Gem of the Cove"),
        ["adventure_pack Magic of Myth Drannor chest end chest 0"],
        "no saga carries the pack's name in the fixture wiki"
    );
}

#[test]
fn recognises_a_store_purchase_as_no_drop() {
    for segment in [
        "DDO Store, Keep on the Borderlands Bonus Items Pack",
        "Terror of Demogorgon, Collector's Edition, Ultimate Fan Bundle",
        "Magic of Myth Drannor, Ultimate Fan Bundle",
    ] {
        assert!(names_store_purchase(segment), "{segment}");
    }
    assert!(!names_store_purchase("Magic of Myth Drannor, any end chest"));
    let (db, report) =
        built_with_augment_dropping_in("Test Gem of the Store", "Magic of Myth Drannor, Ultimate Fan Bundle");
    assert!(drop_rows(&db, "Test Gem of the Store").is_empty());
    assert_eq!(report.pack_augment_loot_link_count, 0);
}

#[test]
fn leaves_a_saga_tier_after_a_saga_name_holding_a_pack_name_to_the_saga() {
    let wiki = wiki_with_augment(
        "Test Gem of the Long Saga",
        "The Magic of Myth Drannor (Legendary), any difficulty",
        |root| {
            let path = root.join("packs/shared/sagas/the-magic-of-myth-drannor.toml");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(
                path,
                r#"
name = "The Magic of Myth Drannor"
wiki_url = "https://ddowiki.com/page/The_Magic_of_Myth_Drannor"
quests = ["Book Burning"]
[[citation]]
url = "https://ddowiki.com/page/The_Magic_of_Myth_Drannor"
read = "2026-10-02"
"#,
            )
            .unwrap();
        },
    );
    let (db, report) = built_with(&wiki);
    assert!(drop_rows(&db, "Test Gem of the Long Saga").is_empty());
    assert_eq!(report.pack_augment_loot_link_count, 0);
}

#[test]
fn links_a_wiki_item_whose_drop_text_names_a_pack_a_saga_shares_to_the_pack() {
    let ours = fixture::database(|root| {
        let path = root.join("packs/unsourced/gear/test-ring-of-sharn.toml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            r#"
name = "Test Ring of Sharn"
is_wiki_only = true
slot = "Ring"
item_category = "Jewelry"
minimum_level = 30
drop_location = "Masterminds of Sharn, any end chest, most optional chests"
"#,
        )
        .unwrap();
    });
    let mut wiki = WikiOverrides::from_database(&ours).unwrap();
    wiki.items.retain(|item| item.name == "Test Ring of Sharn");
    let (db, report) = built_with(&WikiOverrides {
        items: wiki.items,
        quests: wiki.quests,
        sagas: wiki.sagas,
        ..WikiOverrides::default()
    });
    assert_eq!(
        drop_rows(&db, "Test Ring of Sharn"),
        ["adventure_pack Masterminds of Sharn chest any end chest, most optional chests 0"],
        "a wiki item's drop text names the pack as his items' does"
    );
    assert_eq!(report.pack_loot_link_count, 2, "the Golden Age crossbow's link and this one");
}
