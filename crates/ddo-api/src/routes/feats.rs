use crate::db::{
    bonuses_via, booleanize, count, dcs_for, json_row, json_rows, like_pattern, modifiers_for, page, requirements_for,
    stances_for, Filters,
};
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use ddo_model::enums::FeatSource;
use serde::Deserialize;
use serde_json::{json, Value};
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(list)).routes(routes!(detail))
}

#[derive(Deserialize, IntoParams)]
pub struct FeatFilter {
    pub q: Option<String>,
    /// `standard`, `class` or `race`.
    pub source: Option<String>,
    /// A feat group such as `Standard`, `Epic Feat` or `Metamagics`.
    pub group: Option<String>,
    /// `Train`, `Automatic`, `Favor`, …
    pub acquire: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

const COLUMNS: &str = "f.id, f.name, f.source_kind, f.source_id,
                       CASE f.source_kind WHEN 'class' THEN (SELECT c.name FROM classes c WHERE c.id = f.source_id)
                                          WHEN 'race' THEN (SELECT r.name FROM races r WHERE r.id = f.source_id) END AS source_name,
                       f.description, f.icon, f.acquire, f.max_times_acquire, f.sphere, f.auto_acquire_ignores_requirements";

/// Feats matching the filters.
#[utoipa::path(get, path = "/v1/feats", tag = "feats", params(FeatFilter), responses((status = 200, body = Value)))]
async fn list(State(state): State<AppState>, Query(f): Query<FeatFilter>) -> Result<Json<Value>, ApiError> {
    if let Some(s) = &f.source {
        if !FeatSource::ALL.iter().any(|k| k.as_str() == s) {
            return Err(ApiError::BadRequest(format!("unknown source {s:?}")));
        }
    }
    let (limit, offset) = page(f.limit, f.offset);
    state
        .query(move |conn| {
            let mut f_ = Filters::default();
            if let Some(q) = f.q.as_deref().filter(|q| !q.trim().is_empty()) {
                f_.bind("f.name LIKE ? ESCAPE '\\'", like_pattern(q));
            }
            if let Some(s) = &f.source {
                f_.bind("f.source_kind = ?", s.clone());
            }
            if let Some(g) = &f.group {
                f_.bind("EXISTS (SELECT 1 FROM feat_groups fg WHERE fg.feat_id = f.id AND fg.group_name = ?)", g.clone());
            }
            if let Some(a) = &f.acquire {
                f_.bind("f.acquire = ?", a.clone());
            }
            let where_sql = f_.where_sql();
            let total = count(conn, &format!("SELECT COUNT(*) FROM feats f {where_sql}"), f_.params())?;
            let sql = format!("SELECT {COLUMNS} FROM feats f {where_sql} ORDER BY f.name, f.source_kind LIMIT {limit} OFFSET {offset}");
            let mut rows = json_rows(conn, &sql, f_.params())?;
            for r in &mut rows {
                booleanize(r, &["auto_acquire_ignores_requirements"]);
                let id = r["id"].as_i64().unwrap_or(0);
                r["groups"] = Value::Array(groups(conn, id)?);
            }
            Ok(Json(json!({ "total": total, "limit": limit, "offset": offset, "feats": rows })))
        })
        .await
}

fn groups(conn: &rusqlite::Connection, id: i64) -> Result<Vec<Value>, ApiError> {
    Ok(json_rows(conn, "SELECT group_name FROM feat_groups WHERE feat_id = ?1 ORDER BY rowid", [id])?
        .into_iter()
        .map(|r| r["group_name"].clone())
        .collect())
}

/// One feat with groups, requirements, stances, DCs, sub-items, modifiers and derived bonuses.
#[utoipa::path(get, path = "/v1/feats/{id}", tag = "feats", params(("id" = i64, Path)), responses((status = 200, body = Value), (status = 404, body = crate::error::ErrorBody)))]
async fn detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .query(move |conn| {
            let mut feat = json_row(conn, &format!("SELECT {COLUMNS} FROM feats f WHERE f.id = ?1"), [id])?;
            booleanize(&mut feat, &["auto_acquire_ignores_requirements"]);
            feat["groups"] = Value::Array(groups(conn, id)?);
            feat["requirements"] = Value::Array(requirements_for(conn, "feat", id)?);
            feat["auto_acquire_requirements"] = Value::Array(requirements_for(conn, "feat_auto_acquire", id)?);
            let mut conditional =
                json_rows(conn, "SELECT id, groups FROM feat_conditional_groups WHERE feat_id = ?1", [id])?;
            for cg in &mut conditional {
                let cg_id = cg["id"].as_i64().unwrap_or(0);
                cg["requirements"] = Value::Array(requirements_for(conn, "feat_conditional_group", cg_id)?);
            }
            feat["conditional_groups"] = Value::Array(conditional);
            feat["sub_items"] = Value::Array(json_rows(
                conn,
                "SELECT name, icon, description FROM feat_sub_items WHERE feat_id = ?1 ORDER BY sort_order",
                [id],
            )?);
            feat["stances"] = Value::Array(stances_for(conn, "feat", id)?);
            feat["dcs"] = Value::Array(dcs_for(conn, "feat", id)?);
            feat["attack"] = json_rows(
                conn,
                "SELECT name, description, icon FROM attacks WHERE owner_kind = 'feat' AND owner_id = ?1",
                [id],
            )?
            .pop()
            .unwrap_or(Value::Null);
            feat["bonuses"] = Value::Array(bonuses_via(conn, "feat_bonuses", "feat_id", id)?);
            feat["modifiers"] = Value::Array(modifiers_for(conn, "feat", id)?);
            Ok(Json(feat))
        })
        .await
}
