//! Small reference vocabularies. Each is the whole table.

use crate::db::{booleanize, json_rows};
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::State;
use axum::Json;
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(stats))
        .routes(routes!(bonus_types))
        .routes(routes!(equipment_slots))
        .routes(routes!(weapon_types))
        .routes(routes!(damage_types))
        .routes(routes!(augment_slot_types))
        .routes(routes!(adventure_packs))
        .routes(routes!(patrons))
        .routes(routes!(quests))
}

async fn table(
    state: AppState,
    sql: &'static str,
    flags: &'static [&'static str],
) -> Result<Json<Vec<Value>>, ApiError> {
    let rows = state
        .query(move |conn| {
            let mut rows = json_rows(conn, sql, [])?;
            for row in &mut rows {
                booleanize(row, flags);
            }
            Ok(rows)
        })
        .await?;
    Ok(Json(rows))
}

/// Every stat a bonus can apply to.
#[utoipa::path(get, path = "/v1/stats", tag = "lookups", responses((status = 200, body = Vec<Value>)))]
async fn stats(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    table(state, "SELECT id, name, category FROM stats ORDER BY id", &[]).await
}

/// Bonus types and whether two of the same type stack.
#[utoipa::path(get, path = "/v1/bonus-types", tag = "lookups", responses((status = 200, body = Vec<Value>)))]
async fn bonus_types(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    table(state, "SELECT id, name, stacks_with_self FROM bonus_types ORDER BY id", &["stacks_with_self"]).await
}

#[utoipa::path(get, path = "/v1/equipment-slots", tag = "lookups", responses((status = 200, body = Vec<Value>)))]
async fn equipment_slots(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    table(state, "SELECT id, name, sort_order, category FROM equipment_slots ORDER BY sort_order", &[]).await
}

#[utoipa::path(get, path = "/v1/weapon-types", tag = "lookups", responses((status = 200, body = Vec<Value>)))]
async fn weapon_types(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    table(
        state,
        "SELECT wt.id, wt.name, p.name AS proficiency, wt.is_shield FROM weapon_types wt
           LEFT JOIN weapon_proficiencies p ON p.id = wt.proficiency_id ORDER BY wt.id",
        &["is_shield"],
    )
    .await
}

#[utoipa::path(get, path = "/v1/damage-types", tag = "lookups", responses((status = 200, body = Vec<Value>)))]
async fn damage_types(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    table(state, "SELECT id, name, category FROM damage_types ORDER BY id", &[]).await
}

/// Sockets an item can carry: gem colours and crafting-family slots.
#[utoipa::path(get, path = "/v1/augment-slot-types", tag = "lookups", responses((status = 200, body = Vec<Value>)))]
async fn augment_slot_types(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    table(state, "SELECT id, label, family, variant, qualifier FROM augment_slot_types ORDER BY family, label", &[])
        .await
}

#[utoipa::path(get, path = "/v1/adventure-packs", tag = "lookups", responses((status = 200, body = Vec<Value>)))]
async fn adventure_packs(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    table(state, "SELECT id, name, is_free_to_play FROM adventure_packs ORDER BY name", &["is_free_to_play"]).await
}

#[utoipa::path(get, path = "/v1/patrons", tag = "lookups", responses((status = 200, body = Vec<Value>)))]
async fn patrons(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    table(state, "SELECT id, name FROM patrons ORDER BY name", &[]).await
}

/// Every quest and adventure zone with its pack, patron, levels and favor.
#[utoipa::path(get, path = "/v1/quests", tag = "lookups", responses((status = 200, body = Vec<Value>)))]
async fn quests(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    table(
        state,
        "SELECT q.id, q.name, p.name AS pack, pt.name AS patron, q.level, q.epic_level, q.favor, q.is_raid
           FROM quests q LEFT JOIN adventure_packs p ON p.id = q.pack_id LEFT JOIN patrons pt ON pt.id = q.patron_id
          ORDER BY q.name",
        &["is_raid"],
    )
    .await
}
