use crate::db::{bonuses_via, convert_to_booleans, json_row, json_rows, modifiers_for, whole_table_json};
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(sets))
        .routes(routes!(set_detail))
        .routes(routes!(filigrees))
        .routes(routes!(sentient_gems))
}

#[utoipa::path(
    get,
    path = "/v1/sets",
    tag = "sets",
    summary = "List sets",
    description = "Every set bonus ordered by name with its icon, whether it is a filigree set rather than a gear \
                   set, and how many items, augments and tiers it has (`item_count`, `augment_count`, `tier_count`). \
                   Tiers, items, augments and filigrees are on the detail endpoint.",
    responses((status = 200, description = "All set bonuses", body = Vec<Value>))
)]
async fn sets(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    state
        .read_db(|db| {
            let mut sets = json_rows(
                db,
                "SELECT s.id, s.name, s.icon, s.is_filigree_set,
                        (SELECT COUNT(*) FROM set_bonus_items i WHERE i.set_id = s.id) AS item_count,
                        (SELECT COUNT(*) FROM set_bonus_augments a WHERE a.set_id = s.id) AS augment_count,
                        (SELECT COUNT(*) FROM set_bonus_tiers t WHERE t.set_id = s.id) AS tier_count
                   FROM set_bonuses s ORDER BY s.name",
                [],
            )?;
            for set in &mut sets {
                convert_to_booleans(set, &["is_filigree_set"]);
            }
            Ok(Json(sets))
        })
        .await
}

#[utoipa::path(
    get,
    path = "/v1/sets/{id}",
    tag = "sets",
    summary = "Get a set",
    description = "One set with its `tiers` (how many pieces equipped unlock which `bonuses`, the stat, bonus type and \
                   value derived from each tier's plain stat effects as an item's are, and which raw `modifiers`), the `items` \
                   that count towards it with slot and minimum level, the `augments` whose slotting grants it (id, \
                   name, minimum level; crafting-system and named augments), and for filigree sets the `filigrees`.",
    params(("id" = i64, Path, description = "The set's numeric id from the list endpoint")), responses((status = 200, description = "The set with its child collections", body = Value), (status = 404, description = "No set has this id", body = crate::error::ErrorBody))
)]
async fn set_detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut set = json_row(db, "SELECT id, name, icon, is_filigree_set FROM set_bonuses WHERE id = ?1", [id])?;
            convert_to_booleans(&mut set, &["is_filigree_set"]);
            let mut tiers = json_rows(
                db,
                "SELECT id, equipped_count, description FROM set_bonus_tiers WHERE set_id = ?1 ORDER BY equipped_count",
                [id],
            )?;
            for tier in &mut tiers {
                let tier_id = tier["id"].as_i64().unwrap_or(0);
                tier["bonuses"] = Value::Array(bonuses_via(db, "set_bonus_tier_bonuses", "tier_id", tier_id)?);
                tier["modifiers"] = Value::Array(modifiers_for(db, "set_bonus_tier", tier_id)?);
            }
            set["tiers"] = Value::Array(tiers);
            set["items"] = Value::Array(json_rows(
                db,
                "SELECT i.id, i.name, es.name AS slot, i.minimum_level FROM set_bonus_items sbi JOIN items i ON i.id = sbi.item_id
                   JOIN equipment_slots es ON es.id = i.slot_id WHERE sbi.set_id = ?1 ORDER BY i.name",
                [id],
            )?);
            set["augments"] = Value::Array(json_rows(
                db,
                "SELECT a.id, a.name, a.min_level FROM set_bonus_augments sba JOIN augments a ON a.id = sba.augment_id
                  WHERE sba.set_id = ?1 ORDER BY a.name, a.id",
                [id],
            )?);
            set["filigrees"] = Value::Array(filigrees_matching_set(db, Some(id))?);
            Ok(Json(set))
        })
        .await
}

fn filigrees_matching_set(db: &rusqlite::Connection, set_id: Option<i64>) -> Result<Vec<Value>, ApiError> {
    let sql = "SELECT f.id, f.name, f.description, f.icon, f.menu, f.set_id, s.name AS set_name FROM filigrees f
                 LEFT JOIN set_bonuses s ON s.id = f.set_id WHERE (?1 IS NULL OR f.set_id = ?1) ORDER BY f.name";
    let mut filigrees = json_rows(db, sql, [set_id])?;
    for filigree in &mut filigrees {
        let filigree_id = filigree["id"].as_i64().unwrap_or(0);
        filigree["modifiers"] = Value::Array(modifiers_for(db, "filigree", filigree_id)?);
    }
    Ok(filigrees)
}

#[utoipa::path(
    get,
    path = "/v1/filigrees",
    tag = "sets",
    summary = "List filigrees",
    description = "Every sentient-weapon filigree ordered by name with its description, icon, crafting `menu`, the \
                   set it belongs to, and the raw `modifiers` it applies on its own.",
    responses((status = 200, description = "All filigrees", body = Vec<Value>))
)]
async fn filigrees(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    state.read_db(|db| Ok(Json(filigrees_matching_set(db, None)?))).await
}

#[utoipa::path(
    get,
    path = "/v1/sentient-gems",
    tag = "sets",
    summary = "List sentient gems",
    description = "Every sentient jewel a sentient weapon or accessory can carry, ordered by name, with its icon \
                   (served from the `sentient-gems` icon family) and description, which upstream uses for the voice \
                   actor credit. Filigrees slot into the jewel; they are listed by /v1/filigrees.",
    responses((status = 200, description = "Every sentient gem", body = Vec<Value>))
)]
async fn sentient_gems(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    whole_table_json(state, "SELECT id, name, icon, description FROM sentient_gems ORDER BY name", &[]).await
}
