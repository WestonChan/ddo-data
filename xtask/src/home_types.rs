use anyhow::Result;
use ddo_model::bonus_type_wording::{home_declaration_error, StatHomeType};
use ddo_model::enums::BonusType;
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet};

pub fn stat_home_violations(db: &Connection, declarations: &[(&str, StatHomeType)]) -> Result<Vec<String>> {
    let stats = db
        .prepare("SELECT name, id FROM effects WHERE is_stat")?
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?
        .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
    let mut line_types = BTreeSet::new();
    for (table, owner_column) in [
        ("item_effects", "item_id"),
        ("augment_effects", "augment_id"),
        ("feat_effects", "feat_id"),
        ("item_augment_slot_option_effects", "option_id"),
    ] {
        for lines in ddo_api::served_effect_lines(db, table, owner_column)
            .map_err(|error| anyhow::anyhow!("{error:?}"))?
            .into_values()
        {
            for (_, line) in lines {
                if let (Some(effect_id), Some(bonus_type)) = (line["effect_id"].as_i64(), line["bonus_type"].as_str()) {
                    line_types.insert((effect_id, bonus_type.to_owned()));
                }
            }
        }
    }
    let mut violations = Vec::new();
    let mut declared_names = BTreeSet::new();
    for &(name, home) in declarations {
        if !declared_names.insert(name) {
            violations.push(format!("duplicate stat home entry {name:?}"));
        }
        if let Some(error) = home_declaration_error(home.bonus_type()) {
            violations.push(format!("{name:?}: {error}"));
        }
        match stats.get(name) {
            None => violations.push(format!("stat home entry {name:?} names an unknown stat")),
            Some(id) if !line_types.contains(&(*id, home.bonus_type().name().to_owned())) => {
                violations
                    .push(format!("stat home entry {name:?} / {:?} hides no served line", home.bonus_type().name()));
            }
            Some(_) => {}
        }
    }
    let mut skills = db.prepare(
        "SELECT DISTINCT e.name FROM effects e JOIN owner_bonuses ob ON ob.stat_id=e.id
          WHERE e.category='skill' AND ob.bonus_type_id=(SELECT id FROM bonus_types WHERE name=?1) ORDER BY e.name",
    )?;
    for skill in skills.query_map([BonusType::Competence.name()], |row| row.get::<_, String>(0))? {
        let skill = skill?;
        if !declarations.contains(&(skill.as_str(), StatHomeType::Competence)) {
            violations.push(format!("skill {skill:?} has a Competence carrier but no Competence home entry"));
        }
    }
    Ok(violations)
}
