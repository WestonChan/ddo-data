use crate::db::table;
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::State;
use axum::Json;
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(adventure_packs)).routes(routes!(patrons)).routes(routes!(quests))
}

#[utoipa::path(
    get,
    path = "/v1/adventure-packs",
    tag = "quests",
    summary = "List adventure packs",
    description = "Every adventure pack and expansion by name with whether it is free to play. /v1/items accepts \
                   these names in `pack`.",
    responses((status = 200, description = "The whole table", body = Vec<Value>))
)]
async fn adventure_packs(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    table(state, "SELECT id, name, is_free_to_play FROM adventure_packs ORDER BY name", &["is_free_to_play"]).await
}

#[utoipa::path(
    get,
    path = "/v1/patrons",
    tag = "quests",
    summary = "List patrons",
    description = "The favor patrons (The Coin Lords, House Kundarak, ...) that quests belong to.",
    responses((status = 200, description = "The whole table", body = Vec<Value>))
)]
async fn patrons(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    table(state, "SELECT id, name FROM patrons ORDER BY name", &[]).await
}

#[utoipa::path(
    get,
    path = "/v1/quests",
    tag = "quests",
    summary = "List quests",
    description = "Every quest, adventure area and challenge with its pack, patron, heroic and epic levels, favor, \
                   whether it is a raid, the `epic_name` its epic version goes by when that differs (null \
                   otherwise), and the `difficulties` it offers as an array in the order casual, normal, hard, \
                   elite, reaper, solo. Challenges (the Cannith and Eveningstar challenge instances) have \
                   `is_challenge` true and a level range from `level` to `max_level`; regular quests have \
                   `max_level` null. Item detail responses reference these in `quests`.",
    responses((status = 200, description = "The whole table", body = Vec<Value>))
)]
async fn quests(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    table(
        state,
        "SELECT q.id, q.name, p.name AS pack, pt.name AS patron, q.level, q.epic_level, q.favor, q.is_raid,
                q.epic_name, q.difficulties, q.is_challenge, q.max_level
           FROM quests q LEFT JOIN adventure_packs p ON p.id = q.pack_id LEFT JOIN patrons pt ON pt.id = q.patron_id
          ORDER BY q.name",
        &["is_raid", "is_challenge"],
    )
    .await
}
