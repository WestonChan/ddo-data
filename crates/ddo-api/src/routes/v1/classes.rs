use crate::db::{convert_to_booleans, json_row, json_rows};
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use serde_json::{Map, Value};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(classes)).routes(routes!(class_detail))
}

const CLASS_COLUMNS: &str = "id, name, base_class, base_class_id, not_heroic, description, small_icon, large_icon, skill_points, hit_points,
                             alignments, fortitude, reflex, will, bab, spell_points_per_level, casting_stats, class_specific_feat_types";

#[utoipa::path(
    get,
    path = "/v1/classes",
    tag = "classes",
    summary = "List classes",
    description = "Every class and archetype ordered by name with its base class, allowed alignments, hit and \
                   skill points per level, save and base-attack progressions, spell points, casting stats and \
                   class-specific feat types. Archetypes carry `base_class`; epic-only classes are `not_heroic`.",
    responses((status = 200, description = "All classes", body = Vec<Value>))
)]
async fn classes(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    state
        .read_db(|db| {
            let mut classes = json_rows(db, &format!("SELECT {CLASS_COLUMNS} FROM classes ORDER BY name"), [])?;
            for class in &mut classes {
                convert_to_booleans(class, &["not_heroic"]);
            }
            Ok(Json(classes))
        })
        .await
}

#[utoipa::path(
    get,
    path = "/v1/classes/{id}",
    tag = "classes",
    summary = "Get a class",
    description = "One class with its `class_skills` and `auto_buy_skills`, `spell_slots` keyed by class level, its \
                   `spells` with spell level and cost, `feat_slots` by level, `automatic_feats` granted by level, \
                   and the class `feats` it makes available.",
    params(("id" = i64, Path, description = "The class's numeric id from the list endpoint")), responses((status = 200, description = "The class with its child collections", body = Value), (status = 404, description = "No class has this id", body = crate::error::ErrorBody))
)]
async fn class_detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut class = json_row(db, &format!("SELECT {CLASS_COLUMNS} FROM classes WHERE id = ?1"), [id])?;
            convert_to_booleans(&mut class, &["not_heroic"]);
            let skills_in = |skill_table: &str| -> Result<Vec<Value>, ApiError> {
                Ok(json_rows(db, &format!("SELECT skill FROM {skill_table} WHERE class_id = ?1 ORDER BY skill"), [id])?
                    .into_iter()
                    .map(|row| row["skill"].clone())
                    .collect())
            };
            class["class_skills"] = Value::Array(skills_in("class_skills")?);
            class["auto_buy_skills"] = Value::Array(skills_in("class_auto_buy_skills")?);

            let mut spell_slots_by_class_level: Map<String, Value> = Map::new();
            for row in json_rows(db, "SELECT class_level, spell_level, slots FROM class_spell_slots WHERE class_id = ?1 ORDER BY class_level, spell_level", [id])? {
                let class_level = row["class_level"].as_i64().unwrap_or(0).to_string();
                let slots_at_level = spell_slots_by_class_level.entry(class_level).or_insert_with(|| Value::Array(Vec::new()));
                if let Value::Array(slot_counts) = slots_at_level {
                    slot_counts.push(row["slots"].clone());
                }
            }
            class["spell_slots"] = Value::Object(spell_slots_by_class_level);
            class["spells"] = Value::Array(json_rows(
                db,
                "SELECT spell_name AS name, spell_level, cost, max_caster_level, spell_id FROM class_spells WHERE class_id = ?1 ORDER BY spell_level, spell_name",
                [id],
            )?);
            let mut feat_slots = json_rows(db, "SELECT level, feat_type, auto_populate, singular, update_list FROM class_feat_slots WHERE class_id = ?1 ORDER BY level, id", [id])?;
            for feat_slot in &mut feat_slots {
                convert_to_booleans(feat_slot, &["auto_populate", "singular"]);
            }
            class["feat_slots"] = Value::Array(feat_slots);
            class["automatic_feats"] = Value::Array(json_rows(
                db,
                "SELECT level, feat_name AS name, feat_id FROM class_auto_feats WHERE class_id = ?1 ORDER BY level, feat_name",
                [id],
            )?);
            class["feats"] = Value::Array(json_rows(
                db,
                "SELECT id, name, description, icon, acquire, max_times_acquire FROM feats WHERE source_kind = 'class' AND source_id = ?1 ORDER BY name",
                [id],
            )?);
            Ok(Json(class))
        })
        .await
}
