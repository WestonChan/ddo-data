use super::super::quests::{
    adventure_packs_dropping_via_many, quests_dropping_via_many, sources_via_many, wiki_page_url,
};
use crate::db::{convert_to_booleans, effects_via, grouped_rows, json_rows, modifiers_for_owners, sql_integer_list};
use crate::error::ApiError;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::BTreeMap;

fn attach_rows(items: &mut BTreeMap<i64, Value>, name: &str, mut rows: BTreeMap<i64, Vec<Value>>) {
    for (item_id, item) in items {
        item[name] = Value::Array(rows.remove(item_id).unwrap_or_default());
    }
}

fn attach_single(items: &mut BTreeMap<i64, Value>, name: &str, mut rows: BTreeMap<i64, Vec<Value>>) {
    for (item_id, item) in items {
        item[name] = rows.remove(item_id).and_then(|mut found| found.pop()).unwrap_or(Value::Null);
    }
}

pub(super) fn item_records(db: &Connection, ids: &[i64]) -> Result<BTreeMap<i64, Value>, ApiError> {
    let mut items = BTreeMap::new();
    for mut item in json_rows(db, &format!(
        "SELECT i.id, i.name, es.name AS slot, i.item_category AS category, i.item_type, i.minimum_level, i.enhancement_bonus,
                m.name AS material, i.race_required, i.icon, i.description, i.drop_location, i.set_bonus AS set_name,
                i.accepts_sentience, i.is_minor_artifact, i.is_legacy, i.wiki_url
           FROM items i JOIN equipment_slots es ON es.id = i.slot_id LEFT JOIN item_materials m ON m.id = i.material_id
          WHERE i.id IN ({})", sql_integer_list(ids)), [])? {
        convert_to_booleans(&mut item, &["accepts_sentience", "is_minor_artifact", "is_legacy"]);
        let item_id = item["id"].as_i64().ok_or_else(|| anyhow::anyhow!("item record has no numeric id"))?;
        items.insert(item_id, item);
    }
    if items.is_empty() {
        return Ok(items);
    }

    let mut weapons = grouped_rows(
        db,
        ids,
        "SELECT w.item_id AS owner_id, wt.name AS weapon_type, p.name AS proficiency,
        w.handedness, w.damage, w.critical, w.base_dice_count, w.base_dice_sides, w.base_dice_bonus,
        w.damage_multiplier, w.critical_threat_range, w.critical_multiplier, w.attack_modifier, w.damage_modifier
        FROM item_weapon_stats w JOIN weapon_types wt ON wt.id = w.weapon_type_id
        LEFT JOIN weapon_proficiencies p ON p.id = wt.proficiency_id WHERE w.item_id IN ({ids})",
    )?;
    let mut bypasses = grouped_rows(
        db,
        ids,
        "SELECT item_id AS owner_id, bypass FROM item_dr_bypass
        WHERE item_id IN ({ids}) ORDER BY item_id, bypass",
    )?;
    for (item_id, item) in &mut items {
        let weapon = weapons.remove(item_id).and_then(|mut found| found.pop());
        item["weapon"] = match weapon {
            Some(mut weapon) => {
                weapon["dr_bypass"] = Value::Array(
                    bypasses.remove(item_id).unwrap_or_default().into_iter().map(|row| row["bypass"].clone()).collect(),
                );
                weapon
            }
            None => Value::Null,
        };
    }
    attach_single(
        &mut items,
        "armor",
        grouped_rows(
            db,
            ids,
            "SELECT item_id AS owner_id, armor_type, armor_bonus,
        max_dex_bonus, arcane_spell_failure, armor_check_penalty, shield_bonus, damage_reduction,
        mithral_body, adamantine_body FROM item_armor_stats WHERE item_id IN ({ids})",
        )?,
    );

    let mut effects =
        effects_via(db, "item_effects", "item_id", &format!("j.item_id IN ({})", sql_integer_list(ids)), [])?;
    for (item_id, item) in &mut items {
        item["effects"] = Value::Array(effects.remove(item_id).unwrap_or_default());
    }

    let mut slots = grouped_rows(
        db,
        ids,
        "SELECT s.item_id AS owner_id, s.sort_order, t.id AS slot_type_id,
        t.label, t.family, t.variant, t.qualifier FROM item_augment_slots s
        JOIN augment_slot_types t ON t.id = s.slot_id WHERE s.item_id IN ({ids}) ORDER BY s.item_id, s.sort_order",
    )?;
    let mut options = grouped_rows(db, ids, "SELECT o.item_id AS owner_id, o.slot_order, o.id, o.name, o.description,
        o.min_level, o.icon, (SELECT t.label FROM item_augment_slot_option_grants g
        JOIN augment_slot_types t ON t.id = g.slot_id WHERE g.option_id = o.id ORDER BY g.sort_order LIMIT 1) AS grants_slot
        FROM item_augment_slot_options o WHERE o.item_id IN ({ids}) ORDER BY o.item_id, o.slot_order, o.option_order")?;
    let option_ids: Vec<i64> = options
        .values()
        .flatten()
        .map(|option| option["id"].as_i64().ok_or_else(|| anyhow::anyhow!("item augment option has no numeric id")))
        .collect::<Result<_, _>>()?;
    let mut option_sets = grouped_rows(
        db,
        &option_ids,
        "SELECT os.option_id AS owner_id, s.id, s.name
        FROM item_augment_slot_option_sets os JOIN set_bonuses s ON s.id = os.set_id
        WHERE os.option_id IN ({ids}) ORDER BY os.option_id, s.name",
    )?;
    let mut option_effects = effects_via(
        db,
        "item_augment_slot_option_effects",
        "option_id",
        &format!("j.option_id IN ({})", sql_integer_list(&option_ids)),
        [],
    )?;
    let mut option_modifiers = modifiers_for_owners(db, "item_augment_slot_option", &option_ids)?;
    for (item_id, item) in &mut items {
        let mut item_slots = slots.remove(item_id).unwrap_or_default();
        let mut options_by_slot = BTreeMap::<i64, Vec<Value>>::new();
        for mut option in options.remove(item_id).unwrap_or_default() {
            let slot_order = option["slot_order"]
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("item augment option has no numeric slot_order"))?;
            let option_id =
                option["id"].as_i64().ok_or_else(|| anyhow::anyhow!("item augment option has no numeric id"))?;
            option
                .as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("item augment option is not an object"))?
                .remove("slot_order");
            option["sets"] = Value::Array(option_sets.remove(&option_id).unwrap_or_default());
            option["effects"] = Value::Array(option_effects.remove(&option_id).unwrap_or_default());
            option["modifiers"] = Value::Array(option_modifiers.remove(&option_id).unwrap_or_default());
            options_by_slot.entry(slot_order).or_default().push(option);
        }
        for slot in &mut item_slots {
            let slot_order = slot["sort_order"]
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("item augment slot has no numeric sort_order"))?;
            slot["options"] = Value::Array(options_by_slot.remove(&slot_order).unwrap_or_default());
        }
        item["augment_slots"] = Value::Array(item_slots);
    }

    attach_rows(
        &mut items,
        "clickies",
        grouped_rows(
            db,
            ids,
            "SELECT ic.item_id AS owner_id, ic.name,
        ic.clickie_id, ic.spell_id, c.description, c.icon FROM item_clickies ic
        LEFT JOIN clickies c ON c.id = ic.clickie_id WHERE ic.item_id IN ({ids}) ORDER BY ic.item_id, ic.sort_order",
        )?,
    );
    attach_single(
        &mut items,
        "set",
        grouped_rows(
            db,
            ids,
            "SELECT sbi.item_id AS owner_id, s.id, s.name, s.icon
        FROM set_bonus_items sbi JOIN set_bonuses s ON s.id = sbi.set_id WHERE sbi.item_id IN ({ids})",
        )?,
    );

    attach_sources(db, ids, &mut items)?;
    let mut modifiers = modifiers_for_owners(db, "item", ids)?;
    for (item_id, item) in &mut items {
        item["modifiers"] = Value::Array(modifiers.remove(item_id).unwrap_or_default());
    }
    Ok(items)
}

fn attach_sources(db: &Connection, ids: &[i64], items: &mut BTreeMap<i64, Value>) -> Result<(), ApiError> {
    attach_rows(items, "quests", quests_dropping_via_many(db, "item_id", ids)?);

    let mut chains = grouped_rows(
        db,
        ids,
        "SELECT cr.item_id AS owner_id, c.id, c.name, cr.is_rare, c.wiki_url
        FROM sources cr JOIN quest_chains c ON c.id = cr.chain_id
        WHERE cr.item_id IN ({ids}) ORDER BY cr.item_id, c.name",
    )?;
    convert_group_flags(&mut chains, &["is_rare"]);
    attach_rows(items, "quest_chains", chains);

    let mut sagas = grouped_rows(db, ids, "SELECT sr.item_id AS owner_id, s.id, s.name, sr.tier, sr.is_rare, s.wiki_url
        FROM sources sr JOIN sagas s ON s.id = sr.saga_id WHERE sr.item_id IN ({ids})
        ORDER BY sr.item_id, s.name, CASE sr.tier WHEN 'heroic' THEN 1 WHEN 'epic' THEN 2 WHEN 'legendary' THEN 3 ELSE 4 END")?;
    convert_group_flags(&mut sagas, &["is_rare"]);
    attach_rows(items, "sagas", sagas);

    attach_rows(items, "adventure_packs", adventure_packs_dropping_via_many(db, "item_id", ids)?);

    let mut challenges = grouped_rows(
        db,
        ids,
        "SELECT loot.item_id AS owner_id, p.id, p.name, loot.is_rare
        FROM sources loot JOIN adventure_packs p ON p.id = loot.pack_id
        WHERE loot.kind = 'challenge' AND loot.item_id IN ({ids}) ORDER BY loot.item_id, p.name",
    )?;
    convert_group_flags(&mut challenges, &["is_rare"]);
    for rows in challenges.values_mut() {
        for row in rows {
            if let Some(name) = row["name"].as_str() {
                row["wiki_url"] = Value::String(wiki_page_url(name));
            }
        }
    }
    attach_rows(items, "challenge_packs", challenges);

    let mut crafting = grouped_rows(
        db,
        ids,
        "SELECT loot.item_id AS owner_id, cs.id, cs.name,
        loot.is_rare, cs.page AS wiki_url FROM sources loot
        JOIN crafting_systems cs ON cs.id = loot.crafting_system_id
        WHERE loot.item_id IN ({ids}) ORDER BY loot.item_id, cs.name",
    )?;
    convert_group_flags(&mut crafting, &["is_rare"]);
    attach_rows(items, "crafting_systems", crafting);

    let mut vendors = grouped_rows(
        db,
        ids,
        "SELECT vi.item_id AS owner_id, v.id, v.name, v.location,
        vi.cost, vi.is_rare, v.wiki_url FROM sources vi JOIN vendors v ON v.id = vi.vendor_id
        WHERE vi.item_id IN ({ids}) ORDER BY vi.item_id, v.name",
    )?;
    convert_group_flags(&mut vendors, &["is_rare"]);
    attach_rows(items, "vendors", vendors);

    let mut events = grouped_rows(
        db,
        ids,
        "SELECT ei.item_id AS owner_id, e.id, e.name, ei.is_rare, e.wiki_url
        FROM sources ei JOIN events e ON e.id = ei.event_id WHERE ei.item_id IN ({ids})
        ORDER BY ei.item_id, e.name",
    )?;
    convert_group_flags(&mut events, &["is_rare"]);
    attach_rows(items, "events", events);

    attach_rows(
        items,
        "starter_rewards",
        grouped_rows(
            db,
            ids,
            "SELECT item_id AS owner_id, character_level
        FROM sources WHERE kind = 'starter' AND item_id IN ({ids}) ORDER BY item_id, character_level",
        )?,
    );

    attach_rows(items, "sources", sources_via_many(db, "item_id", ids)?);
    Ok(())
}

fn convert_group_flags(groups: &mut BTreeMap<i64, Vec<Value>>, flags: &[&str]) {
    for rows in groups.values_mut() {
        for row in rows {
            convert_to_booleans(row, flags);
        }
    }
}
