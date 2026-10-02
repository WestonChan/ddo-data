use super::quest_series::{quest_chains_rewarding, sagas_rewarding};
use super::quests::quests_dropping_via;
use crate::db::{
    bonuses_via, clamped_page, convert_to_booleans, json_row, json_rows, modifiers_for, row_count,
    substring_like_pattern, whole_table_json, WhereClause,
};
use crate::error::ApiError;
use crate::query::ApiQuery;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use ddo_model::enums::ItemCategory;
use serde::Deserialize;
use serde_json::{json, Value};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(items))
        .routes(routes!(item_detail))
        .routes(routes!(equipment_slots))
        .routes(routes!(weapon_types))
        .routes(routes!(damage_types))
        .routes(routes!(augment_slot_types))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ItemListQuery {
    pub q: Option<String>,
    pub slot: Option<String>,
    pub category: Option<String>,
    pub min_level: Option<i64>,
    pub max_level: Option<i64>,
    pub pack: Option<String>,
    pub raid: Option<bool>,
    pub rare: Option<bool>,
    pub stat: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[utoipa::path(
    get,
    path = "/v1/items",
    tag = "items",
    summary = "List items",
    description = "One page of equipment matching every filter given, ordered by name. Each row carries what a picker \
                   needs: id, name, slot, category, item type, minimum level, enhancement bonus, icon name, the \
                   alphabetically first adventure pack it drops in, whether any of its sources is a raid, whether \
                   it is rare loot from at least one quest (marked rare in Maetrim's drop text or on ddowiki), and \
                   `source`: `maetrim` for an item from DDOBuilderV2's files, `wiki` for one read from ddowiki \
                   because his files lack it (dropped as soon as his files carry an item of that name). Use the detail endpoint for bonuses, sockets and quests. `total` counts every match, not just this page.",
    params(
        ("q" = Option<String>, Query, description = "Case-insensitive substring of the item name"),
        ("slot" = Option<String>, Query, description = "Equipment slot name exactly as /v1/equipment-slots lists it, e.g. `Main Hand`"),
        ("category" = Option<String>, Query, description = "One of `Armor`, `Shield`, `Weapon`, `Jewelry`, `Clothing`; anything else is a 400"),
        ("min_level" = Option<i64>, Query, description = "Only items whose minimum level is at least this"),
        ("max_level" = Option<i64>, Query, description = "Only items whose minimum level is at most this"),
        ("pack" = Option<String>, Query, description = "Adventure pack name as /v1/adventure-packs lists it; matches items dropping from any quest in it"),
        ("raid" = Option<bool>, Query, description = "`true` keeps only items that drop from a raid; `false` and unset apply no filter"),
        ("rare" = Option<bool>, Query, description = "`true` keeps only items that are rare loot from at least one quest, per Maetrim's drop text or ddowiki; `false` and unset apply no filter"),
        ("stat" = Option<String>, Query, description = "Stat name as /v1/stats lists it; keeps items with at least one bonus to it"),
        ("limit" = Option<i64>, Query, description = "Page size, 1 to 10000; defaults to 100; out-of-range values are clamped rather than rejected"),
        ("offset" = Option<i64>, Query, description = "Rows to skip before the first returned row; defaults to 0; negative values are clamped to 0 rather than rejected")
    ),
    responses(
        (status = 200, description = "`total`, `limit`, `offset` and the `items` page", body = Value),
        (status = 400, description = "Unknown category, or an unknown or malformed query parameter", body = crate::error::ErrorBody)
    )
)]
async fn items(
    State(state): State<AppState>,
    ApiQuery(query): ApiQuery<ItemListQuery>,
) -> Result<Json<Value>, ApiError> {
    if let Some(category) = &query.category {
        if !ItemCategory::ALL.iter().any(|known| known.as_str() == category) {
            return Err(ApiError::BadRequest(format!("unknown category {category:?}")));
        }
    }
    let (limit, offset) = clamped_page(query.limit, query.offset);
    state
        .read_db(move |db| {
            let mut where_clause = WhereClause::default();
            if let Some(search_text) = query.q.as_deref().filter(|q| !q.trim().is_empty()) {
                where_clause.add_bound_condition("i.name LIKE ? ESCAPE '\\'", substring_like_pattern(search_text));
            }
            if let Some(slot) = &query.slot {
                where_clause.add_bound_condition("es.name = ?", slot.clone());
            }
            if let Some(category) = &query.category {
                where_clause.add_bound_condition("i.item_category = ?", category.clone());
            }
            if let Some(min_level) = query.min_level {
                where_clause.add_bound_condition("i.minimum_level >= ?", min_level);
            }
            if let Some(max_level) = query.max_level {
                where_clause.add_bound_condition("i.minimum_level <= ?", max_level);
            }
            if let Some(pack) = &query.pack {
                where_clause.add_bound_condition("EXISTS (SELECT 1 FROM quest_loot ql JOIN quests q ON q.id = ql.quest_id JOIN adventure_packs ap ON ap.id = q.pack_id WHERE ql.item_id = i.id AND ap.name = ?)",
                    pack.clone(),
                );
            }
            if query.raid == Some(true) {
                where_clause.add_condition("EXISTS (SELECT 1 FROM quest_loot ql WHERE ql.item_id = i.id AND ql.loot_type = 'raid')");
            }
            if query.rare == Some(true) {
                where_clause.add_condition("EXISTS (SELECT 1 FROM quest_loot ql WHERE ql.item_id = i.id AND ql.is_rare)");
            }
            if let Some(stat) = &query.stat {
                where_clause.add_bound_condition("EXISTS (SELECT 1 FROM item_bonuses ib JOIN bonuses b ON b.id = ib.bonus_id JOIN stats s ON s.id = b.stat_id WHERE ib.item_id = i.id AND s.name = ?)",
                    stat.clone(),
                );
            }
            let where_sql = where_clause.to_sql();
            let from_sql = format!("FROM items i JOIN equipment_slots es ON es.id = i.slot_id {where_sql}");
            let total = row_count(db, &format!("SELECT COUNT(*) {from_sql}"), where_clause.params())?;
            let page_sql = format!(
                "SELECT i.id, i.name, es.name AS slot, i.item_category AS category, i.item_type, i.minimum_level, i.enhancement_bonus, i.icon, i.source,
                        (SELECT MIN(ap.name) FROM quest_loot ql JOIN quests q ON q.id = ql.quest_id LEFT JOIN adventure_packs ap ON ap.id = q.pack_id WHERE ql.item_id = i.id) AS pack,
                        EXISTS (SELECT 1 FROM quest_loot ql WHERE ql.item_id = i.id AND ql.loot_type = 'raid') AS is_raid,
                        EXISTS (SELECT 1 FROM quest_loot ql WHERE ql.item_id = i.id AND ql.is_rare) AS is_rare
                 {from_sql} ORDER BY i.name LIMIT {limit} OFFSET {offset}"
            );
            let mut items = json_rows(db, &page_sql, where_clause.params())?;
            for item in &mut items {
                convert_to_booleans(item, &["is_raid", "is_rare"]);
            }
            Ok(Json(json!({ "total": total, "limit": limit, "offset": offset, "items": items })))
        })
        .await
}

#[utoipa::path(
    get,
    path = "/v1/items/{id}",
    tag = "items",
    summary = "Get an item",
    description = "One item with everything the dataset knows about it: the core row (slot, category, type, minimum \
                   level, enhancement bonus, material, race restriction, description, drop location text, set name, \
                   sentience and minor-artifact flags, wiki URL, and `source`, `maetrim` or `wiki` as in the list), \
                   then `weapon` (dice, threat range, multipliers, \
                   proficiency, `dr_bypass`) or `armor` (AC, max Dex, spell failure, check penalty) when the item is \
                   one, `bonuses` (stat, bonus type, value), `effects` (named effects with value and target), \
                   `augment_slots` (sockets in order with their fixed `options`), `clickies`, `set`, `quests` it drops \
                   from (once per loot type, so a quest that both drops it and gives it as an end reward appears twice) with loot type, raid flag, `is_rare` (rare loot in that quest, per Maetrim's drop text or ddowiki), \
                   `chest` (the chest his drop text names for that quest, lower-cased, such as `end chest` or \
                   `optional chest`; null when it names none, and always null on a `reward` row), the \
                   `difficulties` each offers, ddowiki's `is_free_to_play` for each and its `source` (`maetrim`, or `wiki` \
                   for a quest read from ddowiki because his files lack it; see /v1/quests for the rest of the quest), `quest_chains` and `sagas` whose end reward offers the item (each with `id`, `name` and `is_rare`, a saga also with its reward `tier`; see /v1/quest-chains and /v1/sagas), and the raw `modifiers` the ETL derived the bonuses from.",
    params(("id" = i64, Path, description = "The item's numeric id from the list endpoint")), responses((status = 200, description = "The item with its child collections", body = Value), (status = 404, description = "No item has this id", body = crate::error::ErrorBody))
)]
async fn item_detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut item = json_row(
                db,
                "SELECT i.id, i.name, es.name AS slot, i.item_category AS category, i.item_type, i.minimum_level, i.enhancement_bonus,
                        m.name AS material, i.race_required, i.icon, i.description, i.drop_location, i.set_bonus AS set_name,
                        i.accepts_sentience, i.is_minor_artifact, i.wiki_url, i.source
                   FROM items i JOIN equipment_slots es ON es.id = i.slot_id LEFT JOIN item_materials m ON m.id = i.material_id
                  WHERE i.id = ?1",
                [id],
            )?;
            convert_to_booleans(&mut item, &["accepts_sentience", "is_minor_artifact"]);

            let weapon = json_rows(
                db,
                "SELECT wt.name AS weapon_type, p.name AS proficiency, w.handedness, w.damage, w.critical, w.base_dice_count, w.base_dice_sides,
                        w.base_dice_bonus, w.damage_multiplier, w.critical_threat_range, w.critical_multiplier, w.attack_modifier, w.damage_modifier
                   FROM item_weapon_stats w JOIN weapon_types wt ON wt.id = w.weapon_type_id
                   LEFT JOIN weapon_proficiencies p ON p.id = wt.proficiency_id WHERE w.item_id = ?1",
                [id],
            )?
            .pop();
            item["weapon"] = match weapon {
                Some(mut weapon) => {
                    let dr_bypasses: Vec<Value> = json_rows(db, "SELECT bypass FROM item_dr_bypass WHERE item_id = ?1 ORDER BY bypass", [id])?
                        .into_iter()
                        .map(|row| row["bypass"].clone())
                        .collect();
                    weapon["dr_bypass"] = Value::Array(dr_bypasses);
                    weapon
                }
                None => Value::Null,
            };
            item["armor"] = json_rows(
                db,
                "SELECT armor_type, armor_bonus, max_dex_bonus, arcane_spell_failure, armor_check_penalty, shield_bonus, damage_reduction, mithral_body, adamantine_body
                   FROM item_armor_stats WHERE item_id = ?1",
                [id],
            )?
            .pop()
            .unwrap_or(Value::Null);

            item["bonuses"] = Value::Array(bonuses_via(db, "item_bonuses", "item_id", id)?);
            item["effects"] = Value::Array(json_rows(
                db,
                "SELECT e.id, e.name, e.description, ie.value, ie.target FROM item_effects ie JOIN effects e ON e.id = ie.effect_id
                  WHERE ie.item_id = ?1 ORDER BY ie.sort_order",
                [id],
            )?);

            let mut augment_slots = json_rows(
                db,
                "SELECT s.sort_order, t.id AS slot_type_id, t.label, t.family, t.variant, t.qualifier FROM item_augment_slots s
                   JOIN augment_slot_types t ON t.id = s.slot_id WHERE s.item_id = ?1 ORDER BY s.sort_order",
                [id],
            )?;
            for augment_slot in &mut augment_slots {
                let slot_order = augment_slot["sort_order"].as_i64().unwrap_or(0);
                augment_slot["options"] = Value::Array(json_rows(
                    db,
                    "SELECT name, description, min_level FROM item_augment_slot_options WHERE item_id = ?1 AND slot_order = ?2 ORDER BY option_order",
                    (id, slot_order),
                )?);
            }
            item["augment_slots"] = Value::Array(augment_slots);

            item["clickies"] = Value::Array(json_rows(
                db,
                "SELECT ic.name, ic.clickie_id, ic.spell_id, c.description, c.icon FROM item_clickies ic
                   LEFT JOIN clickies c ON c.id = ic.clickie_id WHERE ic.item_id = ?1 ORDER BY ic.sort_order",
                [id],
            )?);
            item["set"] = json_rows(
                db,
                "SELECT s.id, s.name, s.icon FROM set_bonus_items sbi JOIN set_bonuses s ON s.id = sbi.set_id WHERE sbi.item_id = ?1",
                [id],
            )?
            .pop()
            .unwrap_or(Value::Null);
            item["quests"] = Value::Array(quests_dropping_via(db, "quest_loot", "item_id", id)?);
            item["quest_chains"] = Value::Array(quest_chains_rewarding(db, id)?);
            item["sagas"] = Value::Array(sagas_rewarding(db, id)?);
            item["modifiers"] = Value::Array(modifiers_for(db, "item", id)?);
            Ok(Json(item))
        })
        .await
}

#[utoipa::path(
    get,
    path = "/v1/equipment-slots",
    tag = "items",
    summary = "List equipment slots",
    description = "The equipment slots an item can occupy, in display order, with a category (weapon, armor, \
                   accessory). /v1/items accepts these names in `slot`.",
    responses((status = 200, description = "The whole table", body = Vec<Value>))
)]
async fn equipment_slots(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    whole_table_json(state, "SELECT id, name, sort_order, category FROM equipment_slots ORDER BY sort_order", &[]).await
}

#[utoipa::path(
    get,
    path = "/v1/weapon-types",
    tag = "items",
    summary = "List weapon types",
    description = "Every weapon and shield type with the proficiency it needs and whether it is a shield. Item \
                   `weapon` blocks name their type from this list.",
    responses((status = 200, description = "The whole table", body = Vec<Value>))
)]
async fn weapon_types(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    whole_table_json(
        state,
        "SELECT wt.id, wt.name, p.name AS proficiency, wt.is_shield FROM weapon_types wt
           LEFT JOIN weapon_proficiencies p ON p.id = wt.proficiency_id ORDER BY wt.id",
        &["is_shield"],
    )
    .await
}

#[utoipa::path(
    get,
    path = "/v1/damage-types",
    tag = "items",
    summary = "List damage types",
    description = "Every damage type (physical, elemental, alignment, special) with its category. Spell damage \
                   lines and DR bypass entries use these names.",
    responses((status = 200, description = "The whole table", body = Vec<Value>))
)]
async fn damage_types(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    whole_table_json(state, "SELECT id, name, category FROM damage_types ORDER BY id", &[]).await
}

#[utoipa::path(
    get,
    path = "/v1/augment-slot-types",
    tag = "items",
    summary = "List augment slot types",
    description = "Every socket an item can carry: gem colours (`red`, `colorless`, `sun`, ...) and crafting-family \
                   sockets (`lamordia: melancholic (accessory)`, `isle of dread: set bonus`, ...). `family` says which \
                   kind it is; `label` is what /v1/augments accepts in `slot`.",
    responses((status = 200, description = "The whole table", body = Vec<Value>))
)]
async fn augment_slot_types(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    whole_table_json(
        state,
        "SELECT id, label, family, variant, qualifier FROM augment_slot_types ORDER BY family, label",
        &[],
    )
    .await
}
