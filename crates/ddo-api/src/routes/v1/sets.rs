use crate::db::paged_rows;
use crate::db::{
    convert_to_booleans, enchantments_via, json_row, json_rows, modifiers_for, paged_table_json, TableListSource,
};
use crate::error::ApiError;
use crate::query::{declare_list_parameters, ApiQuery};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(sets))
        .routes(routes!(set_detail))
        .routes(routes!(filigrees))
        .routes(routes!(sentient_gems))
}

const SET_LIST_SELECT: &str = "SELECT s.id, s.name, s.icon, s.is_filigree_set,
    (SELECT COUNT(*) FROM set_bonus_items i WHERE i.set_id = s.id) AS item_count,
    (SELECT COUNT(*) FROM set_bonus_augments a WHERE a.set_id = s.id) AS augment_count,
    (SELECT COUNT(*) FROM set_bonus_tiers t WHERE t.set_id = s.id) AS tier_count
    FROM set_bonuses s";

const FILIGREE_SELECT: &str = "SELECT f.id, f.name, f.description, f.icon, f.menu, f.set_id, s.name AS set_name
    FROM filigrees f LEFT JOIN set_bonuses s ON s.id = f.set_id";

const SETS_SORT_FIELDS: &[(&str, &str)] = &[
    ("name", "listed.name"),
    ("id", "listed.id"),
    ("item_count", "listed.item_count"),
    ("augment_count", "listed.augment_count"),
    ("tier_count", "listed.tier_count"),
    ("icon", "icon"),
    ("is_filigree_set", "is_filigree_set"),
];

declare_list_parameters!(SetsParameters, SETS_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/sets",
    tag = "sets",
    summary = "List sets",
    description = "Lists gear and filigree sets with item, augment and tier counts.",
    params(
        SetsParameters,
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `sets` page", body = crate::routes::v1::response_schemas::SetsPageResponse),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn sets(State(state): State<AppState>, ApiQuery(query, _): ApiQuery) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut page = paged_rows(db, SET_LIST_SELECT, &query, "listed.name", "listed.name", SETS_SORT_FIELDS)?;
            for set in &mut page.rows {
                convert_to_booleans(set, &["is_filigree_set"]);
            }
            Ok(Json(page.into_json("sets")))
        })
        .await
}

#[utoipa::path(
    get,
    path = "/v1/sets/{id}",
    tag = "sets",
    summary = "Get a set",
    description = "Returns a set with tier enchantments, members, augments and filigrees.",
    params(("id" = i64, Path, description = "The set's numeric id from the list endpoint")), responses((status = 200, description = "The set with its child collections", body = crate::routes::v1::response_schemas::SetsDetailResponse), (status = 404, description = "No set has this id", body = crate::error::ErrorBody))
)]
async fn set_detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut set = json_row(db, "SELECT id, name, icon, is_filigree_set FROM set_bonuses WHERE id = ?1", [id])?;
            convert_to_booleans(&mut set, &["is_filigree_set"]);
            let mut tiers = json_rows(
                db,
                "SELECT id, equipped_count FROM set_bonus_tiers WHERE set_id = ?1 ORDER BY equipped_count",
                [id],
            )?;
            let mut enchantments_by_tier = enchantments_via(
                db,
                "set_bonus_tier_enchantments",
                "tier_id",
                "j.tier_id IN (SELECT id FROM set_bonus_tiers WHERE set_id = ?1)",
                [id],
            )?;
            for tier in &mut tiers {
                let tier_id = tier["id"].as_i64().unwrap_or(0);
                tier["enchantments"] = Value::Array(enchantments_by_tier.remove(&tier_id).unwrap_or_default());
                tier["modifiers"] = Value::Array(modifiers_for(db, "set_bonus_tier", tier_id)?);
            }
            set["tiers"] = Value::Array(tiers);
            set["items"] = Value::Array(json_rows(
                db,
                "SELECT i.id, i.name, es.name AS slot, i.minimum_level FROM set_bonus_items sbi JOIN items i ON i.id = sbi.item_id
                   JOIN equipment_slots es ON es.id = i.slot_id WHERE sbi.set_id = ?1 ORDER BY i.name",
                [id],
            )?);
            set["augments"] = Value::Array(json_rows(
                db,
                "SELECT a.id, a.name, a.min_level FROM set_bonus_augments sba JOIN augments a ON a.id = sba.augment_id
                  WHERE sba.set_id = ?1 ORDER BY a.name, a.id",
                [id],
            )?);
            set["filigrees"] = Value::Array(filigrees_matching_set(db, Some(id))?);
            Ok(Json(set))
        })
        .await
}

fn filigrees_matching_set(db: &rusqlite::Connection, set_id: Option<i64>) -> Result<Vec<Value>, ApiError> {
    let sql = format!("{FILIGREE_SELECT} WHERE (?1 IS NULL OR f.set_id = ?1) ORDER BY f.name");
    let mut filigrees = json_rows(db, &sql, [set_id])?;
    for filigree in &mut filigrees {
        let filigree_id = filigree["id"].as_i64().unwrap_or(0);
        filigree["modifiers"] = Value::Array(modifiers_for(db, "filigree", filigree_id)?);
    }
    Ok(filigrees)
}

const FILIGREES_SORT_FIELDS: &[(&str, &str)] = &[
    ("name", "listed.name"),
    ("id", "listed.id"),
    ("menu", "listed.menu"),
    ("set_name", "listed.set_name"),
    ("description", "description"),
    ("icon", "icon"),
    ("set_id", "set_id"),
];

declare_list_parameters!(FiligreesParameters, FILIGREES_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/filigrees",
    tag = "sets",
    summary = "List filigrees",
    description = "Lists filigrees with their sets and modifiers.",
    params(
        FiligreesParameters,
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `filigrees` page", body = crate::routes::v1::response_schemas::FiligreesPageResponse),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn filigrees(State(state): State<AppState>, ApiQuery(query, _): ApiQuery) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut page =
                paged_rows(db, FILIGREE_SELECT, &query, "listed.name", "listed.name", FILIGREES_SORT_FIELDS)?;
            for filigree in &mut page.rows {
                let filigree_id = filigree["id"].as_i64().unwrap_or(0);
                filigree["modifiers"] = Value::Array(modifiers_for(db, "filigree", filigree_id)?);
            }
            Ok(Json(page.into_json("filigrees")))
        })
        .await
}

const SENTIENT_GEMS_SORT_FIELDS: &[(&str, &str)] =
    &[("name", "listed.name"), ("id", "listed.id"), ("description", "description"), ("icon", "icon")];

declare_list_parameters!(SentientGemsParameters, SENTIENT_GEMS_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/sentient-gems",
    tag = "sets",
    summary = "List sentient gems",
    description = "Lists sentient gems with icons and descriptions.",
    params(
        SentientGemsParameters,
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `sentient_gems` page", body = crate::routes::v1::response_schemas::SentientGemsPageResponse),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn sentient_gems(State(state): State<AppState>, ApiQuery(query, _): ApiQuery) -> Result<Json<Value>, ApiError> {
    paged_table_json(
        state,
        query,
        TableListSource {
            select_sql: "SELECT id, name, icon, description FROM sentient_gems",
            rows_key: "sentient_gems",
            name_column: "listed.name",
            default_order: "listed.name",
            sortable_fields: SENTIENT_GEMS_SORT_FIELDS,
            flag_columns: &[],
        },
    )
    .await
}
