use anyhow::Result;
use rusqlite::{types::Value as SqlValue, Connection};
use std::collections::BTreeSet;

pub fn table_names(db: &Connection) -> Result<BTreeSet<String>> {
    let mut statement =
        db.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")?;
    let names = statement.query_map([], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<_>>()?;
    Ok(names)
}

pub fn table_columns(db: &Connection, table: &str) -> Result<Vec<String>> {
    let mut statement = db.prepare(&format!("PRAGMA table_info(\"{table}\")"))?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?.collect::<rusqlite::Result<_>>()?;
    Ok(columns)
}

pub fn table_rows(db: &Connection, table: &str) -> Result<Vec<Vec<SqlValue>>> {
    let columns = table_columns(db, table)?;
    let mut key_columns = Vec::new();
    let mut statement = db.prepare(&format!("PRAGMA table_info(\"{table}\")"))?;
    let table_columns = statement.query_map([], |row| Ok((row.get::<_, String>(1)?, row.get::<_, i64>(5)?)))?;
    for column in table_columns {
        let (name, key_position) = column?;
        if key_position > 0 {
            key_columns.push((key_position, name));
        }
    }
    key_columns.sort_by_key(|(position, _)| *position);
    let order_by = if key_columns.is_empty() {
        (1..=columns.len()).map(|index| index.to_string()).collect::<Vec<_>>().join(", ")
    } else {
        key_columns.iter().map(|(_, name)| format!("\"{name}\"")).collect::<Vec<_>>().join(", ")
    };
    let mut statement = db.prepare(&format!("SELECT * FROM \"{table}\" ORDER BY {order_by}"))?;
    let rows = statement
        .query_map([], |row| {
            (0..columns.len()).map(|index| row.get::<_, SqlValue>(index)).collect::<rusqlite::Result<Vec<_>>>()
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn table_rows_by_rowid(db: &Connection, table: &str) -> Result<Vec<(i64, Vec<SqlValue>)>> {
    let mut statement = db.prepare(&format!("SELECT rowid, * FROM \"{table}\" ORDER BY rowid"))?;
    let column_count = statement.column_count();
    let rows = statement
        .query_map([], |row| {
            Ok((row.get(0)?, (1..column_count).map(|index| row.get(index)).collect::<rusqlite::Result<Vec<_>>>()?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub(crate) struct DatabaseTable {
    pub(crate) columns: Vec<String>,
    pub(crate) rows: Vec<(i64, Vec<SqlValue>)>,
}

pub(crate) fn database_tables(db: &Connection) -> Result<std::collections::BTreeMap<String, DatabaseTable>> {
    table_names(db)?
        .into_iter()
        .map(|table| {
            let columns = table_columns(db, &table)?;
            let rows = table_rows_by_rowid(db, &table)?;
            Ok((table, DatabaseTable { columns, rows }))
        })
        .collect()
}
