use super::crafting::crafting_recipes_yielding_many;
use super::quests::{adventure_packs_dropping_via, quests_dropping_via, sources_via};
use crate::db::{
    convert_to_booleans, effects_via, grouped_rows, json_row, json_rows, modifiers_for, notes_for_owners, paged_query,
    sockets_by_id, sql_integer_list, WhereClause,
};
use crate::entity_slugs::EntityPath;
use crate::error::ApiError;
use crate::query::{declare_list_parameters, declare_query_parameters, ApiQuery};
use crate::state::AppState;
use axum::extract::State;
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
        #[serde(default, deserialize_with = "crate::query::repeated_key_values")]
        pub crafting_system: Vec<String>,
        pub family: Option<String>,
        pub max_level: Option<i64>,
    }
    repeatable: ["crafting_system"]
}

const AUGMENT_FLAG_COLUMNS: &[&str] = &["choose_level", "dual_values", "enter_value", "suppress_set_bonus"];

pub(super) fn attach_augment_collections(db: &rusqlite::Connection, augments: &mut [Value]) -> Result<(), ApiError> {
    let ids = augments.iter().filter_map(|augment| augment["id"].as_i64()).collect::<Vec<_>>();
    let mut effects =
        effects_via(db, "augment_effects", "augment_id", &format!("j.augment_id IN ({})", sql_integer_list(&ids)), [])?;
    let mut slots = grouped_rows(db,&ids,"SELECT link.augment_id AS owner_id,t.label FROM augment_slots link JOIN augment_slot_types t ON t.id=link.slot_id WHERE link.augment_id IN ({ids}) ORDER BY link.augment_id,t.label")?;
    let mut crafting = crafting_recipes_yielding_many(db, &ids)?;
    let mut grants = grouped_rows(db,&ids,"SELECT augment_id AS owner_id,slot_id FROM augment_socket_grants WHERE augment_id IN ({ids}) ORDER BY augment_id,sort_order")?;
    let sockets = sockets_by_id(db)?;
    for augment in augments {
        let id = augment["id"].as_i64().unwrap();
        convert_to_booleans(augment, AUGMENT_FLAG_COLUMNS);
        augment["effects"] = Value::Array(effects.remove(&id).unwrap_or_default());
        augment["slots"] =
            Value::Array(slots.remove(&id).unwrap_or_default().into_iter().map(|row| row["label"].clone()).collect());
        augment["crafting"] = Value::Array(crafting.remove(&id).unwrap_or_default());
        augment["grants_sockets"] = Value::Array(
            grants
                .remove(&id)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|grant| sockets.get(&grant["slot_id"].as_i64().unwrap()).cloned())
                .collect(),
        );
    }
    Ok(())
}

pub(super) fn augments_fitting_socket(db: &rusqlite::Connection, slot_id: i64) -> Result<Vec<Value>, ApiError> {
    let mut augments = json_rows(db,&format!("SELECT {AUGMENT_COLUMNS} FROM augments a JOIN augment_slots fits ON fits.augment_id=a.id WHERE fits.slot_id=?1 ORDER BY a.name,a.min_level,a.slug"),[slot_id])?;
    attach_augment_collections(db, &mut augments)?;
    Ok(augments)
}

const AUGMENT_COLUMNS: &str =
    "a.id, a.name, a.family, a.description, a.effect_description, a.min_level, a.icon, a.choose_level, a.levels,
                       a.level_values, a.level_values2, a.dual_values, a.enter_value, a.suppress_set_bonus, a.set_bonus,
                       a.adds_augment, a.grants_augment, a.weapon_class";

const AUGMENTS_SORT_FIELDS: &[(&str, &str)] =
    &[("name", "a.name"), ("slug", "a.name"), ("id", "a.id"), ("min_level", "a.min_level"), ("family", "a.family"),
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
    ("crafting_system", "(SELECT MIN(system.name) FROM crafting_recipe_augments link JOIN crafting_recipes recipe ON recipe.id=link.recipe_id JOIN crafting_systems system ON system.id=recipe.system_id WHERE link.augment_id=a.id)"),
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
        ("crafting_system" = Option<Vec<String>>, Query, style = Form, explode = true, description = "Repeat crafting system slugs from /v1/crafting-systems to match augments yielded by any; omitted includes all."),
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
            if !filters.crafting_system.is_empty() {
                where_clause.add_bound_list_condition("EXISTS (SELECT 1 FROM crafting_recipe_augments link JOIN crafting_recipes recipe ON recipe.id=link.recipe_id JOIN crafting_systems system ON system.id=recipe.system_id WHERE link.augment_id=a.id AND system.slug IN (?))",filters.crafting_system);
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
                "a.name, a.min_level, a.slug",
                AUGMENTS_SORT_FIELDS,
                &where_clause,
            )?;
            attach_augment_collections(db,&mut page.rows)?;
            Ok(Json(page.into_json("augments")))
        })
        .await
}

#[utoipa::path(
    get,
    path = "/v1/augments/{slug}",
    tag = "augments",
    summary = "Get an augment",
    description = "Returns an augment with effects, crafting recipes, modifiers and drop sources.",
    params(("slug" = String, Path, description = "Current entity slug or numeric id from the corresponding list.")), responses((status = 200, description = "The augment with its child collections", body = crate::routes::v1::response_schemas::AugmentsDetailResponse), (status = 308, description = "Renamed slug; follow Location to its current slug."), (status = 404, description = "No augment has this slug or numeric id", body = crate::error::ErrorBody))
)]
async fn augment_detail(State(state): State<AppState>, EntityPath(id): EntityPath) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut augment = json_row(db, &format!("SELECT {AUGMENT_COLUMNS} FROM augments a WHERE a.id = ?1"), [id])?;
            attach_augment_collections(db, std::slice::from_mut(&mut augment))?;
            augment["notes"] = Value::Array(notes_for_owners(db, "augments", &[id])?.remove(&id).unwrap_or_default());
            augment["quests"] = Value::Array(quests_dropping_via(db, "augment_id", id)?);
            augment["adventure_packs"] = Value::Array(adventure_packs_dropping_via(db, "augment_id", id)?);
            augment["sources"] = Value::Array(sources_via(db, "augment_id", id)?);
            augment["modifiers"] = Value::Array(modifiers_for(db, "augment", id)?);
            Ok(Json(augment))
        })
        .await
}
