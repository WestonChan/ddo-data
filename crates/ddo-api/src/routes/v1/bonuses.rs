use crate::db::{
    clamped_page, effects_via_with_link_order, json_row, json_rows, paged_query, paged_table_json, ListPage,
    TableListSource, WhereClause,
};
use crate::error::ApiError;
use crate::query::{declare_list_parameters, declare_query_parameters, ApiFilterQuery, ApiQuery};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::HashMap;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(bonus_types)).routes(routes!(effects)).routes(routes!(effect_detail))
}

const BONUS_TYPES_SORT_FIELDS: &[(&str, &str)] =
    &[("name", "listed.name"), ("id", "listed.id"), ("stacks_with_self", "listed.stacks_with_self")];

declare_list_parameters!(BonusTypesParameters, BONUS_TYPES_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/bonus-types",
    tag = "bonuses",
    summary = "List bonus types",
    description = "Lists bonus types and whether each type stacks with itself.",
    params(BonusTypesParameters),
    responses((status = 200, description = "Paged bonus types", body = crate::routes::v1::response_schemas::BonusTypesPageResponse),
        (status = 400, description = "Invalid query", body = crate::error::ErrorBody))
)]
async fn bonus_types(State(state): State<AppState>, ApiQuery(query, _): ApiQuery) -> Result<Json<Value>, ApiError> {
    paged_table_json(
        state,
        query,
        TableListSource {
            select_sql: "SELECT id, name, stacks_with_self FROM bonus_types",
            rows_key: "bonus_types",
            name_column: "listed.name",
            default_order: "listed.id",
            sortable_fields: BONUS_TYPES_SORT_FIELDS,
            flag_columns: &["stacks_with_self"],
        },
    )
    .await
}

declare_query_parameters! {
    pub(super) struct EffectFilters {
        pub kind: Option<String>,
    }
}

const EFFECT_KINDS: [&str; 3] = ["stat", "effect", "group"];

const EFFECT_VOCABULARY_SQL: &str = "
    SELECT e.id, e.name, CASE WHEN e.is_stat = 1 THEN 'stat' WHEN e.is_group = 1 THEN 'group' ELSE 'effect' END AS kind,
           '/v1/effects/' || e.id AS detail_path,
           c.item_count, c.augment_count, c.set_count
      FROM effects e JOIN effect_vocabulary_counts c ON c.id = e.id
       AND c.kind = CASE WHEN e.is_stat = 1 THEN 'stat' WHEN e.is_group = 1 THEN 'group' ELSE 'effect' END";

const EFFECT_TYPES_SQL: &str = "
    SELECT v.kind, v.id, bt.name, v.item_count
      FROM effect_vocabulary_bonus_types v JOIN bonus_types bt ON bt.id = v.bonus_type_id
     ORDER BY v.kind, v.id, bt.name";

const EFFECTS_SORT_FIELDS: &[(&str, &str)] = &[
    ("name", "listed.name"),
    ("id", "listed.id"),
    ("kind", "listed.kind"),
    ("detail_path", "listed.detail_path"),
    ("item_count", "listed.item_count"),
    ("augment_count", "listed.augment_count"),
    ("set_count", "listed.set_count"),
];

declare_list_parameters!(
    EffectsParameters,
    EFFECTS_SORT_FIELDS,
    "",
    Some("Case-insensitive own-name or granted-stat substring; without sort, exact, prefix and substring own names rank before stat-only matches.")
);

#[utoipa::path(
    get,
    path = "/v1/effects",
    tag = "bonuses",
    summary = "List effects, groups and stats",
    description = "Lists every effect, group and stat with carrier counts, types and a detail path.",
    params(EffectsParameters,
        ("kind" = Option<String>, Query, description = "Keep `effect`, `group` or `stat` rows only.")),
    responses((status = 200, description = "Paged effect vocabulary", body = crate::routes::v1::response_schemas::EffectsPageResponse),
        (status = 400, description = "Invalid kind or query", body = crate::error::ErrorBody))
)]
async fn effects(
    State(state): State<AppState>,
    ApiQuery(query, filters): ApiQuery<EffectFilters>,
) -> Result<Json<Value>, ApiError> {
    if let Some(kind) = &filters.kind {
        if !EFFECT_KINDS.contains(&kind.as_str()) {
            return Err(ApiError::BadRequest(format!("unknown kind {kind:?}")));
        }
    }
    state
        .read_db(move |db| {
            let mut where_clause = WhereClause::default();
            if let Some(kind) = &filters.kind {
                where_clause.add_bound_condition("kind = ?", kind.clone());
            }
            let mut default_order = "listed.name, listed.kind".to_string();
            if let Some(search_text) = query.q.as_deref().map(str::trim).filter(|text| !text.is_empty()) {
                let search_placeholder = where_clause.add_bound_condition(
                    "(instr(lower(listed.name), lower(?)) > 0 OR
                      (listed.kind IN ('effect', 'group') AND EXISTS (
                          SELECT 1 FROM effect_bonuses eb JOIN effects s ON s.id = eb.target_effect_id
                           WHERE eb.effect_id = listed.id AND (instr(lower(s.name), lower(?)) > 0 OR
                             EXISTS (SELECT 1 FROM effect_bonuses member JOIN effects ms ON ms.id = member.target_effect_id
                                      WHERE member.effect_id = s.id AND instr(lower(ms.name), lower(?)) > 0)))))",
                    search_text.to_string(),
                );
                default_order = format!(
                    "CASE WHEN lower(listed.name) = lower({search_placeholder}) THEN 0
                          WHEN instr(lower(listed.name), lower({search_placeholder})) = 1 THEN 1
                          WHEN instr(lower(listed.name), lower({search_placeholder})) > 0 THEN 2
                          ELSE 3 END, listed.name, listed.kind"
                );
            }
            let mut page = paged_query(
                db,
                "*",
                &format!("({EFFECT_VOCABULARY_SQL}) listed"),
                &query,
                &default_order,
                EFFECTS_SORT_FIELDS,
                &where_clause,
            )?;
            let types = json_rows(db, EFFECT_TYPES_SQL, [])?;
            let mut types_by_row: HashMap<(String, i64), Vec<Value>> = HashMap::new();
            for row in types {
                let key = (row["kind"].as_str().unwrap_or("").to_string(), row["id"].as_i64().unwrap_or(0));
                types_by_row
                    .entry(key)
                    .or_default()
                    .push(json!({"name": row["name"], "item_count": row["item_count"]}));
            }
            for row in &mut page.rows {
                let key = (row["kind"].as_str().unwrap_or("").to_string(), row["id"].as_i64().unwrap_or(0));
                row["bonus_types"] = Value::Array(types_by_row.remove(&key).unwrap_or_default());
            }
            Ok(Json(page.into_json("effects")))
        })
        .await
}

declare_query_parameters! {
    pub(super) struct BacklinkPages {
        pub items_limit: Option<i64>,
        pub items_offset: Option<i64>,
        pub augments_limit: Option<i64>,
        pub augments_offset: Option<i64>,
        pub set_tiers_limit: Option<i64>,
        pub set_tiers_offset: Option<i64>,
        pub include_legacy: Option<bool>,
    }
}

fn carrier_lines(db: &Connection, owner_kind: &str, rows: &[Value]) -> Result<HashMap<(i64, i64), Value>, ApiError> {
    if rows.is_empty() {
        return Ok(HashMap::new());
    }
    let (table, owner_column) = match owner_kind {
        "item" => ("item_effects", "item_id"),
        "augment" => ("augment_effects", "augment_id"),
        "set_bonus_tier" => ("set_bonus_tier_effects", "tier_id"),
        _ => unreachable!("unknown carrier owner kind"),
    };
    let mut links =
        rows.iter().filter_map(|row| Some((row["id"].as_i64()?, row["link_order"].as_i64()?))).collect::<Vec<_>>();
    links.sort_unstable();
    links.dedup();
    let placeholders = (0..links.len())
        .map(|index| format!("(?{}, ?{})", index * 2 + 1, index * 2 + 2))
        .collect::<Vec<_>>()
        .join(", ");
    let condition = format!("(j.{owner_column}, j.sort_order) IN ({placeholders})");
    let by_owner = effects_via_with_link_order(
        db,
        table,
        owner_column,
        &condition,
        rusqlite::params_from_iter(links.into_iter().flat_map(|(owner_id, link_order)| [owner_id, link_order])),
    )?;
    Ok(by_owner
        .into_iter()
        .flat_map(|(owner_id, lines)| lines.into_iter().map(move |(link_order, line)| ((owner_id, link_order), line)))
        .collect())
}

fn carrier_pages(
    db: &Connection,
    kind: &str,
    id: i64,
    pages: BacklinkPages,
) -> Result<(Value, Value, Value), ApiError> {
    let mut bonuses_by_link: HashMap<(String, i64, i64), Vec<Value>> = HashMap::new();
    if kind != "stat" {
        for mut bonus in json_rows(
            db,
            "SELECT ob.owner_kind, ob.owner_id, ob.effect_link_order,
                    s.name AS stat, s.category AS stat_category, bt.name AS bonus_type,
                    ob.amount AS value, ob.amount_source, ob.scale, g.id AS group_id, g.name AS group_name
               FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
               LEFT JOIN effects g ON g.id = ob.group_effect_id
               LEFT JOIN bonus_types bt ON bt.id = ob.bonus_type_id
              WHERE (ob.via_effect_id = ?1 AND ?2 = 'effect') OR (ob.group_effect_id = ?1 AND ?2 = 'group')
              ORDER BY ob.owner_kind, ob.owner_id, ob.effect_link_order, ob.stat_id",
            rusqlite::params![id, kind],
        )? {
            let owner_kind = bonus["owner_kind"].as_str().unwrap_or("").to_string();
            let owner_id = bonus["owner_id"].as_i64().unwrap_or(0);
            let link_order = bonus["effect_link_order"].as_i64().unwrap_or(0);
            let object = bonus.as_object_mut().expect("bonus object");
            object.remove("owner_kind");
            object.remove("owner_id");
            object.remove("effect_link_order");
            let group_id = object.remove("group_id").unwrap_or(Value::Null);
            let group_name = object.remove("group_name").unwrap_or(Value::Null);
            let group = if group_id.is_null() { Value::Null } else { json!({"id": group_id, "name": group_name}) };
            object.insert("group".to_string(), group);
            bonuses_by_link.entry((owner_kind, owner_id, link_order)).or_default().push(bonus);
        }
    }
    let carrier_columns = if kind != "stat" {
        "NULL AS effect_id, NULL AS effect,
         COALESCE(bt.name, (SELECT MIN(fixed_type.name) FROM effect_bonuses fixed_bonus
              JOIN bonus_types fixed_type ON fixed_type.id = fixed_bonus.bonus_type_id
             WHERE fixed_bonus.effect_id = j.effect_id
            HAVING COUNT(DISTINCT fixed_bonus.bonus_type_id) = 1)) AS bonus_type,
         j.value, j.value2, NULL AS amount_source, NULL AS scale"
            .to_string()
    } else {
        "e.id AS effect_id, e.name AS effect, bt.name AS bonus_type, ob.amount AS value,
         NULL AS value2, ob.amount_source, ob.scale"
            .to_string()
    };
    let match_id = if kind == "effect" {
        "j.effect_id"
    } else if kind == "group" {
        "COALESCE(ob.group_effect_id, -1)"
    } else {
        "ob.stat_id"
    };
    let join_for = |owner_kind: &str, owner_column: &str| -> String {
        if kind == "effect" {
            return "LEFT JOIN bonus_types bt ON bt.id = j.bonus_type_id".to_string();
        }
        let mut joined = format!(
            "JOIN owner_bonuses ob ON ob.owner_kind = '{owner_kind}'
             AND ob.owner_id = j.{owner_column} AND ob.effect_link_order = j.sort_order"
        );
        if kind == "stat" || kind == "group" {
            joined.push_str(
                " JOIN effects e ON e.id = COALESCE(ob.via_effect_id, ob.stat_id)
                  LEFT JOIN bonus_types bt ON bt.id = ob.bonus_type_id",
            );
        }
        joined
    };
    let current_items = if pages.include_legacy == Some(true) { "" } else { "WHERE NOT i.is_legacy" };
    let distinct = if kind == "group" { "DISTINCT" } else { "" };
    let items_sql = format!(
        "SELECT {distinct} i.id, i.name, {carrier_columns}, {match_id} AS match_id, j.sort_order AS link_order
           FROM item_effects j JOIN items i ON i.id = j.item_id {match_join}
          {current_items} ORDER BY i.name, j.sort_order",
        match_join = join_for("item", "item_id")
    );
    let augments_sql = format!(
        "SELECT {distinct} a.id, a.name, {carrier_columns}, {match_id} AS match_id, j.sort_order AS link_order
           FROM augment_effects j JOIN augments a ON a.id = j.augment_id {match_join}
          ORDER BY a.name, j.sort_order",
        match_join = join_for("augment", "augment_id")
    );
    let tiers_sql = format!(
        "SELECT {distinct} t.id, s.id AS set_id, s.name AS set_name, t.equipped_count, {carrier_columns}, {match_id} AS match_id,
                j.sort_order AS link_order,
                s.name || ' (' || t.equipped_count || ')' AS name
           FROM set_bonus_tier_effects j JOIN set_bonus_tiers t ON t.id = j.tier_id
           JOIN set_bonuses s ON s.id = t.set_id {match_join} ORDER BY s.name, t.equipped_count, j.sort_order",
        match_join = join_for("set_bonus_tier", "tier_id")
    );
    let page = |links_sql: String,
                owner_columns: &[&str],
                owner_kind: &str,
                limit: Option<i64>,
                offset: Option<i64>|
     -> Result<ListPage, ApiError> {
        let (limit, offset) = clamped_page(limit, offset);
        let link_rows = json_rows(
            db,
            &format!(
                "SELECT * FROM ({links_sql}) listed WHERE listed.match_id = ?1
                  ORDER BY listed.name, listed.id, listed.link_order"
            ),
            [id],
        )?;
        let mut link_rows_by_owner: Vec<(i64, Vec<Value>)> = Vec::new();
        for link_row in link_rows {
            let owner_id = link_row["id"].as_i64().unwrap_or_default();
            match link_rows_by_owner.last_mut() {
                Some((last_owner_id, owner_link_rows)) if *last_owner_id == owner_id => owner_link_rows.push(link_row),
                _ => link_rows_by_owner.push((owner_id, vec![link_row])),
            }
        }
        let total = link_rows_by_owner.len() as i64;
        let page_owners: Vec<(i64, Vec<Value>)> =
            link_rows_by_owner.into_iter().skip(offset as usize).take(limit as usize).collect();
        let page_link_rows: Vec<Value> =
            page_owners.iter().flat_map(|(_, owner_link_rows)| owner_link_rows.iter().cloned()).collect();
        let lines = carrier_lines(db, owner_kind, &page_link_rows)?;
        let mut rows = Vec::with_capacity(page_owners.len());
        for (owner_id, owner_link_rows) in page_owners {
            let mut owner_lines = Vec::with_capacity(owner_link_rows.len());
            for link_row in &owner_link_rows {
                let link_order = link_row["link_order"].as_i64().unwrap_or_default();
                let mut carrier_line = serde_json::Map::new();
                for field in ["effect_id", "effect", "bonus_type", "value", "value2", "amount_source", "scale"] {
                    carrier_line.insert(field.to_string(), link_row[field].clone());
                }
                if kind != "stat" {
                    let key = (owner_kind.to_string(), owner_id, link_order);
                    carrier_line.insert(
                        "bonuses".to_string(),
                        Value::Array(bonuses_by_link.get(&key).cloned().unwrap_or_default()),
                    );
                }
                carrier_line.insert(
                    "line".to_string(),
                    lines.get(&(owner_id, link_order)).cloned().ok_or_else(|| {
                        ApiError::Internal(anyhow::anyhow!("missing {owner_kind} effect line {owner_id}:{link_order}"))
                    })?,
                );
                owner_lines.push(Value::Object(carrier_line));
            }
            let mut row = serde_json::Map::new();
            for field in owner_columns {
                row.insert(field.to_string(), owner_link_rows[0][*field].clone());
            }
            if let Some(Value::Object(first_line)) = owner_lines.first() {
                for (field, value) in first_line {
                    row.insert(field.clone(), value.clone());
                }
            }
            row.insert("lines".to_string(), Value::Array(owner_lines));
            rows.push(Value::Object(row));
        }
        Ok(ListPage { total, limit, offset, rows })
    };
    let items = page(items_sql, &["id", "name"], "item", pages.items_limit, pages.items_offset)?.into_json("items");
    let augments = page(augments_sql, &["id", "name"], "augment", pages.augments_limit, pages.augments_offset)?
        .into_json("augments");
    let tiers = page(
        tiers_sql,
        &["id", "set_id", "set_name", "equipped_count", "name"],
        "set_bonus_tier",
        pages.set_tiers_limit,
        pages.set_tiers_offset,
    )?
    .into_json("set_tiers");
    Ok((items, augments, tiers))
}

#[utoipa::path(
    get, path = "/v1/effects/{id}", tag = "bonuses", summary = "Get an effect",
    description = "Returns an effect, group or stat with its bonus and damage rules, tier group and paged carriers, one row per owner with its lines.",
    params(("id" = i64, Path, description = "Effect, group or stat id from the vocabulary."),
        ("items_limit" = Option<i64>, Query, description = "Maximum items in this page."),
        ("items_offset" = Option<i64>, Query, description = "Items to skip before this page."),
        ("augments_limit" = Option<i64>, Query, description = "Maximum augments in this page."),
        ("augments_offset" = Option<i64>, Query, description = "Augments to skip before this page."),
        ("set_tiers_limit" = Option<i64>, Query, description = "Maximum set tiers in this page."),
        ("set_tiers_offset" = Option<i64>, Query, description = "Set tiers to skip before this page."),
        ("include_legacy" = Option<bool>, Query, description = "Use true to include legacy items in carrier links.")),
    responses((status = 200, description = "Effect and paged carriers", body = crate::routes::v1::response_schemas::EffectsDetailResponse),
        (status = 404, description = "No effect has this id", body = crate::error::ErrorBody))
)]
async fn effect_detail(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    ApiFilterQuery(pages): ApiFilterQuery<BacklinkPages>,
) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut family = json_row(
                db,
                "SELECT id, name, is_stat, is_group, category, verbose_name_template, description_template, wiki_url,
                    default_value, default_value2, tier_group_id, tier FROM effects WHERE id = ?1",
                [id],
            )?;
            family["kind"] = Value::String(
                if family["is_stat"] == 1 {
                    "stat"
                } else if family["is_group"] == 1 {
                    "group"
                } else {
                    "effect"
                }
                .to_string(),
            );
            family.as_object_mut().expect("effect object").remove("is_stat");
            family.as_object_mut().expect("effect object").remove("is_group");
            family["bonuses"] = Value::Array(json_rows(
                db,
                "SELECT s.name AS target, CASE WHEN s.is_group = 1 THEN 'group' ELSE 'stat' END AS target_kind,
                        bt.name AS bonus_type, es.amount_from, es.constant, es.scale, es.rounding
               FROM effect_bonuses es JOIN effects s ON s.id = es.target_effect_id
               LEFT JOIN bonus_types bt ON bt.id = es.bonus_type_id
              WHERE es.effect_id = ?1 ORDER BY es.sort_order",
                [id],
            )?);
            family["damage"] = Value::Array(json_rows(
                db,
                "SELECT t.name AS trigger, dt.name AS damage_type, ed.dice_number, ed.dice_sides,
                        ed.dice_bonus, ed.amount_from, ed.scale FROM effect_damage ed
                  JOIN triggers t ON t.id = ed.trigger_id JOIN damage_types dt ON dt.id = ed.damage_type_id
                 WHERE ed.effect_id = ?1 ORDER BY ed.sort_order",
                [id],
            )?);
            let tier_group_id = family["tier_group_id"].as_i64();
            let tier_rank = family["tier"].clone();
            family.as_object_mut().expect("effect object").remove("tier_group_id");
            family["tier"] = if let Some(tier_group_id) = tier_group_id {
                let mut tier =
                    json_row(db, "SELECT name AS `group` FROM effect_tier_groups WHERE id = ?1", [tier_group_id])?;
                tier["steps"] = Value::Array(json_rows(
                    db,
                    "SELECT id, name, tier AS rank FROM effects WHERE tier_group_id = ?1 ORDER BY tier",
                    [tier_group_id],
                )?);
                tier["rank"] = tier_rank;
                tier
            } else {
                Value::Null
            };
            let kind = family["kind"].as_str().unwrap_or("effect");
            let (items, augments, set_tiers) = carrier_pages(db, kind, id, pages)?;
            family["items"] = items;
            family["augments"] = augments;
            family["set_tiers"] = set_tiers;
            Ok(Json(family))
        })
        .await
}
