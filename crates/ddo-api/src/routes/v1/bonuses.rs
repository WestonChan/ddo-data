use crate::db::{
    json_row, json_rows, paged_query, paged_rows_with_filter, paged_table_json, ListPage, TableListSource, WhereClause,
};
use crate::error::ApiError;
use crate::query::{declare_list_parameters, declare_query_parameters, ApiFilterQuery, ApiQuery, ListQuery};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::HashMap;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(stats))
        .routes(routes!(stat_detail))
        .routes(routes!(bonus_types))
        .routes(routes!(enchantments))
        .routes(routes!(enchantment_detail))
}

const STATS_SORT_FIELDS: &[(&str, &str)] =
    &[("name", "listed.name"), ("id", "listed.id"), ("category", "listed.category")];

declare_list_parameters!(StatsParameters, STATS_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/stats",
    tag = "enchantments",
    summary = "List stats",
    description = "Lists stats and their categories for enchantment and bonus lookups.",
    params(StatsParameters),
    responses((status = 200, description = "Paged stats", body = crate::routes::v1::response_schemas::StatsPageResponse),
        (status = 400, description = "Invalid query", body = crate::error::ErrorBody))
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

const BONUS_TYPES_SORT_FIELDS: &[(&str, &str)] =
    &[("name", "listed.name"), ("id", "listed.id"), ("stacks_with_self", "listed.stacks_with_self")];

declare_list_parameters!(BonusTypesParameters, BONUS_TYPES_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/bonus-types",
    tag = "enchantments",
    summary = "List bonus types",
    description = "Lists bonus types and whether each type stacks with itself.",
    params(BonusTypesParameters),
    responses((status = 200, description = "Paged bonus types", body = crate::routes::v1::response_schemas::BonusTypesPageResponse),
        (status = 400, description = "Invalid query", body = crate::error::ErrorBody))
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

const ENCHANTMENT_KINDS: [&str; 2] = ["stat", "enchantment"];

const ENCHANTMENT_VOCABULARY_SQL: &str = "
    SELECT e.id, e.name, 'enchantment' AS kind, '/v1/enchantments/' || e.id AS detail_path,
           (SELECT COUNT(DISTINCT ie.item_id) FROM item_enchantments ie JOIN items i ON i.id = ie.item_id
            WHERE ie.enchantment_id = e.id AND NOT i.is_legacy) AS item_count,
           (SELECT COUNT(DISTINCT ae.augment_id) FROM augment_enchantments ae WHERE ae.enchantment_id = e.id) AS augment_count,
           (SELECT COUNT(DISTINCT t.set_id) FROM set_bonus_tier_enchantments te
            JOIN set_bonus_tiers t ON t.id = te.tier_id WHERE te.enchantment_id = e.id) AS set_count
      FROM enchantments e
    UNION ALL
    SELECT s.id, s.name, 'stat' AS kind, '/v1/stats/' || s.id AS detail_path,
           (SELECT COUNT(DISTINCT ie.item_id) FROM enchantment_stats es
            JOIN item_enchantments ie ON ie.enchantment_id = es.enchantment_id
            JOIN items i ON i.id = ie.item_id WHERE es.stat_id = s.id AND NOT i.is_legacy) AS item_count,
           (SELECT COUNT(DISTINCT ae.augment_id) FROM enchantment_stats es
            JOIN augment_enchantments ae ON ae.enchantment_id = es.enchantment_id WHERE es.stat_id = s.id) AS augment_count,
           (SELECT COUNT(DISTINCT t.set_id) FROM enchantment_stats es
            JOIN set_bonus_tier_enchantments te ON te.enchantment_id = es.enchantment_id
            JOIN set_bonus_tiers t ON t.id = te.tier_id WHERE es.stat_id = s.id) AS set_count
      FROM stats s";

const ENCHANTMENT_TYPES_SQL: &str = "
    SELECT kind, id, name, COUNT(DISTINCT item_id) AS item_count FROM (
        SELECT 'enchantment' AS kind, e.id, bt.name, ie.item_id
          FROM enchantments e JOIN item_enchantments ie ON ie.enchantment_id = e.id
          JOIN items i ON i.id = ie.item_id AND NOT i.is_legacy
          LEFT JOIN enchantment_stats es ON es.enchantment_id = e.id
          JOIN bonus_types bt ON bt.id = COALESCE(es.bonus_type_id, ie.bonus_type_id)
        UNION ALL
        SELECT 'stat', es.stat_id, bt.name, ie.item_id
          FROM enchantment_stats es JOIN item_enchantments ie ON ie.enchantment_id = es.enchantment_id
          JOIN items i ON i.id = ie.item_id AND NOT i.is_legacy
          JOIN bonus_types bt ON bt.id = COALESCE(es.bonus_type_id, ie.bonus_type_id)
        UNION ALL
        SELECT 'enchantment', e.id, bt.name, NULL
          FROM enchantments e JOIN augment_enchantments ae ON ae.enchantment_id = e.id
          LEFT JOIN enchantment_stats es ON es.enchantment_id = e.id
          JOIN bonus_types bt ON bt.id = COALESCE(es.bonus_type_id, ae.bonus_type_id)
        UNION ALL
        SELECT 'stat', es.stat_id, bt.name, NULL
          FROM enchantment_stats es JOIN augment_enchantments ae ON ae.enchantment_id = es.enchantment_id
          JOIN bonus_types bt ON bt.id = COALESCE(es.bonus_type_id, ae.bonus_type_id)
        UNION ALL
        SELECT 'enchantment', e.id, bt.name, NULL
          FROM enchantments e JOIN set_bonus_tier_enchantments te ON te.enchantment_id = e.id
          LEFT JOIN enchantment_stats es ON es.enchantment_id = e.id
          JOIN bonus_types bt ON bt.id = COALESCE(es.bonus_type_id, te.bonus_type_id)
        UNION ALL
        SELECT 'stat', es.stat_id, bt.name, NULL
          FROM enchantment_stats es JOIN set_bonus_tier_enchantments te ON te.enchantment_id = es.enchantment_id
          JOIN bonus_types bt ON bt.id = COALESCE(es.bonus_type_id, te.bonus_type_id)
    ) GROUP BY kind, id, name ORDER BY kind, id, name";

const ENCHANTMENTS_SORT_FIELDS: &[(&str, &str)] = &[
    ("name", "listed.name"),
    ("id", "listed.id"),
    ("kind", "listed.kind"),
    ("detail_path", "listed.detail_path"),
    ("item_count", "listed.item_count"),
    ("augment_count", "listed.augment_count"),
    ("set_count", "listed.set_count"),
];

declare_list_parameters!(EnchantmentsParameters, ENCHANTMENTS_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/enchantments",
    tag = "enchantments",
    summary = "List enchantments and stats",
    description = "Lists every enchantment family and stat with carrier counts, types and a detail path.",
    params(EnchantmentsParameters,
        ("kind" = Option<String>, Query, description = "Keep `enchantment` or `stat` rows only.")),
    responses((status = 200, description = "Paged enchantment vocabulary", body = crate::routes::v1::response_schemas::EnchantmentsPageResponse),
        (status = 400, description = "Invalid kind or query", body = crate::error::ErrorBody))
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
            let mut page = paged_rows_with_filter(
                db,
                ENCHANTMENT_VOCABULARY_SQL,
                &query,
                "listed.name",
                "listed.name, listed.kind",
                ENCHANTMENTS_SORT_FIELDS,
                where_clause,
            )?;
            let types = json_rows(db, ENCHANTMENT_TYPES_SQL, [])?;
            let mut types_by_row: HashMap<(String, i64), Vec<Value>> = HashMap::new();
            for row in types {
                let key = (row["kind"].as_str().unwrap_or("").to_string(), row["id"].as_i64().unwrap_or(0));
                types_by_row
                    .entry(key)
                    .or_default()
                    .push(json!({"name": row["name"], "item_count": row["item_count"]}));
            }
            for row in &mut page.rows {
                let key = (row["kind"].as_str().unwrap_or("").to_string(), row["id"].as_i64().unwrap_or(0));
                row["bonus_types"] = Value::Array(types_by_row.remove(&key).unwrap_or_default());
            }
            Ok(Json(page.into_json("enchantments")))
        })
        .await
}

declare_query_parameters! {
    pub(super) struct BacklinkPages {
        pub items_limit: Option<i64>,
        pub items_offset: Option<i64>,
        pub augments_limit: Option<i64>,
        pub augments_offset: Option<i64>,
        pub set_tiers_limit: Option<i64>,
        pub set_tiers_offset: Option<i64>,
    }
}

const BACKLINK_SORT_FIELDS: &[(&str, &str)] = &[("name", "listed.name"), ("id", "listed.id")];

fn carrier_pages(
    db: &Connection,
    owner_condition: &str,
    id: i64,
    pages: BacklinkPages,
) -> Result<(Value, Value, Value), ApiError> {
    let items_sql = format!(
        "SELECT i.id, i.name, j.value, j.value2, matched.match_id FROM item_enchantments j JOIN items i ON i.id = j.item_id
          {owner_condition} ORDER BY i.name, j.sort_order"
    );
    let augments_sql = format!(
        "SELECT a.id, a.name, j.value, j.value2, matched.match_id FROM augment_enchantments j JOIN augments a ON a.id = j.augment_id
          {owner_condition} ORDER BY a.name, j.sort_order"
    );
    let tiers_sql = format!(
        "SELECT t.id, s.id AS set_id, s.name AS set_name, t.equipped_count, j.value, j.value2, matched.match_id,
                s.name || ' (' || t.equipped_count || ')' AS name
           FROM set_bonus_tier_enchantments j JOIN set_bonus_tiers t ON t.id = j.tier_id
           JOIN set_bonuses s ON s.id = t.set_id {owner_condition} ORDER BY s.name, t.equipped_count, j.sort_order"
    );
    let page = |sql: String, limit: Option<i64>, offset: Option<i64>| -> Result<ListPage, ApiError> {
        let query = ListQuery { q: None, limit, offset, sort: Vec::new() };
        let mut where_clause = WhereClause::default();
        where_clause.add_bound_condition("listed.match_id = ?", id);
        let mut result = paged_query(
            db,
            "*",
            &format!("({sql}) listed"),
            &query,
            "listed.name",
            BACKLINK_SORT_FIELDS,
            &where_clause,
        )?;
        for row in &mut result.rows {
            row.as_object_mut().expect("backlink object").remove("match_id");
        }
        Ok(result)
    };
    let items = page(items_sql, pages.items_limit, pages.items_offset)?.into_json("items");
    let augments = page(augments_sql, pages.augments_limit, pages.augments_offset)?.into_json("augments");
    let tiers = page(tiers_sql, pages.set_tiers_limit, pages.set_tiers_offset)?.into_json("set_tiers");
    Ok((items, augments, tiers))
}

fn carrier_condition(kind: &str) -> String {
    if kind == "enchantment" {
        "JOIN (SELECT id AS match_id FROM enchantments) matched ON matched.match_id = j.enchantment_id".to_string()
    } else {
        "JOIN enchantment_stats es ON es.enchantment_id = j.enchantment_id
         JOIN (SELECT id AS match_id FROM stats) matched ON matched.match_id = es.stat_id"
            .to_string()
    }
}

#[utoipa::path(
    get, path = "/v1/enchantments/{id}", tag = "enchantments", summary = "Get an enchantment",
    description = "Returns a family, its stat rules and ladder, and paged item, augment and set-tier links.",
    params(("id" = i64, Path, description = "Family id from an enchantment-kind vocabulary row."),
        ("items_limit" = Option<i64>, Query, description = "Maximum item links in this page."),
        ("items_offset" = Option<i64>, Query, description = "Item links to skip."),
        ("augments_limit" = Option<i64>, Query, description = "Maximum augment links in this page."),
        ("augments_offset" = Option<i64>, Query, description = "Augment links to skip."),
        ("set_tiers_limit" = Option<i64>, Query, description = "Maximum set-tier links in this page."),
        ("set_tiers_offset" = Option<i64>, Query, description = "Set-tier links to skip.")),
    responses((status = 200, description = "Family and paged carriers", body = crate::routes::v1::response_schemas::EnchantmentsDetailResponse),
        (status = 404, description = "No family has this id", body = crate::error::ErrorBody))
)]
async fn enchantment_detail(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    ApiFilterQuery(pages): ApiFilterQuery<BacklinkPages>,
) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut family = json_row(
                db,
                "SELECT id, name, text_template, description_template, amount_count, wiki_url, stacking_note,
                    default_value, default_value2, ladder_id, ladder_rank FROM enchantments WHERE id = ?1",
                [id],
            )?;
            family["stats"] = Value::Array(json_rows(
                db,
                "SELECT s.name AS stat, bt.name AS bonus_type, es.amount_from, es.constant, es.scale, es.rounding
               FROM enchantment_stats es JOIN stats s ON s.id = es.stat_id
               LEFT JOIN bonus_types bt ON bt.id = es.bonus_type_id
              WHERE es.enchantment_id = ?1 ORDER BY es.sort_order",
                [id],
            )?);
            let ladder_id = family["ladder_id"].as_i64();
            let ladder_rank = family["ladder_rank"].clone();
            family.as_object_mut().expect("family object").remove("ladder_id");
            family.as_object_mut().expect("family object").remove("ladder_rank");
            family["ladder"] = if let Some(ladder_id) = ladder_id {
                let mut ladder = json_row(db, "SELECT id, name FROM enchantment_ladders WHERE id = ?1", [ladder_id])?;
                ladder["steps"] = Value::Array(json_rows(
                    db,
                    "SELECT id, name, ladder_rank AS rank FROM enchantments WHERE ladder_id = ?1 ORDER BY ladder_rank",
                    [ladder_id],
                )?);
                ladder["rank"] = ladder_rank;
                ladder
            } else {
                Value::Null
            };
            let (items, augments, set_tiers) = carrier_pages(db, &carrier_condition("enchantment"), id, pages)?;
            family["items"] = items;
            family["augments"] = augments;
            family["set_tiers"] = set_tiers;
            Ok(Json(family))
        })
        .await
}

#[utoipa::path(
    get, path = "/v1/stats/{id}", tag = "enchantments", summary = "Get a stat",
    description = "Returns a stat with paged item, augment and set-tier links through its families.",
    params(("id" = i64, Path, description = "Stat id from a stat-kind vocabulary row."),
        ("items_limit" = Option<i64>, Query, description = "Maximum item links in this page."),
        ("items_offset" = Option<i64>, Query, description = "Item links to skip."),
        ("augments_limit" = Option<i64>, Query, description = "Maximum augment links in this page."),
        ("augments_offset" = Option<i64>, Query, description = "Augment links to skip."),
        ("set_tiers_limit" = Option<i64>, Query, description = "Maximum set-tier links in this page."),
        ("set_tiers_offset" = Option<i64>, Query, description = "Set-tier links to skip.")),
    responses((status = 200, description = "Stat and paged carriers", body = crate::routes::v1::response_schemas::StatsDetailResponse),
        (status = 404, description = "No stat has this id", body = crate::error::ErrorBody))
)]
async fn stat_detail(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    ApiFilterQuery(pages): ApiFilterQuery<BacklinkPages>,
) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut stat = json_row(db, "SELECT id, name, category FROM stats WHERE id = ?1", [id])?;
            let (items, augments, set_tiers) = carrier_pages(db, &carrier_condition("stat"), id, pages)?;
            stat["items"] = items;
            stat["augments"] = augments;
            stat["set_tiers"] = set_tiers;
            Ok(Json(stat))
        })
        .await
}
