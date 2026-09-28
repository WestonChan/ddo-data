use crate::db::{bonuses_via, booleanize, count, json_row, json_rows, like_pattern, modifiers_for, page, Filters};
use crate::error::ApiError;
use crate::query::ApiQuery;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(list)).routes(routes!(detail))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AugmentFilter {
    pub q: Option<String>,
    pub slot: Option<String>,
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

#[utoipa::path(
    get,
    path = "/v1/augments",
    tag = "augments",
    summary = "List augments",
    description = "One page of augments ordered by name then minimum level, each with the socket labels it fits \
                   (`slots`), its `bonuses`, and the crafting fields DDOBuilderV2 records (level tables, dual values, \
                   set bonus, granted augments). Filter by `slot` to get the candidates for one socket on an item.",
    params(
        ("q" = Option<String>, Query, description = "Case-insensitive substring of the augment name"),
        ("slot" = Option<String>, Query, description = "Socket label as /v1/augment-slot-types lists it, e.g. `red` or `lamordia: melancholic (accessory)`; case-insensitive"),
        ("family" = Option<String>, Query, description = "Augment family, e.g. `standard`, `lamordia`, `dino`, `crafting`"),
        ("max_level" = Option<i64>, Query, description = "Only augments usable at this character level or lower; augments with no minimum level always pass"),
        ("limit" = Option<i64>, Query, description = "Page size, 1 to 10000; defaults to 100; out-of-range values are clamped rather than rejected"),
        ("offset" = Option<i64>, Query, description = "Rows to skip before the first returned row; defaults to 0; negative values are clamped to 0 rather than rejected")
    ),
    responses(
        (status = 200, description = "`total`, `limit`, `offset` and the `augments` page", body = Value),
        (status = 400, description = "Unknown or malformed query parameter", body = crate::error::ErrorBody)
    )
)]
async fn list(State(state): State<AppState>, ApiQuery(f): ApiQuery<AugmentFilter>) -> Result<Json<Value>, ApiError> {
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

#[utoipa::path(
    get,
    path = "/v1/augments/{id}",
    tag = "augments",
    summary = "Get an augment",
    description = "One augment as the list returns it, plus the raw `modifiers` its bonuses were derived from, \
                   including the conditional and dice-valued ones that do not reduce to a bonus.",
    params(("id" = i64, Path, description = "The augment's numeric id from the list endpoint")), responses((status = 200, description = "The augment with its child collections", body = Value), (status = 404, description = "No augment has this id", body = crate::error::ErrorBody))
)]
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
