use super::super::quests::{
    adventure_packs_dropping_via_many, quests_dropping_via_many, sources_via_many, wiki_page_url,
};
use crate::db::{
    convert_to_booleans, effects_via, grouped_rows, json_rows, modifiers_for_owners, notes_for_owners, sql_integer_list,
};
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
        "SELECT i.id, i.slug, i.name, es.name AS slot, i.item_category AS category, i.item_type, i.minimum_level, i.enhancement_bonus,
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
    attach_rows(&mut items, "notes", notes_for_owners(db, "items", ids)?);

    let sockets = crate::db::sockets_by_id(db)?;
    let mut slots = grouped_rows(
        db,
        ids,
        "SELECT socket.item_id AS owner_id,socket.sort_order,socket.slot_id,
        sets.id AS option_set_id,sets.slug AS option_set_slug,sets.name AS option_set_name
        FROM item_augment_slots socket LEFT JOIN socket_option_sets sets ON sets.id=socket.option_set_id
        WHERE socket.item_id IN ({ids}) ORDER BY socket.item_id,socket.sort_order",
    )?;
    let set_ids = slots.values().flatten().filter_map(|slot| slot["option_set_id"].as_i64()).collect::<Vec<_>>();
    let options = super::super::socket_option_sets::options_for_sets(db, &set_ids)?;
    for (item_id, item) in &mut items {
        let mut item_slots = slots.remove(item_id).unwrap_or_default();
        for slot in &mut item_slots {
            let slot_id = slot["slot_id"].as_i64().unwrap();
            let choices = slot["option_set_id"].as_i64().and_then(|id| options.get(&id)).cloned().unwrap_or_default();
            let fields = slot.as_object_mut().unwrap();
            fields.remove("slot_id");
            fields.extend(sockets[&slot_id].as_object().unwrap().clone());
            fields.insert("options".into(), Value::Array(choices));
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
        ic.clickie_id, c.slug AS clickie_slug, ic.spell_id, sp.slug AS spell_slug, c.description, c.icon FROM item_clickies ic
        LEFT JOIN clickies c ON c.id = ic.clickie_id LEFT JOIN spells sp ON sp.id = ic.spell_id WHERE ic.item_id IN ({ids}) ORDER BY ic.item_id, ic.sort_order",
        )?,
    );
    let mut sets = grouped_rows(
        db,
        ids,
        "SELECT sbi.item_id AS owner_id, s.id, s.slug, s.name, s.icon
        FROM set_bonus_items sbi JOIN set_bonuses s ON s.id = sbi.set_id
        JOIN items i ON i.id = sbi.item_id WHERE sbi.item_id IN ({ids})
        ORDER BY sbi.item_id, (s.name <> i.set_bonus), s.name",
    )?;
    for (item_id, item) in &mut items {
        let item_sets = sets.remove(item_id).unwrap_or_default();
        item["set"] = item_sets.first().cloned().unwrap_or(Value::Null);
        item["sets"] = Value::Array(item_sets);
    }

    attach_sources(db, ids, &mut items)?;
    let mut modifiers = modifiers_for_owners(db, "item", ids)?;
    for (item_id, item) in &mut items {
        item["modifiers"] = Value::Array(modifiers.remove(item_id).unwrap_or_default());
    }
    Ok(items)
}

fn attach_sources(db: &Connection, ids: &[i64], items: &mut BTreeMap<i64, Value>) -> Result<(), ApiError> {
    let sources = sources_via_many(db, "item_id", ids)?;
    attach_rows(items, "quests", quests_dropping_via_many(db, "item_id", ids)?);

    let mut chains = grouped_rows(
        db,
        ids,
        "SELECT cr.item_id AS owner_id, c.id, c.slug, c.name, cr.is_rare, c.wiki_url
        FROM sources cr JOIN quest_chains c ON c.id = cr.chain_id
        WHERE cr.item_id IN ({ids}) ORDER BY cr.item_id, c.name",
    )?;
    convert_group_flags(&mut chains, &["is_rare"]);
    attach_rows(items, "quest_chains", chains);

    let mut sagas = grouped_rows(db, ids, "SELECT sr.item_id AS owner_id, s.id, s.slug, s.name, sr.tier, sr.is_rare, s.wiki_url
        FROM sources sr JOIN sagas s ON s.id = sr.saga_id WHERE sr.item_id IN ({ids})
        ORDER BY sr.item_id, s.name, CASE sr.tier WHEN 'heroic' THEN 1 WHEN 'epic' THEN 2 WHEN 'legendary' THEN 3 ELSE 4 END")?;
    convert_group_flags(&mut sagas, &["is_rare"]);
    attach_rows(items, "sagas", sagas);

    attach_rows(items, "adventure_packs", adventure_packs_dropping_via_many(db, "item_id", ids)?);

    let mut challenges = grouped_rows(
        db,
        ids,
        "SELECT loot.item_id AS owner_id, p.id, p.slug, p.name, loot.is_rare
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
        "SELECT loot.item_id AS owner_id, cs.id, cs.slug, cs.name,
        loot.is_rare, cs.page AS wiki_url FROM sources loot
        JOIN crafting_systems cs ON cs.id = loot.crafting_system_id
        WHERE loot.item_id IN ({ids}) ORDER BY loot.item_id, cs.name",
    )?;
    convert_group_flags(&mut crafting, &["is_rare"]);
    attach_rows(items, "crafting_systems", crafting);

    let mut vendors = grouped_rows(
        db,
        ids,
        "SELECT vi.item_id AS owner_id, v.id, v.slug, v.name, v.location,
        vi.cost, vi.is_rare, v.wiki_url FROM sources vi JOIN vendors v ON v.id = vi.vendor_id
        WHERE vi.item_id IN ({ids}) ORDER BY vi.item_id, v.name",
    )?;
    convert_group_flags(&mut vendors, &["is_rare"]);
    for (item_id, offers) in &mut vendors {
        for offer in offers {
            offer["price"] = sources
                .get(item_id)
                .into_iter()
                .flatten()
                .find(|source| source["kind"] == "vendor" && source["name"] == offer["name"])
                .map(|source| source["price"].clone())
                .unwrap_or_else(|| Value::Array(Vec::new()));
        }
    }
    attach_rows(items, "vendors", vendors);

    let mut events = grouped_rows(
        db,
        ids,
        "SELECT ei.item_id AS owner_id, e.id, e.slug, e.name, ei.is_rare, e.wiki_url
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

    attach_rows(items, "sources", sources);
    Ok(())
}

fn convert_group_flags(groups: &mut BTreeMap<i64, Vec<Value>>, flags: &[&str]) {
    for rows in groups.values_mut() {
        for row in rows {
            convert_to_booleans(row, flags);
        }
    }
}
