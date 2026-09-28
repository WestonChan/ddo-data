use crate::db::{booleanize, json_row, json_rows};
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use serde_json::{Map, Value};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(races)).routes(routes!(race)).routes(routes!(classes)).routes(routes!(class))
}

const RACE_COLUMNS: &str = "id, name, short_name, description, starting_world, build_points, iconic_class, is_construct, no_past_life, skill_points";
const RACE_FLAGS: &[&str] = &["is_construct", "no_past_life"];

#[utoipa::path(
    get,
    path = "/v1/races",
    tag = "characters",
    summary = "List races",
    description = "Every playable race ordered by name with its short name, description, starting world, build \
                   points, the iconic class if it is an iconic race, and the construct and past-life flags.",
    responses((status = 200, description = "All playable races", body = Vec<Value>))
)]
async fn races(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    state
        .query(|conn| {
            let mut rows = json_rows(conn, &format!("SELECT {RACE_COLUMNS} FROM races ORDER BY name"), [])?;
            for r in &mut rows {
                booleanize(r, RACE_FLAGS);
            }
            Ok(Json(rows))
        })
        .await
}

#[utoipa::path(
    get,
    path = "/v1/races/{id}",
    tag = "characters",
    summary = "Get a race",
    description = "One race with its `ability_modifiers`, the `granted_feats` every member gets, the racial `feats` \
                   it makes available, its `feat_slots` by level, and the skills it auto-buys.",
    params(("id" = i64, Path, description = "The race's numeric id from the list endpoint")), responses((status = 200, description = "The race with its child collections", body = Value), (status = 404, description = "No race has this id", body = crate::error::ErrorBody))
)]
async fn race(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .query(move |conn| {
            let mut race = json_row(conn, &format!("SELECT {RACE_COLUMNS} FROM races WHERE id = ?1"), [id])?;
            booleanize(&mut race, RACE_FLAGS);
            race["ability_modifiers"] = Value::Array(json_rows(
                conn,
                "SELECT s.name AS stat, m.modifier FROM race_ability_modifiers m JOIN stats s ON s.id = m.stat_id WHERE m.race_id = ?1 ORDER BY s.id",
                [id],
            )?);
            race["granted_feats"] = Value::Array(json_rows(
                conn,
                "SELECT g.feat_name AS name, g.feat_id FROM race_granted_feats g WHERE g.race_id = ?1 ORDER BY g.sort_order",
                [id],
            )?);
            race["feats"] = Value::Array(json_rows(
                conn,
                "SELECT id, name, description, icon, acquire, max_times_acquire FROM feats WHERE source_kind = 'race' AND source_id = ?1 ORDER BY name",
                [id],
            )?);
            race["feat_slots"] = Value::Array(json_rows(conn, "SELECT level, feat_type, update_list FROM race_feat_slots WHERE race_id = ?1 ORDER BY level", [id])?);
            race["auto_buy_skills"] = Value::Array(
                json_rows(conn, "SELECT skill FROM race_auto_buy_skills WHERE race_id = ?1 ORDER BY skill", [id])?.into_iter().map(|r| r["skill"].clone()).collect(),
            );
            Ok(Json(race))
        })
        .await
}

const CLASS_COLUMNS: &str = "id, name, base_class, base_class_id, not_heroic, description, small_icon, large_icon, skill_points, hit_points,
                             alignments, fortitude, reflex, will, bab, spell_points_per_level, casting_stats, class_specific_feat_types";

#[utoipa::path(
    get,
    path = "/v1/classes",
    tag = "characters",
    summary = "List classes",
    description = "Every class and archetype ordered by name with its base class, allowed alignments, hit and \
                   skill points per level, save and base-attack progressions, spell points, casting stats and \
                   class-specific feat types. Archetypes carry `base_class`; epic-only classes are `not_heroic`.",
    responses((status = 200, description = "All classes", body = Vec<Value>))
)]
async fn classes(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    state
        .query(|conn| {
            let mut rows = json_rows(conn, &format!("SELECT {CLASS_COLUMNS} FROM classes ORDER BY name"), [])?;
            for r in &mut rows {
                booleanize(r, &["not_heroic"]);
            }
            Ok(Json(rows))
        })
        .await
}

#[utoipa::path(
    get,
    path = "/v1/classes/{id}",
    tag = "characters",
    summary = "Get a class",
    description = "One class with its `class_skills` and `auto_buy_skills`, `spell_slots` keyed by class level, its \
                   `spells` with spell level and cost, `feat_slots` by level, `automatic_feats` granted by level, \
                   and the class `feats` it makes available.",
    params(("id" = i64, Path, description = "The class's numeric id from the list endpoint")), responses((status = 200, description = "The class with its child collections", body = Value), (status = 404, description = "No class has this id", body = crate::error::ErrorBody))
)]
async fn class(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .query(move |conn| {
            let mut class = json_row(conn, &format!("SELECT {CLASS_COLUMNS} FROM classes WHERE id = ?1"), [id])?;
            booleanize(&mut class, &["not_heroic"]);
            let skills = |table: &str| -> Result<Vec<Value>, ApiError> {
                Ok(json_rows(conn, &format!("SELECT skill FROM {table} WHERE class_id = ?1 ORDER BY skill"), [id])?
                    .into_iter()
                    .map(|r| r["skill"].clone())
                    .collect())
            };
            class["class_skills"] = Value::Array(skills("class_skills")?);
            class["auto_buy_skills"] = Value::Array(skills("class_auto_buy_skills")?);

            let mut slots: Map<String, Value> = Map::new();
            for row in json_rows(conn, "SELECT class_level, spell_level, slots FROM class_spell_slots WHERE class_id = ?1 ORDER BY class_level, spell_level", [id])? {
                let level = row["class_level"].as_i64().unwrap_or(0).to_string();
                let entry = slots.entry(level).or_insert_with(|| Value::Array(Vec::new()));
                if let Value::Array(v) = entry {
                    v.push(row["slots"].clone());
                }
            }
            class["spell_slots"] = Value::Object(slots);
            class["spells"] = Value::Array(json_rows(
                conn,
                "SELECT spell_name AS name, spell_level, cost, max_caster_level, spell_id FROM class_spells WHERE class_id = ?1 ORDER BY spell_level, spell_name",
                [id],
            )?);
            let mut feat_slots = json_rows(conn, "SELECT level, feat_type, auto_populate, singular, update_list FROM class_feat_slots WHERE class_id = ?1 ORDER BY level, id", [id])?;
            for s in &mut feat_slots {
                booleanize(s, &["auto_populate", "singular"]);
            }
            class["feat_slots"] = Value::Array(feat_slots);
            class["automatic_feats"] = Value::Array(json_rows(
                conn,
                "SELECT level, feat_name AS name, feat_id FROM class_auto_feats WHERE class_id = ?1 ORDER BY level, feat_name",
                [id],
            )?);
            class["feats"] = Value::Array(json_rows(
                conn,
                "SELECT id, name, description, icon, acquire, max_times_acquire FROM feats WHERE source_kind = 'class' AND source_id = ?1 ORDER BY name",
                [id],
            )?);
            Ok(Json(class))
        })
        .await
}
