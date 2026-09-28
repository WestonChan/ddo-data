use crate::db::count;
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::State;
use axum::Json;
use ddo_model::DatasetVersion;
use serde::Serialize;
use std::collections::BTreeMap;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(version))
}

#[derive(Serialize, ToSchema)]
pub struct VersionInfo {
    pub schema_version: i64,
    pub api_commit: Option<&'static str>,
    #[schema(value_type = Object)]
    pub dataset: DatasetVersion,
    pub counts: BTreeMap<String, i64>,
}

const COUNTED: &[&str] = &[
    "items",
    "augments",
    "set_bonuses",
    "filigrees",
    "sentient_gems",
    "guild_buffs",
    "clickies",
    "feats",
    "races",
    "classes",
    "enhancement_trees",
    "enhancements",
    "spells",
    "optional_buffs",
    "quests",
    "modifiers",
    "requirements",
    "bonuses",
    "crafting_systems",
    "crafting_recipes",
    "crafting_ingredients",
];

#[utoipa::path(
    get,
    path = "/v1/version",
    tag = "meta",
    summary = "Get the dataset version",
    description = "Which DDOBuilderV2 commit the data was built from and when, which ddo-data commit the API binary \
                   was built from (`api_commit`, null for local builds), the schema version, and row counts for \
                   the main tables. The dataset SHA is the same value every response carries in its \
                   `X-Dataset-Version` header; a change in it means every cached response is stale.",
    responses((status = 200, description = "Dataset, schema and counts", body = VersionInfo))
)]
async fn version(State(state): State<AppState>) -> Result<Json<VersionInfo>, ApiError> {
    let dataset = state.dataset().clone();
    let schema_version = state.schema_version();
    let counts = state
        .query(|conn| {
            let mut counts = BTreeMap::new();
            for table in COUNTED {
                counts.insert((*table).to_string(), count(conn, &format!("SELECT COUNT(*) FROM {table}"), [])?);
            }
            Ok(counts)
        })
        .await?;
    Ok(Json(VersionInfo { schema_version, api_commit: option_env!("DDO_API_COMMIT"), dataset, counts }))
}
