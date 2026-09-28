use crate::db::{
    bonuses_via, booleanize, count, json_row, json_rows, like_pattern, modifiers_for, page, table, Filters,
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

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list))
        .routes(routes!(detail))
        .routes(routes!(equipment_slots))
        .routes(routes!(weapon_types))
        .routes(routes!(damage_types))
        .routes(routes!(augment_slot_types))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemFilter {
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
                   alphabetically first adventure pack it drops in, whether any of its sources is a raid, and whether \
                   it is rare loot from at least one quest (marked rare in Maetrim's drop text or on ddowiki). Use the detail endpoint for bonuses, sockets and quests. `total` counts every match, not just this page.",
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
async fn list(State(state): State<AppState>, ApiQuery(f): ApiQuery<ItemFilter>) -> Result<Json<Value>, ApiError> {
    if let Some(c) = &f.category {
        if !ItemCategory::ALL.iter().any(|k| k.as_str() == c) {
            return Err(ApiError::BadRequest(format!("unknown category {c:?}")));
        }
    }
    let (limit, offset) = page(f.limit, f.offset);
    state
        .query(move |conn| {
            let mut f_ = Filters::default();
            if let Some(q) = f.q.as_deref().filter(|q| !q.trim().is_empty()) {
                f_.bind("i.name LIKE ? ESCAPE '\\'", like_pattern(q));
            }
            if let Some(slot) = &f.slot {
                f_.bind("es.name = ?", slot.clone());
            }
            if let Some(c) = &f.category {
                f_.bind("i.item_category = ?", c.clone());
            }
            if let Some(n) = f.min_level {
                f_.bind("i.minimum_level >= ?", n);
            }
            if let Some(n) = f.max_level {
                f_.bind("i.minimum_level <= ?", n);
            }
            if let Some(pack) = &f.pack {
                f_.bind("EXISTS (SELECT 1 FROM quest_loot ql JOIN quests q ON q.id = ql.quest_id JOIN adventure_packs ap ON ap.id = q.pack_id WHERE ql.item_id = i.id AND ap.name = ?)",
                    pack.clone(),
                );
            }
            if f.raid == Some(true) {
                f_.clause("EXISTS (SELECT 1 FROM quest_loot ql WHERE ql.item_id = i.id AND ql.loot_type = 'raid')");
            }
            if f.rare == Some(true) {
                f_.clause("EXISTS (SELECT 1 FROM quest_loot ql WHERE ql.item_id = i.id AND ql.is_rare)");
            }
            if let Some(stat) = &f.stat {
                f_.bind("EXISTS (SELECT 1 FROM item_bonuses ib JOIN bonuses b ON b.id = ib.bonus_id JOIN stats s ON s.id = b.stat_id WHERE ib.item_id = i.id AND s.name = ?)",
                    stat.clone(),
                );
            }
            let where_sql = f_.where_sql();
            let from = format!("FROM items i JOIN equipment_slots es ON es.id = i.slot_id {where_sql}");
            let total = count(conn, &format!("SELECT COUNT(*) {from}"), f_.params())?;
            let sql = format!(
                "SELECT i.id, i.name, es.name AS slot, i.item_category AS category, i.item_type, i.minimum_level, i.enhancement_bonus, i.icon,
                        (SELECT MIN(ap.name) FROM quest_loot ql JOIN quests q ON q.id = ql.quest_id LEFT JOIN adventure_packs ap ON ap.id = q.pack_id WHERE ql.item_id = i.id) AS pack,
                        EXISTS (SELECT 1 FROM quest_loot ql WHERE ql.item_id = i.id AND ql.loot_type = 'raid') AS is_raid,
                        EXISTS (SELECT 1 FROM quest_loot ql WHERE ql.item_id = i.id AND ql.is_rare) AS is_rare
                 {from} ORDER BY i.name LIMIT {limit} OFFSET {offset}"
            );
            let mut items = json_rows(conn, &sql, f_.params())?;
            for item in &mut items {
                booleanize(item, &["is_raid", "is_rare"]);
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
                   sentience and minor-artifact flags, wiki URL), then `weapon` (dice, threat range, multipliers, \
                   proficiency, `dr_bypass`) or `armor` (AC, max Dex, spell failure, check penalty) when the item is \
                   one, `bonuses` (stat, bonus type, value), `effects` (named effects with value and target), \
                   `augment_slots` (sockets in order with their fixed `options`), `clickies`, `set`, `quests` it drops \
                   from with loot type, raid flag, `is_rare` (rare loot in that quest, per Maetrim's drop text or ddowiki) and the \
                   `difficulties` each offers, and the raw `modifiers` the ETL derived the bonuses from.",
    params(("id" = i64, Path, description = "The item's numeric id from the list endpoint")), responses((status = 200, description = "The item with its child collections", body = Value), (status = 404, description = "No item has this id", body = crate::error::ErrorBody))
)]
async fn detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .query(move |conn| {
            let mut item = json_row(
                conn,
                "SELECT i.id, i.name, es.name AS slot, i.item_category AS category, i.item_type, i.minimum_level, i.enhancement_bonus,
                        m.name AS material, i.race_required, i.icon, i.description, i.drop_location, i.set_bonus AS set_name,
                        i.accepts_sentience, i.is_minor_artifact, i.wiki_url
                   FROM items i JOIN equipment_slots es ON es.id = i.slot_id LEFT JOIN item_materials m ON m.id = i.material_id
                  WHERE i.id = ?1",
                [id],
            )?;
            booleanize(&mut item, &["accepts_sentience", "is_minor_artifact"]);

            let weapon = json_rows(
                conn,
                "SELECT wt.name AS weapon_type, p.name AS proficiency, w.handedness, w.damage, w.critical, w.base_dice_count, w.base_dice_sides,
                        w.base_dice_bonus, w.damage_multiplier, w.critical_threat_range, w.critical_multiplier, w.attack_modifier, w.damage_modifier
                   FROM item_weapon_stats w JOIN weapon_types wt ON wt.id = w.weapon_type_id
                   LEFT JOIN weapon_proficiencies p ON p.id = wt.proficiency_id WHERE w.item_id = ?1",
                [id],
            )?
            .pop();
            item["weapon"] = match weapon {
                Some(mut w) => {
                    let bypass: Vec<Value> = json_rows(conn, "SELECT bypass FROM item_dr_bypass WHERE item_id = ?1 ORDER BY bypass", [id])?
                        .into_iter()
                        .map(|r| r["bypass"].clone())
                        .collect();
                    w["dr_bypass"] = Value::Array(bypass);
                    w
                }
                None => Value::Null,
            };
            item["armor"] = json_rows(
                conn,
                "SELECT armor_type, armor_bonus, max_dex_bonus, arcane_spell_failure, armor_check_penalty, shield_bonus, damage_reduction, mithral_body, adamantine_body
                   FROM item_armor_stats WHERE item_id = ?1",
                [id],
            )?
            .pop()
            .unwrap_or(Value::Null);

            item["bonuses"] = Value::Array(bonuses_via(conn, "item_bonuses", "item_id", id)?);
            item["effects"] = Value::Array(json_rows(
                conn,
                "SELECT e.id, e.name, e.description, ie.value, ie.target FROM item_effects ie JOIN effects e ON e.id = ie.effect_id
                  WHERE ie.item_id = ?1 ORDER BY ie.sort_order",
                [id],
            )?);

            let mut slots = json_rows(
                conn,
                "SELECT s.sort_order, t.id AS slot_type_id, t.label, t.family, t.variant, t.qualifier FROM item_augment_slots s
                   JOIN augment_slot_types t ON t.id = s.slot_id WHERE s.item_id = ?1 ORDER BY s.sort_order",
                [id],
            )?;
            for slot in &mut slots {
                let order = slot["sort_order"].as_i64().unwrap_or(0);
                slot["options"] = Value::Array(json_rows(
                    conn,
                    "SELECT name, description, min_level FROM item_augment_slot_options WHERE item_id = ?1 AND slot_order = ?2 ORDER BY option_order",
                    (id, order),
                )?);
            }
            item["augment_slots"] = Value::Array(slots);

            item["clickies"] = Value::Array(json_rows(
                conn,
                "SELECT ic.name, ic.clickie_id, ic.spell_id, c.description, c.icon FROM item_clickies ic
                   LEFT JOIN clickies c ON c.id = ic.clickie_id WHERE ic.item_id = ?1 ORDER BY ic.sort_order",
                [id],
            )?);
            item["set"] = json_rows(
                conn,
                "SELECT s.id, s.name, s.icon FROM set_bonus_items sbi JOIN set_bonuses s ON s.id = sbi.set_id WHERE sbi.item_id = ?1",
                [id],
            )?
            .pop()
            .unwrap_or(Value::Null);
            let mut quests = json_rows(
                conn,
                "SELECT q.id, q.name, q.level, q.epic_level, q.is_raid, q.difficulties, ap.name AS pack, pt.name AS patron, ql.loot_type, ql.is_rare
                   FROM quest_loot ql JOIN quests q ON q.id = ql.quest_id
                   LEFT JOIN adventure_packs ap ON ap.id = q.pack_id LEFT JOIN patrons pt ON pt.id = q.patron_id
                  WHERE ql.item_id = ?1 ORDER BY q.name",
                [id],
            )?;
            for q in &mut quests {
                booleanize(q, &["is_raid", "is_rare"]);
            }
            item["quests"] = Value::Array(quests);
            item["modifiers"] = Value::Array(modifiers_for(conn, "item", id)?);
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
    table(state, "SELECT id, name, sort_order, category FROM equipment_slots ORDER BY sort_order", &[]).await
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
    table(
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
    table(state, "SELECT id, name, category FROM damage_types ORDER BY id", &[]).await
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
    table(state, "SELECT id, label, family, variant, qualifier FROM augment_slot_types ORDER BY family, label", &[])
        .await
}
