use crate::database_tables::table_columns;
use crate::delta::{is_rebuilt_column, TableMerge, TABLE_MERGES};
use anyhow::Result;
use rusqlite::{params_from_iter, types::Value as SqlValue, Connection};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FilledCell {
    pub table: String,
    pub slug: String,
    pub column: String,
    pub ours_id: i64,
}

fn is_blank(value: &SqlValue) -> bool {
    match value {
        SqlValue::Null => true,
        SqlValue::Text(text) => text.is_empty(),
        _ => false,
    }
}

struct ReferenceTarget {
    our_slugs_by_id: BTreeMap<i64, String>,
    his_ids_by_slug: BTreeMap<String, i64>,
}

fn row_ids_by_slug(db: &Connection, table: &str) -> Result<BTreeMap<String, i64>> {
    let mut statement = db.prepare(&format!("SELECT slug, id FROM \"{table}\" WHERE slug IS NOT NULL"))?;
    let rows = statement
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

fn references(db: &Connection, table: &str) -> Result<BTreeMap<String, String>> {
    let mut statement = db.prepare(&format!("PRAGMA foreign_key_list(\"{table}\")"))?;
    let rows = statement
        .query_map([], |row| Ok((row.get::<_, String>(3)?, row.get::<_, String>(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

pub fn fill_blank_cells_from_ours(his: &Connection, ours: &Connection) -> Result<Vec<FilledCell>> {
    let mut filled = Vec::new();
    for (table, rule) in TABLE_MERGES {
        if !matches!(rule, TableMerge::Entity { .. }) {
            continue;
        }
        let columns = table_columns(his, table)?;
        if !columns.iter().any(|column| column == "slug") {
            continue;
        }
        let references = references(his, table)?;
        let his_ids = row_ids_by_slug(his, table)?;
        let our_ids = row_ids_by_slug(ours, table)?;
        let reference_slugs: BTreeMap<&str, ReferenceTarget> = references
            .iter()
            .filter_map(|(column, target)| {
                let our_slugs_by_id =
                    row_ids_by_slug(ours, target).ok()?.into_iter().map(|(slug, id)| (id, slug)).collect();
                let his_ids_by_slug = row_ids_by_slug(his, target).ok()?;
                Some((column.as_str(), ReferenceTarget { our_slugs_by_id, his_ids_by_slug }))
            })
            .collect();
        let fillable: Vec<&String> = columns
            .iter()
            .filter(|column| !matches!(column.as_str(), "id" | "slug") && !is_rebuilt_column(table, column))
            .filter(|column| references.get(*column).is_none_or(|_| reference_slugs.contains_key(column.as_str())))
            .collect();
        let select = format!(
            "SELECT {} FROM \"{table}\" WHERE id = ?1",
            fillable.iter().map(|column| format!("\"{column}\"")).collect::<Vec<_>>().join(", ")
        );
        let mut read_ours = ours.prepare(&select)?;
        let mut read_his = his.prepare(&select)?;
        for (slug, our_id) in &our_ids {
            let Some(his_id) = his_ids.get(slug) else { continue };
            let our_values = read_ours.query_row([our_id], |row| {
                (0..fillable.len()).map(|index| row.get::<_, SqlValue>(index)).collect::<rusqlite::Result<Vec<_>>>()
            })?;
            let his_values = read_his.query_row([his_id], |row| {
                (0..fillable.len()).map(|index| row.get::<_, SqlValue>(index)).collect::<rusqlite::Result<Vec<_>>>()
            })?;
            for ((column, our_value), his_value) in fillable.iter().zip(our_values).zip(his_values) {
                if !is_blank(&his_value) || is_blank(&our_value) {
                    continue;
                }
                let value = match reference_slugs.get(column.as_str()) {
                    Some(target) => {
                        let SqlValue::Integer(referenced_id) = our_value else { continue };
                        let Some(id) = target
                            .our_slugs_by_id
                            .get(&referenced_id)
                            .and_then(|slug| target.his_ids_by_slug.get(slug))
                        else {
                            continue;
                        };
                        SqlValue::Integer(*id)
                    }
                    None => our_value,
                };
                his.execute(
                    &format!("UPDATE \"{table}\" SET \"{column}\" = ?1 WHERE id = ?2"),
                    params_from_iter([value, SqlValue::Integer(*his_id)]),
                )?;
                filled.push(FilledCell {
                    table: (*table).to_string(),
                    slug: slug.clone(),
                    column: (*column).clone(),
                    ours_id: *our_id,
                });
            }
        }
    }
    filled.sort();
    Ok(filled)
}

pub(crate) fn blanked_upstream(
    previous: &Connection,
    previous_filled: &[FilledCell],
    new_filled: &[FilledCell],
) -> Result<Vec<FilledCell>> {
    let mut previous_rows: BTreeMap<&str, BTreeMap<String, i64>> = BTreeMap::new();
    let mut blanked = Vec::new();
    for cell in new_filled {
        if previous_filled.binary_search(cell).is_ok() {
            continue;
        }
        if !previous_rows.contains_key(cell.table.as_str()) {
            previous_rows.insert(cell.table.as_str(), row_ids_by_slug(previous, &cell.table)?);
        }
        if previous_rows[cell.table.as_str()].contains_key(&cell.slug) {
            blanked.push(cell.clone());
        }
    }
    Ok(blanked)
}
