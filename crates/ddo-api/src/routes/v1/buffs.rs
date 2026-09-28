use crate::db::{json_rows, modifiers_for};
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::State;
use axum::Json;
use ddo_model::enums::ModifierSource;
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(guild_buffs)).routes(routes!(optional_buffs))
}

fn with_modifiers(
    conn: &rusqlite::Connection,
    sql: &str,
    source: ModifierSource,
) -> Result<Json<Vec<Value>>, ApiError> {
    let mut rows = json_rows(conn, sql, [])?;
    for row in &mut rows {
        let id = row["id"].as_i64().unwrap_or(0);
        row["modifiers"] = Value::Array(modifiers_for(conn, source.as_str(), id)?);
    }
    Ok(Json(rows))
}

#[utoipa::path(
    get,
    path = "/v1/guild-buffs",
    tag = "buffs",
    summary = "List guild buffs",
    description = "Every guild airship amenity ordered by the guild level that unlocks it (`guild_level`), then \
                   name, with its description and the raw `modifiers` it applies. Most modifiers have `amount_type` \
                   `TotalLevel` and 40 `amounts`, one per character level, so the bonus a character gets is the \
                   entry at its level minus one; the rest are `Simple` with a single amount.",
    responses((status = 200, description = "Every guild buff", body = Vec<Value>))
)]
async fn guild_buffs(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    state
        .query(|conn| {
            with_modifiers(
                conn,
                "SELECT id, name, description, guild_level FROM guild_buffs ORDER BY guild_level, name",
                ModifierSource::GuildBuff,
            )
        })
        .await
}

#[utoipa::path(
    get,
    path = "/v1/optional-buffs",
    tag = "buffs",
    summary = "List optional buffs",
    description = "The spell, enhancement, epic destiny, bard song, monk finisher, potion, Stone of Change ritual \
                   and legacy guild buffs a planner can toggle on for a character, ordered by name, each with its \
                   icon, description and the raw `modifiers` it applies. A modifier's `requirements` name the \
                   stance or gear it needs, e.g. a shield ritual that only counts with the Shield stance.",
    responses((status = 200, description = "Every optional buff", body = Vec<Value>))
)]
async fn optional_buffs(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    state
        .query(|conn| {
            with_modifiers(
                conn,
                "SELECT id, name, icon, description FROM optional_buffs ORDER BY name",
                ModifierSource::OptionalBuff,
            )
        })
        .await
}
