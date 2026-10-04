use crate::db::{modifiers_for, paged_rows};
use crate::error::ApiError;
use crate::query::{declare_list_parameters, ApiQuery, ListQuery};
use crate::state::AppState;
use axum::extract::State;
use axum::Json;
use ddo_model::enums::ModifierSource;
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(guild_buffs)).routes(routes!(optional_buffs))
}

fn buffs_with_modifiers(
    db: &rusqlite::Connection,
    select_sql: &str,
    query: &ListQuery,
    rows_key: &str,
    default_order: &str,
    sortable_fields: &[(&str, &str)],
    modifier_source: ModifierSource,
) -> Result<Json<Value>, ApiError> {
    let mut page = paged_rows(db, select_sql, query, "listed.name", default_order, sortable_fields)?;
    for buff in &mut page.rows {
        let buff_id = buff["id"].as_i64().unwrap_or(0);
        buff["modifiers"] = Value::Array(modifiers_for(db, modifier_source.as_str(), buff_id)?);
    }
    Ok(Json(page.into_json(rows_key)))
}

const GUILD_BUFFS_SORT_FIELDS: &[(&str, &str)] = &[
    ("name", "listed.name"),
    ("id", "listed.id"),
    ("guild_level", "listed.guild_level"),
    ("description", "description"),
];

declare_list_parameters!(GuildBuffsParameters, GUILD_BUFFS_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/guild-buffs",
    tag = "buffs",
    summary = "List guild buffs",
    description = "Lists guild buffs with unlock levels and modifiers.",
    params(
        GuildBuffsParameters,
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `guild_buffs` page", body = crate::routes::v1::response_schemas::GuildBuffsPageResponse),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn guild_buffs(State(state): State<AppState>, ApiQuery(query, _): ApiQuery) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            buffs_with_modifiers(
                db,
                "SELECT id, name, description, guild_level FROM guild_buffs",
                &query,
                "guild_buffs",
                "listed.guild_level, listed.name",
                GUILD_BUFFS_SORT_FIELDS,
                ModifierSource::GuildBuff,
            )
        })
        .await
}

const OPTIONAL_BUFFS_SORT_FIELDS: &[(&str, &str)] =
    &[("name", "listed.name"), ("id", "listed.id"), ("description", "description"), ("icon", "icon")];

declare_list_parameters!(OptionalBuffsParameters, OPTIONAL_BUFFS_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/optional-buffs",
    tag = "buffs",
    summary = "List optional buffs",
    description = "Lists optional planner buffs with their modifiers.",
    params(
        OptionalBuffsParameters,
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `optional_buffs` page", body = crate::routes::v1::response_schemas::OptionalBuffsPageResponse),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn optional_buffs(State(state): State<AppState>, ApiQuery(query, _): ApiQuery) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            buffs_with_modifiers(
                db,
                "SELECT id, name, icon, description FROM optional_buffs",
                &query,
                "optional_buffs",
                "listed.name",
                OPTIONAL_BUFFS_SORT_FIELDS,
                ModifierSource::OptionalBuff,
            )
        })
        .await
}
