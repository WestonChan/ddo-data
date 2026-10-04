use crate::db::paged_rows;
use crate::db::{
    bonuses_via, convert_to_booleans, json_row, json_rows, modifiers_for, paged_table_json, TableListSource,
};
use crate::error::ApiError;
use crate::query::{declare_list_parameters, ApiQuery};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(sets))
        .routes(routes!(set_detail))
        .routes(routes!(filigrees))
        .routes(routes!(sentient_gems))
}

const SET_LIST_SELECT: &str = "SELECT s.id, s.name, s.icon, s.is_filigree_set,
    (SELECT COUNT(*) FROM set_bonus_items i WHERE i.set_id = s.id) AS item_count,
    (SELECT COUNT(*) FROM set_bonus_augments a WHERE a.set_id = s.id) AS augment_count,
    (SELECT COUNT(*) FROM set_bonus_tiers t WHERE t.set_id = s.id) AS tier_count
    FROM set_bonuses s";

const FILIGREE_SELECT: &str = "SELECT f.id, f.name, f.description, f.icon, f.menu, f.set_id, s.name AS set_name
    FROM filigrees f LEFT JOIN set_bonuses s ON s.id = f.set_id";

const SETS_SORT_FIELDS: &[(&str, &str)] = &[
    ("name", "listed.name"),
    ("id", "listed.id"),
    ("item_count", "listed.item_count"),
    ("augment_count", "listed.augment_count"),
    ("tier_count", "listed.tier_count"),
];

declare_list_parameters!(SetsParameters, SETS_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/sets",
    tag = "sets",
    summary = "List sets",
    description = "Every set bonus ordered by name with its icon, whether it is a filigree set rather than a gear \
                   set, and how many items, augments and tiers it has (`item_count`, `augment_count`, `tier_count`). \
                   Tiers, items, augments and filigrees are on the detail endpoint.",
    params(
        SetsParameters,
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `sets` page", body = Value),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn sets(State(state): State<AppState>, ApiQuery(query, _): ApiQuery) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut page = paged_rows(db, SET_LIST_SELECT, &query, "listed.name", "listed.name", SETS_SORT_FIELDS)?;
            for set in &mut page.rows {
                convert_to_booleans(set, &["is_filigree_set"]);
            }
            Ok(Json(page.into_json("sets")))
        })
        .await
}

#[utoipa::path(
    get,
    path = "/v1/sets/{id}",
    tag = "sets",
    summary = "Get a set",
    description = "One set with its `tiers` (how many pieces equipped unlock which `bonuses`, the stat, bonus type and \
                   value derived from each tier's plain stat effects as an item's are, and which raw `modifiers`), the `items` \
                   that count towards it with slot and minimum level, the `augments` whose slotting grants it (id, \
                   name, minimum level; crafting-system and named augments), and for filigree sets the `filigrees`.",
    params(("id" = i64, Path, description = "The set's numeric id from the list endpoint")), responses((status = 200, description = "The set with its child collections", body = Value), (status = 404, description = "No set has this id", body = crate::error::ErrorBody))
)]
async fn set_detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut set = json_row(db, "SELECT id, name, icon, is_filigree_set FROM set_bonuses WHERE id = ?1", [id])?;
            convert_to_booleans(&mut set, &["is_filigree_set"]);
            let mut tiers = json_rows(
                db,
                "SELECT id, equipped_count FROM set_bonus_tiers WHERE set_id = ?1 ORDER BY equipped_count",
                [id],
            )?;
            let tier_lines = json_rows(
                db,
                "SELECT l.tier_id, e.text_template,
                        COALESCE(l.value, e.default_value) AS value,
                        COALESCE(l.value2, e.default_value2) AS value2,
                        bt.name AS bonus_type
                 FROM set_bonus_tier_enchantments l
                 JOIN set_bonus_tiers t ON t.id = l.tier_id
                 JOIN enchantments e ON e.id = l.enchantment_id
                 LEFT JOIN bonus_types bt ON bt.id = l.bonus_type_id
                 WHERE t.set_id = ?1 ORDER BY t.equipped_count, l.sort_order",
                [id],
            )?;
            let mut lines_by_tier: std::collections::HashMap<i64, Vec<String>> = std::collections::HashMap::new();
            for line in tier_lines {
                let tier_id = line["tier_id"].as_i64().unwrap_or(0);
                lines_by_tier.entry(tier_id).or_default().push(render_set_tier_line(&line));
            }
            for tier in &mut tiers {
                let tier_id = tier["id"].as_i64().unwrap_or(0);
                tier["description"] = lines_by_tier
                    .remove(&tier_id)
                    .filter(|lines| !lines.is_empty())
                    .map_or(Value::Null, |lines| Value::String(lines.join("\n")));
                tier["bonuses"] = Value::Array(bonuses_via(db, "set_bonus_tier_enchantments", "tier_id", tier_id)?);
                tier["modifiers"] = Value::Array(modifiers_for(db, "set_bonus_tier", tier_id)?);
            }
            set["tiers"] = Value::Array(tiers);
            set["items"] = Value::Array(json_rows(
                db,
                "SELECT i.id, i.name, es.name AS slot, i.minimum_level FROM set_bonus_items sbi JOIN items i ON i.id = sbi.item_id
                   JOIN equipment_slots es ON es.id = i.slot_id WHERE sbi.set_id = ?1 ORDER BY i.name",
                [id],
            )?);
            set["augments"] = Value::Array(json_rows(
                db,
                "SELECT a.id, a.name, a.min_level FROM set_bonus_augments sba JOIN augments a ON a.id = sba.augment_id
                  WHERE sba.set_id = ?1 ORDER BY a.name, a.id",
                [id],
            )?);
            set["filigrees"] = Value::Array(filigrees_matching_set(db, Some(id))?);
            Ok(Json(set))
        })
        .await
}

fn render_set_tier_line(line: &Value) -> String {
    let signed_value = line["value"].as_i64().map(|amount| format!("{amount:+}")).unwrap_or_default();
    let signed_value2 = line["value2"].as_i64().map(|amount| format!("{amount:+}")).unwrap_or_default();
    line["text_template"]
        .as_str()
        .unwrap_or("")
        .replace("+{1}", "{1}")
        .replace("+{2}", "{2}")
        .replace("{1}", &signed_value)
        .replace("{2}", &signed_value2)
        .replace("%b1", line["bonus_type"].as_str().unwrap_or(""))
}

fn filigrees_matching_set(db: &rusqlite::Connection, set_id: Option<i64>) -> Result<Vec<Value>, ApiError> {
    let sql = format!("{FILIGREE_SELECT} WHERE (?1 IS NULL OR f.set_id = ?1) ORDER BY f.name");
    let mut filigrees = json_rows(db, &sql, [set_id])?;
    for filigree in &mut filigrees {
        let filigree_id = filigree["id"].as_i64().unwrap_or(0);
        filigree["modifiers"] = Value::Array(modifiers_for(db, "filigree", filigree_id)?);
    }
    Ok(filigrees)
}

const FILIGREES_SORT_FIELDS: &[(&str, &str)] =
    &[("name", "listed.name"), ("id", "listed.id"), ("menu", "listed.menu"), ("set_name", "listed.set_name")];

declare_list_parameters!(FiligreesParameters, FILIGREES_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/filigrees",
    tag = "sets",
    summary = "List filigrees",
    description = "Every sentient-weapon filigree ordered by name with its description, icon, crafting `menu`, the \
                   set it belongs to, and the raw `modifiers` it applies on its own.",
    params(
        FiligreesParameters,
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `filigrees` page", body = Value),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn filigrees(State(state): State<AppState>, ApiQuery(query, _): ApiQuery) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut page =
                paged_rows(db, FILIGREE_SELECT, &query, "listed.name", "listed.name", FILIGREES_SORT_FIELDS)?;
            for filigree in &mut page.rows {
                let filigree_id = filigree["id"].as_i64().unwrap_or(0);
                filigree["modifiers"] = Value::Array(modifiers_for(db, "filigree", filigree_id)?);
            }
            Ok(Json(page.into_json("filigrees")))
        })
        .await
}

const SENTIENT_GEMS_SORT_FIELDS: &[(&str, &str)] = &[("name", "listed.name"), ("id", "listed.id")];

declare_list_parameters!(SentientGemsParameters, SENTIENT_GEMS_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/sentient-gems",
    tag = "sets",
    summary = "List sentient gems",
    description = "Every sentient jewel a sentient weapon or accessory can carry, ordered by name, with its icon \
                   (served from the `sentient-gems` icon family) and description, which upstream uses for the voice \
                   actor credit. Filigrees slot into the jewel; they are listed by /v1/filigrees.",
    params(
        SentientGemsParameters,
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `sentient_gems` page", body = Value),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn sentient_gems(State(state): State<AppState>, ApiQuery(query, _): ApiQuery) -> Result<Json<Value>, ApiError> {
    paged_table_json(
        state,
        query,
        TableListSource {
            select_sql: "SELECT id, name, icon, description FROM sentient_gems",
            rows_key: "sentient_gems",
            name_column: "listed.name",
            default_order: "listed.name",
            sortable_fields: SENTIENT_GEMS_SORT_FIELDS,
            flag_columns: &[],
        },
    )
    .await
}
