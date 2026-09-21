use crate::db::{booleanize, dcs_for, json_row, json_rows, modifiers_for, requirements_for, stances_for};
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(list)).routes(routes!(detail))
}

const TREE_COLUMNS: &str = "t.id, t.name, t.version, t.kind, t.is_legacy, t.icon, t.background,
                            (SELECT COUNT(*) FROM enhancements e WHERE e.tree_id = t.id) AS enhancement_count";

/// Every enhancement tree with its kind and requirements.
#[utoipa::path(get, path = "/v1/enhancement-trees", tag = "enhancements", responses((status = 200, body = Vec<Value>)))]
async fn list(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    state
        .query(|conn| {
            let mut rows = json_rows(
                conn,
                &format!("SELECT {TREE_COLUMNS} FROM enhancement_trees t ORDER BY t.kind, t.name"),
                [],
            )?;
            for t in &mut rows {
                booleanize(t, &["is_legacy"]);
                let id = t["id"].as_i64().unwrap_or(0);
                t["requirements"] = Value::Array(requirements_for(conn, "enhancement_tree", id)?);
            }
            Ok(Json(rows))
        })
        .await
}

fn ability_children(conn: &rusqlite::Connection, owner: &str, row: &mut Value) -> Result<(), ApiError> {
    let id = row["id"].as_i64().unwrap_or(0);
    row["requirements"] = Value::Array(requirements_for(conn, owner, id)?);
    row["modifiers"] = Value::Array(modifiers_for(conn, owner, id)?);
    row["stances"] = Value::Array(stances_for(conn, owner, id)?);
    row["dcs"] = Value::Array(dcs_for(conn, owner, id)?);
    row["attack"] = json_rows(
        conn,
        "SELECT name, description, icon FROM attacks WHERE owner_kind = ?1 AND owner_id = ?2",
        (owner, id),
    )?
    .pop()
    .unwrap_or(Value::Null);
    Ok(())
}

/// One tree with every enhancement, each with its selections, requirements, modifiers, stances
/// and DCs. Positions are `x` (column) and `y` (tier row, 0 = core).
#[utoipa::path(get, path = "/v1/enhancement-trees/{id}", tag = "enhancements", params(("id" = i64, Path)), responses((status = 200, body = Value), (status = 404, body = crate::error::ErrorBody)))]
async fn detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .query(move |conn| {
            let mut tree = json_row(conn, &format!("SELECT {TREE_COLUMNS} FROM enhancement_trees t WHERE t.id = ?1"), [id])?;
            booleanize(&mut tree, &["is_legacy"]);
            tree["requirements"] = Value::Array(requirements_for(conn, "enhancement_tree", id)?);
            let mut enhancements = json_rows(
                conn,
                "SELECT id, internal_name, name, description, icon, x, y, cost_per_rank, ranks, min_spent, is_tier5, is_clickie, arrows
                   FROM enhancements WHERE tree_id = ?1 ORDER BY y, x, id",
                [id],
            )?;
            for e in &mut enhancements {
                booleanize(e, &["is_tier5", "is_clickie"]);
                ability_children(conn, "enhancement", e)?;
                let eid = e["id"].as_i64().unwrap_or(0);
                e["exclusions"] = Value::Array(
                    json_rows(conn, "SELECT internal_name FROM enhancement_selector_exclusions WHERE enhancement_id = ?1", [eid])?
                        .into_iter()
                        .map(|r| r["internal_name"].clone())
                        .collect(),
                );
                let mut selections = json_rows(
                    conn,
                    "SELECT id, name, description, icon, cost_per_rank, ranks, min_spent, is_clickie FROM enhancement_selections
                      WHERE enhancement_id = ?1 ORDER BY sort_order",
                    [eid],
                )?;
                for s in &mut selections {
                    booleanize(s, &["is_clickie"]);
                    ability_children(conn, "enhancement_selection", s)?;
                }
                e["selections"] = Value::Array(selections);
            }
            tree["enhancements"] = Value::Array(enhancements);
            Ok(Json(tree))
        })
        .await
}
