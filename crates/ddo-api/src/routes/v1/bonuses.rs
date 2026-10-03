use crate::db::{paged_rows_with_filter, paged_table_json, TableListSource, WhereClause};
use crate::error::ApiError;
use crate::query::{declare_list_parameters, declare_query_parameters, ApiQuery};
use crate::state::AppState;
use axum::extract::State;
use axum::Json;
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(stats)).routes(routes!(bonus_types)).routes(routes!(enchantments))
}

const STATS_SORT_FIELDS: &[(&str, &str)] =
    &[("name", "listed.name"), ("id", "listed.id"), ("category", "listed.category")];

declare_list_parameters!(StatsParameters, STATS_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/stats",
    tag = "bonuses",
    summary = "List stats",
    description = "Every stat a bonus can apply to, with its category (ability, skill, save, spell power and so on). \
                   Bonus rows everywhere else name stats by these names, and /v1/items accepts them in `enchantment`.",
    params(
        StatsParameters,
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `stats` page", body = Value),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn stats(State(state): State<AppState>, ApiQuery(query, _): ApiQuery) -> Result<Json<Value>, ApiError> {
    paged_table_json(
        state,
        query,
        TableListSource {
            select_sql: "SELECT id, name, category FROM stats",
            rows_key: "stats",
            name_column: "listed.name",
            default_order: "listed.id",
            sortable_fields: STATS_SORT_FIELDS,
            flag_columns: &[],
        },
    )
    .await
}

const BONUS_TYPES_SORT_FIELDS: &[(&str, &str)] = &[("name", "listed.name"), ("id", "listed.id")];

declare_list_parameters!(BonusTypesParameters, BONUS_TYPES_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/bonus-types",
    tag = "bonuses",
    summary = "List bonus types",
    description = "Every bonus type (Enhancement, Insightful, Quality, ...) and whether two bonuses of that type \
                   stack with each other (`stacks_with_self`). Ids 1 to 33 are the site's own vocabulary; the rest \
                   come from DDOBuilderV2.",
    params(
        BonusTypesParameters,
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `bonus_types` page", body = Value),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn bonus_types(State(state): State<AppState>, ApiQuery(query, _): ApiQuery) -> Result<Json<Value>, ApiError> {
    paged_table_json(
        state,
        query,
        TableListSource {
            select_sql: "SELECT id, name, stacks_with_self FROM bonus_types",
            rows_key: "bonus_types",
            name_column: "listed.name",
            default_order: "listed.id",
            sortable_fields: BONUS_TYPES_SORT_FIELDS,
            flag_columns: &["stacks_with_self"],
        },
    )
    .await
}

declare_query_parameters! {
    pub(super) struct EnchantmentFilters {
        pub kind: Option<String>,
    }
}

const ENCHANTMENT_KINDS: [&str; 2] = ["stat", "effect"];

const ENCHANTMENTS_CARRIED_BY_ITEMS_SQL: &str = "SELECT name, kind, item_count FROM (
         SELECT s.name, 'stat' AS kind, COUNT(DISTINCT ib.item_id) AS item_count
         FROM stats s JOIN bonuses b ON b.stat_id = s.id JOIN item_bonuses ib ON ib.bonus_id = b.id
         JOIN items i ON i.id = ib.item_id WHERE NOT i.is_legacy GROUP BY s.id
         UNION ALL
         SELECT e.name, 'effect' AS kind, COUNT(DISTINCT ie.item_id) AS item_count
         FROM effects e JOIN item_effects ie ON ie.effect_id = e.id
         JOIN items i ON i.id = ie.item_id WHERE NOT i.is_legacy GROUP BY e.id
     )";

const ENCHANTMENTS_SORT_FIELDS: &[(&str, &str)] =
    &[("name", "listed.name"), ("kind", "listed.kind"), ("item_count", "listed.item_count")];

declare_list_parameters!(EnchantmentsParameters, ENCHANTMENTS_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/enchantments",
    tag = "bonuses",
    summary = "List enchantments",
    description = "The vocabulary of /v1/items' `enchantment` filter, for an enchantment picker: every stat (as \
                   /v1/stats lists it) and every named effect (as an item's `effects` give it) that at least one \
                   item carries, each as `name`, `kind` (`stat` or `effect`) and `item_count`, the number of items \
                   carrying it: items with a bonus of their own to the stat (set tiers are not counted), or items \
                   whose `effects` name the effect. Legacy items (`is_legacy`) are not counted, so the counts match \
                   what /v1/items lists by default; names only legacy items or no item carries are left out. A name that is both a stat \
                   and an effect appears once per kind. Ordered by name, a stat before an effect of the same name.",
    params(
        EnchantmentsParameters,
        ("kind" = Option<String>, Query,
            description = "`stat` or `effect` keeps only names of that kind; unset lists both; anything else is a 400"),
    ),
    responses(
        (status = 200, description = "`total`, `limit`, `offset` and the `enchantments` page", body = Value),
        (status = 400, description = "Unknown kind, or an unknown or malformed query parameter", body = crate::error::ErrorBody)
    )
)]
async fn enchantments(
    State(state): State<AppState>,
    ApiQuery(query, filters): ApiQuery<EnchantmentFilters>,
) -> Result<Json<Value>, ApiError> {
    if let Some(kind) = &filters.kind {
        if !ENCHANTMENT_KINDS.contains(&kind.as_str()) {
            return Err(ApiError::BadRequest(format!("unknown kind {kind:?}")));
        }
    }
    state
        .read_db(move |db| {
            let mut where_clause = WhereClause::default();
            if let Some(kind) = &filters.kind {
                where_clause.add_bound_condition("kind = ?", kind.clone());
            }
            let page = paged_rows_with_filter(
                db,
                ENCHANTMENTS_CARRIED_BY_ITEMS_SQL,
                &query,
                "listed.name",
                "listed.name, listed.kind DESC",
                ENCHANTMENTS_SORT_FIELDS,
                where_clause,
            )?;
            Ok(Json(page.into_json("enchantments")))
        })
        .await
}
