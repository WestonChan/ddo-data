use crate::db::{
    clamped_page, convert_to_booleans, json_row, json_rows, modifiers_for, row_count, stances_for,
    substring_like_pattern, WhereClause,
};
use crate::error::ApiError;
use crate::query::ApiQuery;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(spells)).routes(routes!(spell_detail)).routes(routes!(clickies))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SpellListQuery {
    pub q: Option<String>,
    pub school: Option<String>,
    pub class: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

const SPELL_COLUMNS: &str = "s.id, s.name, s.description, s.icon, s.schools, s.max_caster_level, s.cost, s.metamagics";

#[utoipa::path(
    get,
    path = "/v1/spells",
    tag = "spells",
    summary = "List spells",
    description = "One page of spells ordered by name with description, icon, `schools`, maximum caster level, \
                   spell point `cost` and the `metamagics` that apply. Damage, saving throws and which classes get \
                   the spell at which level are on the detail endpoint.",
    params(
        ("q" = Option<String>, Query, description = "Case-insensitive substring of the spell name"),
        ("school" = Option<String>, Query, description = "Spell school, e.g. `Evocation`; matches spells listing it among their schools"),
        ("class" = Option<String>, Query, description = "Class name as /v1/classes lists it; keeps spells on that class's list"),
        ("limit" = Option<i64>, Query, description = "Page size, 1 to 10000; defaults to 100; out-of-range values are clamped rather than rejected"),
        ("offset" = Option<i64>, Query, description = "Rows to skip before the first returned row; defaults to 0; negative values are clamped to 0 rather than rejected")
    ),
    responses(
        (status = 200, description = "`total`, `limit`, `offset` and the `spells` page", body = Value),
        (status = 400, description = "Unknown or malformed query parameter", body = crate::error::ErrorBody)
    )
)]
async fn spells(
    State(state): State<AppState>,
    ApiQuery(query): ApiQuery<SpellListQuery>,
) -> Result<Json<Value>, ApiError> {
    let (limit, offset) = clamped_page(query.limit, query.offset);
    state
        .read_db(move |db| {
            let mut where_clause = WhereClause::default();
            if let Some(search_text) = query.q.as_deref().filter(|q| !q.trim().is_empty()) {
                where_clause.add_bound_condition("s.name LIKE ? ESCAPE '\\'", substring_like_pattern(search_text));
            }
            if let Some(school) = &query.school {
                where_clause.add_bound_condition(
                    "EXISTS (SELECT 1 FROM json_each(s.schools) j WHERE j.value = ?)",
                    school.clone(),
                );
            }
            if let Some(class) = &query.class {
                where_clause.add_bound_condition("EXISTS (SELECT 1 FROM class_spells cs JOIN classes c ON c.id = cs.class_id WHERE cs.spell_id = s.id AND c.name = ?)",
                    class.clone(),
                );
            }
            let where_sql = where_clause.to_sql();
            let total = row_count(db, &format!("SELECT COUNT(*) FROM spells s {where_sql}"), where_clause.params())?;
            let page_sql = format!("SELECT {SPELL_COLUMNS} FROM spells s {where_sql} ORDER BY s.name LIMIT {limit} OFFSET {offset}");
            let spells = json_rows(db, &page_sql, where_clause.params())?;
            Ok(Json(json!({ "total": total, "limit": limit, "offset": offset, "spells": spells })))
        })
        .await
}

#[utoipa::path(
    get,
    path = "/v1/spells/{id}",
    tag = "spells",
    summary = "Get a spell",
    description = "One spell with its `damage` lines (base and per-caster-level dice, damage type, spell power), \
                   `dcs` (save type, what it is versus, which stat sets it), the `classes` that cast it with spell \
                   level and cost, `stances` it grants, and raw `modifiers`.",
    params(("id" = i64, Path, description = "The spell's numeric id from the list endpoint")), responses((status = 200, description = "The spell with its child collections", body = Value), (status = 404, description = "No spell has this id", body = crate::error::ErrorBody))
)]
async fn spell_detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut spell = json_row(db, &format!("SELECT {SPELL_COLUMNS} FROM spells s WHERE s.id = ?1"), [id])?;
            spell["damage"] = Value::Array(json_rows(
                db,
                "SELECT base_dice_number, base_dice_sides, base_dice_bonus, per_caster_levels, bonus_dice_number, bonus_dice_sides, bonus_dice_bonus, damage, spell_power
                   FROM spell_damage WHERE spell_id = ?1 ORDER BY sort_order",
                [id],
            )?);
            let mut dcs = json_rows(
                db,
                "SELECT dc_type, dc_versus, schools, casting_stat_mod, amount, mod_abilities FROM spell_dcs WHERE spell_id = ?1 ORDER BY sort_order",
                [id],
            )?;
            for dc in &mut dcs {
                convert_to_booleans(dc, &["casting_stat_mod"]);
            }
            spell["dcs"] = Value::Array(dcs);
            spell["classes"] = Value::Array(json_rows(
                db,
                "SELECT c.id AS class_id, c.name AS class, cs.spell_level, cs.cost, cs.max_caster_level FROM class_spells cs JOIN classes c ON c.id = cs.class_id
                  WHERE cs.spell_id = ?1 ORDER BY c.name",
                [id],
            )?);
            spell["stances"] = Value::Array(stances_for(db, "spell", id)?);
            spell["modifiers"] = Value::Array(modifiers_for(db, "spell", id)?);
            Ok(Json(spell))
        })
        .await
}

#[utoipa::path(
    get,
    path = "/v1/clickies",
    tag = "spells",
    summary = "List clickies",
    description = "Every clickie (an item-granted spell-like ability) ordered by name, with description, icon, \
                   school and the raw `modifiers` it applies. Items reference these by name in their `clickies`.",
    responses((status = 200, description = "All clickies", body = Vec<Value>))
)]
async fn clickies(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    state
        .read_db(|db| {
            let mut clickies =
                json_rows(db, "SELECT id, name, description, icon, school FROM clickies ORDER BY name", [])?;
            for clickie in &mut clickies {
                let clickie_id = clickie["id"].as_i64().unwrap_or(0);
                clickie["modifiers"] = Value::Array(modifiers_for(db, "clickie", clickie_id)?);
            }
            Ok(Json(clickies))
        })
        .await
}
