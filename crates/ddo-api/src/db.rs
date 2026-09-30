use crate::error::ApiError;
use ddo_model::enums::CorrectionKind;
use rusqlite::types::ValueRef;
use rusqlite::{Connection, Params, Row};
use serde_json::{json, Map, Value};
use std::collections::HashMap;

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
    let escaped: String =
        search_text.chars().flat_map(|c| if matches!(c, '%' | '_' | '\\') { vec!['\\', c] } else { vec![c] }).collect();
    format!("%{}%", escaped.trim())
}

pub(crate) fn clamped_page(limit: Option<i64>, offset: Option<i64>) -> (i64, i64) {
    (limit.unwrap_or(100).clamp(1, 10_000), offset.unwrap_or(0).max(0))
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

pub(crate) fn stances_for(db: &Connection, owner_kind: &str, owner_id: i64) -> Result<Vec<Value>, ApiError> {
    let mut stances = json_rows(
        db,
        "SELECT id, name, description, icon, group_name, auto_controlled, incompatible FROM stances
          WHERE owner_kind = ?1 AND owner_id = ?2 ORDER BY sort_order",
        (owner_kind, owner_id),
    )?;
    for stance in &mut stances {
        convert_to_booleans(stance, &["auto_controlled"]);
        let stance_id = stance["id"].as_i64().unwrap_or(0);
        stance["requirements"] = Value::Array(requirements_for(db, "stance", stance_id)?);
        stance["modifiers"] = Value::Array(modifiers_for(db, "stance", stance_id)?);
    }
    Ok(stances)
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

const CORRECTION_COLUMNS: &str = "name, field, from_value, to_value, reason, source";

pub(crate) fn corrections_by_name(
    db: &Connection,
    kind: CorrectionKind,
) -> Result<HashMap<String, Vec<Value>>, ApiError> {
    let mut statement = db.prepare_cached(&format!(
        "SELECT {CORRECTION_COLUMNS} FROM corrections WHERE kind = ?1 ORDER BY name, field"
    ))?;
    let named_corrections = statement.query_map([kind.as_str()], named_correction_json)?;
    let mut corrections_by_name: HashMap<String, Vec<Value>> = HashMap::new();
    for named_correction in named_corrections {
        let (name, correction) = named_correction?;
        corrections_by_name.entry(name).or_default().push(correction);
    }
    Ok(corrections_by_name)
}

pub(crate) fn corrections_for(db: &Connection, kind: CorrectionKind, name: &str) -> Result<Vec<Value>, ApiError> {
    let mut statement = db.prepare_cached(&format!(
        "SELECT {CORRECTION_COLUMNS} FROM corrections WHERE kind = ?1 AND name = ?2 ORDER BY field"
    ))?;
    let corrections = statement.query_map((kind.as_str(), name), named_correction_json)?;
    Ok(corrections
        .map(|named_correction| named_correction.map(|(_, correction)| correction))
        .collect::<Result<_, _>>()?)
}

fn named_correction_json(row: &Row) -> rusqlite::Result<(String, Value)> {
    let (from_value, to_value): (String, String) = (row.get(2)?, row.get(3)?);
    let correction = json!({
        "field": row.get::<_, String>(1)?,
        "from": json_from_stored_json(&from_value),
        "to": json_from_stored_json(&to_value),
        "reason": row.get::<_, String>(4)?,
        "source": row.get::<_, String>(5)?,
    });
    Ok((row.get(0)?, correction))
}

fn json_from_stored_json(stored_json: &str) -> Value {
    serde_json::from_str(stored_json).unwrap_or_else(|_| Value::String(stored_json.to_string()))
}

pub(crate) fn bonuses_via(
    db: &Connection,
    junction_table: &str,
    owner_column: &str,
    owner_id: i64,
) -> Result<Vec<Value>, ApiError> {
    let sql = format!(
        "SELECT b.id, b.name, b.description, s.name AS stat, s.category AS stat_category, bt.name AS bonus_type, b.value, b.value2
           FROM {junction_table} j JOIN bonuses b ON b.id = j.bonus_id JOIN stats s ON s.id = b.stat_id
           LEFT JOIN bonus_types bt ON bt.id = b.bonus_type_id
          WHERE j.{owner_column} = ?1 ORDER BY j.sort_order"
    );
    json_rows(db, &sql, [owner_id])
}

#[derive(Default)]
pub(crate) struct WhereClause {
    conditions: Vec<String>,
    bound_values: Vec<rusqlite::types::Value>,
}

impl WhereClause {
    pub(crate) fn add_bound_condition(&mut self, condition: &str, bound_value: impl Into<rusqlite::types::Value>) {
        self.bound_values.push(bound_value.into());
        self.conditions.push(condition.replace('?', &format!("?{}", self.bound_values.len())));
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

pub(crate) async fn whole_table_json(
    state: crate::state::AppState,
    sql: &'static str,
    flag_columns: &'static [&'static str],
) -> Result<axum::Json<Vec<Value>>, ApiError> {
    let rows = state
        .read_db(move |db| {
            let mut rows = json_rows(db, sql, [])?;
            for row in &mut rows {
                convert_to_booleans(row, flag_columns);
            }
            Ok(rows)
        })
        .await?;
    Ok(axum::Json(rows))
}
