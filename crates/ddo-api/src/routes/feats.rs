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
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(list)).routes(routes!(detail))
}

#[derive(Deserialize)]
pub struct FeatFilter {
    pub q: Option<String>,
    pub source: Option<String>,
    pub group: Option<String>,
    pub acquire: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

const COLUMNS: &str = "f.id, f.name, f.source_kind, f.source_id,
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
        ("limit" = Option<i64>, Query, description = "Page size, 1 to 10000; defaults to 100"),
        ("offset" = Option<i64>, Query, description = "Rows to skip before the first returned row; defaults to 0")
    ),
    responses(
        (status = 200, description = "`total`, `limit`, `offset` and the `feats` page", body = Value),
        (status = 400, description = "Unknown source", body = crate::error::ErrorBody)
    )
)]
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

#[utoipa::path(
    get,
    path = "/v1/feats/{id}",
    tag = "feats",
    summary = "Get a feat",
    description = "One feat with its `requirements` to train it, `auto_acquire_requirements` for automatic grants, \
                   `conditional_groups` (alternative requirement sets), `sub_items` (the choices a selector feat \
                   offers), `stances`, `dcs`, an `attack` if the feat is one, its derived `bonuses`, and the raw \
                   `modifiers` they came from.",
    params(("id" = i64, Path, description = "The feat's numeric id from the list endpoint")), responses((status = 200, description = "The feat with its child collections", body = Value), (status = 404, description = "No feat has this id", body = crate::error::ErrorBody))
)]
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
