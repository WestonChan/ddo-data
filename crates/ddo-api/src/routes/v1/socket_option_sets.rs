use super::crafting::crafting_entries;
use crate::db::{
    effects_via, grouped_rows, json_row, json_rows, modifiers_for_owners, paged_query, sockets_by_id, sql_integer_list,
    WhereClause,
};
use crate::entity_slugs::EntityPath;
use crate::error::ApiError;
use crate::query::{declare_list_parameters, declare_query_parameters, ApiQuery};
use crate::state::AppState;
use axum::{extract::State, Json};
use rusqlite::Connection;
use serde_json::Value;
use std::collections::BTreeMap;
use utoipa_axum::{router::OpenApiRouter, routes};

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(socket_option_sets)).routes(routes!(socket_option_set_detail))
}

pub(super) fn options_for_sets(db: &Connection, ids: &[i64]) -> Result<BTreeMap<i64, Vec<Value>>, ApiError> {
    let mut options = grouped_rows(db,ids,"SELECT o.set_id AS owner_id,o.id AS option_id,o.name,o.description,o.min_level,o.icon,
        (SELECT t.label FROM socket_option_grants g JOIN augment_slot_types t ON t.id=g.slot_id WHERE g.option_id=o.id ORDER BY g.sort_order LIMIT 1) AS grants_slot
        FROM socket_options o WHERE o.set_id IN ({ids}) ORDER BY o.set_id,o.option_order")?;
    let option_ids = options.values().flatten().filter_map(|option| option["option_id"].as_i64()).collect::<Vec<_>>();
    let effects = effects_via(
        db,
        "socket_option_effects",
        "option_id",
        &format!("j.option_id IN ({})", sql_integer_list(&option_ids)),
        [],
    )?;
    let modifiers = modifiers_for_owners(db, "socket_option", &option_ids)?;
    let sets = grouped_rows(db,&option_ids,"SELECT link.option_id AS owner_id,s.id,s.slug,s.name FROM socket_option_set_bonuses link JOIN set_bonuses s ON s.id=link.set_bonus_id WHERE link.option_id IN ({ids}) ORDER BY link.option_id,s.name")?;
    let grants = grouped_rows(db,&option_ids,"SELECT option_id AS owner_id,slot_id FROM socket_option_grants WHERE option_id IN ({ids}) ORDER BY option_id,sort_order")?;
    let sockets = sockets_by_id(db)?;
    let crafting = crafting_entries(db,grouped_rows(db,&option_ids,"SELECT o.id AS owner_id,r.id AS recipe_id,system.name AS system
        FROM socket_options o JOIN socket_option_sets s ON s.id=o.set_id JOIN crafting_steps step ON step.slot_id=s.slot_id
        JOIN crafting_recipes r ON r.step_id=step.id JOIN crafting_systems system ON system.id=r.system_id
        WHERE o.id IN ({ids}) AND (
          EXISTS (SELECT 1 FROM crafting_recipe_set_options price WHERE price.option_id=o.id AND price.recipe_id=r.id)
          OR (NOT EXISTS (SELECT 1 FROM crafting_recipe_set_options price WHERE price.option_id=o.id)
              AND NOT EXISTS (SELECT 1 FROM crafting_recipe_set_options price WHERE price.recipe_id=r.id)))
        ORDER BY o.id,system.name,r.sort_order")?)?;
    for option in options.values_mut().flatten() {
        let id = option["option_id"].as_i64().unwrap();
        option["effects"] = Value::Array(effects.get(&id).cloned().unwrap_or_default());
        option["modifiers"] = Value::Array(modifiers.get(&id).cloned().unwrap_or_default());
        option["sets"] = Value::Array(sets.get(&id).cloned().unwrap_or_default());
        option["crafting"] = Value::Array(crafting.get(&id).cloned().unwrap_or_default());
        option["grants_sockets"] = Value::Array(
            grants
                .get(&id)
                .into_iter()
                .flatten()
                .filter_map(|grant| sockets.get(&grant["slot_id"].as_i64().unwrap()).cloned())
                .collect(),
        );
        option.as_object_mut().unwrap().remove("option_id");
    }
    Ok(options)
}

declare_query_parameters! {
    struct SocketOptionSetFilters {
        pub slot: Option<String>,
    }
}

const SET_COLUMNS: &str = "s.id,s.slug,s.name,s.variant,t.label AS label";
const SET_SORT_FIELDS: &[(&str, &str)] = &[
    ("id", "s.id"),
    ("slug", "s.slug"),
    ("name", "s.name"),
    ("variant", "s.variant"),
    ("label", "t.label"),
    ("slot", "t.label"),
];
declare_list_parameters!(SocketOptionSetsParameters, SET_SORT_FIELDS, "");

#[utoipa::path(get,path="/v1/socket-option-sets",tag="crafting",summary="List socket option sets",
    description="Lists fixed choice sets owned by socket labels.",params(SocketOptionSetsParameters,
        ("slot" = Option<String>,Query,description="Socket label from /v1/augment-slot-types; omitted includes every label.")),
    responses((status=200,description="The paged socket option sets",body=super::response_schemas::SocketOptionSetsResponse),
        (status=400,description="Unknown or malformed query parameter",body=crate::error::ErrorBody)))]
async fn socket_option_sets(
    State(state): State<AppState>,
    ApiQuery(query, filters): ApiQuery<SocketOptionSetFilters>,
) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let filters = filters.validated(db)?;

            let mut conditions = WhereClause::default();
            conditions.add_name_search(query.q.as_deref(), "s.name");
            if let Some(label) = filters.slot {
                conditions.add_bound_condition("t.label=?", label.to_lowercase());
            }
            Ok(Json(
                paged_query(
                    db,
                    SET_COLUMNS,
                    "socket_option_sets s JOIN augment_slot_types t ON t.id=s.slot_id",
                    &query,
                    "s.name,s.variant,s.slug",
                    SET_SORT_FIELDS,
                    &conditions,
                )?
                .into_json("socket_option_sets"),
            ))
        })
        .await
}

#[utoipa::path(get,path="/v1/socket-option-sets/{slug}",tag="crafting",summary="Get a socket option set",
    description="Returns a socket's fixed choices and the items that carry this set.",
    params(("slug"=String,Path,description="Current socket option set slug or numeric id from the list.")),
    responses((status=200,description="The set with options and carrier items",body=super::response_schemas::SocketOptionSetResponse),
        (status=308,description="Renamed slug; follow Location to its current slug."),
        (status=404,description="No socket option set has this slug or numeric id",body=crate::error::ErrorBody)))]
async fn socket_option_set_detail(
    State(state): State<AppState>,
    EntityPath(id): EntityPath,
) -> Result<Json<Value>, ApiError> {
    state.read_db(move |db| {
        let mut set = json_row(db,&format!("SELECT {SET_COLUMNS} FROM socket_option_sets s JOIN augment_slot_types t ON t.id=s.slot_id WHERE s.id=?1"),[id])?;
        set["options"] = Value::Array(options_for_sets(db,&[id])?.remove(&id).unwrap_or_default());
        set["items"] = Value::Array(json_rows(db,"SELECT DISTINCT i.id,i.slug,i.name,i.minimum_level FROM item_augment_slots socket JOIN items i ON i.id=socket.item_id WHERE socket.option_set_id=?1 ORDER BY i.name,i.slug",[id])?);
        Ok(Json(set))
    }).await
}
