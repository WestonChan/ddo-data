use crate::db::{
    effect_bonus_value_sql, json_row, json_rows, paged_query, paged_table_json, ListPage, TableListSource, WhereClause,
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
        .routes(routes!(effects))
        .routes(routes!(effect_detail))
}

const STATS_SORT_FIELDS: &[(&str, &str)] =
    &[("name", "listed.name"), ("id", "listed.id"), ("category", "listed.category")];

declare_list_parameters!(StatsParameters, STATS_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/stats",
    tag = "bonuses",
    summary = "List stats",
    description = "Lists stats and their categories for effect and bonus lookups.",
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
    tag = "bonuses",
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
    pub(super) struct EffectFilters {
        pub kind: Option<String>,
    }
}

const EFFECT_KINDS: [&str; 2] = ["stat", "effect"];

const EFFECT_VOCABULARY_SQL: &str = "
    SELECT e.id, e.name, 'effect' AS kind, '/v1/effects/' || e.id AS detail_path,
           c.item_count, c.augment_count, c.set_count
      FROM effects e JOIN effect_vocabulary_counts c ON c.kind = 'effect' AND c.id = e.id
    UNION ALL
    SELECT s.id, s.name, 'stat' AS kind, '/v1/stats/' || s.id AS detail_path,
           c.item_count, c.augment_count, c.set_count
      FROM stats s JOIN effect_vocabulary_counts c ON c.kind = 'stat' AND c.id = s.id";

const EFFECT_TYPES_SQL: &str = "
    SELECT v.kind, v.id, bt.name, v.item_count
      FROM effect_vocabulary_bonus_types v JOIN bonus_types bt ON bt.id = v.bonus_type_id
     ORDER BY v.kind, v.id, bt.name";

const EFFECTS_SORT_FIELDS: &[(&str, &str)] = &[
    ("name", "listed.name"),
    ("id", "listed.id"),
    ("kind", "listed.kind"),
    ("detail_path", "listed.detail_path"),
    ("item_count", "listed.item_count"),
    ("augment_count", "listed.augment_count"),
    ("set_count", "listed.set_count"),
];

declare_list_parameters!(
    EffectsParameters,
    EFFECTS_SORT_FIELDS,
    "",
    Some("Case-insensitive own-name or granted-stat substring; without sort, exact, prefix and substring own names rank before stat-only matches.")
);

#[utoipa::path(
    get,
    path = "/v1/effects",
    tag = "bonuses",
    summary = "List effects and stats",
    description = "Lists every effect family and stat with carrier counts, types and a detail path.",
    params(EffectsParameters,
        ("kind" = Option<String>, Query, description = "Keep `effect` or `stat` rows only.")),
    responses((status = 200, description = "Paged effect vocabulary", body = crate::routes::v1::response_schemas::EffectsPageResponse),
        (status = 400, description = "Invalid kind or query", body = crate::error::ErrorBody))
)]
async fn effects(
    State(state): State<AppState>,
    ApiQuery(query, filters): ApiQuery<EffectFilters>,
) -> Result<Json<Value>, ApiError> {
    if let Some(kind) = &filters.kind {
        if !EFFECT_KINDS.contains(&kind.as_str()) {
            return Err(ApiError::BadRequest(format!("unknown kind {kind:?}")));
        }
    }
    state
        .read_db(move |db| {
            let mut where_clause = WhereClause::default();
            if let Some(kind) = &filters.kind {
                where_clause.add_bound_condition("kind = ?", kind.clone());
            }
            let mut default_order = "listed.name, listed.kind".to_string();
            if let Some(search_text) = query.q.as_deref().map(str::trim).filter(|text| !text.is_empty()) {
                let search_placeholder = where_clause.add_bound_condition(
                    "(instr(lower(listed.name), lower(?)) > 0 OR
                      (listed.kind = 'effect' AND EXISTS (
                          SELECT 1 FROM effect_bonuses eb JOIN stats s ON s.id = eb.stat_id
                           WHERE eb.effect_id = listed.id AND instr(lower(s.name), lower(?)) > 0)))",
                    search_text.to_string(),
                );
                default_order = format!(
                    "CASE WHEN lower(listed.name) = lower({search_placeholder}) THEN 0
                          WHEN instr(lower(listed.name), lower({search_placeholder})) = 1 THEN 1
                          WHEN instr(lower(listed.name), lower({search_placeholder})) > 0 THEN 2
                          ELSE 3 END, listed.name, listed.kind"
                );
            }
            let mut page = paged_query(
                db,
                "*",
                &format!("({EFFECT_VOCABULARY_SQL}) listed"),
                &query,
                &default_order,
                EFFECTS_SORT_FIELDS,
                &where_clause,
            )?;
            let types = json_rows(db, EFFECT_TYPES_SQL, [])?;
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
            Ok(Json(page.into_json("effects")))
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
        pub include_legacy: Option<bool>,
    }
}

const BACKLINK_SORT_FIELDS: &[(&str, &str)] = &[("name", "listed.name"), ("id", "listed.id")];

fn carrier_pages(
    db: &Connection,
    kind: &str,
    id: i64,
    pages: BacklinkPages,
) -> Result<(Value, Value, Value), ApiError> {
    let (match_join, match_id, carrier_columns) = if kind == "effect" {
        ("", "j.effect_id", "j.value, j.value2".to_string())
    } else {
        (
            "JOIN effect_bonuses es ON es.effect_id = j.effect_id
             JOIN effects e ON e.id = j.effect_id
             LEFT JOIN bonus_types stat_type ON stat_type.id = es.bonus_type_id
             LEFT JOIN bonus_types link_type ON link_type.id = j.bonus_type_id",
            "es.stat_id",
            format!(
                "e.id AS effect_id, e.name AS effect, COALESCE(stat_type.name, link_type.name) AS bonus_type, {} AS value",
                effect_bonus_value_sql("j", "e", "es")
            ),
        )
    };
    let current_items = if pages.include_legacy == Some(true) { "" } else { "WHERE NOT i.is_legacy" };
    let items_sql = format!(
        "SELECT i.id, i.name, {carrier_columns}, {match_id} AS match_id
           FROM item_effects j JOIN items i ON i.id = j.item_id {match_join}
          {current_items} ORDER BY i.name, j.sort_order"
    );
    let augments_sql = format!(
        "SELECT a.id, a.name, {carrier_columns}, {match_id} AS match_id
           FROM augment_effects j JOIN augments a ON a.id = j.augment_id {match_join}
          ORDER BY a.name, j.sort_order"
    );
    let tiers_sql = format!(
        "SELECT t.id, s.id AS set_id, s.name AS set_name, t.equipped_count, {carrier_columns}, {match_id} AS match_id,
                s.name || ' (' || t.equipped_count || ')' AS name
           FROM set_bonus_tier_effects j JOIN set_bonus_tiers t ON t.id = j.tier_id
           JOIN set_bonuses s ON s.id = t.set_id {match_join} ORDER BY s.name, t.equipped_count, j.sort_order"
    );
    let page =
        |sql: String, limit: Option<i64>, offset: Option<i64>, distinct_total: bool| -> Result<ListPage, ApiError> {
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
            if distinct_total {
                result.total = db.query_row(
                    &format!("SELECT COUNT(DISTINCT listed.id) FROM ({sql}) listed WHERE listed.match_id = ?1"),
                    [id],
                    |row| row.get(0),
                )?;
            }
            for row in &mut result.rows {
                row.as_object_mut().expect("backlink object").remove("match_id");
            }
            Ok(result)
        };
    let items = page(items_sql, pages.items_limit, pages.items_offset, true)?.into_json("items");
    let augments = page(augments_sql, pages.augments_limit, pages.augments_offset, false)?.into_json("augments");
    let tiers = page(tiers_sql, pages.set_tiers_limit, pages.set_tiers_offset, false)?.into_json("set_tiers");
    Ok((items, augments, tiers))
}

#[utoipa::path(
    get, path = "/v1/effects/{id}", tag = "bonuses", summary = "Get an effect",
    description = "Returns a family, its stat rules and ladder, and paged item, augment and set-tier links.",
    params(("id" = i64, Path, description = "Family id from an effect-kind vocabulary row."),
        ("items_limit" = Option<i64>, Query, description = "Maximum item links in this page."),
        ("items_offset" = Option<i64>, Query, description = "Item links to skip."),
        ("augments_limit" = Option<i64>, Query, description = "Maximum augment links in this page."),
        ("augments_offset" = Option<i64>, Query, description = "Augment links to skip."),
        ("set_tiers_limit" = Option<i64>, Query, description = "Maximum set-tier links in this page."),
        ("set_tiers_offset" = Option<i64>, Query, description = "Set-tier links to skip."),
        ("include_legacy" = Option<bool>, Query, description = "Use true to include legacy items in carrier links.")),
    responses((status = 200, description = "Family and paged carriers", body = crate::routes::v1::response_schemas::EffectsDetailResponse),
        (status = 404, description = "No family has this id", body = crate::error::ErrorBody))
)]
async fn effect_detail(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    ApiFilterQuery(pages): ApiFilterQuery<BacklinkPages>,
) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut family = json_row(
                db,
                "SELECT id, name, text_template, description_template, amount_count, wiki_url, stacking_note,
                    default_value, default_value2, ladder_id, ladder_rank FROM effects WHERE id = ?1",
                [id],
            )?;
            family["stats"] = Value::Array(json_rows(
                db,
                "SELECT s.name AS stat, bt.name AS bonus_type, es.amount_from, es.constant, es.scale, es.rounding
               FROM effect_bonuses es JOIN stats s ON s.id = es.stat_id
               LEFT JOIN bonus_types bt ON bt.id = es.bonus_type_id
              WHERE es.effect_id = ?1 ORDER BY es.sort_order",
                [id],
            )?);
            let ladder_id = family["ladder_id"].as_i64();
            let ladder_rank = family["ladder_rank"].clone();
            family.as_object_mut().expect("family object").remove("ladder_id");
            family.as_object_mut().expect("family object").remove("ladder_rank");
            family["ladder"] = if let Some(ladder_id) = ladder_id {
                let mut ladder = json_row(db, "SELECT id, name FROM effect_ladders WHERE id = ?1", [ladder_id])?;
                ladder["steps"] = Value::Array(json_rows(
                    db,
                    "SELECT id, name, ladder_rank AS rank FROM effects WHERE ladder_id = ?1 ORDER BY ladder_rank",
                    [ladder_id],
                )?);
                ladder["rank"] = ladder_rank;
                ladder
            } else {
                Value::Null
            };
            let (items, augments, set_tiers) = carrier_pages(db, "effect", id, pages)?;
            family["items"] = items;
            family["augments"] = augments;
            family["set_tiers"] = set_tiers;
            Ok(Json(family))
        })
        .await
}

#[utoipa::path(
    get, path = "/v1/stats/{id}", tag = "bonuses", summary = "Get a stat",
    description = "Returns a stat with paged item, augment and set-tier links through its families.",
    params(("id" = i64, Path, description = "Stat id from a stat-kind vocabulary row."),
        ("items_limit" = Option<i64>, Query, description = "Maximum item links in this page."),
        ("items_offset" = Option<i64>, Query, description = "Item links to skip."),
        ("augments_limit" = Option<i64>, Query, description = "Maximum augment links in this page."),
        ("augments_offset" = Option<i64>, Query, description = "Augment links to skip."),
        ("set_tiers_limit" = Option<i64>, Query, description = "Maximum set-tier links in this page."),
        ("set_tiers_offset" = Option<i64>, Query, description = "Set-tier links to skip."),
        ("include_legacy" = Option<bool>, Query, description = "Use true to include legacy items in carrier links.")),
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
            let (items, augments, set_tiers) = carrier_pages(db, "stat", id, pages)?;
            stat["items"] = items;
            stat["augments"] = augments;
            stat["set_tiers"] = set_tiers;
            Ok(Json(stat))
        })
        .await
}
