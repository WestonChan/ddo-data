use ddo_model::bonus_type_wording::{StatHomeType, STAT_HOME_TYPES};
use rusqlite::Connection;
use xtask::home_types::stat_home_violations;

#[test]
fn stat_home_lint_rejects_unknown_dead_and_missing_skill_entries() {
    let mut db = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(&xtask::workspace_root().join("data"), &mut db).unwrap();
    let violations = stat_home_violations(&db, STAT_HOME_TYPES).unwrap();
    assert!(violations.is_empty(), "{violations:?}");

    let mut unknown = STAT_HOME_TYPES.to_vec();
    unknown.push(("Unknown Stat Home", StatHomeType::Competence));
    assert!(stat_home_violations(&db, &unknown)
        .unwrap()
        .iter()
        .any(|message| message.contains("Unknown Stat Home") && message.contains("unknown stat")));

    let mut dead = STAT_HOME_TYPES.to_vec();
    dead.push(("Strength", StatHomeType::Resistance));
    assert!(stat_home_violations(&db, &dead)
        .unwrap()
        .iter()
        .any(|message| message.contains("Strength") && message.contains("hides no served line")));

    let missing_skill = STAT_HOME_TYPES.iter().copied().filter(|(name, _)| *name != "Bluff").collect::<Vec<_>>();
    assert!(stat_home_violations(&db, &missing_skill)
        .unwrap()
        .iter()
        .any(|message| message.contains("Bluff") && message.contains("no Competence home entry")));
}
