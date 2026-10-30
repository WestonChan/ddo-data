use crate::map::effect_map::EFFECT_MAP;
use anyhow::Result;
use ddo_model::enums::BonusType;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::BTreeMap;

pub(super) fn insert_effect_vocabulary(db: &Connection) -> Result<()> {
    for (alias, canonical_name) in &EFFECT_MAP.bonus_type_aliases {
        if let Some(bonus_type) = BonusType::parse(canonical_name) {
            db.execute(
                "INSERT INTO bonus_type_aliases (name, bonus_type_id) VALUES (?1, ?2)",
                params![alias, bonus_type.id()],
            )?;
        }
    }
    insert_effect_vocabulary_counts(db)
}

pub(crate) fn insert_data_effect_vocabulary(db: &Connection) -> Result<()> {
    for (alias, canonical_name) in &EFFECT_MAP.bonus_type_aliases {
        if let Some(id) = db
            .query_row("SELECT id FROM bonus_types WHERE name = ?1", [canonical_name], |row| row.get::<_, i64>(0))
            .optional()?
        {
            db.execute("INSERT INTO bonus_type_aliases (name, bonus_type_id) VALUES (?1, ?2)", params![alias, id])?;
        }
    }
    insert_effect_vocabulary_counts(db)
}

pub(crate) fn item_home_bonus_types(db: &Connection) -> Result<BTreeMap<i64, i64>> {
    let mut statement = db.prepare(
        "SELECT candidate.id, candidate.bonus_type_id FROM effect_vocabulary_bonus_types candidate
          WHERE candidate.item_count > 0
            AND candidate.item_count = (
                SELECT MAX(other.item_count) FROM effect_vocabulary_bonus_types other
                 WHERE other.id = candidate.id AND other.kind = candidate.kind)
            AND (SELECT COUNT(*) FROM effect_vocabulary_bonus_types tied
                  WHERE tied.id = candidate.id AND tied.kind = candidate.kind
                    AND tied.item_count = candidate.item_count) = 1",
    )?;
    let homes = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
    Ok(homes)
}

pub(super) fn write_effect_home_types(db: &Connection, groupings: &crate::groupings::Groupings) -> Result<()> {
    let inferred_homes = item_home_bonus_types(db)?;
    let mut statement = db.prepare(&format!(
        "SELECT e.id, e.name, e.is_stat, e.home_bonus_type_id, fixed_type.bonus_type_id
           FROM effects e LEFT JOIN ({fixed_types}) fixed_type ON fixed_type.effect_id=e.id",
        fixed_types = ddo_model::bonus_type_wording::FIXED_EFFECT_BONUS_TYPES_SQL,
    ))?;
    let mut update = db.prepare("UPDATE effects SET home_bonus_type_id = ?2 WHERE id = ?1")?;
    for row in statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, bool>(2)?,
            row.get::<_, Option<i64>>(3)?,
            row.get::<_, Option<i64>>(4)?,
        ))
    })? {
        let (id, name, is_stat, current_home, fixed_type) = row?;
        let inferred_home = current_home.or(fixed_type).or_else(|| inferred_homes.get(&id).copied());
        let home = if is_stat {
            ddo_model::bonus_type_wording::stat_home_type(&name).map(BonusType::id)
        } else {
            groupings
                .effect_named(&name)
                .map(|effect| effect.home_bonus_type.as_deref().and_then(BonusType::parse).map(BonusType::id))
                .unwrap_or(inferred_home)
                .filter(|id| !ddo_model::bonus_type_wording::ALWAYS_UNNAMED_TYPES.iter().any(|kind| kind.id() == *id))
        };
        update.execute(params![id, home])?;
    }
    Ok(())
}

pub(crate) fn write_stat_home_types(db: &Connection) -> Result<()> {
    db.execute("UPDATE effects SET home_bonus_type_id = NULL WHERE is_stat", [])?;
    let mut write_home = db.prepare(
        "UPDATE effects SET home_bonus_type_id = (SELECT id FROM bonus_types WHERE name = ?2) WHERE is_stat AND name = ?1",
    )?;
    for &(name, home) in ddo_model::bonus_type_wording::STAT_HOME_TYPES {
        write_home.execute(params![name, home.bonus_type().name()])?;
    }
    Ok(())
}

fn insert_effect_vocabulary_counts(db: &Connection) -> Result<()> {
    db.execute_batch(
        "INSERT INTO effect_vocabulary_counts (kind, id, item_count, augment_count, set_count)
         SELECT 'effect', e.id,
                (SELECT COUNT(DISTINCT ie.item_id) FROM item_effects ie JOIN items i ON i.id = ie.item_id
                 WHERE ie.effect_id = e.id AND NOT i.is_legacy),
                (SELECT COUNT(DISTINCT ae.augment_id) FROM augment_effects ae WHERE ae.effect_id = e.id),
                (SELECT COUNT(DISTINCT t.set_id) FROM set_bonus_tier_effects te
                 JOIN set_bonus_tiers t ON t.id = te.tier_id WHERE te.effect_id = e.id)
           FROM effects e WHERE e.is_stat = 0 AND e.is_group = 0;
         INSERT INTO effect_vocabulary_counts (kind, id, item_count, augment_count, set_count)
         SELECT 'stat', e.id,
                (SELECT COUNT(DISTINCT ob.owner_id) FROM owner_bonuses ob JOIN items i ON i.id = ob.owner_id
                 WHERE ob.stat_id = e.id AND ob.owner_kind = 'item' AND NOT i.is_legacy),
                (SELECT COUNT(DISTINCT owner_id) FROM owner_bonuses ob
                 WHERE ob.stat_id = e.id AND ob.owner_kind = 'augment'),
                (SELECT COUNT(DISTINCT t.set_id) FROM owner_bonuses ob
                 JOIN set_bonus_tiers t ON t.id = ob.owner_id
                 WHERE ob.stat_id = e.id AND ob.owner_kind = 'set_bonus_tier')
           FROM effects e WHERE e.is_stat = 1;
         INSERT INTO effect_vocabulary_counts (kind, id, item_count, augment_count, set_count)
         SELECT 'group', e.id,
                (SELECT COUNT(DISTINCT ob.owner_id) FROM owner_bonuses ob JOIN items i ON i.id = ob.owner_id
                 WHERE ob.group_effect_id = e.id AND ob.owner_kind = 'item' AND NOT i.is_legacy),
                (SELECT COUNT(DISTINCT owner_id) FROM owner_bonuses ob
                 WHERE ob.group_effect_id = e.id AND ob.owner_kind = 'augment'),
                (SELECT COUNT(DISTINCT t.set_id) FROM owner_bonuses ob
                 JOIN set_bonus_tiers t ON t.id = ob.owner_id
                 WHERE ob.group_effect_id = e.id AND ob.owner_kind = 'set_bonus_tier')
           FROM effects e WHERE e.is_group = 1;
         INSERT INTO effect_vocabulary_bonus_types (kind, id, bonus_type_id, item_count)
         SELECT kind, id, bonus_type_id, COUNT(DISTINCT item_id) FROM (
             SELECT 'effect' AS kind, ie.effect_id AS id, COALESCE(eb.bonus_type_id, ie.bonus_type_id) AS bonus_type_id,
                    ie.item_id
               FROM item_effects ie JOIN items i ON i.id = ie.item_id AND NOT i.is_legacy
               JOIN effects e ON e.id = ie.effect_id AND e.is_stat = 0 AND e.is_group = 0
               LEFT JOIN effect_bonuses eb ON eb.effect_id = ie.effect_id
             UNION ALL
             SELECT 'effect', ae.effect_id, COALESCE(eb.bonus_type_id, ae.bonus_type_id), NULL
               FROM augment_effects ae JOIN effects e ON e.id = ae.effect_id AND e.is_stat = 0 AND e.is_group = 0
               LEFT JOIN effect_bonuses eb ON eb.effect_id = ae.effect_id
             UNION ALL
             SELECT 'effect', te.effect_id, COALESCE(eb.bonus_type_id, te.bonus_type_id), NULL
               FROM set_bonus_tier_effects te JOIN effects e ON e.id = te.effect_id AND e.is_stat = 0 AND e.is_group = 0
               LEFT JOIN effect_bonuses eb ON eb.effect_id = te.effect_id
             UNION ALL
             SELECT 'stat', ob.stat_id, ob.bonus_type_id,
                    CASE WHEN i.is_legacy = 0 THEN ob.owner_id END
               FROM owner_bonuses ob JOIN items i ON i.id = ob.owner_id WHERE ob.owner_kind = 'item'
             UNION ALL
             SELECT 'stat', ob.stat_id, ob.bonus_type_id, NULL
               FROM owner_bonuses ob WHERE ob.owner_kind IN ('augment', 'set_bonus_tier')
             UNION ALL
             SELECT 'group', ob.group_effect_id, ob.bonus_type_id,
                    CASE WHEN i.is_legacy = 0 THEN ob.owner_id END
               FROM owner_bonuses ob JOIN items i ON i.id = ob.owner_id
              WHERE ob.owner_kind = 'item' AND ob.group_effect_id IS NOT NULL
             UNION ALL
             SELECT 'group', ob.group_effect_id, ob.bonus_type_id, NULL
               FROM owner_bonuses ob WHERE ob.owner_kind IN ('augment', 'set_bonus_tier')
                 AND ob.group_effect_id IS NOT NULL
         ) WHERE bonus_type_id IS NOT NULL GROUP BY kind, id, bonus_type_id;",
    )?;
    Ok(())
}
