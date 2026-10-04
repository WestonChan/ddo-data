use crate::map::effect_map::EFFECT_MAP;
use anyhow::Result;
use ddo_model::enums::BonusType;
use rusqlite::{params, Connection};

pub(super) fn insert_effect_vocabulary(db: &Connection) -> Result<()> {
    for (alias, canonical_name) in &EFFECT_MAP.bonus_type_aliases {
        if let Some(bonus_type) = BonusType::parse(canonical_name) {
            db.execute(
                "INSERT INTO bonus_type_aliases (name, bonus_type_id) VALUES (?1, ?2)",
                params![alias, bonus_type.id()],
            )?;
        }
    }
    db.execute_batch(
        "INSERT INTO effect_vocabulary_counts (kind, id, item_count, augment_count, set_count)
         SELECT 'effect', e.id,
                (SELECT COUNT(DISTINCT ie.item_id) FROM item_effects ie JOIN items i ON i.id = ie.item_id
                 WHERE ie.effect_id = e.id AND NOT i.is_legacy),
                (SELECT COUNT(DISTINCT ae.augment_id) FROM augment_effects ae WHERE ae.effect_id = e.id),
                (SELECT COUNT(DISTINCT t.set_id) FROM set_bonus_tier_effects te
                 JOIN set_bonus_tiers t ON t.id = te.tier_id WHERE te.effect_id = e.id)
           FROM effects e;
         INSERT INTO effect_vocabulary_counts (kind, id, item_count, augment_count, set_count)
         SELECT 'stat', s.id,
                (SELECT COUNT(DISTINCT ie.item_id) FROM effect_bonuses eb
                 JOIN item_effects ie ON ie.effect_id = eb.effect_id
                 JOIN items i ON i.id = ie.item_id WHERE eb.stat_id = s.id AND NOT i.is_legacy),
                (SELECT COUNT(DISTINCT ae.augment_id) FROM effect_bonuses eb
                 JOIN augment_effects ae ON ae.effect_id = eb.effect_id WHERE eb.stat_id = s.id),
                (SELECT COUNT(DISTINCT t.set_id) FROM effect_bonuses eb
                 JOIN set_bonus_tier_effects te ON te.effect_id = eb.effect_id
                 JOIN set_bonus_tiers t ON t.id = te.tier_id WHERE eb.stat_id = s.id)
           FROM stats s;
         INSERT INTO effect_vocabulary_bonus_types (kind, id, bonus_type_id, item_count)
         SELECT kind, id, bonus_type_id, COUNT(DISTINCT item_id) FROM (
             SELECT 'effect' AS kind, ie.effect_id AS id, COALESCE(eb.bonus_type_id, ie.bonus_type_id) AS bonus_type_id,
                    ie.item_id
               FROM item_effects ie JOIN items i ON i.id = ie.item_id AND NOT i.is_legacy
               LEFT JOIN effect_bonuses eb ON eb.effect_id = ie.effect_id
             UNION ALL
             SELECT 'stat', eb.stat_id, COALESCE(eb.bonus_type_id, ie.bonus_type_id), ie.item_id
               FROM effect_bonuses eb JOIN item_effects ie ON ie.effect_id = eb.effect_id
               JOIN items i ON i.id = ie.item_id AND NOT i.is_legacy
             UNION ALL
             SELECT 'effect', ae.effect_id, COALESCE(eb.bonus_type_id, ae.bonus_type_id), NULL
               FROM augment_effects ae LEFT JOIN effect_bonuses eb ON eb.effect_id = ae.effect_id
             UNION ALL
             SELECT 'stat', eb.stat_id, COALESCE(eb.bonus_type_id, ae.bonus_type_id), NULL
               FROM effect_bonuses eb JOIN augment_effects ae ON ae.effect_id = eb.effect_id
             UNION ALL
             SELECT 'effect', te.effect_id, COALESCE(eb.bonus_type_id, te.bonus_type_id), NULL
               FROM set_bonus_tier_effects te LEFT JOIN effect_bonuses eb ON eb.effect_id = te.effect_id
             UNION ALL
             SELECT 'stat', eb.stat_id, COALESCE(eb.bonus_type_id, te.bonus_type_id), NULL
               FROM effect_bonuses eb JOIN set_bonus_tier_effects te ON te.effect_id = eb.effect_id
         ) WHERE bonus_type_id IS NOT NULL GROUP BY kind, id, bonus_type_id;",
    )?;
    Ok(())
}
