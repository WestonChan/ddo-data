use crate::db::{booleanize, json_row, json_rows, modifiers_for};
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(list)).routes(routes!(detail)).routes(routes!(filigrees))
}

#[utoipa::path(
    get,
    path = "/v1/sets",
    tag = "sets",
    summary = "List sets",
    description = "Every set bonus ordered by name with its icon, whether it is a filigree set rather than a gear \
                   set, and how many items and tiers it has. Tiers, items and filigrees are on the detail endpoint.",
    responses((status = 200, description = "All set bonuses", body = Vec<Value>))
)]
async fn list(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    state
        .query(|conn| {
            let mut rows = json_rows(
                conn,
                "SELECT s.id, s.name, s.icon, s.is_filigree_set,
                        (SELECT COUNT(*) FROM set_bonus_items i WHERE i.set_id = s.id) AS item_count,
                        (SELECT COUNT(*) FROM set_bonus_tiers t WHERE t.set_id = s.id) AS tier_count
                   FROM set_bonuses s ORDER BY s.name",
                [],
            )?;
            for r in &mut rows {
                booleanize(r, &["is_filigree_set"]);
            }
            Ok(Json(rows))
        })
        .await
}

#[utoipa::path(
    get,
    path = "/v1/sets/{id}",
    tag = "sets",
    summary = "Get a set",
    description = "One set with its `tiers` (how many pieces equipped unlock which raw `modifiers`), the `items` \
                   that count towards it with slot and minimum level, and for filigree sets the `filigrees`.",
    params(("id" = i64, Path, description = "The set's numeric id from the list endpoint")), responses((status = 200, description = "The set with its child collections", body = Value), (status = 404, description = "No set has this id", body = crate::error::ErrorBody))
)]
async fn detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .query(move |conn| {
            let mut set = json_row(conn, "SELECT id, name, icon, is_filigree_set FROM set_bonuses WHERE id = ?1", [id])?;
            booleanize(&mut set, &["is_filigree_set"]);
            let mut tiers = json_rows(
                conn,
                "SELECT id, equipped_count, description FROM set_bonus_tiers WHERE set_id = ?1 ORDER BY equipped_count",
                [id],
            )?;
            for t in &mut tiers {
                let tier_id = t["id"].as_i64().unwrap_or(0);
                t["modifiers"] = Value::Array(modifiers_for(conn, "set_bonus_tier", tier_id)?);
            }
            set["tiers"] = Value::Array(tiers);
            set["items"] = Value::Array(json_rows(
                conn,
                "SELECT i.id, i.name, es.name AS slot, i.minimum_level FROM set_bonus_items sbi JOIN items i ON i.id = sbi.item_id
                   JOIN equipment_slots es ON es.id = i.slot_id WHERE sbi.set_id = ?1 ORDER BY i.name",
                [id],
            )?);
            set["filigrees"] = Value::Array(filigree_rows(conn, Some(id))?);
            Ok(Json(set))
        })
        .await
}

fn filigree_rows(conn: &rusqlite::Connection, set_id: Option<i64>) -> Result<Vec<Value>, ApiError> {
    let sql = "SELECT f.id, f.name, f.description, f.icon, f.menu, f.set_id, s.name AS set_name FROM filigrees f
                 LEFT JOIN set_bonuses s ON s.id = f.set_id WHERE (?1 IS NULL OR f.set_id = ?1) ORDER BY f.name";
    let mut rows = json_rows(conn, sql, [set_id])?;
    for f in &mut rows {
        let id = f["id"].as_i64().unwrap_or(0);
        f["modifiers"] = Value::Array(modifiers_for(conn, "filigree", id)?);
    }
    Ok(rows)
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
    state.query(|conn| Ok(Json(filigree_rows(conn, None)?))).await
}
