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

fn built_with_files(files: &[(&str, &str)]) -> (Connection, BuildReport) {
    built_with(&WikiOverrides::from_toml_files(files).unwrap())
}

fn drop_rows(db: &Connection, loot_name: &str) -> Vec<String> {
    let mut statement = db
        .prepare(
            "SELECT d.source_kind || ' ' || COALESCE(q.name, c.name, s.name, p.name) || ' ' || COALESCE(d.loot_type, '-')
                    || ' ' || COALESCE(d.chest, '-') || ' ' || d.is_rare
               FROM drops d LEFT JOIN quests q ON q.id = d.quest_id LEFT JOIN quest_chains c ON c.id = d.chain_id
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
    let quests_text = "# Test values, not read from ddowiki: a quest whose name lies inside the pack name Magic of Myth Drannor.\n\
         [[quest]]\nname = \"Myth Drannor\"\npage = \"https://ddowiki.com/page/Myth_Drannor\"\nread = \"2026-10-02\"\n\
         free_to_play = false\npack = \"Magic of Myth Drannor\"\nlevel = 30\nfavor = 0\nis_raid = false\ndifficulties = [\"normal\"]\n";
    let (db, report) = built_with_files(&[("quests.toml", quests_text)]);
    assert_eq!(drop_rows(&db, GOLDEN_AGE_CROSSBOW), ["adventure_pack Magic of Myth Drannor chest any end chest 1"]);
    assert_eq!(report.pack_loot_link_count, 1);
}

#[test]
fn leaves_a_saga_end_reward_naming_a_pack_to_the_saga() {
    let (db, _) = built_with(&WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap());
    assert!(
        drop_rows(&db, "Band of Diani ir'Wynarn").iter().all(|row| !row.starts_with("adventure_pack")),
        "'Masterminds of Sharn saga: Epic end reward' names the pack Masterminds of Sharn too: {:?}",
        drop_rows(&db, "Band of Diani ir'Wynarn")
    );
}

#[test]
fn links_an_augment_whose_drop_text_names_a_pack_to_the_pack() {
    let augments_text = "# Test values, not read from ddowiki: an augment whose drop line names only a pack.\n\
         [[augment]]\nname = \"Test Gem of Myth Drannor\"\npage = \"https://ddowiki.com/page/Item:Test_Gem_of_Myth_Drannor\"\n\
         read = \"2026-10-02\"\nfamily = \"Named\"\nmin_level = 20\nslots = [\"green\"]\n\
         description = \"Test description.\\nDrops in: Magic of Myth Drannor, any end chest\"\n";
    let (db, report) = built_with_files(&[("augments.toml", augments_text)]);
    assert_eq!(
        drop_rows(&db, "Test Gem of Myth Drannor"),
        ["adventure_pack Magic of Myth Drannor chest any end chest 0"]
    );
    assert_eq!(report.pack_augment_loot_link_count, 1);
}

#[test]
fn leaves_a_segment_naming_a_saga_reward_tier_unlinked_to_the_pack() {
    let augments_text =
        "# Test values, not read from ddowiki: an augment whose drop line is Maetrim's wording for a saga reward.\n\
         [[augment]]\nname = \"Test Gem of the Saga\"\npage = \"https://ddowiki.com/page/Item:Test_Gem_of_the_Saga\"\n\
         read = \"2026-10-02\"\nfamily = \"Named\"\nmin_level = 20\nslots = [\"green\"]\n\
         description = \"Test description.\\nDrops in: Magic of Myth Drannor (Heroic), Elite and True Elite\"\n";
    let (db, report) = built_with_files(&[("augments.toml", augments_text)]);
    assert!(drop_rows(&db, "Test Gem of the Saga").is_empty(), "True Elite is a saga's reward list");
    assert_eq!(report.pack_augment_loot_link_count, 0);
}

fn built_with_augment_dropping_in(augment_name: &str, drop_line: &str) -> (Connection, BuildReport) {
    let augments_text = format!(
        "# Test values, not read from ddowiki.\n[[augment]]\nname = \"{augment_name}\"\n\
         page = \"https://ddowiki.com/page/Item:Test_Gem\"\nread = \"2026-10-02\"\nfamily = \"Named\"\n\
         min_level = 20\nslots = [\"green\"]\ndescription = \"Test description.\\nDrops in: {drop_line}\"\n"
    );
    let wiki_files = [
        ("augments.toml", augments_text.as_str()),
        ("quests.toml", include_str!("fixtures/wiki/quests.toml")),
        ("sagas.toml", include_str!("fixtures/wiki/sagas.toml")),
    ];
    built_with_files(&wiki_files)
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
    let augments_text =
        "# Test values, not read from ddowiki: a saga whose name holds a pack's, as a renamed pack's would.\n\
         [[augment]]\nname = \"Test Gem of the Long Saga\"\npage = \"https://ddowiki.com/page/Item:Test_Gem\"\n\
         read = \"2026-10-02\"\nfamily = \"Named\"\nmin_level = 20\nslots = [\"green\"]\n\
         description = \"Test description.\\nDrops in: The Magic of Myth Drannor (Legendary), any difficulty\"\n";
    let sagas_text = "# Test values, not read from ddowiki.\n[[saga]]\nname = \"The Magic of Myth Drannor\"\n\
         page = \"https://ddowiki.com/page/The_Magic_of_Myth_Drannor\"\nread = \"2026-10-02\"\nquests = [\"Book Burning\"]\n";
    let (db, report) = built_with_files(&[("augments.toml", augments_text), ("sagas.toml", sagas_text)]);
    assert!(drop_rows(&db, "Test Gem of the Long Saga").is_empty());
    assert_eq!(report.pack_augment_loot_link_count, 0);
}
