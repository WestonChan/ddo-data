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
    pub api_commit: Option<String>,
    #[schema(value_type = Object)]
    pub dataset: DatasetVersion,
    pub counts: BTreeMap<String, i64>,
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
    ("bonuses", "bonuses"),
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
    description = "Which DDOBuilderV2 commit the data was built from and when, which ddo-data commit the API binary \
                   was built from (`api_commit`, null for local builds), the schema version, and row counts for \
                   the main tables (`legacy_items` counts the items flagged `is_legacy`, which /v1/items leaves out unless `include_legacy=true`; `sources` counts every link between loot and a source, and splits by kind into `quest_loot` and `quest_augment_loot`, a quest's item and augment drops, `quest_chain_rewards` and `saga_rewards`, the items each quest chain's and saga's end reward offers, `pack_loot` and `pack_augment_loot`, the items and augments credited to any quest of an adventure pack, `challenge_rewards`, the items and augments a challenge pack's turn-in rewards offer, `crafting_system_sources`, the items and augments a wiki crafting system makes, `starter_items`, the iconic heroes' starter gear, and `vendor_items` and `event_items`, the items each vendor offers and each event rewards; `vendors` and `events` count those read from ddowiki) plus `wiki_items`, `wiki_quests` and `wiki_augments`, the items, quests and augments read from ddowiki because \
                   DDOBuilderV2 lacks them. \
                   The dataset SHA is the same value every response carries in its `X-Dataset-Version` header; a change in it or in `api_commit` changes every `ETag`, so every cached response revalidates to fresh content.",
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
