use crate::db::whole_table_json;
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::State;
use axum::Json;
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(stats)).routes(routes!(bonus_types))
}

#[utoipa::path(
    get,
    path = "/v1/stats",
    tag = "bonuses",
    summary = "List stats",
    description = "Every stat a bonus can apply to, with its category (ability, skill, save, spell power and so on). \
                   Bonus rows everywhere else name stats by these names, and /v1/items accepts them in `stat`.",
    responses((status = 200, description = "The whole table", body = Vec<Value>))
)]
async fn stats(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    whole_table_json(state, "SELECT id, name, category FROM stats ORDER BY id", &[]).await
}

#[utoipa::path(
    get,
    path = "/v1/bonus-types",
    tag = "bonuses",
    summary = "List bonus types",
    description = "Every bonus type (Enhancement, Insightful, Quality, ...) and whether two bonuses of that type \
                   stack with each other (`stacks_with_self`). Ids 1 to 33 are the site's own vocabulary; the rest \
                   come from DDOBuilderV2.",
    responses((status = 200, description = "The whole table", body = Vec<Value>))
)]
async fn bonus_types(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    whole_table_json(state, "SELECT id, name, stacks_with_self FROM bonus_types ORDER BY id", &["stacks_with_self"])
        .await
}
