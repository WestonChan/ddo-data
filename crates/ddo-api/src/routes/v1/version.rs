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
    #[schema(schema_with = schema_version_field)]
    pub schema_version: i64,
    #[schema(schema_with = api_commit_field)]
    pub api_commit: Option<String>,
    #[schema(schema_with = dataset_field)]
    pub dataset: DatasetVersion,
    #[schema(schema_with = counts_field)]
    pub counts: BTreeMap<String, i64>,
}

#[allow(dead_code)]
#[derive(ToSchema)]
pub(super) struct VersionDatasetResponse {
    #[schema(schema_with = upstream_sha_field)]
    pub upstream_sha: String,
    #[schema(schema_with = built_at_field)]
    pub built_at: String,
}

fn schema_version_field() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    super::response_schemas::described_schema::<i64>("SQLite schema version of this dataset.")
}

fn api_commit_field() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    super::response_schemas::described_schema::<Option<String>>("API build commit serving this dataset.")
}

fn dataset_field() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    super::response_schemas::described_schema::<VersionDatasetResponse>("Upstream commit and database build time.")
}

fn upstream_sha_field() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    super::response_schemas::described_schema::<String>("Commit of the upstream DDOBuilderV2 data.")
}

fn built_at_field() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    super::response_schemas::described_schema::<String>("UTC time this database was built.")
}

fn counts_field() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    super::response_schemas::described_schema::<BTreeMap<String, i64>>("Counts of each stored entity and link table.")
}

const COUNTED_ROWS: &[(&str, &str)] = &[
    ("items", "items"),
    ("legacy_items", "items WHERE is_legacy"),
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
    ("sources", "sources"),
    ("quest_loot", "sources WHERE kind = 'quest' AND item_id IS NOT NULL"),
    ("quest_augment_loot", "sources WHERE kind = 'quest' AND augment_id IS NOT NULL"),
    ("pack_loot", "sources WHERE kind = 'adventure_pack' AND item_id IS NOT NULL"),
    ("pack_augment_loot", "sources WHERE kind = 'adventure_pack' AND augment_id IS NOT NULL"),
    ("challenge_rewards", "sources WHERE kind = 'challenge'"),
    ("crafting_system_sources", "sources WHERE kind = 'crafting_system'"),
    ("starter_items", "sources WHERE kind = 'starter'"),
    ("vendors", "vendors"),
    ("vendor_items", "sources WHERE kind = 'vendor'"),
    ("events", "events"),
    ("event_items", "sources WHERE kind = 'event'"),
    ("modifiers", "modifiers"),
    ("requirements", "requirements"),
    ("enchantments", "enchantments"),
    ("enchantment_stats", "enchantment_stats"),
    ("item_enchantments", "item_enchantments"),
    ("augment_enchantments", "augment_enchantments"),
    ("set_bonus_tier_enchantments", "set_bonus_tier_enchantments"),
    ("feat_enchantments", "feat_enchantments"),
    ("item_augment_slot_option_enchantments", "item_augment_slot_option_enchantments"),
    ("crafting_systems", "crafting_systems"),
    ("crafting_recipes", "crafting_recipes"),
    ("crafting_ingredients", "crafting_ingredients"),
    ("quest_chains", "quest_chains"),
    ("quest_chain_rewards", "sources WHERE kind = 'quest_chain'"),
    ("sagas", "sagas"),
    ("saga_rewards", "sources WHERE kind = 'saga'"),
    ("wiki_items", "items WHERE provenance = 'wiki'"),
    ("wiki_quests", "quests WHERE provenance = 'wiki'"),
    ("wiki_augments", "augments WHERE provenance = 'wiki'"),
];

#[utoipa::path(
    get,
    path = "/v1/version",
    tag = "meta",
    summary = "Get the dataset version",
    description = "Returns the dataset commit, schema version, build time and row counts.",
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
        api_commit: state.api_commit().map(str::to_string),
        dataset,
        counts: row_counts_by_name,
    }))
}
