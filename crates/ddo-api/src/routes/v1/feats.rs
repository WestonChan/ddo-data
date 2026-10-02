use crate::db::{
    attack_for, bonuses_via, clamped_page, convert_to_booleans, dcs_for, json_row, json_rows, modifiers_for,
    requirements_for, row_count, stances_for, substring_like_pattern, WhereClause,
};
use crate::error::ApiError;
use crate::query::{ApiQuery, QueryParameters};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use ddo_model::enums::FeatSource;
use serde::Deserialize;
use serde_json::{json, Value};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(feats)).routes(routes!(feat_detail))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FeatListQuery {
    pub q: Option<String>,
    pub source: Option<String>,
    pub group: Option<String>,
    pub acquire: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl QueryParameters for FeatListQuery {}

const FEAT_COLUMNS: &str = "f.id, f.name, f.source_kind, f.source_id,
                       CASE f.source_kind WHEN 'class' THEN (SELECT c.name FROM classes c WHERE c.id = f.source_id)
                                          WHEN 'race' THEN (SELECT r.name FROM races r WHERE r.id = f.source_id) END AS source_name,
                       f.description, f.icon, f.acquire, f.max_times_acquire, f.sphere, f.auto_acquire_ignores_requirements";

#[utoipa::path(
    get,
    path = "/v1/feats",
    tag = "feats",
    summary = "List feats",
    description = "One page of feats ordered by name. `source_kind` says where the feat comes from: `standard` for \
                   the general list, `class` or `race` for feats granted by one class or race, with `source_name` \
                   naming it. Each row also carries how the feat is acquired, how many times, its sphere, and the \
                   feat `groups` it belongs to. Requirements and bonuses are on the detail endpoint.",
    params(
        ("q" = Option<String>, Query, description = "Case-insensitive substring of the feat name"),
        ("source" = Option<String>, Query, description = "`standard`, `class` or `race`; anything else is a 400"),
        ("group" = Option<String>, Query, description = "Feat group name, e.g. `Metamagic`; keeps feats listed in that group"),
        ("acquire" = Option<String>, Query, description = "How the feat is taken, e.g. `Train`, `Automatic`, `Special`"),
        ("limit" = Option<i64>, Query, description = "Page size, 1 to 10000; defaults to 100; out-of-range values are clamped rather than rejected"),
        ("offset" = Option<i64>, Query, description = "Rows to skip before the first returned row; defaults to 0; negative values are clamped to 0 rather than rejected")
    ),
    responses(
        (status = 200, description = "`total`, `limit`, `offset` and the `feats` page", body = Value),
        (status = 400, description = "Unknown source, or an unknown or malformed query parameter", body = crate::error::ErrorBody)
    )
)]
async fn feats(
    State(state): State<AppState>,
    ApiQuery(query): ApiQuery<FeatListQuery>,
) -> Result<Json<Value>, ApiError> {
    if let Some(source) = &query.source {
        if !FeatSource::ALL.iter().any(|known| known.as_str() == source) {
            return Err(ApiError::BadRequest(format!("unknown source {source:?}")));
        }
    }
    let (limit, offset) = clamped_page(query.limit, query.offset);
    state
        .read_db(move |db| {
            let mut where_clause = WhereClause::default();
            if let Some(search_text) = query.q.as_deref().filter(|q| !q.trim().is_empty()) {
                where_clause.add_bound_condition("f.name LIKE ? ESCAPE '\\'", substring_like_pattern(search_text));
            }
            if let Some(source) = &query.source {
                where_clause.add_bound_condition("f.source_kind = ?", source.clone());
            }
            if let Some(group) = &query.group {
                where_clause.add_bound_condition(
                    "EXISTS (SELECT 1 FROM feat_groups fg WHERE fg.feat_id = f.id AND fg.group_name = ?)",
                    group.clone(),
                );
            }
            if let Some(acquire) = &query.acquire {
                where_clause.add_bound_condition("f.acquire = ?", acquire.clone());
            }
            let where_sql = where_clause.to_sql();
            let total = row_count(db, &format!("SELECT COUNT(*) FROM feats f {where_sql}"), where_clause.params())?;
            let page_sql = format!("SELECT {FEAT_COLUMNS} FROM feats f {where_sql} ORDER BY f.name, f.source_kind LIMIT {limit} OFFSET {offset}");
            let mut feats = json_rows(db, &page_sql, where_clause.params())?;
            for feat in &mut feats {
                convert_to_booleans(feat, &["auto_acquire_ignores_requirements"]);
                let feat_id = feat["id"].as_i64().unwrap_or(0);
                feat["groups"] = Value::Array(feat_group_names(db, feat_id)?);
            }
            Ok(Json(json!({ "total": total, "limit": limit, "offset": offset, "feats": feats })))
        })
        .await
}

fn feat_group_names(db: &rusqlite::Connection, feat_id: i64) -> Result<Vec<Value>, ApiError> {
    Ok(json_rows(db, "SELECT group_name FROM feat_groups WHERE feat_id = ?1 ORDER BY rowid", [feat_id])?
        .into_iter()
        .map(|row| row["group_name"].clone())
        .collect())
}

#[utoipa::path(
    get,
    path = "/v1/feats/{id}",
    tag = "feats",
    summary = "Get a feat",
    description = "One feat with its `requirements` to train it, `auto_acquire_requirements` for automatic grants, \
                   `conditional_groups` (alternative requirement sets), `sub_items` (the choices a selector feat \
                   offers), `stances`, `dcs`, an `attack` if the feat is one (name, description, icon, \
                   `cooldown_seconds`, and the `duration_seconds` of what it applies afterwards), the attack's \
                   `this_attack_modifiers` (bonuses to its own hit, e.g. `BonusDamagePercent`) and \
                   `follow_on_modifiers` (what it applies afterwards, e.g. `AllowSneakAttack`), its derived \
                   `bonuses`, and the raw `modifiers` they came from.",
    params(("id" = i64, Path, description = "The feat's numeric id from the list endpoint")), responses((status = 200, description = "The feat with its child collections", body = Value), (status = 404, description = "No feat has this id", body = crate::error::ErrorBody))
)]
async fn feat_detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut feat = json_row(db, &format!("SELECT {FEAT_COLUMNS} FROM feats f WHERE f.id = ?1"), [id])?;
            convert_to_booleans(&mut feat, &["auto_acquire_ignores_requirements"]);
            feat["groups"] = Value::Array(feat_group_names(db, id)?);
            feat["requirements"] = Value::Array(requirements_for(db, "feat", id)?);
            feat["auto_acquire_requirements"] = Value::Array(requirements_for(db, "feat_auto_acquire", id)?);
            let mut conditional_groups =
                json_rows(db, "SELECT id, groups FROM feat_conditional_groups WHERE feat_id = ?1", [id])?;
            for conditional_group in &mut conditional_groups {
                let conditional_group_id = conditional_group["id"].as_i64().unwrap_or(0);
                conditional_group["requirements"] =
                    Value::Array(requirements_for(db, "feat_conditional_group", conditional_group_id)?);
            }
            feat["conditional_groups"] = Value::Array(conditional_groups);
            feat["sub_items"] = Value::Array(json_rows(
                db,
                "SELECT name, icon, description FROM feat_sub_items WHERE feat_id = ?1 ORDER BY sort_order",
                [id],
            )?);
            feat["stances"] = Value::Array(stances_for(db, "feat", id)?);
            feat["dcs"] = Value::Array(dcs_for(db, "feat", id)?);
            feat["attack"] = attack_for(db, "feat", id)?;
            feat["this_attack_modifiers"] = Value::Array(modifiers_for(db, "feat_this_attack", id)?);
            feat["follow_on_modifiers"] = Value::Array(modifiers_for(db, "feat_follow_on", id)?);
            feat["bonuses"] = Value::Array(bonuses_via(db, "feat_bonuses", "feat_id", id)?);
            feat["modifiers"] = Value::Array(modifiers_for(db, "feat", id)?);
            Ok(Json(feat))
        })
        .await
}
