use crate::db::{json_rows, substring_like_pattern, whole_table_json, WhereClause};
use crate::error::ApiError;
use crate::query::{ApiQuery, QueryParameters};
use crate::state::AppState;
use axum::extract::State;
use axum::Json;
use serde::Deserialize;
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(stats)).routes(routes!(bonus_types)).routes(routes!(enchantments))
}

#[utoipa::path(
    get,
    path = "/v1/stats",
    tag = "bonuses",
    summary = "List stats",
    description = "Every stat a bonus can apply to, with its category (ability, skill, save, spell power and so on). \
                   Bonus rows everywhere else name stats by these names, and /v1/items accepts them in `enchantment`.",
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EnchantmentListQuery {
    pub q: Option<String>,
    pub kind: Option<String>,
}

impl QueryParameters for EnchantmentListQuery {}

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
        ("q" = Option<String>, Query, description = "Case-insensitive substring of the enchantment name; unset or blank lists every name"),
        ("kind" = Option<String>, Query, description = "`stat` or `effect` keeps only names of that kind; unset lists both; anything else is a 400")
    ),
    responses(
        (status = 200, description = "Every matching name with its kind and item count", body = Vec<Value>),
        (status = 400, description = "Unknown kind, or an unknown or malformed query parameter", body = crate::error::ErrorBody)
    )
)]
async fn enchantments(
    State(state): State<AppState>,
    ApiQuery(query): ApiQuery<EnchantmentListQuery>,
) -> Result<Json<Vec<Value>>, ApiError> {
    if let Some(kind) = &query.kind {
        if !ENCHANTMENT_KINDS.contains(&kind.as_str()) {
            return Err(ApiError::BadRequest(format!("unknown kind {kind:?}")));
        }
    }
    state
        .read_db(move |db| {
            let mut where_clause = WhereClause::default();
            if let Some(search_text) = query.q.as_deref().filter(|q| !q.trim().is_empty()) {
                where_clause.add_bound_condition("name LIKE ? ESCAPE '\\'", substring_like_pattern(search_text));
            }
            if let Some(kind) = &query.kind {
                where_clause.add_bound_condition("kind = ?", kind.clone());
            }
            let where_sql = where_clause.to_sql();
            let enchantments_sql = format!("{ENCHANTMENTS_CARRIED_BY_ITEMS_SQL} {where_sql} ORDER BY name, kind DESC");
            Ok(Json(json_rows(db, &enchantments_sql, where_clause.params())?))
        })
        .await
}
