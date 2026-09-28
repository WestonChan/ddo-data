use ddo_etl::build::build;
use ddo_etl::xml::guild_buffs;
use ddo_model::DatasetVersion;
use rusqlite::{params, Connection};
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataFiles")
}

fn built() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    let version = DatasetVersion { upstream_sha: "test".into(), built_at: "2026-09-27T00:00:00Z".into() };
    build(&fixtures(), &mut conn, &version).expect("build succeeds on fixtures");
    conn
}

fn modifiers(conn: &Connection, kind: &str, id: i64) -> Vec<(String, Option<String>, Option<String>)> {
    conn.prepare(
        "SELECT m.effect_type, bt.name, m.amounts FROM modifiers m LEFT JOIN bonus_types bt ON bt.id = m.bonus_type_id
          WHERE m.source_kind = ?1 AND m.source_id = ?2 ORDER BY m.sort_order",
    )
    .unwrap()
    .query_map(params![kind, id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

#[test]
fn parses_guild_buffs() {
    let list = guild_buffs::parse(&fixtures().join("GuildBuffs.xml")).unwrap();
    let names: Vec<&str> = list.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["Sign of the Silver Flame I", "Old Sully's Grog Cellar", "Tactical Training Room"]);
    let flame = &list[0];
    assert_eq!(flame.guild_level, Some(10));
    let resistance = &flame.effects[0];
    assert_eq!(resistance.amount_type.as_deref(), Some("TotalLevel"));
    assert_eq!(resistance.amounts.len(), 40, "one value per character level");
    assert_eq!((resistance.amounts[0], resistance.amounts[10], resistance.amounts[39]), (5.0, 10.0, 15.0));
    assert_eq!(list[2].effects.len(), 3);
    assert!(list[2].description.as_deref().unwrap().contains("Slicing blow"));
}

#[test]
fn writes_guild_buffs_with_per_level_modifiers() {
    let conn = built();
    let (id, level): (i64, i64) = conn
        .query_row("SELECT id, guild_level FROM guild_buffs WHERE name = 'Tactical Training Room'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(level, 21);
    let types: Vec<String> = modifiers(&conn, "guild_buff", id).into_iter().map(|m| m.0).collect();
    assert_eq!(types, ["Weapon_DamageCritical", "TacticalDC", "Weapon_Attack"]);

    let sully: i64 =
        conn.query_row("SELECT id FROM guild_buffs WHERE name = 'Old Sully''s Grog Cellar'", [], |r| r.get(0)).unwrap();
    assert_eq!(
        modifiers(&conn, "guild_buff", sully),
        [("AbilityBonus".to_string(), Some("Guild".to_string()), Some("[2]".to_string()))]
    );
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM guild_buffs", [], |r| r.get::<_, i64>(0)).unwrap(), 3);
}
