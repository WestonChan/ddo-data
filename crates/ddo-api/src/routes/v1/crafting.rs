use super::vocabularies::VocabularySource;
use super::ValueVocabulary;
use crate::db::{
    convert_to_booleans, effects_via_with_link_order, grouped_rows, json_row, json_rows, modifiers_for_owners,
    paged_rows_with_filter, sockets_by_id, sql_integer_list, WhereClause,
};
use crate::entity_slugs::EntityPath;
use crate::error::ApiError;
use crate::query::{declare_list_parameters, declare_query_parameters, ApiQuery};
use crate::state::AppState;
use axum::extract::State;
use axum::Json;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::BTreeMap;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(crafting_systems)).routes(routes!(crafting_system_detail))
}

static INGREDIENT_USES: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    format!("WITH ingredient_uses(ingredient_id,system_id) AS ({})", ddo_model::schema::CRAFTING_INGREDIENT_USES)
});

pub(super) fn crafting_systems_vocabulary() -> VocabularySource {
    VocabularySource::new(
        "slug",
        &["name"],
        "SELECT s.id, s.slug, s.name, s.page, p.name AS pack, facts.has_items,
                (SELECT station.name || COALESCE(', ' || station.place,'')
                 FROM crafting_system_stations link JOIN crafting_stations station ON station.id=link.station_id
                 WHERE link.system_id=s.id ORDER BY link.sort_order LIMIT 1) AS npc
             FROM crafting_systems s JOIN crafting_system_item_facts facts ON facts.crafting_system_id = s.id
             LEFT JOIN adventure_packs p ON p.id = s.pack_id",
    )
    .with_counted_listing(
        |vocabulary| {
            format!(
                "{vocabulary}.*, COALESCE(ingredients.ingredient_count, 0) AS ingredient_count,
             COALESCE(recipes.recipe_count, 0) AS recipe_count,
             COALESCE(reached_items.item_count, 0) AS item_count"
            )
        },
        |vocabulary| {
            format!(
                "LEFT JOIN (
                    {} SELECT system_id, COUNT(*) AS ingredient_count FROM ingredient_uses GROUP BY system_id
                 ) ingredients ON ingredients.system_id = {vocabulary}.id
                 LEFT JOIN (SELECT system_id, COUNT(*) AS recipe_count FROM crafting_recipes GROUP BY system_id) recipes
                    ON recipes.system_id = {vocabulary}.id
                 LEFT JOIN (
                    SELECT reach.crafting_system_id, COUNT(*) AS item_count
                    FROM crafting_system_items reach JOIN items i ON i.id = reach.item_id
                    WHERE NOT i.is_legacy GROUP BY reach.crafting_system_id
                 ) reached_items ON reached_items.crafting_system_id = {vocabulary}.id",
                INGREDIENT_USES.as_str()
            )
        },
    )
    .served_by("/v1/crafting-systems", "crafting_systems")
    .for_entities(ddo_model::enums::EntityKind::CraftingSystem)
}

fn attach_augment_families(db: &Connection, systems: &mut [Value]) -> Result<(), ApiError> {
    let ids = systems.iter().filter_map(|system| system["id"].as_i64()).collect::<Vec<_>>();
    let mut families = grouped_rows(db, &ids, "SELECT system_id AS owner_id,family FROM crafting_system_families WHERE system_id IN ({ids}) ORDER BY system_id,family")?;
    for system in systems {
        system["families"] = Value::Array(
            families
                .remove(&system["id"].as_i64().unwrap())
                .unwrap_or_default()
                .into_iter()
                .map(|row| row["family"].clone())
                .collect(),
        );
    }
    Ok(())
}

const COST_COLUMNS: &str = "ingredient.name AS ingredient,ingredient.id AS ingredient_id,ingredient.slug AS ingredient_slug,
    item.name AS item,item.id AS item_id,item.slug AS item_slug,COALESCE(ingredient.tier,cost.tier) AS tier,cost.quantity";

fn unlocks_for(db: &Connection, parent: &str, ids: &[i64]) -> Result<BTreeMap<i64, Vec<Value>>, ApiError> {
    grouped_rows(db, ids, &format!("SELECT u.{parent} AS owner_id,u.kind,u.value,u.text,
        q.name AS quest,q.id AS quest_id,q.slug AS quest_slug,p.name AS patron,p.id AS patron_id,p.slug AS patron_slug,
        pack.name AS pack,pack.id AS pack_id,pack.slug AS pack_slug,i.name AS item,i.id AS item_id,i.slug AS item_slug,
        ingredient.name AS crafting_ingredient,ingredient.id AS crafting_ingredient_id,ingredient.slug AS crafting_ingredient_slug,
        f.name AS feat,f.id AS feat_id,f.slug AS feat_slug FROM crafting_unlocks u
        LEFT JOIN quests q ON q.id=u.quest_id LEFT JOIN patrons p ON p.id=u.patron_id LEFT JOIN adventure_packs pack ON pack.id=u.pack_id
        LEFT JOIN items i ON i.id=u.item_id LEFT JOIN crafting_ingredients ingredient ON ingredient.id=u.crafting_ingredient_id LEFT JOIN feats f ON f.id=u.feat_id
        WHERE u.{parent} IN ({{ids}}) ORDER BY u.{parent},u.sort_order"))
}

pub(super) fn steps_by_id(db: &Connection, ids: &[i64]) -> Result<BTreeMap<i64, Value>, ApiError> {
    let sockets = sockets_by_id(db)?;
    let mut effects = effects_via_with_link_order(
        db,
        "crafting_step_effects",
        "step_id",
        &format!("j.step_id IN ({})", sql_integer_list(ids)),
        [],
    )?;
    let scopes = json_rows(db, &format!("SELECT e.step_id,e.sort_order,s.name AS equipment_slot FROM crafting_step_effects e LEFT JOIN equipment_slots s ON s.id=e.equipment_slot_id WHERE e.step_id IN ({})", sql_integer_list(ids)), [])?
        .into_iter().map(|row| ((row["step_id"].as_i64().unwrap(),row["sort_order"].as_i64().unwrap()),row["equipment_slot"].clone())).collect::<BTreeMap<_,_>>();
    let mut modifiers = modifiers_for_owners(db, "crafting_step", ids)?;
    let mut costs = grouped_rows(db, ids, &format!("SELECT cost.step_id AS owner_id,1 AS from_step,cost.tier AS grade,{COST_COLUMNS} FROM crafting_step_costs cost LEFT JOIN crafting_ingredients ingredient ON ingredient.id=cost.ingredient_id LEFT JOIN items item ON item.id=cost.item_id WHERE cost.step_id IN ({{ids}}) ORDER BY cost.step_id,cost.sort_order"))?;
    let mut steps = BTreeMap::new();
    for mut step in json_rows(
        db,
        &format!(
            "SELECT id,name,slot_id,is_random,consumed_outputs FROM crafting_steps WHERE id IN ({}) ORDER BY sort_order",
            sql_integer_list(ids)
        ),
        [],
    )? {
        let id = step["id"].as_i64().unwrap();
        step["socket"] = step["slot_id"].as_i64().and_then(|slot| sockets.get(&slot)).cloned().unwrap_or(Value::Null);
        convert_to_booleans(&mut step, &["is_random"]);
        step["effects"] = Value::Array(
            effects
                .remove(&id)
                .unwrap_or_default()
                .into_iter()
                .map(|(order, mut line)| {
                    line["equipment_slot"] = scopes.get(&(id, order)).cloned().unwrap_or(Value::Null);
                    line
                })
                .collect(),
        );
        step["modifiers"] = Value::Array(modifiers.remove(&id).unwrap_or_default());
        let mut grades = BTreeMap::<String, Vec<Value>>::new();
        for mut cost in costs.remove(&id).unwrap_or_default() {
            let grade = cost.as_object_mut().unwrap().remove("grade").unwrap().as_str().unwrap().to_string();
            convert_to_booleans(&mut cost, &["from_step"]);
            grades.entry(grade).or_default().push(cost);
        }
        step["costs"] = Value::Array(
            ddo_model::enums::CraftingTier::ALL
                .iter()
                .filter_map(|tier| {
                    grades.remove(tier.as_str()).map(|cost| serde_json::json!({"tier":tier.as_str(),"cost":cost}))
                })
                .collect(),
        );
        for key in ["id", "slot_id"] {
            step.as_object_mut().unwrap().remove(key);
        }
        steps.insert(id, step);
    }
    Ok(steps)
}

pub(super) fn recipes_by_id(db: &Connection, ids: &[i64]) -> Result<BTreeMap<i64, Value>, ApiError> {
    let sockets = sockets_by_id(db)?;
    let mut costs = grouped_rows(db, ids, &format!("SELECT cost.recipe_id AS owner_id,cost.from_step,{COST_COLUMNS} FROM crafting_recipe_effective_costs cost LEFT JOIN crafting_ingredients ingredient ON ingredient.id=cost.ingredient_id LEFT JOIN items item ON item.id=cost.item_id WHERE cost.recipe_id IN ({{ids}}) ORDER BY cost.recipe_id,cost.from_step,cost.sort_order"))?;
    let mut augments = grouped_rows(db, ids, "SELECT link.recipe_id AS owner_id,a.id,a.slug,a.name,a.min_level FROM crafting_recipe_augments link JOIN augments a ON a.id=link.augment_id WHERE link.recipe_id IN ({ids}) ORDER BY link.recipe_id,link.sort_order")?;
    let mut options = grouped_rows(db, ids, "SELECT link.recipe_id AS owner_id,s.name AS 'set',s.id AS set_id,s.slug AS set_slug,o.name FROM crafting_recipe_set_options link JOIN socket_options o ON o.id=link.option_id JOIN socket_option_sets s ON s.id=o.set_id WHERE link.recipe_id IN ({ids}) ORDER BY link.recipe_id,s.slug,o.option_order")?;
    let mut unlocks = unlocks_for(db, "recipe_id", ids)?;
    let mut recipes = BTreeMap::new();
    for mut recipe in json_rows(db, &format!("SELECT r.id,r.tier,r.option,r.note,step.name AS step,step.consumed_outputs,t.label AS slot,g.label AS grants_slot,r.grants_slot_id,
        ingredient.name AS yields_ingredient,ingredient.id AS yields_ingredient_id,ingredient.slug AS yields_ingredient_slug
        FROM crafting_recipes r LEFT JOIN crafting_steps step ON step.id=r.step_id LEFT JOIN augment_slot_types t ON t.id=step.slot_id
        LEFT JOIN augment_slot_types g ON g.id=r.grants_slot_id LEFT JOIN crafting_ingredients ingredient ON ingredient.id=r.yields_ingredient_id WHERE r.id IN ({}) ORDER BY r.sort_order",sql_integer_list(ids)),[])? {
        let id = recipe["id"].as_i64().unwrap();
        let mut recipe_costs = costs.remove(&id).unwrap_or_default();
        recipe["cost_from_step"] = Value::Bool(!recipe_costs.is_empty() && recipe_costs.iter().all(|cost| cost["from_step"]==1));
        for cost in &mut recipe_costs { convert_to_booleans(cost, &["from_step"]); }
        recipe["cost"] = Value::Array(recipe_costs);
        recipe["augments"] = Value::Array(augments.remove(&id).unwrap_or_default());
        recipe["set_options"] = Value::Array(options.remove(&id).unwrap_or_default());
        recipe["unlocks"] = Value::Array(unlocks.remove(&id).unwrap_or_default());
        recipe["grants_sockets"] = Value::Array(recipe["grants_slot_id"].as_i64().and_then(|slot| sockets.get(&slot)).cloned().into_iter().collect());
        for key in ["id","grants_slot_id"] { recipe.as_object_mut().unwrap().remove(key); }
        recipes.insert(id,recipe);
    }
    Ok(recipes)
}

pub(super) fn crafting_entries(
    db: &Connection,
    links: BTreeMap<i64, Vec<Value>>,
) -> Result<BTreeMap<i64, Vec<Value>>, ApiError> {
    let ids = links.values().flatten().filter_map(|link| link["recipe_id"].as_i64()).collect::<Vec<_>>();
    let recipes = recipes_by_id(db, &ids)?;
    Ok(links.into_iter().map(|(owner,links)| (owner,links.into_iter().map(|link| {
        let recipe = &recipes[&link["recipe_id"].as_i64().unwrap()];
        serde_json::json!({"system":link["system"],"tier":recipe["tier"],"option":recipe["option"],"step":recipe["step"],"consumed_outputs":recipe["consumed_outputs"],"cost":recipe["cost"],"cost_from_step":recipe["cost_from_step"]})
    }).collect())).collect())
}

pub(super) fn crafting_recipes_yielding_many(
    db: &Connection,
    ids: &[i64],
) -> Result<BTreeMap<i64, Vec<Value>>, ApiError> {
    crafting_entries(db,grouped_rows(db,ids,"SELECT link.augment_id AS owner_id,r.id AS recipe_id,s.name AS system FROM crafting_recipe_augments link JOIN crafting_recipes r ON r.id=link.recipe_id JOIN crafting_systems s ON s.id=r.system_id WHERE link.augment_id IN ({ids}) ORDER BY link.augment_id,s.name,r.sort_order")?)
}

const CRAFTING_SYSTEMS_SORT_FIELDS: &[(&str, &str)] = &[
    ("name", "listed.name"),
    ("slug", "listed.name"),
    ("id", "listed.id"),
    ("pack", "listed.pack"),
    ("ingredient_count", "listed.ingredient_count"),
    ("recipe_count", "listed.recipe_count"),
    ("item_count", "listed.item_count"),
    ("has_items", "listed.has_items"),
    ("npc", "npc"),
    ("page", "page"),
];

declare_list_parameters!(CraftingSystemsParameters, CRAFTING_SYSTEMS_SORT_FIELDS, "");

declare_query_parameters! {
    struct CraftingSystemFilters {
        has_items: Option<bool>,
    }
}

#[utoipa::path(
    get,
    path = "/v1/crafting-systems",
    tag = "crafting",
    summary = "List crafting systems",
    description = "Lists crafting systems with their ingredient, recipe and non-legacy item counts.",
    params(
        CraftingSystemsParameters,
        CraftingSystemFilters,
        ("has_items" = Option<bool>, Query, description = "Whether the system makes or upgrades at least one non-legacy named item; omitted includes every system."),
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `crafting_systems` page", body = crate::routes::v1::response_schemas::CraftingSystemsPageResponse),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn crafting_systems(
    State(state): State<AppState>,
    ApiQuery(query, filters): ApiQuery<CraftingSystemFilters>,
) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let filters = filters.validated(db)?;
            let mut where_clause = WhereClause::default();
            if let Some(has_items) = filters.has_items {
                where_clause.add_bound_condition("listed.has_items = ?", has_items);
            }
            let mut page = paged_rows_with_filter(
                db,
                ValueVocabulary::CraftingSystems.select_sql(),
                &query,
                "listed.name",
                ValueVocabulary::CraftingSystems.default_order(),
                CRAFTING_SYSTEMS_SORT_FIELDS,
                where_clause,
            )?;
            attach_augment_families(db, &mut page.rows)?;
            for row in &mut page.rows {
                convert_to_booleans(row, &["has_items"]);
            }
            Ok(Json(page.into_json("crafting_systems")))
        })
        .await
}

#[utoipa::path(
    get,
    path = "/v1/crafting-systems/{slug}",
    tag = "crafting",
    summary = "Get a crafting system",
    description = "Returns crafting stations, unlocks, steps, choices and effective recipe costs, with a non-legacy item count.",
    params(("slug" = String, Path, description = "Current entity slug or numeric id from the corresponding list.")),
    responses(
        (status = 200, description = "The system with its ingredients and recipes", body = crate::routes::v1::response_schemas::CraftingSystemsDetailResponse),
        (status = 308, description = "Renamed slug; follow Location to its current slug."), (status = 404, description = "No crafting system has this slug or numeric id", body = crate::error::ErrorBody)
    )
)]
async fn crafting_system_detail(
    State(state): State<AppState>,
    EntityPath(id): EntityPath,
) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let ingredient_uses = INGREDIENT_USES.as_str();
            let mut crafting_system =
                json_row(db, &format!("SELECT * FROM ({}) listed WHERE listed.id = ?1", ValueVocabulary::CraftingSystems.select_sql()), [id])?;
            attach_augment_families(db, std::slice::from_mut(&mut crafting_system))?;
            convert_to_booleans(&mut crafting_system, &["has_items"]);
            crafting_system["ingredients"] = Value::Array(json_rows(
                db,
                &format!("{ingredient_uses} SELECT id, name, tier, bind, source FROM crafting_ingredients WHERE id IN (SELECT ingredient_id FROM ingredient_uses WHERE system_id = ?1) ORDER BY name"),
                [id],
            )?);
            let recipe_ids = json_rows(db,"SELECT id FROM crafting_recipes WHERE system_id=?1 ORDER BY sort_order",[id])?.iter().filter_map(|row| row["id"].as_i64()).collect::<Vec<_>>();
            let mut recipes = recipes_by_id(db,&recipe_ids)?;
            crafting_system["recipes"] = Value::Array(recipe_ids.into_iter().filter_map(|id| recipes.remove(&id)).collect());
            let step_ids = json_rows(db,"SELECT id FROM crafting_steps WHERE system_id=?1 ORDER BY sort_order",[id])?.iter().filter_map(|row| row["id"].as_i64()).collect::<Vec<_>>();
            let mut steps = steps_by_id(db,&step_ids)?;
            crafting_system["steps"] = Value::Array(step_ids.into_iter().filter_map(|id| steps.remove(&id)).collect());
            crafting_system["unlocks"] = Value::Array(unlocks_for(db,"system_id",&[id])?.remove(&id).unwrap_or_default());
            let mut stations = json_rows(db,"SELECT s.id,s.slug,s.name,s.variant,s.kind,s.place,q.name AS quest,q.id AS quest_id,q.slug AS quest_slug,link.note FROM crafting_system_stations link JOIN crafting_stations s ON s.id=link.station_id LEFT JOIN quests q ON q.id=s.quest_id WHERE link.system_id=?1 ORDER BY link.sort_order",[id])?;
            let station_ids = stations.iter().filter_map(|station| station["id"].as_i64()).collect::<Vec<_>>();
            let mut station_unlocks = unlocks_for(db,"station_id",&station_ids)?;
            for station in &mut stations {
                station["unlocks"] = Value::Array(station_unlocks.remove(&station["id"].as_i64().unwrap()).unwrap_or_default());
            }
            crafting_system["stations"] = Value::Array(stations);
            Ok(Json(crafting_system))
        })
        .await
}
