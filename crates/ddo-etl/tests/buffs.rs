use ddo_etl::build::build_database;
use ddo_etl::corrections::Corrections;
use ddo_etl::wiki::WikiOverrides;
use ddo_etl::xml::{guild_buffs, optional_buffs};
use ddo_model::DatasetVersion;
use rusqlite::{params, Connection};
use std::path::PathBuf;

fn data_files_fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataFiles")
}

fn built_fixture_db() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    let dataset_version = DatasetVersion { upstream_sha: "test".into(), built_at: "2026-09-27T00:00:00Z".into() };
    build_database(
        &data_files_fixture_dir(),
        &WikiOverrides::from_dir(&data_files_fixture_dir().parent().unwrap().join("wiki")).unwrap(),
        &Corrections::default(),
        &mut db,
        &dataset_version,
    )
    .expect("build succeeds on fixtures");
    db
}

fn modifier_rows(db: &Connection, source_kind: &str, source_id: i64) -> Vec<(String, Option<String>, Option<String>)> {
    db.prepare(
        "SELECT m.effect_type, bt.name, m.amounts FROM modifiers m LEFT JOIN bonus_types bt ON bt.id = m.bonus_type_id
          WHERE m.source_kind = ?1 AND m.source_id = ?2 ORDER BY m.sort_order",
    )
    .unwrap()
    .query_map(params![source_kind, source_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

#[test]
fn parses_guild_buffs() {
    let parsed_buffs = guild_buffs::parse(&data_files_fixture_dir().join("GuildBuffs.xml")).unwrap();
    let names: Vec<&str> = parsed_buffs.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["Sign of the Silver Flame I", "Old Sully's Grog Cellar", "Tactical Training Room"]);
    let flame = &parsed_buffs[0];
    assert_eq!(flame.guild_level, Some(10));
    let resistance = &flame.effects[0];
    assert_eq!(resistance.amount_type.as_deref(), Some("TotalLevel"));
    assert_eq!(resistance.amounts.len(), 40, "one value per character level");
    assert_eq!((resistance.amounts[0], resistance.amounts[10], resistance.amounts[39]), (5.0, 10.0, 15.0));
    assert_eq!(parsed_buffs[2].effects.len(), 3);
    assert!(parsed_buffs[2].description.as_deref().unwrap().contains("Slicing blow"));
}

#[test]
fn writes_guild_buffs_with_per_level_modifiers() {
    let db = built_fixture_db();
    let (id, level): (i64, i64) = db
        .query_row("SELECT id, guild_level FROM guild_buffs WHERE name = 'Tactical Training Room'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(level, 21);
    let types: Vec<String> = modifier_rows(&db, "guild_buff", id).into_iter().map(|m| m.0).collect();
    assert_eq!(types, ["Weapon_DamageCritical", "TacticalDC", "Weapon_Attack"]);

    let sully: i64 =
        db.query_row("SELECT id FROM guild_buffs WHERE name = 'Old Sully''s Grog Cellar'", [], |r| r.get(0)).unwrap();
    assert_eq!(
        modifier_rows(&db, "guild_buff", sully),
        [("AbilityBonus".to_string(), Some("Guild".to_string()), Some("[2]".to_string()))]
    );
    assert_eq!(db.query_row("SELECT COUNT(*) FROM guild_buffs", [], |r| r.get::<_, i64>(0)).unwrap(), 3);
}

#[test]
fn parses_optional_buffs_across_group_comments() {
    let parsed_buffs = optional_buffs::parse(&data_files_fixture_dir().join("SelfAndPartyBuffs.xml")).unwrap();
    let names: Vec<&str> = parsed_buffs.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(
        names,
        ["Barkskin", "Bless", "Deadly Weapons", "Stone of Change: Alchemical Shield Eldritch Ritual"],
        "the <!--Spell Buffs--> style group comments are skipped"
    );
    assert_eq!(parsed_buffs[2].icon.as_deref(), Some("DeadlyWeapons"), "Icon may follow Description");
    assert_eq!(parsed_buffs[1].effects.len(), 2);
    assert!(parsed_buffs[3].effects[0].requirements.is_some());
}

#[test]
fn writes_optional_buffs_with_modifiers_and_their_requirements() {
    let db = built_fixture_db();
    assert_eq!(db.query_row("SELECT COUNT(*) FROM optional_buffs", [], |r| r.get::<_, i64>(0)).unwrap(), 4);
    let (bless, icon): (i64, String) = db
        .query_row("SELECT id, icon FROM optional_buffs WHERE name = 'Bless'", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    assert_eq!(icon, "Bless");
    assert_eq!(
        modifier_rows(&db, "optional_buff", bless),
        [
            ("Weapon_Attack".to_string(), Some("Morale".to_string()), Some("[1]".to_string())),
            ("SaveBonus".to_string(), Some("Morale".to_string()), Some("[1]".to_string())),
        ]
    );
    let shield_requirement: String = db
        .query_row(
            "SELECT r.items FROM optional_buffs b JOIN modifiers m ON m.source_kind = 'optional_buff' AND m.source_id = b.id
               JOIN requirements r ON r.owner_kind = 'modifier' AND r.owner_id = m.id
              WHERE b.name = 'Stone of Change: Alchemical Shield Eldritch Ritual'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(shield_requirement, r#"["Shield"]"#);
}
