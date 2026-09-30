use super::crafting::crafting_recipes_yielding;
use crate::db::{
    bonuses_via, clamped_page, convert_to_booleans, corrections_for, json_row, json_rows, modifiers_for, row_count,
    substring_like_pattern, WhereClause,
};
use crate::error::ApiError;
use crate::query::ApiQuery;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use ddo_model::enums::CorrectionKind;
use serde::Deserialize;
use serde_json::{json, Value};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(augments)).routes(routes!(augment_detail))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AugmentListQuery {
    pub q: Option<String>,
    pub slot: Option<String>,
    pub family: Option<String>,
    pub max_level: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

const AUGMENT_FLAG_COLUMNS: &[&str] = &["choose_level", "dual_values", "enter_value", "suppress_set_bonus"];

fn attach_child_collections(db: &rusqlite::Connection, augment: &mut Value) -> Result<(), ApiError> {
    let augment_id = augment["id"].as_i64().unwrap_or(0);
    let slot_labels: Vec<Value> = json_rows(
        db,
        "SELECT t.label FROM augment_slots s JOIN augment_slot_types t ON t.id = s.slot_id WHERE s.augment_id = ?1 ORDER BY t.label",
        [augment_id],
    )?
    .into_iter()
    .map(|row| row["label"].clone())
    .collect();
    augment["slots"] = Value::Array(slot_labels);
    augment["bonuses"] = Value::Array(bonuses_via(db, "augment_bonuses", "augment_id", augment_id)?);
    augment["crafting"] = Value::Array(crafting_recipes_yielding(db, augment_id)?);
    Ok(())
}

const AUGMENT_COLUMNS: &str =
    "a.id, a.name, a.family, a.description, a.effect_description, a.min_level, a.icon, a.choose_level, a.levels,
                       a.level_values, a.level_values2, a.dual_values, a.enter_value, a.suppress_set_bonus, a.set_bonus,
                       a.adds_augment, a.grants_augment, a.weapon_class";

#[utoipa::path(
    get,
    path = "/v1/augments",
    tag = "augments",
    summary = "List augments",
    description = "One page of augments ordered by name then minimum level, each with the socket labels it fits \
                   (`slots`), its `bonuses`, the crafting fields DDOBuilderV2 records (level tables, dual values, \
                   set bonus, granted augments), and `crafting`: the wiki crafting recipes that yield it, each with \
                   its `system` name, `tier`, the wiki's `option` label and its `cost` as \
                   `{ ingredient, tier, quantity }` entries (see /v1/crafting-systems); empty when no recipe read \
                   from the wiki yields it. Filter by `slot` to get the candidates for one socket on an item.",
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
async fn augments(
    State(state): State<AppState>,
    ApiQuery(query): ApiQuery<AugmentListQuery>,
) -> Result<Json<Value>, ApiError> {
    let (limit, offset) = clamped_page(query.limit, query.offset);
    state
        .read_db(move |db| {
            let mut where_clause = WhereClause::default();
            if let Some(search_text) = query.q.as_deref().filter(|q| !q.trim().is_empty()) {
                where_clause.add_bound_condition("a.name LIKE ? ESCAPE '\\'", substring_like_pattern(search_text));
            }
            if let Some(slot_label) = &query.slot {
                where_clause.add_bound_condition("EXISTS (SELECT 1 FROM augment_slots s JOIN augment_slot_types t ON t.id = s.slot_id WHERE s.augment_id = a.id AND t.label = ?)",
                    slot_label.to_lowercase(),
                );
            }
            if let Some(family) = &query.family {
                where_clause.add_bound_condition("a.family = ?", family.clone());
            }
            if let Some(max_level) = query.max_level {
                where_clause.add_bound_condition("(a.min_level IS NULL OR a.min_level <= ?)", max_level);
            }
            let where_sql = where_clause.to_sql();
            let total = row_count(db, &format!("SELECT COUNT(*) FROM augments a {where_sql}"), where_clause.params())?;
            let page_sql = format!("SELECT {AUGMENT_COLUMNS} FROM augments a {where_sql} ORDER BY a.name, a.min_level LIMIT {limit} OFFSET {offset}");
            let mut augments = json_rows(db, &page_sql, where_clause.params())?;
            for augment in &mut augments {
                convert_to_booleans(augment, AUGMENT_FLAG_COLUMNS);
                attach_child_collections(db, augment)?;
            }
            Ok(Json(json!({ "total": total, "limit": limit, "offset": offset, "augments": augments })))
        })
        .await
}

#[utoipa::path(
    get,
    path = "/v1/augments/{id}",
    tag = "augments",
    summary = "Get an augment",
    description = "One augment as the list returns it, including its `crafting` recipes, plus the raw `modifiers` \
                   its bonuses were derived from, including the conditional and dice-valued ones that do not reduce \
                   to a bonus, and `corrections`: each known mistake in Maetrim's value for augments of this name \
                   that the dataset replaced, as `{ field, from, to, reason, source }` with `from` his value and \
                   `to` the value shown (empty when none).",
    params(("id" = i64, Path, description = "The augment's numeric id from the list endpoint")), responses((status = 200, description = "The augment with its child collections", body = Value), (status = 404, description = "No augment has this id", body = crate::error::ErrorBody))
)]
async fn augment_detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut augment = json_row(db, &format!("SELECT {AUGMENT_COLUMNS} FROM augments a WHERE a.id = ?1"), [id])?;
            convert_to_booleans(&mut augment, AUGMENT_FLAG_COLUMNS);
            attach_child_collections(db, &mut augment)?;
            augment["modifiers"] = Value::Array(modifiers_for(db, "augment", id)?);
            let augment_name = augment["name"].as_str().unwrap_or_default().to_string();
            augment["corrections"] = Value::Array(corrections_for(db, CorrectionKind::Augment, &augment_name)?);
            Ok(Json(augment))
        })
        .await
}
