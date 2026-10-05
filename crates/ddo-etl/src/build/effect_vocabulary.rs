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
