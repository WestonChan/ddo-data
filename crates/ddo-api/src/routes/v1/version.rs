use crate::db::row_count;
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

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(version_report))
}

#[derive(Serialize, ToSchema)]
pub(super) struct VersionReport {
    pub schema_version: i64,
    pub api_commit: Option<&'static str>,
    #[schema(value_type = Object)]
    pub dataset: DatasetVersion,
    pub counts: BTreeMap<String, i64>,
}

const COUNTED_ROWS: &[(&str, &str)] = &[
    ("items", "items"),
    ("augments", "augments"),
    ("set_bonuses", "set_bonuses"),
    ("filigrees", "filigrees"),
    ("sentient_gems", "sentient_gems"),
    ("guild_buffs", "guild_buffs"),
    ("clickies", "clickies"),
    ("feats", "feats"),
    ("races", "races"),
    ("classes", "classes"),
    ("enhancement_trees", "enhancement_trees"),
    ("enhancements", "enhancements"),
    ("spells", "spells"),
    ("optional_buffs", "optional_buffs"),
    ("quests", "quests"),
    ("modifiers", "modifiers"),
    ("requirements", "requirements"),
    ("bonuses", "bonuses"),
    ("crafting_systems", "crafting_systems"),
    ("crafting_recipes", "crafting_recipes"),
    ("crafting_ingredients", "crafting_ingredients"),
    ("wiki_items", "items WHERE source = 'wiki'"),
    ("wiki_quests", "quests WHERE source = 'wiki'"),
    ("corrections", "corrections"),
];

#[utoipa::path(
    get,
    path = "/v1/version",
    tag = "meta",
    summary = "Get the dataset version",
    description = "Which DDOBuilderV2 commit the data was built from and when, which ddo-data commit the API binary \
                   was built from (`api_commit`, null for local builds), the schema version, and row counts for \
                   the main tables plus `wiki_items` and `wiki_quests`, the items and quests read from ddowiki because \
                   DDOBuilderV2 lacks them, and \
                   `corrections`, the known mistakes in DDOBuilderV2's values the dataset replaced. \
                   The dataset SHA is the same value every response carries in its `X-Dataset-Version` header; a change in it means every cached response is stale.",
    responses((status = 200, description = "Dataset, schema and counts", body = VersionReport))
)]
async fn version_report(State(state): State<AppState>) -> Result<Json<VersionReport>, ApiError> {
    let dataset = state.dataset_version().clone();
    let schema_version = state.schema_version();
    let row_counts_by_name = state
        .read_db(|db| {
            let mut row_counts_by_name = BTreeMap::new();
            for (count_name, counted_rows_sql) in COUNTED_ROWS {
                let counted_row_count = row_count(db, &format!("SELECT COUNT(*) FROM {counted_rows_sql}"), [])?;
                row_counts_by_name.insert((*count_name).to_string(), counted_row_count);
            }
            Ok(row_counts_by_name)
        })
        .await?;
    Ok(Json(VersionReport {
        schema_version,
        api_commit: option_env!("DDO_API_COMMIT"),
        dataset,
        counts: row_counts_by_name,
    }))
}
