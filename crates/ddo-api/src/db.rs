//! Query helpers. Most endpoints return rows as JSON objects keyed by column name; columns that
//! hold a JSON array as text (`amounts`, `targets`, `schools`, …) are expanded into real arrays.

use crate::error::ApiError;
use rusqlite::types::ValueRef;
use rusqlite::{Connection, Params, Row};
use serde_json::{Map, Value};

/// Every row of `sql` as a JSON object.
pub fn json_rows<P: Params>(conn: &Connection, sql: &str, params: P) -> Result<Vec<Value>, ApiError> {
    let mut stmt = conn.prepare_cached(sql)?;
    let names: Vec<String> = stmt.column_names().into_iter().map(str::to_string).collect();
    let rows = stmt.query_map(params, |row| Ok(row_to_json(row, &names)))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// The first row of `sql` as a JSON object, or `NotFound`.
pub fn json_row<P: Params>(conn: &Connection, sql: &str, params: P) -> Result<Value, ApiError> {
    let mut stmt = conn.prepare_cached(sql)?;
    let names: Vec<String> = stmt.column_names().into_iter().map(str::to_string).collect();
    let mut rows = stmt.query(params)?;
    match rows.next()? {
        Some(row) => Ok(row_to_json(row, &names)),
        None => Err(ApiError::NotFound),
    }
}

pub fn count<P: Params>(conn: &Connection, sql: &str, params: P) -> Result<i64, ApiError> {
    Ok(conn.query_row(sql, params, |r| r.get(0))?)
}

fn row_to_json(row: &Row, names: &[String]) -> Value {
    let mut map = Map::with_capacity(names.len());
    for (i, name) in names.iter().enumerate() {
        let value = match row.get_ref(i) {
            Ok(ValueRef::Null) | Err(_) => Value::Null,
            Ok(ValueRef::Integer(n)) => Value::from(n),
            Ok(ValueRef::Real(f)) => Value::from(f),
            Ok(ValueRef::Text(t)) => text_value(std::str::from_utf8(t).unwrap_or("")),
            Ok(ValueRef::Blob(_)) => Value::Null,
        };
        map.insert(name.clone(), value);
    }
    Value::Object(map)
}

/// JSON arrays stored as text come back as arrays; everything else stays a string.
fn text_value(t: &str) -> Value {
    if t.starts_with('[') && t.ends_with(']') {
        if let Ok(v @ Value::Array(_)) = serde_json::from_str::<Value>(t) {
            return v;
        }
    }
    Value::String(t.to_string())
}

/// Turn `0`/`1` integer columns named in `flags` into booleans, in place.
pub fn booleanize(value: &mut Value, flags: &[&str]) {
    if let Value::Object(map) = value {
        for flag in flags {
            if let Some(Value::Number(n)) = map.get(*flag) {
                let b = n.as_i64().unwrap_or(0) != 0;
                map.insert((*flag).to_string(), Value::Bool(b));
            }
        }
    }
}

/// `%term%` for a `LIKE` search, with LIKE metacharacters escaped.
pub fn like_pattern(q: &str) -> String {
    let escaped: String =
        q.chars().flat_map(|c| if matches!(c, '%' | '_' | '\\') { vec!['\\', c] } else { vec![c] }).collect();
    format!("%{}%", escaped.trim())
}

/// Clamp paging parameters to sane bounds.
pub fn page(limit: Option<i64>, offset: Option<i64>) -> (i64, i64) {
    (limit.unwrap_or(100).clamp(1, 10_000), offset.unwrap_or(0).max(0))
}

/// Requirements attached to an owner, in group and sort order.
pub fn requirements_for(conn: &Connection, owner_kind: &str, owner_id: i64) -> Result<Vec<Value>, ApiError> {
    json_rows(
        conn,
        "SELECT group_kind, group_index, req_type, items, value FROM requirements
          WHERE owner_kind = ?1 AND owner_id = ?2 ORDER BY group_index, sort_order",
        (owner_kind, owner_id),
    )
}

/// Modifiers attached to a source, each with its own requirements nested.
pub fn modifiers_for(conn: &Connection, source_kind: &str, source_id: i64) -> Result<Vec<Value>, ApiError> {
    let mut rows = json_rows(
        conn,
        "SELECT m.id, m.sort_order, m.effect_type, m.extra_types, m.bonus, bt.name AS bonus_type, m.amount_type, m.amounts,
                m.targets, m.value, m.dice_number, m.dice_sides, m.dice_bonus, m.dice_damage, m.damage, m.percent, m.rank,
                m.cap, m.stack_source, m.display_name, m.apply_as_item_effect, m.is_item_specific, m.is_rare
           FROM modifiers m LEFT JOIN bonus_types bt ON bt.id = m.bonus_type_id
          WHERE m.source_kind = ?1 AND m.source_id = ?2 ORDER BY m.sort_order",
        (source_kind, source_id),
    )?;
    for row in &mut rows {
        booleanize(row, &["percent", "apply_as_item_effect", "is_item_specific", "is_rare"]);
        let id = row["id"].as_i64().unwrap_or(0);
        let reqs = requirements_for(conn, "modifier", id)?;
        row["requirements"] = Value::Array(reqs);
    }
    Ok(rows)
}

pub fn stances_for(conn: &Connection, owner_kind: &str, owner_id: i64) -> Result<Vec<Value>, ApiError> {
    let mut rows = json_rows(
        conn,
        "SELECT id, name, description, icon, group_name, auto_controlled, incompatible FROM stances
          WHERE owner_kind = ?1 AND owner_id = ?2 ORDER BY sort_order",
        (owner_kind, owner_id),
    )?;
    for row in &mut rows {
        booleanize(row, &["auto_controlled"]);
        let id = row["id"].as_i64().unwrap_or(0);
        row["requirements"] = Value::Array(requirements_for(conn, "stance", id)?);
    }
    Ok(rows)
}

pub fn dcs_for(conn: &Connection, owner_kind: &str, owner_id: i64) -> Result<Vec<Value>, ApiError> {
    json_rows(
        conn,
        "SELECT name, description, icon, dc_type, dc_versus, mod_ability, amount, tactical, other, skill, class_level, base_class_level
           FROM dcs WHERE owner_kind = ?1 AND owner_id = ?2 ORDER BY sort_order",
        (owner_kind, owner_id),
    )
}

/// Derived bonuses through a junction table (`item_bonuses`, `augment_bonuses`, `feat_bonuses`).
pub fn bonuses_via(conn: &Connection, junction: &str, key: &str, id: i64) -> Result<Vec<Value>, ApiError> {
    let sql = format!(
        "SELECT b.id, b.name, b.description, s.name AS stat, s.category AS stat_category, bt.name AS bonus_type, b.value, b.value2
           FROM {junction} j JOIN bonuses b ON b.id = j.bonus_id JOIN stats s ON s.id = b.stat_id
           LEFT JOIN bonus_types bt ON bt.id = b.bonus_type_id
          WHERE j.{key} = ?1 ORDER BY j.sort_order"
    );
    json_rows(conn, &sql, [id])
}

/// Accumulates `WHERE` clauses and their positional parameters for a list endpoint.
#[derive(Default)]
pub struct Filters {
    clauses: Vec<String>,
    params: Vec<rusqlite::types::Value>,
}

impl Filters {
    /// A clause with one `?` placeholder, bound to `value`.
    pub fn bind(&mut self, clause: &str, value: impl Into<rusqlite::types::Value>) {
        self.params.push(value.into());
        self.clauses.push(clause.replace('?', &format!("?{}", self.params.len())));
    }

    /// A clause with no parameters.
    pub fn clause(&mut self, clause: &str) {
        self.clauses.push(clause.to_string());
    }

    pub fn where_sql(&self) -> String {
        if self.clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", self.clauses.join(" AND "))
        }
    }

    pub fn params(&self) -> impl Params + '_ {
        rusqlite::params_from_iter(self.params.iter())
    }
}
