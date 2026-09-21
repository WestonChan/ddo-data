use crate::db::{bonuses_via, booleanize, count, json_row, json_rows, like_pattern, modifiers_for, page, Filters};
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(list)).routes(routes!(detail))
}

#[derive(Deserialize, IntoParams)]
pub struct AugmentFilter {
    pub q: Option<String>,
    /// Slot type label, e.g. `red` or `isle of dread: scale (armor)`.
    pub slot: Option<String>,
    /// Source family (file stem), e.g. `Ruby`.
    pub family: Option<String>,
    pub max_level: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

const FLAGS: &[&str] = &["choose_level", "dual_values", "enter_value", "suppress_set_bonus"];

fn attach(conn: &rusqlite::Connection, a: &mut Value) -> Result<(), ApiError> {
    booleanize(a, FLAGS);
    let id = a["id"].as_i64().unwrap_or(0);
    let slots: Vec<Value> = json_rows(
        conn,
        "SELECT t.label FROM augment_slots s JOIN augment_slot_types t ON t.id = s.slot_id WHERE s.augment_id = ?1 ORDER BY t.label",
        [id],
    )?
    .into_iter()
    .map(|r| r["label"].clone())
    .collect();
    a["slots"] = Value::Array(slots);
    a["bonuses"] = Value::Array(bonuses_via(conn, "augment_bonuses", "augment_id", id)?);
    Ok(())
}

const COLUMNS: &str =
    "a.id, a.name, a.family, a.description, a.effect_description, a.min_level, a.icon, a.choose_level, a.levels,
                       a.level_values, a.level_values2, a.dual_values, a.enter_value, a.suppress_set_bonus, a.set_bonus,
                       a.adds_augment, a.grants_augment, a.weapon_class";

/// Augments, each with the slot types it fits and its derived bonuses.
#[utoipa::path(get, path = "/v1/augments", tag = "augments", params(AugmentFilter), responses((status = 200, body = Value)))]
async fn list(State(state): State<AppState>, Query(f): Query<AugmentFilter>) -> Result<Json<Value>, ApiError> {
    let (limit, offset) = page(f.limit, f.offset);
    state
        .query(move |conn| {
            let mut f_ = Filters::default();
            if let Some(q) = f.q.as_deref().filter(|q| !q.trim().is_empty()) {
                f_.bind("a.name LIKE ? ESCAPE '\\'", like_pattern(q));
            }
            if let Some(slot) = &f.slot {
                f_.bind("EXISTS (SELECT 1 FROM augment_slots s JOIN augment_slot_types t ON t.id = s.slot_id WHERE s.augment_id = a.id AND t.label = ?)",
                    slot.to_lowercase(),
                );
            }
            if let Some(fam) = &f.family {
                f_.bind("a.family = ?", fam.clone());
            }
            if let Some(n) = f.max_level {
                f_.bind("(a.min_level IS NULL OR a.min_level <= ?)", n);
            }
            let where_sql = f_.where_sql();
            let total = count(conn, &format!("SELECT COUNT(*) FROM augments a {where_sql}"), f_.params())?;
            let sql = format!("SELECT {COLUMNS} FROM augments a {where_sql} ORDER BY a.name, a.min_level LIMIT {limit} OFFSET {offset}");
            let mut rows = json_rows(conn, &sql, f_.params())?;
            for a in &mut rows {
                attach(conn, a)?;
            }
            Ok(Json(json!({ "total": total, "limit": limit, "offset": offset, "augments": rows })))
        })
        .await
}

/// One augment with its full modifiers.
#[utoipa::path(get, path = "/v1/augments/{id}", tag = "augments", params(("id" = i64, Path)), responses((status = 200, body = Value), (status = 404, body = crate::error::ErrorBody)))]
async fn detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .query(move |conn| {
            let mut a = json_row(conn, &format!("SELECT {COLUMNS} FROM augments a WHERE a.id = ?1"), [id])?;
            attach(conn, &mut a)?;
            a["modifiers"] = Value::Array(modifiers_for(conn, "augment", id)?);
            Ok(Json(a))
        })
        .await
}
