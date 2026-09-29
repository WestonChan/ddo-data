use crate::db::{
    attack_for, convert_to_booleans, dcs_for, json_row, json_rows, modifiers_for, requirements_for, stances_for,
};
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(enhancement_trees)).routes(routes!(enhancement_tree_detail))
}

const ENHANCEMENT_TREE_COLUMNS: &str = "t.id, t.name, t.version, t.kind, t.is_legacy, t.icon, t.background,
                            (SELECT COUNT(*) FROM enhancements e WHERE e.tree_id = t.id) AS enhancement_count";

#[utoipa::path(
    get,
    path = "/v1/enhancement-trees",
    tag = "enhancements",
    summary = "List enhancement trees",
    description = "Every enhancement, destiny and reaper tree ordered by kind then name, with its version, icon, \
                   background art name, whether it is a legacy tree, how many enhancements it holds, and the \
                   `requirements` to access it. The enhancements themselves are on the detail endpoint.",
    responses((status = 200, description = "All enhancement trees", body = Vec<Value>))
)]
async fn enhancement_trees(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    state
        .read_db(|db| {
            let mut enhancement_trees = json_rows(
                db,
                &format!("SELECT {ENHANCEMENT_TREE_COLUMNS} FROM enhancement_trees t ORDER BY t.kind, t.name"),
                [],
            )?;
            for enhancement_tree in &mut enhancement_trees {
                convert_to_booleans(enhancement_tree, &["is_legacy"]);
                let tree_id = enhancement_tree["id"].as_i64().unwrap_or(0);
                enhancement_tree["requirements"] = Value::Array(requirements_for(db, "enhancement_tree", tree_id)?);
            }
            Ok(Json(enhancement_trees))
        })
        .await
}

fn attach_ability_children(db: &rusqlite::Connection, owner_kind: &str, ability: &mut Value) -> Result<(), ApiError> {
    let owner_id = ability["id"].as_i64().unwrap_or(0);
    ability["requirements"] = Value::Array(requirements_for(db, owner_kind, owner_id)?);
    ability["modifiers"] = Value::Array(modifiers_for(db, owner_kind, owner_id)?);
    ability["follow_on_modifiers"] = Value::Array(modifiers_for(db, &format!("{owner_kind}_follow_on"), owner_id)?);
    ability["this_attack_modifiers"] = Value::Array(modifiers_for(db, &format!("{owner_kind}_this_attack"), owner_id)?);
    ability["stances"] = Value::Array(stances_for(db, owner_kind, owner_id)?);
    ability["dcs"] = Value::Array(dcs_for(db, owner_kind, owner_id)?);
    ability["attack"] = attack_for(db, owner_kind, owner_id)?;
    Ok(())
}

#[utoipa::path(
    get,
    path = "/v1/enhancement-trees/{id}",
    tag = "enhancements",
    summary = "Get an enhancement tree",
    description = "One tree with every `enhancement` in grid order (`x`, `y`), each with cost per rank, ranks, \
                   points that must be spent in the tree first, tier-5 and clickie flags, `arrows` to prerequisites, \
                   `exclusions`, and for selector enhancements the `selections`. Enhancements and selections both \
                   carry `requirements`, raw `modifiers`, `stances`, `dcs` and an `attack` (with its \
                   `cooldown_seconds` and `duration_seconds`) when they grant one. The attack's `cooldown_seconds`, \
                   the `duration_seconds` of what it applies afterwards, the bonuses it applies to its own hit as \
                   `this_attack_modifiers` (e.g. `BonusDamagePercent`) and those after-use bonuses as \
                   `follow_on_modifiers` (effect type named for the attack bonus, e.g. `BonusAlacrity`, with \
                   per-rank `amounts`) also sit on the enhancement or selection itself.",
    params(("id" = i64, Path, description = "The tree's numeric id from the list endpoint")), responses((status = 200, description = "The tree with its child collections", body = Value), (status = 404, description = "No tree has this id", body = crate::error::ErrorBody))
)]
async fn enhancement_tree_detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut enhancement_tree = json_row(db, &format!("SELECT {ENHANCEMENT_TREE_COLUMNS} FROM enhancement_trees t WHERE t.id = ?1"), [id])?;
            convert_to_booleans(&mut enhancement_tree, &["is_legacy"]);
            enhancement_tree["requirements"] = Value::Array(requirements_for(db, "enhancement_tree", id)?);
            let mut enhancements = json_rows(
                db,
                "SELECT id, internal_name, name, description, icon, x, y, cost_per_rank, ranks, min_spent, is_tier5, is_clickie, arrows,
                        cooldown_seconds, duration_seconds
                   FROM enhancements WHERE tree_id = ?1 ORDER BY y, x, id",
                [id],
            )?;
            for enhancement in &mut enhancements {
                convert_to_booleans(enhancement, &["is_tier5", "is_clickie"]);
                attach_ability_children(db, "enhancement", enhancement)?;
                let enhancement_id = enhancement["id"].as_i64().unwrap_or(0);
                enhancement["exclusions"] = Value::Array(
                    json_rows(db, "SELECT internal_name FROM enhancement_selector_exclusions WHERE enhancement_id = ?1", [enhancement_id])?
                        .into_iter()
                        .map(|row| row["internal_name"].clone())
                        .collect(),
                );
                let mut selections = json_rows(
                    db,
                    "SELECT id, name, description, icon, cost_per_rank, ranks, min_spent, is_clickie, cooldown_seconds, duration_seconds
                       FROM enhancement_selections
                      WHERE enhancement_id = ?1 ORDER BY sort_order",
                    [enhancement_id],
                )?;
                for selection in &mut selections {
                    convert_to_booleans(selection, &["is_clickie"]);
                    attach_ability_children(db, "enhancement_selection", selection)?;
                }
                enhancement["selections"] = Value::Array(selections);
            }
            enhancement_tree["enhancements"] = Value::Array(enhancements);
            Ok(Json(enhancement_tree))
        })
        .await
}
