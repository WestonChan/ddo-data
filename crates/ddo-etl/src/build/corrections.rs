use super::{BuildReport, StaleCorrection};
use crate::corrections::{CorrectableField, Correction, CorrectionValue, Corrections, FieldShape};
use anyhow::{bail, Context, Result};
use ddo_model::enums::{CorrectionKind, RowSource};
use rusqlite::types::Value as SqlValue;
use rusqlite::{params, OptionalExtension, Transaction};

pub(super) fn apply_corrections(
    transaction: &Transaction,
    corrections: &Corrections,
    report: &mut BuildReport,
) -> Result<()> {
    let (renames, field_corrections): (Vec<&Correction>, Vec<&Correction>) = corrections
        .entries
        .iter()
        .partition(|correction| correction.correctable_field().is_ok_and(|field| field.shape == FieldShape::RowName));
    for correction in field_corrections.into_iter().chain(renames) {
        apply_correction(transaction, correction, report)
            .with_context(|| format!("correction file {}: {}", correction.file_name, correction.label()))?;
    }
    Ok(())
}

fn apply_correction(transaction: &Transaction, correction: &Correction, report: &mut BuildReport) -> Result<()> {
    let field = correction.correctable_field()?;
    let row_ids = maetrim_row_ids_named(transaction, correction.kind, &correction.name)?;
    if row_ids.is_empty() {
        bail!(
            "no {} in Maetrim's files is named {:?} (matched against {}.name); use his exact name",
            correction.kind.as_str(),
            correction.name,
            correction.kind.table_name()
        );
    }
    let mut maetrim_values: Vec<CorrectionValue> = Vec::new();
    for row_id in &row_ids {
        let maetrim_value = current_value(transaction, correction.kind, field, *row_id)?;
        if !maetrim_values.contains(&maetrim_value) {
            maetrim_values.push(maetrim_value);
        }
    }
    if maetrim_values != [correction.from.clone()] {
        let maetrim_value_texts: Vec<String> = maetrim_values.iter().map(CorrectionValue::to_json).collect();
        report.stale_corrections.push(StaleCorrection {
            kind: correction.kind.as_str().to_string(),
            name: correction.name.clone(),
            field: correction.field.clone(),
            expected_value: correction.from.to_json(),
            maetrim_value: maetrim_value_texts.join(" / "),
            file_name: correction.file_name.clone(),
        });
        report.correction_stale_count += 1;
        return Ok(());
    }
    let table_name = correction.kind.table_name();
    match field.shape {
        FieldShape::Integer | FieldShape::Flag | FieldShape::Text => {
            write_column(transaction, table_name, field.column, &row_ids, correction.to.to_sql())?;
        }
        FieldShape::NamedReference { referenced_table } => {
            let referenced_id = match correction.to.as_text() {
                Some(referenced_name) => {
                    SqlValue::Integer(id_named(transaction, referenced_table, referenced_name)?.with_context(|| {
                        format!(
                            "to {referenced_name:?} is not in {referenced_table}; {} takes a name his files use",
                            field.name
                        )
                    })?)
                }
                None => SqlValue::Null,
            };
            write_column(transaction, table_name, field.column, &row_ids, referenced_id)?;
        }
        FieldShape::SetName => relink_item_sets(transaction, &row_ids, correction.to.as_text())?,
        FieldShape::RowName => rename_row(transaction, correction)?,
    }
    transaction.execute(
        "INSERT INTO corrections (kind, name, field, from_value, to_value, reason, source, read)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            correction.kind.as_str(),
            correction.name,
            correction.field,
            correction.from.to_json(),
            correction.to.to_json(),
            correction.reason,
            correction.source,
            correction.read
        ],
    )?;
    report.correction_applied_count += 1;
    Ok(())
}

fn maetrim_row_ids_named(transaction: &Transaction, kind: CorrectionKind, name: &str) -> Result<Vec<i64>> {
    let maetrim_rows_only = match kind {
        CorrectionKind::Item | CorrectionKind::Quest => format!(" AND source = '{}'", RowSource::Maetrim.as_str()),
        _ => String::new(),
    };
    let mut statement = transaction
        .prepare(&format!("SELECT id FROM {} WHERE name = ?1{maetrim_rows_only} ORDER BY id", kind.table_name()))?;
    let row_ids = statement.query_map(params![name], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    Ok(row_ids)
}

fn current_value(
    transaction: &Transaction,
    kind: CorrectionKind,
    field: &CorrectableField,
    row_id: i64,
) -> Result<CorrectionValue> {
    let table_name = kind.table_name();
    let value_sql = match field.shape {
        FieldShape::NamedReference { referenced_table } => format!(
            "SELECT {referenced_table}.name FROM {table_name} LEFT JOIN {referenced_table} ON {referenced_table}.id = {table_name}.{} WHERE {table_name}.id = ?1",
            field.column
        ),
        _ => format!("SELECT {} FROM {table_name} WHERE id = ?1", field.column),
    };
    let sql_value: SqlValue = transaction.query_row(&value_sql, params![row_id], |r| r.get(0))?;
    Ok(CorrectionValue::from_sql(sql_value))
}

fn write_column(
    transaction: &Transaction,
    table_name: &str,
    column: &str,
    row_ids: &[i64],
    sql_value: SqlValue,
) -> Result<()> {
    let mut statement = transaction.prepare(&format!("UPDATE {table_name} SET {column} = ?2 WHERE id = ?1"))?;
    for row_id in row_ids {
        statement.execute(params![row_id, sql_value])?;
    }
    Ok(())
}

fn relink_item_sets(transaction: &Transaction, item_ids: &[i64], set_name: Option<&str>) -> Result<()> {
    let set_id = match set_name {
        Some(set_name) => Some(id_named(transaction, "set_bonuses", set_name)?.with_context(|| {
            format!("to {set_name:?} is not a set in Maetrim's SetBonuses.xml or FiligreeSets/; use his spelling")
        })?),
        None => None,
    };
    for item_id in item_ids {
        transaction.execute("UPDATE items SET set_bonus = ?2 WHERE id = ?1", params![item_id, set_name])?;
        transaction.execute("DELETE FROM set_bonus_items WHERE item_id = ?1", params![item_id])?;
        if let Some(set_id) = set_id {
            transaction
                .execute("INSERT INTO set_bonus_items (set_id, item_id) VALUES (?1, ?2)", params![set_id, item_id])?;
        }
    }
    Ok(())
}

fn rename_row(transaction: &Transaction, correction: &Correction) -> Result<()> {
    let table_name = correction.kind.table_name();
    let new_name = correction.to.as_text().context("a row's name cannot be null")?;
    if id_named(transaction, table_name, new_name)?.is_some() {
        bail!(
            "to {new_name:?} is already the name of another {}; a rename cannot merge two rows",
            correction.kind.as_str()
        );
    }
    transaction
        .execute(&format!("UPDATE {table_name} SET name = ?2 WHERE name = ?1"), params![correction.name, new_name])?;
    if correction.kind == CorrectionKind::SetBonus {
        for set_naming_table in ["items", "augments"] {
            transaction.execute(
                &format!("UPDATE {set_naming_table} SET set_bonus = ?2 WHERE set_bonus = ?1"),
                params![correction.name, new_name],
            )?;
        }
    }
    Ok(())
}

fn id_named(transaction: &Transaction, table_name: &str, name: &str) -> Result<Option<i64>> {
    Ok(transaction
        .query_row(&format!("SELECT id FROM {table_name} WHERE name = ?1"), params![name], |r| r.get(0))
        .optional()?)
}
