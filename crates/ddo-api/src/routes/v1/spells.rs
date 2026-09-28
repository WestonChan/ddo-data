use crate::db::{booleanize, count, json_row, json_rows, like_pattern, modifiers_for, page, stances_for, Filters};
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(list)).routes(routes!(detail)).routes(routes!(clickies))
}

#[derive(Deserialize)]
pub struct SpellFilter {
    pub q: Option<String>,
    pub school: Option<String>,
    pub class: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

const COLUMNS: &str = "s.id, s.name, s.description, s.icon, s.schools, s.max_caster_level, s.cost, s.metamagics";

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
        ("limit" = Option<i64>, Query, description = "Page size, 1 to 10000; defaults to 100"),
        ("offset" = Option<i64>, Query, description = "Rows to skip before the first returned row; defaults to 0")
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `spells` page", body = Value))
)]
async fn list(State(state): State<AppState>, Query(f): Query<SpellFilter>) -> Result<Json<Value>, ApiError> {
    let (limit, offset) = page(f.limit, f.offset);
    state
        .query(move |conn| {
            let mut f_ = Filters::default();
            if let Some(q) = f.q.as_deref().filter(|q| !q.trim().is_empty()) {
                f_.bind("s.name LIKE ? ESCAPE '\\'", like_pattern(q));
            }
            if let Some(school) = &f.school {
                f_.bind("EXISTS (SELECT 1 FROM json_each(s.schools) j WHERE j.value = ?)", school.clone());
            }
            if let Some(class) = &f.class {
                f_.bind("EXISTS (SELECT 1 FROM class_spells cs JOIN classes c ON c.id = cs.class_id WHERE cs.spell_id = s.id AND c.name = ?)",
                    class.clone(),
                );
            }
            let where_sql = f_.where_sql();
            let total = count(conn, &format!("SELECT COUNT(*) FROM spells s {where_sql}"), f_.params())?;
            let sql = format!("SELECT {COLUMNS} FROM spells s {where_sql} ORDER BY s.name LIMIT {limit} OFFSET {offset}");
            let rows = json_rows(conn, &sql, f_.params())?;
            Ok(Json(json!({ "total": total, "limit": limit, "offset": offset, "spells": rows })))
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
async fn detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .query(move |conn| {
            let mut spell = json_row(conn, &format!("SELECT {COLUMNS} FROM spells s WHERE s.id = ?1"), [id])?;
            spell["damage"] = Value::Array(json_rows(
                conn,
                "SELECT base_dice_number, base_dice_sides, base_dice_bonus, per_caster_levels, bonus_dice_number, bonus_dice_sides, bonus_dice_bonus, damage, spell_power
                   FROM spell_damage WHERE spell_id = ?1 ORDER BY sort_order",
                [id],
            )?);
            let mut dcs = json_rows(
                conn,
                "SELECT dc_type, dc_versus, schools, casting_stat_mod, amount, mod_abilities FROM spell_dcs WHERE spell_id = ?1 ORDER BY sort_order",
                [id],
            )?;
            for dc in &mut dcs {
                booleanize(dc, &["casting_stat_mod"]);
            }
            spell["dcs"] = Value::Array(dcs);
            spell["classes"] = Value::Array(json_rows(
                conn,
                "SELECT c.id AS class_id, c.name AS class, cs.spell_level, cs.cost, cs.max_caster_level FROM class_spells cs JOIN classes c ON c.id = cs.class_id
                  WHERE cs.spell_id = ?1 ORDER BY c.name",
                [id],
            )?);
            spell["stances"] = Value::Array(stances_for(conn, "spell", id)?);
            spell["modifiers"] = Value::Array(modifiers_for(conn, "spell", id)?);
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
        .query(|conn| {
            let mut rows =
                json_rows(conn, "SELECT id, name, description, icon, school FROM clickies ORDER BY name", [])?;
            for c in &mut rows {
                let id = c["id"].as_i64().unwrap_or(0);
                c["modifiers"] = Value::Array(modifiers_for(conn, "clickie", id)?);
            }
            Ok(Json(rows))
        })
        .await
}
