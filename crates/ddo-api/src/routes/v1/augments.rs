use super::crafting::crafting_recipes_yielding;
use super::quests::{adventure_packs_dropping_via, quests_dropping_via, sources_via};
use crate::db::{bonuses_via, convert_to_booleans, json_row, json_rows, modifiers_for, paged_query, WhereClause};
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

fn attach_child_collections(db: &rusqlite::Connection, augment: &mut Value) -> Result<(), ApiError> {
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
    augment["bonuses"] = Value::Array(bonuses_via(db, "augment_enchantments", "augment_id", augment_id)?);
    augment["crafting"] = Value::Array(crafting_recipes_yielding(db, augment_id)?);
    Ok(())
}

const AUGMENT_COLUMNS: &str =
    "a.id, a.name, a.family, a.description, a.effect_description, a.min_level, a.icon, a.choose_level, a.levels,
                       a.level_values, a.level_values2, a.dual_values, a.enter_value, a.suppress_set_bonus, a.set_bonus,
                       a.adds_augment, a.grants_augment, a.weapon_class";

const AUGMENTS_SORT_FIELDS: &[(&str, &str)] =
    &[("name", "a.name"), ("id", "a.id"), ("min_level", "a.min_level"), ("family", "a.family")];

declare_list_parameters!(AugmentsParameters, AUGMENTS_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/augments",
    tag = "augments",
    summary = "List augments",
    description = "One page of augments ordered by name then minimum level, each with the socket labels it fits \
                   (`slots`), its `bonuses`, the crafting fields DDOBuilderV2 records (level tables, dual values, \
                   set bonus, granted augments), and `crafting`: the wiki crafting recipes that yield it, each with \
                   its `system` name, `tier`, the wiki's `option` label and its `cost` as \
                   `{ ingredient, tier, quantity }` entries (see /v1/crafting-systems); empty when no recipe read \
                   from the wiki yields it. The augments DDOBuilderV2 lacks are read whole from ddowiki and listed \
                   like his. Filter by `slot` to get the candidates for \
                   one socket on an item.",
    params(
        AugmentsParameters,
        ("slot" = Option<String>, Query, description = "Socket label as /v1/augment-slot-types lists it, e.g. `red` or `lamordia: melancholic (accessory)`; case-insensitive"),
        ("family" = Option<String>, Query, description = "Augment family, e.g. `standard`, `lamordia`, `dino`, `crafting`"),
        ("max_level" = Option<i64>, Query, description = "Only augments usable at this character level or lower; augments with no minimum level always pass"),
    ),
    responses(
        (status = 200, description = "`total`, `limit`, `offset` and the `augments` page", body = Value),
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
            for augment in &mut page.rows {
                convert_to_booleans(augment, AUGMENT_FLAG_COLUMNS);
                attach_child_collections(db, augment)?;
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
    description = "One augment as the list returns it, including its `crafting` recipes, plus `quests`, the quests it drops in, read from the `Drops in` text of Maetrim's description and ddowiki's rare drops, once per loot type, each \
                   with the fields item detail `quests` carry (loot type, raid flag, `is_rare`, `chest`, difficulties, \
                   pack, patron; empty when neither names a quest), `adventure_packs` and `sources` as item detail \
                   carries them (packs reached through any source kind, once per distinct combination of pack, \
                   `loot_type`, `is_rare` and `chest`, preserving each source's rarity and chest and collapsing \
                   identical combinations, sorted by pack name, loot type, rarity and chest; and every \
                   source in one array sorted by kind and name), and the raw `modifiers` its bonuses were \
                   derived from, including the conditional and dice-valued ones that do not reduce to a bonus.",
    params(("id" = i64, Path, description = "The augment's numeric id from the list endpoint")), responses((status = 200, description = "The augment with its child collections", body = Value), (status = 404, description = "No augment has this id", body = crate::error::ErrorBody))
)]
async fn augment_detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut augment = json_row(db, &format!("SELECT {AUGMENT_COLUMNS} FROM augments a WHERE a.id = ?1"), [id])?;
            convert_to_booleans(&mut augment, AUGMENT_FLAG_COLUMNS);
            attach_child_collections(db, &mut augment)?;
            augment["quests"] = Value::Array(quests_dropping_via(db, "augment_id", id)?);
            augment["adventure_packs"] = Value::Array(adventure_packs_dropping_via(db, "augment_id", id)?);
            augment["sources"] = Value::Array(sources_via(db, "augment_id", id)?);
            augment["modifiers"] = Value::Array(modifiers_for(db, "augment", id)?);
            Ok(Json(augment))
        })
        .await
}
