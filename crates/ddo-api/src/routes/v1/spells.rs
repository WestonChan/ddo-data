use crate::db::{
    convert_to_booleans, json_row, json_rows, modifiers_for, paged_query, paged_rows, stances_for, WhereClause,
};
use crate::error::ApiError;
use crate::query::{declare_list_parameters, declare_query_parameters, ApiQuery};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(spells)).routes(routes!(spell_detail)).routes(routes!(clickies))
}

declare_query_parameters! {
    pub(super) struct SpellFilters {
        pub school: Option<String>,
        pub class: Option<String>,
    }
}

const SPELL_COLUMNS: &str = "s.id, s.name, s.description, s.icon, s.schools, s.max_caster_level, s.cost, s.metamagics";

const SPELLS_SORT_FIELDS: &[(&str, &str)] = &[
    ("name", "s.name"),
    ("id", "s.id"),
    ("school", "s.schools"),
    ("cost", "cost"),
    ("description", "description"),
    ("icon", "icon"),
    ("max_caster_level", "max_caster_level"),
    (
        "class",
        "(SELECT MIN(c.name) FROM class_spells cs JOIN classes c ON c.id = cs.class_id WHERE cs.spell_id = s.id)",
    ),
];

declare_list_parameters!(SpellsParameters, SPELLS_SORT_FIELDS, "`school` orders the stored schools list.");

#[utoipa::path(
    get,
    path = "/v1/spells",
    tag = "spells",
    summary = "List spells",
    description = "Lists spells with schools, spell-point cost and maximum caster level.",
    params(
        SpellsParameters,
        ("school" = Option<String>, Query, description = "Spell school, e.g. `Evocation`; matches spells listing it among their schools"),
        ("class" = Option<String>, Query, description = "Class name as /v1/classes lists it; keeps spells on that class's list"),
    ),
    responses(
        (status = 200, description = "`total`, `limit`, `offset` and the `spells` page", body = crate::routes::v1::response_schemas::SpellsPageResponse),
        (status = 400, description = "Unknown or malformed query parameter", body = crate::error::ErrorBody)
    )
)]
async fn spells(
    State(state): State<AppState>,
    ApiQuery(query, filters): ApiQuery<SpellFilters>,
) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut where_clause = WhereClause::default();
            where_clause.add_name_search(query.q.as_deref(), "s.name");
            if let Some(school) = &filters.school {
                where_clause.add_bound_condition(
                    "EXISTS (SELECT 1 FROM json_each(s.schools) j WHERE j.value = ?)",
                    school.clone(),
                );
            }
            if let Some(class) = &filters.class {
                where_clause.add_bound_condition("EXISTS (SELECT 1 FROM class_spells cs JOIN classes c ON c.id = cs.class_id WHERE cs.spell_id = s.id AND c.name = ?)",
                    class.clone(),
                );
            }
            let page = paged_query(
                db,
                SPELL_COLUMNS,
                "spells s",
                &query,
                "s.name",
                SPELLS_SORT_FIELDS,
                &where_clause,
            )?;
            Ok(Json(page.into_json("spells")))
        })
        .await
}

#[utoipa::path(
    get,
    path = "/v1/spells/{id}",
    tag = "spells",
    summary = "Get a spell",
    description = "Returns a spell with damage, saves and the classes that learn it.",
    params(("id" = i64, Path, description = "The spell's numeric id from the list endpoint")), responses((status = 200, description = "The spell with its child collections", body = crate::routes::v1::response_schemas::SpellsDetailResponse), (status = 404, description = "No spell has this id", body = crate::error::ErrorBody))
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

const CLICKIES_SORT_FIELDS: &[(&str, &str)] = &[
    ("name", "listed.name"),
    ("id", "listed.id"),
    ("school", "listed.school"),
    ("description", "description"),
    ("icon", "icon"),
];

declare_list_parameters!(ClickiesParameters, CLICKIES_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/clickies",
    tag = "spells",
    summary = "List clickies",
    description = "Lists item-granted clickies with spell schools and modifiers.",
    params(
        ClickiesParameters,
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `clickies` page", body = crate::routes::v1::response_schemas::ClickiesPageResponse),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn clickies(State(state): State<AppState>, ApiQuery(query, _): ApiQuery) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let select_sql = "SELECT id, name, description, icon, school FROM clickies";
            let mut page = paged_rows(db, select_sql, &query, "listed.name", "listed.name", CLICKIES_SORT_FIELDS)?;
            for row in &mut page.rows {
                let clickie_id = row["id"].as_i64().unwrap_or(0);
                row["modifiers"] = Value::Array(modifiers_for(db, "clickie", clickie_id)?);
            }
            Ok(Json(page.into_json("clickies")))
        })
        .await
}
