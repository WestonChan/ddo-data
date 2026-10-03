use crate::db::paged_rows;
use crate::db::{convert_to_booleans, json_row, json_rows};
use crate::error::ApiError;
use crate::query::{declare_list_parameters, ApiQuery};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(races)).routes(routes!(race_detail))
}

const RACE_COLUMNS: &str = "id, name, short_name, description, starting_world, build_points, iconic_class, is_construct, no_past_life, skill_points";
const RACE_FLAG_COLUMNS: &[&str] = &["is_construct", "no_past_life"];

const RACES_SORT_FIELDS: &[(&str, &str)] = &[
    ("name", "listed.name"),
    ("id", "listed.id"),
    ("starting_world", "listed.starting_world"),
    ("build_points", "listed.build_points"),
    ("skill_points", "listed.skill_points"),
];

declare_list_parameters!(RacesParameters, RACES_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/races",
    tag = "races",
    summary = "List races",
    description = "Every playable race ordered by name with its short name, description, starting world, build \
                   points, the iconic class if it is an iconic race, and the construct and past-life flags.",
    params(
        RacesParameters,
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `races` page", body = Value),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn races(State(state): State<AppState>, ApiQuery(query, _): ApiQuery) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let select_sql = format!("SELECT {RACE_COLUMNS} FROM races");
            let mut page = paged_rows(db, &select_sql, &query, "listed.name", "listed.name", RACES_SORT_FIELDS)?;
            for row in &mut page.rows {
                convert_to_booleans(row, RACE_FLAG_COLUMNS);
            }
            Ok(Json(page.into_json("races")))
        })
        .await
}

#[utoipa::path(
    get,
    path = "/v1/races/{id}",
    tag = "races",
    summary = "Get a race",
    description = "One race with its `ability_modifiers`, the `granted_feats` every member gets, the racial `feats` \
                   it makes available, its `feat_slots` by level, and the skills it auto-buys.",
    params(("id" = i64, Path, description = "The race's numeric id from the list endpoint")), responses((status = 200, description = "The race with its child collections", body = Value), (status = 404, description = "No race has this id", body = crate::error::ErrorBody))
)]
async fn race_detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut race = json_row(db, &format!("SELECT {RACE_COLUMNS} FROM races WHERE id = ?1"), [id])?;
            convert_to_booleans(&mut race, RACE_FLAG_COLUMNS);
            race["ability_modifiers"] = Value::Array(json_rows(
                db,
                "SELECT s.name AS stat, m.modifier FROM race_ability_modifiers m JOIN stats s ON s.id = m.stat_id WHERE m.race_id = ?1 ORDER BY s.id",
                [id],
            )?);
            race["granted_feats"] = Value::Array(json_rows(
                db,
                "SELECT g.feat_name AS name, g.feat_id FROM race_granted_feats g WHERE g.race_id = ?1 ORDER BY g.sort_order",
                [id],
            )?);
            race["feats"] = Value::Array(json_rows(
                db,
                "SELECT id, name, description, icon, acquire, max_times_acquire FROM feats WHERE source_kind = 'race' AND source_id = ?1 ORDER BY name",
                [id],
            )?);
            race["feat_slots"] = Value::Array(json_rows(db, "SELECT level, feat_type, update_list FROM race_feat_slots WHERE race_id = ?1 ORDER BY level", [id])?);
            race["auto_buy_skills"] = Value::Array(
                json_rows(db, "SELECT skill FROM race_auto_buy_skills WHERE race_id = ?1 ORDER BY skill", [id])?.into_iter().map(|row| row["skill"].clone()).collect(),
            );
            Ok(Json(race))
        })
        .await
}
