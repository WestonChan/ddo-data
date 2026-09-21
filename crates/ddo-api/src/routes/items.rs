//! Items: a filterable list shaped for the picker, and a detail view joining every satellite.

use crate::db::{bonuses_via, booleanize, count, json_row, json_rows, like_pattern, modifiers_for, page, Filters};
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use ddo_model::enums::ItemCategory;
use serde::Deserialize;
use serde_json::{json, Value};
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(list)).routes(routes!(detail))
}

#[derive(Deserialize, IntoParams)]
pub struct ItemFilter {
    /// Case-insensitive substring of the name.
    pub q: Option<String>,
    /// Equipment slot name, e.g. `Main Hand`.
    pub slot: Option<String>,
    /// `Armor`, `Shield`, `Weapon`, `Jewelry` or `Clothing`.
    pub category: Option<String>,
    pub min_level: Option<i64>,
    pub max_level: Option<i64>,
    /// Adventure pack name; matches items dropping in that pack's quests.
    pub pack: Option<String>,
    /// `true` for raid loot only.
    pub raid: Option<bool>,
    /// Stat name; matches items with a bonus to it.
    pub stat: Option<String>,
    /// Page size, 1–10000 (default 100).
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Items matching the filters, with the fields the picker shows.
#[utoipa::path(get, path = "/v1/items", tag = "items", params(ItemFilter), responses((status = 200, body = Value)))]
async fn list(State(state): State<AppState>, Query(f): Query<ItemFilter>) -> Result<Json<Value>, ApiError> {
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
                        EXISTS (SELECT 1 FROM quest_loot ql WHERE ql.item_id = i.id AND ql.loot_type = 'raid') AS is_raid
                 {from} ORDER BY i.name LIMIT {limit} OFFSET {offset}"
            );
            let mut items = json_rows(conn, &sql, f_.params())?;
            for item in &mut items {
                booleanize(item, &["is_raid"]);
            }
            Ok(Json(json!({ "total": total, "limit": limit, "offset": offset, "items": items })))
        })
        .await
}

/// One item with its weapon or armor stats, bonuses, effects, sockets, clickies, set and quests.
#[utoipa::path(get, path = "/v1/items/{id}", tag = "items", params(("id" = i64, Path)), responses((status = 200, body = Value), (status = 404, body = crate::error::ErrorBody)))]
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
                "SELECT q.id, q.name, q.level, q.epic_level, q.is_raid, ap.name AS pack, pt.name AS patron, ql.loot_type
                   FROM quest_loot ql JOIN quests q ON q.id = ql.quest_id
                   LEFT JOIN adventure_packs ap ON ap.id = q.pack_id LEFT JOIN patrons pt ON pt.id = q.patron_id
                  WHERE ql.item_id = ?1 ORDER BY q.name",
                [id],
            )?;
            for q in &mut quests {
                booleanize(q, &["is_raid"]);
            }
            item["quests"] = Value::Array(quests);
            item["modifiers"] = Value::Array(modifiers_for(conn, "item", id)?);
            Ok(Json(item))
        })
        .await
}
