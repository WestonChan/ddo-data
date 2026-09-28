use crate::db::{booleanize, json_rows};
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::State;
use axum::Json;
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(stats))
        .routes(routes!(bonus_types))
        .routes(routes!(equipment_slots))
        .routes(routes!(weapon_types))
        .routes(routes!(damage_types))
        .routes(routes!(augment_slot_types))
        .routes(routes!(adventure_packs))
        .routes(routes!(patrons))
        .routes(routes!(quests))
}

async fn table(
    state: AppState,
    sql: &'static str,
    flags: &'static [&'static str],
) -> Result<Json<Vec<Value>>, ApiError> {
    let rows = state
        .query(move |conn| {
            let mut rows = json_rows(conn, sql, [])?;
            for row in &mut rows {
                booleanize(row, flags);
            }
            Ok(rows)
        })
        .await?;
    Ok(Json(rows))
}

#[utoipa::path(
    get,
    path = "/v1/stats",
    tag = "lookups",
    summary = "List stats",
    description = "Every stat a bonus can apply to, with its category (ability, skill, save, spell power and so on). \
                   Bonus rows everywhere else name stats by these names, and /v1/items accepts them in `stat`.",
    responses((status = 200, description = "The whole table", body = Vec<Value>))
)]
async fn stats(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    table(state, "SELECT id, name, category FROM stats ORDER BY id", &[]).await
}

#[utoipa::path(
    get,
    path = "/v1/bonus-types",
    tag = "lookups",
    summary = "List bonus types",
    description = "Every bonus type (Enhancement, Insightful, Quality, ...) and whether two bonuses of that type \
                   stack with each other (`stacks_with_self`). Ids 1 to 33 are the site's own vocabulary; the rest \
                   come from DDOBuilderV2.",
    responses((status = 200, description = "The whole table", body = Vec<Value>))
)]
async fn bonus_types(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    table(state, "SELECT id, name, stacks_with_self FROM bonus_types ORDER BY id", &["stacks_with_self"]).await
}

#[utoipa::path(
    get,
    path = "/v1/equipment-slots",
    tag = "lookups",
    summary = "List equipment slots",
    description = "The equipment slots an item can occupy, in display order, with a category (weapon, armor, \
                   accessory). /v1/items accepts these names in `slot`.",
    responses((status = 200, description = "The whole table", body = Vec<Value>))
)]
async fn equipment_slots(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    table(state, "SELECT id, name, sort_order, category FROM equipment_slots ORDER BY sort_order", &[]).await
}

#[utoipa::path(
    get,
    path = "/v1/weapon-types",
    tag = "lookups",
    summary = "List weapon types",
    description = "Every weapon and shield type with the proficiency it needs and whether it is a shield. Item \
                   `weapon` blocks name their type from this list.",
    responses((status = 200, description = "The whole table", body = Vec<Value>))
)]
async fn weapon_types(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    table(
        state,
        "SELECT wt.id, wt.name, p.name AS proficiency, wt.is_shield FROM weapon_types wt
           LEFT JOIN weapon_proficiencies p ON p.id = wt.proficiency_id ORDER BY wt.id",
        &["is_shield"],
    )
    .await
}

#[utoipa::path(
    get,
    path = "/v1/damage-types",
    tag = "lookups",
    summary = "List damage types",
    description = "Every damage type (physical, elemental, alignment, special) with its category. Spell damage \
                   lines and DR bypass entries use these names.",
    responses((status = 200, description = "The whole table", body = Vec<Value>))
)]
async fn damage_types(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    table(state, "SELECT id, name, category FROM damage_types ORDER BY id", &[]).await
}

#[utoipa::path(
    get,
    path = "/v1/augment-slot-types",
    tag = "lookups",
    summary = "List augment slot types",
    description = "Every socket an item can carry: gem colours (`red`, `colorless`, `sun`, ...) and crafting-family \
                   sockets (`lamordia: melancholic (accessory)`, `isle of dread: set bonus`, ...). `family` says which \
                   kind it is; `label` is what /v1/augments accepts in `slot`.",
    responses((status = 200, description = "The whole table", body = Vec<Value>))
)]
async fn augment_slot_types(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    table(state, "SELECT id, label, family, variant, qualifier FROM augment_slot_types ORDER BY family, label", &[])
        .await
}

#[utoipa::path(
    get,
    path = "/v1/adventure-packs",
    tag = "lookups",
    summary = "List adventure packs",
    description = "Every adventure pack and expansion by name with whether it is free to play. /v1/items accepts \
                   these names in `pack`.",
    responses((status = 200, description = "The whole table", body = Vec<Value>))
)]
async fn adventure_packs(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    table(state, "SELECT id, name, is_free_to_play FROM adventure_packs ORDER BY name", &["is_free_to_play"]).await
}

#[utoipa::path(
    get,
    path = "/v1/patrons",
    tag = "lookups",
    summary = "List patrons",
    description = "The favor patrons (The Coin Lords, House Kundarak, ...) that quests belong to.",
    responses((status = 200, description = "The whole table", body = Vec<Value>))
)]
async fn patrons(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    table(state, "SELECT id, name FROM patrons ORDER BY name", &[]).await
}

#[utoipa::path(
    get,
    path = "/v1/quests",
    tag = "lookups",
    summary = "List quests",
    description = "Every quest and adventure area with its pack, patron, heroic and epic levels, favor, and \
                   whether it is a raid. Item detail responses reference these in `quests`.",
    responses((status = 200, description = "The whole table", body = Vec<Value>))
)]
async fn quests(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    table(
        state,
        "SELECT q.id, q.name, p.name AS pack, pt.name AS patron, q.level, q.epic_level, q.favor, q.is_raid
           FROM quests q LEFT JOIN adventure_packs p ON p.id = q.pack_id LEFT JOIN patrons pt ON pt.id = q.patron_id
          ORDER BY q.name",
        &["is_raid"],
    )
    .await
}
