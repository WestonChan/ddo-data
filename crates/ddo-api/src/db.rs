use crate::error::ApiError;
use crate::query::{sort_order, ListQuery};
use rusqlite::types::ValueRef;
use rusqlite::{Connection, Params, Row};
use serde_json::{Map, Value};

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
