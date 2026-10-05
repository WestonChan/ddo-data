use crate::error::ApiError;
use crate::query::{sort_order, ListQuery};
use rusqlite::types::ValueRef;
use rusqlite::{Connection, Params, Row};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};

pub(crate) fn json_rows<P: Params>(db: &Connection, sql: &str, params: P) -> Result<Vec<Value>, ApiError> {
    let mut statement = db.prepare_cached(sql)?;
    let column_names: Vec<String> = statement.column_names().into_iter().map(str::to_string).collect();
    let rows = statement.query_map(params, |row| Ok(json_object_from_row(row, &column_names)))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub(crate) fn json_row<P: Params>(db: &Connection, sql: &str, params: P) -> Result<Value, ApiError> {
    let mut statement = db.prepare_cached(sql)?;
    let column_names: Vec<String> = statement.column_names().into_iter().map(str::to_string).collect();
    let mut rows = statement.query(params)?;
    match rows.next()? {
        Some(row) => Ok(json_object_from_row(row, &column_names)),
        None => Err(ApiError::NotFound),
    }
}

pub(crate) fn row_count<P: Params>(db: &Connection, sql: &str, params: P) -> Result<i64, ApiError> {
    Ok(db.query_row(sql, params, |row| row.get(0))?)
}

fn json_object_from_row(row: &Row, column_names: &[String]) -> Value {
    let mut object = Map::with_capacity(column_names.len());
    for (column_index, column_name) in column_names.iter().enumerate() {
        let column_value = match row.get_ref(column_index) {
            Ok(ValueRef::Null) | Err(_) => Value::Null,
            Ok(ValueRef::Integer(n)) => Value::from(n),
            Ok(ValueRef::Real(f)) => Value::from(f),
            Ok(ValueRef::Text(text)) => json_from_text(std::str::from_utf8(text).unwrap_or("")),
            Ok(ValueRef::Blob(_)) => Value::Null,
        };
        object.insert(column_name.clone(), column_value);
    }
    Value::Object(object)
}

fn json_from_text(text: &str) -> Value {
    if text.starts_with('[') && text.ends_with(']') {
        if let Ok(array @ Value::Array(_)) = serde_json::from_str::<Value>(text) {
            return array;
        }
    }
    Value::String(text.to_string())
}

pub(crate) fn convert_to_booleans(row: &mut Value, flag_columns: &[&str]) {
    if let Value::Object(object) = row {
        for column in flag_columns {
            if let Some(Value::Number(n)) = object.get(*column) {
                let is_set = n.as_i64().unwrap_or(0) != 0;
                object.insert((*column).to_string(), Value::Bool(is_set));
            }
        }
    }
}

pub(crate) fn substring_like_pattern(search_text: &str) -> String {
    format!("%{}%", like_escaped_text(search_text.trim()))
}

pub(crate) fn like_escaped_text(text: &str) -> String {
    text.chars().flat_map(|c| if matches!(c, '%' | '_' | '\\') { vec!['\\', c] } else { vec![c] }).collect()
}

pub(crate) fn clamped_page(limit: Option<i64>, offset: Option<i64>) -> (i64, i64) {
    (limit.unwrap_or(100).clamp(1, 10_000), offset.unwrap_or(0).max(0))
}

pub(crate) struct ListPage {
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    pub rows: Vec<Value>,
}

impl ListPage {
    pub(crate) fn into_json(self, rows_key: &str) -> Value {
        let mut envelope = Map::new();
        envelope.insert("total".into(), self.total.into());
        envelope.insert("limit".into(), self.limit.into());
        envelope.insert("offset".into(), self.offset.into());
        envelope.insert(rows_key.into(), Value::Array(self.rows));
        Value::Object(envelope)
    }
}

pub(crate) fn paged_rows(
    db: &Connection,
    select_sql: &str,
    query: &ListQuery,
    name_column: &str,
    default_order: &str,
    sortable_fields: &[(&str, &str)],
) -> Result<ListPage, ApiError> {
    paged_rows_with_filter(db, select_sql, query, name_column, default_order, sortable_fields, WhereClause::default())
}

pub(crate) fn paged_rows_with_filter(
    db: &Connection,
    select_sql: &str,
    query: &ListQuery,
    name_column: &str,
    default_order: &str,
    sortable_fields: &[(&str, &str)],
    mut where_clause: WhereClause,
) -> Result<ListPage, ApiError> {
    where_clause.add_name_search(query.q.as_deref(), name_column);
    paged_query(db, "*", &format!("({select_sql}) listed"), query, default_order, sortable_fields, &where_clause)
}

pub(crate) fn paged_query(
    db: &Connection,
    columns: &str,
    tables: &str,
    query: &ListQuery,
    default_order: &str,
    sortable_fields: &[(&str, &str)],
    where_clause: &WhereClause,
) -> Result<ListPage, ApiError> {
    let order_sql = sort_order(&query.sort, sortable_fields, default_order)?;
    let (limit, offset) = clamped_page(query.limit, query.offset);
    let from_sql = format!("FROM {tables} {}", where_clause.to_sql());
    let total = row_count(db, &format!("SELECT COUNT(*) {from_sql}"), where_clause.params())?;
    let rows = json_rows(
        db,
        &format!("SELECT {columns} {from_sql} ORDER BY {order_sql} LIMIT {limit} OFFSET {offset}"),
        where_clause.params(),
    )?;
    Ok(ListPage { total, limit, offset, rows })
}

pub(crate) fn requirements_for(db: &Connection, owner_kind: &str, owner_id: i64) -> Result<Vec<Value>, ApiError> {
    json_rows(
        db,
        "SELECT group_kind, group_index, req_type, items, value FROM requirements
          WHERE owner_kind = ?1 AND owner_id = ?2 ORDER BY group_index, sort_order",
        (owner_kind, owner_id),
    )
}

pub(crate) fn modifiers_for(db: &Connection, source_kind: &str, source_id: i64) -> Result<Vec<Value>, ApiError> {
    let mut modifiers = json_rows(
        db,
        "SELECT m.id, m.sort_order, m.effect_type, m.extra_types, m.bonus, bt.name AS bonus_type, m.amount_type, m.amounts,
                m.targets, m.value, m.dice_number, m.dice_sides, m.dice_bonus, m.dice_damage, m.damage, m.percent, m.rank,
                m.cap, m.stack_source, m.display_name, m.apply_as_item_effect, m.is_item_specific, m.is_rare
           FROM modifiers m LEFT JOIN bonus_types bt ON bt.id = m.bonus_type_id
          WHERE m.source_kind = ?1 AND m.source_id = ?2 ORDER BY m.sort_order",
        (source_kind, source_id),
    )?;
    for modifier in &mut modifiers {
        convert_to_booleans(modifier, &["percent", "apply_as_item_effect", "is_item_specific", "is_rare"]);
        let modifier_id = modifier["id"].as_i64().unwrap_or(0);
        modifier["requirements"] = Value::Array(requirements_for(db, "modifier", modifier_id)?);
    }
    Ok(modifiers)
}

const STANCE_COLUMNS: &str = "id, name, description, icon, group_name, auto_controlled, incompatible";

fn stance_filter(owner_kind: &str, owner_id: i64) -> WhereClause {
    let mut where_clause = WhereClause::default();
    where_clause.add_bound_condition("owner_kind = ?", owner_kind.to_string());
    where_clause.add_bound_condition("owner_id = ?", owner_id);
    where_clause
}

pub(crate) fn stances_for(db: &Connection, owner_kind: &str, owner_id: i64) -> Result<Vec<Value>, ApiError> {
    let where_clause = stance_filter(owner_kind, owner_id);
    let mut stances = json_rows(
        db,
        &format!("SELECT {STANCE_COLUMNS} FROM stances {} ORDER BY sort_order", where_clause.to_sql()),
        where_clause.params(),
    )?;
    for stance in &mut stances {
        attach_stance_children(db, stance)?;
    }
    Ok(stances)
}

pub(crate) fn paged_stances_for(
    db: &Connection,
    owner_kind: &str,
    owner_id: i64,
    query: &ListQuery,
    sortable_fields: &[(&str, &str)],
) -> Result<ListPage, ApiError> {
    let mut where_clause = stance_filter(owner_kind, owner_id);
    where_clause.add_name_search(query.q.as_deref(), "name");
    let mut page = paged_query(db, STANCE_COLUMNS, "stances", query, "sort_order", sortable_fields, &where_clause)?;
    for stance in &mut page.rows {
        attach_stance_children(db, stance)?;
    }
    Ok(page)
}

fn attach_stance_children(db: &Connection, stance: &mut Value) -> Result<(), ApiError> {
    convert_to_booleans(stance, &["auto_controlled"]);
    let stance_id = stance["id"].as_i64().unwrap_or(0);
    stance["requirements"] = Value::Array(requirements_for(db, "stance", stance_id)?);
    stance["modifiers"] = Value::Array(modifiers_for(db, "stance", stance_id)?);
    Ok(())
}

pub(crate) fn attack_for(db: &Connection, owner_kind: &str, owner_id: i64) -> Result<Value, ApiError> {
    Ok(json_rows(
        db,
        "SELECT name, description, icon, cooldown_seconds, duration_seconds FROM attacks
          WHERE owner_kind = ?1 AND owner_id = ?2",
        (owner_kind, owner_id),
    )?
    .pop()
    .unwrap_or(Value::Null))
}

pub(crate) fn dcs_for(db: &Connection, owner_kind: &str, owner_id: i64) -> Result<Vec<Value>, ApiError> {
    json_rows(
        db,
        "SELECT name, description, icon, dc_type, dc_versus, mod_ability, amount, tactical, other, skill, class_level, base_class_level
           FROM dcs WHERE owner_kind = ?1 AND owner_id = ?2 ORDER BY sort_order",
        (owner_kind, owner_id),
    )
}

pub(crate) fn bonuses_via(
    db: &Connection,
    junction_table: &str,
    owner_column: &str,
    owner_id: i64,
) -> Result<Vec<Value>, ApiError> {
    let owner_kind = match junction_table {
        "feat_effects" => "feat",
        _ => unreachable!("bonus rendering is only used for feats"),
    };
    let sql = format!(
        "SELECT e.id, e.verbose_name_template, e.description_template, s.name AS stat, s.category AS stat_category,
                bt.name AS bonus_type,
                ob.amount AS value,
                CASE WHEN es.amount_from = 2 THEN ob.amount END AS value2,
                COALESCE(j.value, e.default_value) AS template_value,
                COALESCE(j.value2, e.default_value2) AS template_value2
           FROM {junction_table} j JOIN effects e ON e.id = j.effect_id
           JOIN owner_bonuses ob ON ob.owner_kind = '{owner_kind}' AND ob.owner_id = j.{owner_column}
                AND ob.effect_link_order = j.sort_order
           JOIN effects s ON s.id = ob.stat_id
           LEFT JOIN effect_bonuses es ON es.effect_id = e.id AND es.target_effect_id = COALESCE(ob.group_effect_id, ob.stat_id)
                AND (es.bonus_type_id IS ob.bonus_type_id OR es.bonus_type_id IS NULL)
           LEFT JOIN bonus_types bt ON bt.id = ob.bonus_type_id
          WHERE j.{owner_column} = ?1
          ORDER BY j.sort_order, es.sort_order"
    );
    let mut bonuses = json_rows(db, &sql, [owner_id])?;
    for bonus in &mut bonuses {
        let stat_name = bonus["stat"].as_str().unwrap_or("");
        let value = bonus["value"].as_i64();
        let name = match value {
            Some(value) if value < 0 => format!("{stat_name} {value}"),
            Some(value) => format!("{stat_name} +{value}"),
            None => stat_name.to_string(),
        };
        let description = bonus["description_template"].as_str().map(|template| {
            let rendered_title = render_effect_template(bonus["verbose_name_template"].as_str().unwrap_or(""), bonus);
            format!("{rendered_title}: {}", render_description_template(template, bonus))
        });
        let object = bonus.as_object_mut().expect("query returns objects");
        object.remove("verbose_name_template");
        object.remove("description_template");
        object.remove("template_value");
        object.remove("template_value2");
        object.insert("name".to_string(), Value::String(name));
        object.insert("description".to_string(), description.map_or(Value::Null, Value::String));
    }
    Ok(bonuses)
}

pub(crate) fn effects_via<P: Params>(
    db: &Connection,
    junction_table: &str,
    owner_column: &str,
    owner_condition: &str,
    selector_params: P,
) -> Result<BTreeMap<i64, Vec<Value>>, ApiError> {
    Ok(effects_via_with_link_order(db, junction_table, owner_column, owner_condition, selector_params)?
        .into_iter()
        .map(|(owner_id, lines)| (owner_id, lines.into_iter().map(|(_, line)| line).collect()))
        .collect())
}

pub(crate) fn effects_via_with_link_order<P: Params>(
    db: &Connection,
    junction_table: &str,
    owner_column: &str,
    owner_condition: &str,
    selector_params: P,
) -> Result<BTreeMap<i64, Vec<(i64, Value)>>, ApiError> {
    let owner_kind = match junction_table {
        "item_effects" => "item",
        "augment_effects" => "augment",
        "set_bonus_tier_effects" => "set_bonus_tier",
        "feat_effects" => "feat",
        "item_augment_slot_option_effects" => "item_augment_slot_option",
        _ => unreachable!("unknown effect owner table"),
    };
    let bonus_owner_condition = owner_condition
        .replace(&format!("j.{owner_column}"), "filtered_bonus.owner_id")
        .replace("j.sort_order", "filtered_bonus.effect_link_order");
    let sql = format!(
        "SELECT j.{owner_column} AS owner_id, j.sort_order, e.id AS effect_id, e.name,
                COALESCE(e.verbose_name_template, '%b1 ' || e.name || ' +{{1}}') AS verbose_name_template,
                e.set_bonus_line_template, e.description_template, j.value, j.value2,
                COALESCE(j.value, e.default_value,
                    (SELECT MIN(eb.constant) FROM effect_bonuses eb
                      WHERE eb.effect_id = e.id AND eb.amount_from = 0
                     HAVING COUNT(*) > 0 AND COUNT(eb.constant) = COUNT(*)
                        AND COUNT(DISTINCT eb.constant) = 1)) AS template_value,
                COALESCE(j.value2, e.default_value2) AS template_value2,
                link_type.name AS template_bonus_type,
                home_type.name AS home_bonus_type,
                COALESCE(link_type.name, home_type.name) AS bonus_type,
                tg.name AS tier_group, e.tier AS tier_rank,
                s.name AS stat, s.category AS stat_category,
                stat_type.name AS stat_bonus_type, ob.amount AS stat_value,
                ob.amount_source, ob.scale, g.id AS group_id, g.name AS group_name
           FROM {junction_table} j JOIN effects e ON e.id = j.effect_id
           LEFT JOIN effect_tier_groups tg ON tg.id = e.tier_group_id
           LEFT JOIN bonus_types link_type ON link_type.id = j.bonus_type_id
           LEFT JOIN bonus_types home_type ON home_type.id = e.home_bonus_type_id
           LEFT JOIN (SELECT * FROM owner_bonuses filtered_bonus
                       WHERE filtered_bonus.owner_kind = '{owner_kind}' AND {bonus_owner_condition}) ob
                ON ob.owner_id = j.{owner_column}
                AND ob.effect_link_order = j.sort_order
           LEFT JOIN effects s ON s.id = ob.stat_id
           LEFT JOIN effects g ON g.id = ob.group_effect_id
           LEFT JOIN bonus_types stat_type ON stat_type.id = ob.bonus_type_id
          WHERE {owner_condition}
          ORDER BY j.{owner_column}, j.sort_order, ob.stat_id"
    );
    let rows = json_rows(db, &sql, selector_params)?;
    let damage_rows = json_rows(
        db,
        "SELECT ed.effect_id, t.name AS trigger, dt.name AS damage_type, ed.dice_number,
                ed.dice_sides, ed.dice_bonus, ed.amount_from, ed.scale
           FROM effect_damage ed JOIN triggers t ON t.id = ed.trigger_id
           JOIN damage_types dt ON dt.id = ed.damage_type_id
          ORDER BY ed.effect_id, ed.sort_order",
        [],
    )?;
    let mut damage_by_effect: HashMap<i64, Vec<Value>> = HashMap::new();
    for mut damage in damage_rows {
        let effect_id = damage["effect_id"].as_i64().unwrap_or_default();
        damage.as_object_mut().expect("damage row").remove("effect_id");
        damage_by_effect.entry(effect_id).or_default().push(damage);
    }
    let mut by_owner: BTreeMap<i64, Vec<(i64, Value)>> = BTreeMap::new();
    let mut position_by_link: HashMap<(i64, i64), usize> = HashMap::new();
    for row in rows {
        let owner_id = row["owner_id"].as_i64().unwrap_or_default();
        let sort_order = row["sort_order"].as_i64().unwrap_or_default();
        let lines = by_owner.entry(owner_id).or_default();
        let position = match position_by_link.get(&(owner_id, sort_order)) {
            Some(position) => *position,
            None => {
                let tier = if row["tier_group"].is_null() {
                    Value::Null
                } else {
                    serde_json::json!({
                        "group": row["tier_group"], "rank": row["tier_rank"]
                    })
                };
                let verbose_name = if owner_kind == "set_bonus_tier" && !row["stat_value"].is_null() {
                    render_set_bonus_line(&row)
                } else {
                    render_verbose_name(&row)
                };
                let description = row["description_template"]
                    .as_str()
                    .map(|template| Value::String(render_description_template(template, &row)))
                    .unwrap_or(Value::Null);
                let position = lines.len();
                lines.push((sort_order, serde_json::json!({
                    "effect_id": row["effect_id"], "name": row["name"], "tier": tier,
                    "verbose_name": verbose_name, "description": description,
                    "value": row["template_value"], "value2": row["template_value2"],
                    "bonus_type": row["bonus_type"], "bonuses": [],
                    "damage": damage_by_effect.get(&row["effect_id"].as_i64().unwrap_or_default()).cloned().unwrap_or_default()
                })));
                position_by_link.insert((owner_id, sort_order), position);
                position
            }
        };
        if !row["stat_value"].is_null() {
            lines[position].1["bonuses"].as_array_mut().expect("bonus array").push(serde_json::json!({
                "stat": row["stat"], "stat_category": row["stat_category"],
                "bonus_type": row["stat_bonus_type"], "value": row["stat_value"],
                "amount_source": row["amount_source"], "scale": row["scale"],
                "group": if row["group_id"].is_null() { Value::Null } else {
                    serde_json::json!({"id": row["group_id"], "name": row["group_name"]})
                }
            }));
        }
    }
    Ok(by_owner)
}

pub(crate) fn effects_for_owner(
    db: &Connection,
    junction_table: &str,
    owner_column: &str,
    owner_id: i64,
) -> Result<Vec<Value>, ApiError> {
    let owner_condition = format!("j.{owner_column} = ?1");
    Ok(effects_via(db, junction_table, owner_column, &owner_condition, [owner_id])?
        .remove(&owner_id)
        .unwrap_or_default())
}

fn render_effect_template(template: &str, row: &Value) -> String {
    let value = row["template_value"].as_i64();
    let value2 = row["template_value2"].as_i64();
    let signed_value = value.map(|number| format!("{number:+}")).unwrap_or_default();
    let signed_value2 = value2.map(|number| format!("{number:+}")).unwrap_or_default();
    let negative_value = value.map(|number| format!("-{}", number.unsigned_abs())).unwrap_or_default();
    let negative_value2 = value2.map(|number| format!("-{}", number.unsigned_abs())).unwrap_or_default();
    template
        .replace("+{1}", &signed_value)
        .replace("+{2}", &signed_value2)
        .replace("-{1}", &negative_value)
        .replace("-{2}", &negative_value2)
        .replace("{1}", &value.map(|number| number.to_string()).unwrap_or_default())
        .replace("{2}", &value2.map(|number| number.to_string()).unwrap_or_default())
        .replace("%b1", row["template_bonus_type"].as_str().or_else(|| row["bonus_type"].as_str()).unwrap_or(""))
}

fn render_description_template(template: &str, row: &Value) -> String {
    let mut signed_template = String::with_capacity(template.len());
    for clause in template.split_inclusive(['.', ';', ':']) {
        let mut remainder = clause;
        while let Some(position) = remainder.find(['{']) {
            let (prefix, candidate) = remainder.split_at(position);
            signed_template.push_str(prefix);
            let amount_slot = if candidate.starts_with("{1}") {
                Some("{1}")
            } else if candidate.starts_with("{2}") {
                Some("{2}")
            } else {
                None
            };
            if let Some(slot) = amount_slot {
                let suffix = &candidate[slot.len()..];
                let is_bonus_amount = !suffix.starts_with('%')
                    && (suffix.to_ascii_lowercase().contains("bonus")
                        || (clause.to_ascii_lowercase().contains("bonus") && suffix.starts_with(" Damage")));
                if is_bonus_amount && !signed_template.ends_with(['+', '-']) {
                    signed_template.push('+');
                }
                signed_template.push_str(slot);
                remainder = suffix;
            } else {
                signed_template.push('{');
                remainder = &candidate[1..];
            }
        }
        signed_template.push_str(remainder);
    }
    render_effect_template(&signed_template, row)
}

fn render_verbose_name(row: &Value) -> String {
    let template = row["verbose_name_template"].as_str().unwrap_or("");
    let mut rendered_row = row.clone();
    let bonus_type = row["bonus_type"].as_str().unwrap_or("");
    let home_type = row["home_bonus_type"].as_str().unwrap_or("");
    let visible_type = if bonus_type == home_type {
        ""
    } else if bonus_type == "Insight" {
        "Insightful"
    } else {
        bonus_type
    };
    rendered_row["template_bonus_type"] = Value::String(visible_type.to_string());
    let name = row["name"].as_str().unwrap_or("");
    let template = if !template.contains("{1}") && !template.contains("{2}") && row["template_value"].is_number() {
        name
    } else {
        template
    };
    render_effect_template(template, &rendered_row).split_whitespace().collect::<Vec<_>>().join(" ")
}

fn render_set_bonus_line(row: &Value) -> String {
    let template = row["set_bonus_line_template"].as_str().map(str::to_string).unwrap_or_else(|| {
        let unit = if row["verbose_name_template"].as_str().is_some_and(|template| template.contains("{1}%")) {
            "%"
        } else {
            ""
        };
        format!("+{{1}}{unit} %b1 Bonus to {}", row["name"].as_str().unwrap_or(""))
    });
    render_effect_template(&template, row).split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod effect_template_tests {
    use super::{render_description_template, render_effect_template, render_verbose_name};
    use serde_json::json;

    #[test]
    fn a_negative_amount_replaces_an_explicit_plus_sign() {
        let row = json!({"template_value": -1, "template_value2": null, "bonus_type": "Penalty"});
        assert_eq!(render_effect_template("Curse of Weakness +{1}: {1} %b1", &row), "Curse of Weakness -1: -1 Penalty");
        assert_eq!(render_effect_template("Curse +{2}", &row), "Curse ");
        assert_eq!(render_effect_template("Unwieldy: -{1} Dexterity", &row), "Unwieldy: -1 Dexterity");
    }

    #[test]
    fn wiki_line_formats_keep_their_per_effect_number_positions() {
        let row = json!({
            "name": "Orb Bonus", "verbose_name_template": "+{1} Orb Bonus",
            "template_value": 15, "template_value2": null,
            "bonus_type": "Orb", "home_bonus_type": "Orb"
        });
        assert_eq!(render_verbose_name(&row), "+15 Orb Bonus");
        let row = json!({
            "name": "Combustion", "verbose_name_template": "%b1 Combustion +{1}",
            "template_value": 71, "template_value2": null,
            "bonus_type": "Insight", "home_bonus_type": "Equipment"
        });
        assert_eq!(render_verbose_name(&row), "Insightful Combustion +71");
    }

    #[test]
    fn descriptions_sign_bonus_amounts_without_signing_other_numbers() {
        let row = json!({
            "template_value": 29, "template_value2": 4,
            "template_bonus_type": "Implement", "bonus_type": "Implement"
        });
        assert_eq!(
            render_description_template("Passive: {1} %b1 bonus to Universal spell power.", &row),
            "Passive: +29 Implement bonus to Universal spell power."
        );
        assert_eq!(
            render_description_template("This item gives a {1} Luck bonus to all saves.", &row),
            "This item gives a +29 Luck bonus to all saves."
        );
        assert_eq!(
            render_description_template("The spell has DC {1} and lasts {2} seconds.", &row),
            "The spell has DC 29 and lasts 4 seconds."
        );
        assert_eq!(
            render_description_template("Gain {1}% Enhancement bonus to Ranged attack speed.", &row),
            "Gain 29% Enhancement bonus to Ranged attack speed."
        );
        assert_eq!(
            render_description_template("Maximum Dexterity bonus {1} higher than normal.", &row),
            "Maximum Dexterity bonus 29 higher than normal."
        );
        let penalty = json!({
            "template_value": -2, "template_value2": null,
            "template_bonus_type": "Penalty", "bonus_type": "Penalty"
        });
        assert_eq!(
            render_description_template("{1} %b1 bonus to Dexterity.", &penalty),
            "-2 Penalty bonus to Dexterity."
        );
    }
}

#[derive(Default)]
pub(crate) struct WhereClause {
    conditions: Vec<String>,
    bound_values: Vec<rusqlite::types::Value>,
}

impl WhereClause {
    pub(crate) fn add_repeated_filter<V: Clone + Into<rusqlite::types::Value>>(
        &mut self,
        any_condition: &str,
        all_condition: &str,
        values: &[V],
        match_mode: Option<&str>,
    ) {
        if values.is_empty() {
            return;
        }
        if match_mode == Some("all") {
            for value in values {
                self.add_bound_condition(all_condition, value.clone());
            }
        } else {
            self.add_bound_list_condition(any_condition, values.iter().cloned());
        }
    }

    pub(crate) fn add_name_search(&mut self, search_text: Option<&str>, name_column: &str) {
        if let Some(search_text) = search_text.filter(|search_text| !search_text.trim().is_empty()) {
            self.add_bound_condition(&format!("{name_column} LIKE ? ESCAPE '\\'"), substring_like_pattern(search_text));
        }
    }

    pub(crate) fn add_bound_condition(
        &mut self,
        condition: &str,
        bound_value: impl Into<rusqlite::types::Value>,
    ) -> String {
        self.bound_values.push(bound_value.into());
        let placeholder = format!("?{}", self.bound_values.len());
        self.conditions.push(condition.replace('?', &placeholder));
        placeholder
    }

    pub(crate) fn add_bound_list_condition(
        &mut self,
        condition: &str,
        bound_values: impl IntoIterator<Item = impl Into<rusqlite::types::Value>>,
    ) {
        let first_placeholder_number = self.bound_values.len() + 1;
        self.bound_values.extend(bound_values.into_iter().map(Into::into));
        let placeholders: Vec<String> =
            (first_placeholder_number..=self.bound_values.len()).map(|number| format!("?{number}")).collect();
        self.conditions.push(condition.replace('?', &placeholders.join(", ")));
    }

    pub(crate) fn add_condition(&mut self, condition: &str) {
        self.conditions.push(condition.to_string());
    }

    pub(crate) fn to_sql(&self) -> String {
        if self.conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", self.conditions.join(" AND "))
        }
    }

    pub(crate) fn params(&self) -> impl Params + '_ {
        rusqlite::params_from_iter(self.bound_values.iter())
    }
}

pub(crate) struct TableListSource {
    pub select_sql: &'static str,
    pub rows_key: &'static str,
    pub name_column: &'static str,
    pub default_order: &'static str,
    pub sortable_fields: &'static [(&'static str, &'static str)],
    pub flag_columns: &'static [&'static str],
}

pub(crate) async fn paged_table_json(
    state: crate::state::AppState,
    query: ListQuery,
    list: TableListSource,
) -> Result<axum::Json<Value>, ApiError> {
    let envelope = state
        .read_db(move |db| {
            let mut page =
                paged_rows(db, list.select_sql, &query, list.name_column, list.default_order, list.sortable_fields)?;
            for row in &mut page.rows {
                convert_to_booleans(row, list.flag_columns);
            }
            Ok(page.into_json(list.rows_key))
        })
        .await?;
    Ok(axum::Json(envelope))
}
