use crate::db::{json_row, json_rows};
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use rusqlite::Connection;
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(crafting_systems)).routes(routes!(crafting_system_detail))
}

const CRAFTING_SYSTEM_COLUMNS_AND_JOINS: &str = "s.id, s.name, s.page, p.name AS pack, s.npc,
        (SELECT COUNT(*) FROM crafting_ingredients i WHERE i.system_id = s.id) AS ingredient_count,
        (SELECT COUNT(*) FROM crafting_recipes r WHERE r.system_id = s.id) AS recipe_count
   FROM crafting_systems s LEFT JOIN adventure_packs p ON p.id = s.pack_id";

fn attach_augment_families(db: &Connection, crafting_system: &mut Value) -> Result<(), ApiError> {
    let system_id = crafting_system["id"].as_i64().unwrap_or(0);
    let families =
        json_rows(db, "SELECT family FROM crafting_system_families WHERE system_id = ?1 ORDER BY family", [system_id])?;
    crafting_system["families"] = Value::Array(families.into_iter().map(|row| row["family"].clone()).collect());
    Ok(())
}

fn recipe_cost(db: &Connection, recipe_id: i64) -> Result<Vec<Value>, ApiError> {
    json_rows(
        db,
        "SELECT i.name AS ingredient, i.tier, ri.quantity FROM crafting_recipe_ingredients ri
           JOIN crafting_ingredients i ON i.id = ri.ingredient_id
          WHERE ri.recipe_id = ?1 ORDER BY i.id",
        [recipe_id],
    )
}

pub fn crafting_recipes_yielding(db: &Connection, augment_id: i64) -> Result<Vec<Value>, ApiError> {
    let mut recipes = json_rows(
        db,
        "SELECT r.id, s.name AS system, r.tier, r.option FROM crafting_recipe_augments ra
           JOIN crafting_recipes r ON r.id = ra.recipe_id JOIN crafting_systems s ON s.id = r.system_id
          WHERE ra.augment_id = ?1 ORDER BY s.name, r.sort_order",
        [augment_id],
    )?;
    for recipe in &mut recipes {
        let recipe_id = recipe.as_object_mut().and_then(|r| r.remove("id")).and_then(|v| v.as_i64()).unwrap_or(0);
        recipe["cost"] = Value::Array(recipe_cost(db, recipe_id)?);
    }
    Ok(recipes)
}

#[utoipa::path(
    get,
    path = "/v1/crafting-systems",
    tag = "crafting",
    summary = "List crafting systems",
    description = "Every crafting system read from ddowiki, one per wiki crafting page, ordered by name: its `page`, \
                   the adventure `pack` it belongs to (null when the wiki names none), the crafting `npc` or station \
                   (free text, may be null), the Maetrim augment `families` its options live in (the values \
                   /v1/augments accepts in `family`), and how many ingredients and recipes the wiki lists \
                   (`ingredient_count`, `recipe_count`). Empty until a system has been read.",
    responses((status = 200, description = "The whole table with each system's families and counts", body = Vec<Value>))
)]
async fn crafting_systems(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    let crafting_systems = state
        .read_db(|db| {
            let mut crafting_systems =
                json_rows(db, &format!("SELECT {CRAFTING_SYSTEM_COLUMNS_AND_JOINS} ORDER BY s.name"), [])?;
            for crafting_system in &mut crafting_systems {
                attach_augment_families(db, crafting_system)?;
            }
            Ok(crafting_systems)
        })
        .await?;
    Ok(Json(crafting_systems))
}

#[utoipa::path(
    get,
    path = "/v1/crafting-systems/{id}",
    tag = "crafting",
    summary = "Get a crafting system",
    description = "One crafting system as the list returns it, plus its `ingredients` (each with `name`, `tier` of \
                   heroic, epic, legendary or any, and the wiki's free-text `bind` and `source`, either may be null) \
                   and its `recipes` in page order. Each recipe carries its `tier`, the socket label it fills \
                   (`slot`, as /v1/augment-slot-types lists it, null when the wiki row names none), the wiki's \
                   label for the row (`option`), a free-text `note`, the `augments` it yields (`id`, `name`, \
                   `min_level`; every Maetrim augment of that name in the system's families, so a name that repeats \
                   per level or element appears once per augment; empty when the row has no augment counterpart, \
                   and then `note` says why) and its `cost`, one `{ ingredient, tier, quantity }` per ingredient.",
    params(("id" = i64, Path, description = "The crafting system's numeric id from the list endpoint")),
    responses(
        (status = 200, description = "The system with its ingredients and recipes", body = Value),
        (status = 404, description = "No crafting system has this id", body = crate::error::ErrorBody)
    )
)]
async fn crafting_system_detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut crafting_system =
                json_row(db, &format!("SELECT {CRAFTING_SYSTEM_COLUMNS_AND_JOINS} WHERE s.id = ?1"), [id])?;
            attach_augment_families(db, &mut crafting_system)?;
            crafting_system["ingredients"] = Value::Array(json_rows(
                db,
                "SELECT id, name, tier, bind, source FROM crafting_ingredients WHERE system_id = ?1 ORDER BY id",
                [id],
            )?);
            let mut recipes = json_rows(
                db,
                "SELECT r.id, r.tier, t.label AS slot, r.option, r.note FROM crafting_recipes r
                   LEFT JOIN augment_slot_types t ON t.id = r.slot_id
                  WHERE r.system_id = ?1 ORDER BY r.sort_order",
                [id],
            )?;
            for recipe in &mut recipes {
                let recipe_id = recipe["id"].as_i64().unwrap_or(0);
                recipe["augments"] = Value::Array(json_rows(
                    db,
                    "SELECT a.id, a.name, a.min_level FROM crafting_recipe_augments ra JOIN augments a ON a.id = ra.augment_id
                      WHERE ra.recipe_id = ?1 ORDER BY a.id",
                    [recipe_id],
                )?);
                recipe["cost"] = Value::Array(recipe_cost(db, recipe_id)?);
            }
            crafting_system["recipes"] = Value::Array(recipes);
            Ok(Json(crafting_system))
        })
        .await
}
