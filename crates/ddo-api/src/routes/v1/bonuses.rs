use crate::db::{json_row, json_rows, paged_query, paged_table_json, ListPage, TableListSource, WhereClause};
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
    OpenApiRouter::new().routes(routes!(bonus_types)).routes(routes!(effects)).routes(routes!(effect_detail))
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
    SELECT e.id, e.name, CASE WHEN e.is_stat = 1 THEN 'stat' ELSE 'effect' END AS kind,
           '/v1/effects/' || e.id AS detail_path,
           c.item_count, c.augment_count, c.set_count
      FROM effects e JOIN effect_vocabulary_counts c ON c.id = e.id
       AND c.kind = CASE WHEN e.is_stat = 1 THEN 'stat' ELSE 'effect' END";

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
                          SELECT 1 FROM effect_bonuses eb JOIN effects s ON s.id = eb.stat_id
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
    let carrier_columns = if kind == "effect" {
        "NULL AS effect_id, NULL AS effect, NULL AS bonus_type,
         COALESCE(ob.amount, j.value) AS value, j.value2, ob.amount_source, ob.scale"
            .to_string()
    } else {
        "e.id AS effect_id, e.name AS effect, bt.name AS bonus_type, ob.amount AS value,
         NULL AS value2, ob.amount_source, ob.scale"
            .to_string()
    };
    let match_id = if kind == "effect" { "j.effect_id" } else { "ob.stat_id" };
    let join_for = |owner_kind: &str, owner_column: &str| -> String {
        let join_kind = if kind == "effect" { "LEFT JOIN" } else { "JOIN" };
        let mut joined = format!(
            "{join_kind} owner_bonuses ob ON ob.owner_kind = '{owner_kind}'
             AND ob.owner_id = j.{owner_column} AND ob.effect_link_order = j.sort_order"
        );
        if kind == "stat" {
            joined.push_str(
                " JOIN effects e ON e.id = COALESCE(ob.via_effect_id, ob.stat_id)
                  LEFT JOIN bonus_types bt ON bt.id = ob.bonus_type_id",
            );
        }
        joined
    };
    let current_items = if pages.include_legacy == Some(true) { "" } else { "WHERE NOT i.is_legacy" };
    let items_sql = format!(
        "SELECT i.id, i.name, {carrier_columns}, {match_id} AS match_id
           FROM item_effects j JOIN items i ON i.id = j.item_id {match_join}
          {current_items} ORDER BY i.name, j.sort_order",
        match_join = join_for("item", "item_id")
    );
    let augments_sql = format!(
        "SELECT a.id, a.name, {carrier_columns}, {match_id} AS match_id
           FROM augment_effects j JOIN augments a ON a.id = j.augment_id {match_join}
          ORDER BY a.name, j.sort_order",
        match_join = join_for("augment", "augment_id")
    );
    let tiers_sql = format!(
        "SELECT t.id, s.id AS set_id, s.name AS set_name, t.equipped_count, {carrier_columns}, {match_id} AS match_id,
                s.name || ' (' || t.equipped_count || ')' AS name
           FROM set_bonus_tier_effects j JOIN set_bonus_tiers t ON t.id = j.tier_id
           JOIN set_bonuses s ON s.id = t.set_id {match_join} ORDER BY s.name, t.equipped_count, j.sort_order",
        match_join = join_for("set_bonus_tier", "tier_id")
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
    description = "Returns an effect or stat, its bonus and damage rules, tier group and paged carriers.",
    params(("id" = i64, Path, description = "Effect or stat id from the vocabulary."),
        ("items_limit" = Option<i64>, Query, description = "Maximum item links in this page."),
        ("items_offset" = Option<i64>, Query, description = "Item links to skip."),
        ("augments_limit" = Option<i64>, Query, description = "Maximum augment links in this page."),
        ("augments_offset" = Option<i64>, Query, description = "Augment links to skip."),
        ("set_tiers_limit" = Option<i64>, Query, description = "Maximum set-tier links in this page."),
        ("set_tiers_offset" = Option<i64>, Query, description = "Set-tier links to skip."),
        ("include_legacy" = Option<bool>, Query, description = "Use true to include legacy items in carrier links.")),
    responses((status = 200, description = "Effect and paged carriers", body = crate::routes::v1::response_schemas::EffectsDetailResponse),
        (status = 404, description = "No effect has this id", body = crate::error::ErrorBody))
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
                "SELECT id, name, is_stat, category, text_template, description_template, wiki_url,
                    default_value, default_value2, tier_group_id, tier FROM effects WHERE id = ?1",
                [id],
            )?;
            family["kind"] = Value::String(if family["is_stat"] == 1 { "stat" } else { "effect" }.to_string());
            family.as_object_mut().expect("effect object").remove("is_stat");
            family["bonuses"] = Value::Array(json_rows(
                db,
                "SELECT s.name AS stat, bt.name AS bonus_type, es.amount_from, es.constant, es.scale, es.rounding
               FROM effect_bonuses es JOIN effects s ON s.id = es.stat_id
               LEFT JOIN bonus_types bt ON bt.id = es.bonus_type_id
              WHERE es.effect_id = ?1 ORDER BY es.sort_order",
                [id],
            )?);
            family["damage"] = Value::Array(json_rows(
                db,
                "SELECT t.name AS trigger, dt.name AS damage_type, ed.dice_number, ed.dice_sides,
                        ed.dice_bonus, ed.amount_from, ed.scale FROM effect_damage ed
                  JOIN triggers t ON t.id = ed.trigger_id JOIN damage_types dt ON dt.id = ed.damage_type_id
                 WHERE ed.effect_id = ?1 ORDER BY ed.sort_order",
                [id],
            )?);
            let tier_group_id = family["tier_group_id"].as_i64();
            let tier_rank = family["tier"].clone();
            family.as_object_mut().expect("effect object").remove("tier_group_id");
            family["tier"] = if let Some(tier_group_id) = tier_group_id {
                let mut tier =
                    json_row(db, "SELECT name AS `group` FROM effect_tier_groups WHERE id = ?1", [tier_group_id])?;
                tier["steps"] = Value::Array(json_rows(
                    db,
                    "SELECT id, name, tier AS rank FROM effects WHERE tier_group_id = ?1 ORDER BY tier",
                    [tier_group_id],
                )?);
                tier["rank"] = tier_rank;
                tier
            } else {
                Value::Null
            };
            let kind = family["kind"].as_str().unwrap_or("effect");
            let (items, augments, set_tiers) = carrier_pages(db, kind, id, pages)?;
            family["items"] = items;
            family["augments"] = augments;
            family["set_tiers"] = set_tiers;
            Ok(Json(family))
        })
        .await
}
