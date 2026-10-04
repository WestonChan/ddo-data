use super::crafting::crafting_recipes_yielding;
use super::quests::{adventure_packs_dropping_via, quests_dropping_via, sources_via};
use crate::db::{
    convert_to_booleans, effects_for_owner, effects_via, json_row, json_rows, modifiers_for, paged_query, WhereClause,
};
use crate::error::ApiError;
use crate::query::{declare_list_parameters, declare_query_parameters, ApiQuery};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(augments)).routes(routes!(augment_detail))
}

declare_query_parameters! {
    pub(super) struct AugmentFilters {
        pub slot: Option<String>,
        pub family: Option<String>,
        pub max_level: Option<i64>,
    }
}

const AUGMENT_FLAG_COLUMNS: &[&str] = &["choose_level", "dual_values", "enter_value", "suppress_set_bonus"];

fn attach_child_collections(
    db: &rusqlite::Connection,
    augment: &mut Value,
    effects: Vec<Value>,
) -> Result<(), ApiError> {
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
    augment["effects"] = Value::Array(effects);
    augment["crafting"] = Value::Array(crafting_recipes_yielding(db, augment_id)?);
    Ok(())
}

const AUGMENT_COLUMNS: &str =
    "a.id, a.name, a.family, a.description, a.effect_description, a.min_level, a.icon, a.choose_level, a.levels,
                       a.level_values, a.level_values2, a.dual_values, a.enter_value, a.suppress_set_bonus, a.set_bonus,
                       a.adds_augment, a.grants_augment, a.weapon_class";

const AUGMENTS_SORT_FIELDS: &[(&str, &str)] =
    &[("name", "a.name"), ("id", "a.id"), ("min_level", "a.min_level"), ("family", "a.family"),
    ("adds_augment", "adds_augment"),
    ("choose_level", "choose_level"),
    ("description", "description"),
    ("dual_values", "dual_values"),
    ("effect_description", "effect_description"),
    ("enter_value", "enter_value"),
    ("grants_augment", "grants_augment"),
    ("icon", "icon"),
    ("level_values", "level_values"),
    ("level_values2", "level_values2"),
    ("levels", "levels"),
    ("set_bonus", "set_bonus"),
    ("suppress_set_bonus", "suppress_set_bonus"),
    ("weapon_class", "weapon_class"),
    ("slot", "(SELECT MIN(t.label) FROM augment_slots s JOIN augment_slot_types t ON t.id = s.slot_id WHERE s.augment_id = a.id)"),
    ("max_level", "a.min_level"),
];

declare_list_parameters!(AugmentsParameters, AUGMENTS_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/augments",
    tag = "augments",
    summary = "List augments",
    description = "Lists augments with sockets, rendered effects and crafting recipes.",
    params(
        AugmentsParameters,
        ("slot" = Option<String>, Query, description = "Socket label as /v1/augment-slot-types lists it, e.g. `red` or `lamordia: melancholic (accessory)`; case-insensitive"),
        ("family" = Option<String>, Query, description = "Augment family, e.g. `standard`, `lamordia`, `dino`, `crafting`"),
        ("max_level" = Option<i64>, Query, description = "Only augments usable at this character level or lower; augments with no minimum level always pass"),
    ),
    responses(
        (status = 200, description = "`total`, `limit`, `offset` and the `augments` page", body = crate::routes::v1::response_schemas::AugmentsPageResponse),
        (status = 400, description = "Unknown or malformed query parameter", body = crate::error::ErrorBody)
    )
)]
async fn augments(
    State(state): State<AppState>,
    ApiQuery(query, filters): ApiQuery<AugmentFilters>,
) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut where_clause = WhereClause::default();
            where_clause.add_name_search(query.q.as_deref(), "a.name");
            if let Some(slot_label) = &filters.slot {
                where_clause.add_bound_condition("EXISTS (SELECT 1 FROM augment_slots s JOIN augment_slot_types t ON t.id = s.slot_id WHERE s.augment_id = a.id AND t.label = ?)",
                    slot_label.to_lowercase(),
                );
            }
            if let Some(family) = &filters.family {
                where_clause.add_bound_condition("a.family = ?", family.clone());
            }
            if let Some(max_level) = filters.max_level {
                where_clause.add_bound_condition("(a.min_level IS NULL OR a.min_level <= ?)", max_level);
            }
            let mut page = paged_query(
                db,
                AUGMENT_COLUMNS,
                "augments a",
                &query,
                "a.name, a.min_level",
                AUGMENTS_SORT_FIELDS,
                &where_clause,
            )?;
            let augment_ids: Vec<i64> = page.rows.iter().filter_map(|augment| augment["id"].as_i64()).collect();
            let mut effects_by_augment = if augment_ids.is_empty() {
                std::collections::BTreeMap::new()
            } else {
                let placeholders = (1..=augment_ids.len()).map(|index| format!("?{index}")).collect::<Vec<_>>().join(", ");
                effects_via(
                    db,
                    "augment_effects",
                    "augment_id",
                    &format!("j.augment_id IN ({placeholders})"),
                    rusqlite::params_from_iter(augment_ids.iter()),
                )?
            };
            for augment in &mut page.rows {
                convert_to_booleans(augment, AUGMENT_FLAG_COLUMNS);
                let augment_id = augment["id"].as_i64().unwrap_or(0);
                attach_child_collections(db, augment, effects_by_augment.remove(&augment_id).unwrap_or_default())?;
            }
            Ok(Json(page.into_json("augments")))
        })
        .await
}

#[utoipa::path(
    get,
    path = "/v1/augments/{id}",
    tag = "augments",
    summary = "Get an augment",
    description = "Returns an augment with effects, crafting recipes, modifiers and drop sources.",
    params(("id" = i64, Path, description = "The augment's numeric id from the list endpoint")), responses((status = 200, description = "The augment with its child collections", body = crate::routes::v1::response_schemas::AugmentsDetailResponse), (status = 404, description = "No augment has this id", body = crate::error::ErrorBody))
)]
async fn augment_detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut augment = json_row(db, &format!("SELECT {AUGMENT_COLUMNS} FROM augments a WHERE a.id = ?1"), [id])?;
            convert_to_booleans(&mut augment, AUGMENT_FLAG_COLUMNS);
            let effects = effects_for_owner(db, "augment_effects", "augment_id", id)?;
            attach_child_collections(db, &mut augment, effects)?;
            augment["quests"] = Value::Array(quests_dropping_via(db, "augment_id", id)?);
            augment["adventure_packs"] = Value::Array(adventure_packs_dropping_via(db, "augment_id", id)?);
            augment["sources"] = Value::Array(sources_via(db, "augment_id", id)?);
            augment["modifiers"] = Value::Array(modifiers_for(db, "augment", id)?);
            Ok(Json(augment))
        })
        .await
}
