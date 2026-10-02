use super::items::item_wiki_url;
use super::wiki::folded_effect_name;
use super::{bonus_name, BuildReport, StaleCorrection, StaleCorrectionCause};
use crate::corrections::{BonusAddition, CorrectableField, Correction, CorrectionValue, Corrections, FieldShape};
use anyhow::{bail, Context, Result};
use ddo_model::enums::{CorrectionKind, ModifierSource, RowSource};
use rusqlite::types::Value as SqlValue;
use rusqlite::{params, OptionalExtension, Transaction};

pub(super) fn apply_quest_corrections(
    transaction: &Transaction,
    corrections: &Corrections,
    report: &mut BuildReport,
) -> Result<()> {
    apply_corrections_where(transaction, corrections, report, |kind| kind == CorrectionKind::Quest)
}

pub(super) fn apply_non_quest_corrections(
    transaction: &Transaction,
    corrections: &Corrections,
    report: &mut BuildReport,
) -> Result<()> {
    apply_corrections_where(transaction, corrections, report, |kind| kind != CorrectionKind::Quest)
}

fn apply_corrections_where(
    transaction: &Transaction,
    corrections: &Corrections,
    report: &mut BuildReport,
    applies_to_kind: impl Fn(CorrectionKind) -> bool,
) -> Result<()> {
    let (renames, field_corrections): (Vec<&Correction>, Vec<&Correction>) =
        corrections.entries.iter().filter(|correction| applies_to_kind(correction.kind)).partition(|correction| {
            correction.correctable_field().is_ok_and(|field| field.shape == FieldShape::RowName)
        });
    for correction in field_corrections.into_iter().chain(renames) {
        apply_correction(transaction, correction, report)
            .with_context(|| format!("correction file {}: {}", correction.file_name, correction.label()))?;
    }
    Ok(())
}

fn apply_correction(transaction: &Transaction, correction: &Correction, report: &mut BuildReport) -> Result<()> {
    let field = correction.correctable_field()?;
    let row_ids = maetrim_row_ids_named(transaction, correction, &correction.name)?;
    if row_ids.is_empty() {
        let new_name = correction.to.as_text().filter(|_| field.shape == FieldShape::RowName);
        if let Some(new_name) = new_name {
            if !maetrim_row_ids_named(transaction, correction, new_name)?.is_empty() {
                record_stale_correction(
                    report,
                    correction,
                    StaleCorrectionCause::RenameDoneUpstream { new_name: new_name.to_string() },
                );
                return Ok(());
            }
        }
        let family_text = match &correction.family {
            Some(family) => format!(" in family {family:?}"),
            None => String::new(),
        };
        let rename_text = match new_name {
            Some(new_name) => format!(", nor {new_name:?}, the name it renames to"),
            None => String::new(),
        };
        bail!(
            "no {} in Maetrim's files{family_text} is named {:?}{rename_text} (matched against {}.{}); use his exact name",
            correction.kind.as_str(),
            correction.name,
            correction.kind.table_name(),
            correction.kind.name_column()
        );
    }
    let mut maetrim_values: Vec<CorrectionValue> = Vec::new();
    for row_id in &row_ids {
        let maetrim_value = current_value(transaction, correction, field, *row_id)?;
        if !maetrim_values.contains(&maetrim_value) {
            maetrim_values.push(maetrim_value);
        }
    }
    if maetrim_values != [correction.from.clone()] {
        let maetrim_value_texts: Vec<String> = maetrim_values.iter().map(CorrectionValue::to_json).collect();
        record_stale_correction(
            report,
            correction,
            StaleCorrectionCause::ValueChanged {
                expected_value: correction.from.to_json(),
                maetrim_value: maetrim_value_texts.join(" / "),
            },
        );
        return Ok(());
    }
    write_correction(transaction, correction, field, &row_ids)?;
    transaction.execute(
        "INSERT INTO corrections (kind, name, qualifier, field, from_value, to_value, reason, source, read)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            correction.kind.as_str(),
            correction.name,
            correction.qualifier(),
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

fn record_stale_correction(report: &mut BuildReport, correction: &Correction, cause: StaleCorrectionCause) {
    report.stale_corrections.push(StaleCorrection {
        kind: correction.kind.as_str().to_string(),
        name: correction.name.clone(),
        field: correction.field.clone(),
        cause,
        file_name: correction.file_name.clone(),
    });
    report.correction_stale_count += 1;
}

fn write_correction(
    transaction: &Transaction,
    correction: &Correction,
    field: &CorrectableField,
    row_ids: &[i64],
) -> Result<()> {
    let table_name = correction.kind.table_name();
    match field.shape {
        FieldShape::Integer if correction.kind == CorrectionKind::AugmentBonus => {
            let (stat_id, bonus_type_id) = bonus_key_ids(transaction, correction)?;
            let new_value = match correction.to {
                CorrectionValue::Integer(number) => number,
                _ => bail!("a bonus value is an integer"),
            };
            repoint_augment_bonuses(transaction, row_ids, stat_id, bonus_type_id, bonus_type_id, Some(new_value))?;
        }
        FieldShape::BonusTypeName => {
            let (stat_id, bonus_type_id) = bonus_key_ids(transaction, correction)?;
            let new_type_name = correction.to.as_text().context("a bonus type cannot be null")?;
            let new_type_id = bonus_type_id_named(transaction, new_type_name)?;
            repoint_augment_bonuses(transaction, row_ids, stat_id, bonus_type_id, new_type_id, None)?;
        }
        FieldShape::BonusAddition => {
            let CorrectionValue::Bonus(bonus) = &correction.to else { bail!("an add names the bonus in to") };
            add_bonus(transaction, BonusLinkTable::of(correction.kind)?, row_ids, bonus)?;
        }
        FieldShape::EffectAddition => {
            let effect_name = correction.to.as_text().context("an add names the effect in to")?;
            add_item_effect(transaction, row_ids, effect_name)?;
        }
        FieldShape::SocketAddition => {
            let socket_label = correction.to.as_text().context("an add names the socket label in to")?;
            add_item_socket(transaction, row_ids, socket_label)?;
        }
        FieldShape::Removal => remove_rows(transaction, correction.kind, row_ids)?,
        FieldShape::Integer | FieldShape::Flag | FieldShape::Text => {
            write_column(transaction, table_name, field.column, row_ids, correction.to.to_sql())?;
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
            write_column(transaction, table_name, field.column, row_ids, referenced_id)?;
        }
        FieldShape::SetName => relink_item_sets(transaction, row_ids, correction.to.as_text())?,
        FieldShape::RowName => rename_rows(transaction, correction, row_ids)?,
    }
    Ok(())
}

fn maetrim_row_ids_named(transaction: &Transaction, correction: &Correction, row_name: &str) -> Result<Vec<i64>> {
    let kind = correction.kind;
    let maetrim_rows_only = match kind {
        CorrectionKind::Item
        | CorrectionKind::ItemBonus
        | CorrectionKind::ItemEffect
        | CorrectionKind::ItemSocket
        | CorrectionKind::Quest
        | CorrectionKind::Augment
        | CorrectionKind::AugmentBonus => {
            format!(" AND source = '{}'", RowSource::Maetrim.as_str())
        }
        _ => String::new(),
    };
    let family_only = if correction.family.is_some() { " AND family = ?2" } else { " AND ?2 IS NULL" };
    let mut statement = transaction.prepare(&format!(
        "SELECT id FROM {} WHERE {} = ?1{maetrim_rows_only}{family_only} ORDER BY id",
        kind.table_name(),
        kind.name_column()
    ))?;
    let row_ids =
        statement.query_map(params![row_name, correction.family], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    Ok(row_ids)
}

fn current_value(
    transaction: &Transaction,
    correction: &Correction,
    field: &CorrectableField,
    row_id: i64,
) -> Result<CorrectionValue> {
    let table_name = correction.kind.table_name();
    let value_sql = match field.shape {
        FieldShape::Removal => return Ok(CorrectionValue::Integer(0)),
        FieldShape::Integer if correction.kind == CorrectionKind::AugmentBonus => {
            let (stat_id, bonus_type_id) = bonus_key_ids(transaction, correction)?;
            return bonus_value(transaction, BonusLinkTable::AUGMENT, row_id, stat_id, bonus_type_id);
        }
        FieldShape::BonusTypeName => {
            let (stat_id, bonus_type_id) = bonus_key_ids(transaction, correction)?;
            return augment_bonus_type_on_stat(transaction, row_id, stat_id, bonus_type_id);
        }
        FieldShape::BonusAddition => {
            let (stat_id, bonus_type_id) = bonus_key_ids(transaction, correction)?;
            return bonus_value(transaction, BonusLinkTable::of(correction.kind)?, row_id, stat_id, bonus_type_id);
        }
        FieldShape::EffectAddition => {
            let effect_name = correction.to.as_text().context("an add names the effect in to")?;
            let carries_effect = item_effect_names(transaction, row_id)?
                .iter()
                .any(|carried_effect_name| folded_effect_name(carried_effect_name) == folded_effect_name(effect_name));
            return Ok(if carries_effect { correction.to.clone() } else { CorrectionValue::Null });
        }
        FieldShape::SocketAddition => {
            let socket_label = correction.to.as_text().context("an add names the socket label in to")?;
            let carries_label: bool = transaction.query_row(
                "SELECT EXISTS (SELECT 1 FROM item_augment_slots JOIN augment_slot_types ON augment_slot_types.id = item_augment_slots.slot_id
                                 WHERE item_augment_slots.item_id = ?1 AND augment_slot_types.label = ?2)",
                params![row_id, socket_label],
                |r| r.get(0),
            )?;
            return Ok(if carries_label { correction.to.clone() } else { CorrectionValue::Null });
        }
        FieldShape::NamedReference { referenced_table } => format!(
            "SELECT {referenced_table}.name FROM {table_name} LEFT JOIN {referenced_table} ON {referenced_table}.id = {table_name}.{} WHERE {table_name}.id = ?1",
            field.column
        ),
        FieldShape::RowName => format!("SELECT {} FROM {table_name} WHERE id = ?1", correction.kind.name_column()),
        _ => format!("SELECT {} FROM {table_name} WHERE id = ?1", field.column),
    };
    let sql_value: SqlValue = transaction.query_row(&value_sql, params![row_id], |r| r.get(0))?;
    Ok(CorrectionValue::from_sql(sql_value))
}

fn bonus_key_ids(transaction: &Transaction, correction: &Correction) -> Result<(i64, i64)> {
    let (stat_name, bonus_type_name) = correction.bonus_key().context("an augment_bonus correction names its bonus")?;
    let stat_id = id_named(transaction, "stats", stat_name)?
        .with_context(|| format!("stat {stat_name:?} is not in the stats table; use its exact name"))?;
    Ok((stat_id, bonus_type_id_named(transaction, bonus_type_name)?))
}

fn bonus_type_id_named(transaction: &Transaction, bonus_type_name: &str) -> Result<i64> {
    id_named(transaction, "bonus_types", bonus_type_name)?
        .with_context(|| format!("bonus type {bonus_type_name:?} is not in the bonus_types table; use its exact name"))
}

#[derive(Clone, Copy)]
struct BonusLinkTable {
    table_name: &'static str,
    owner_column: &'static str,
}

impl BonusLinkTable {
    const AUGMENT: Self = Self { table_name: "augment_bonuses", owner_column: "augment_id" };
    const ITEM: Self = Self { table_name: "item_bonuses", owner_column: "item_id" };

    fn of(kind: CorrectionKind) -> Result<Self> {
        match kind {
            CorrectionKind::AugmentBonus => Ok(Self::AUGMENT),
            CorrectionKind::ItemBonus => Ok(Self::ITEM),
            _ => bail!("only an augment_bonus or item_bonus correction adds a bonus"),
        }
    }
}

fn bonus_value(
    transaction: &Transaction,
    link_table: BonusLinkTable,
    owner_id: i64,
    stat_id: i64,
    bonus_type_id: i64,
) -> Result<CorrectionValue> {
    let BonusLinkTable { table_name, owner_column } = link_table;
    let bonus_value: Option<Option<i64>> = transaction
        .query_row(
            &format!(
                "SELECT bonuses.value FROM {table_name} JOIN bonuses ON bonuses.id = {table_name}.bonus_id
                  WHERE {table_name}.{owner_column} = ?1 AND bonuses.stat_id = ?2 AND bonuses.bonus_type_id = ?3
                  ORDER BY {table_name}.sort_order LIMIT 1"
            ),
            params![owner_id, stat_id, bonus_type_id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(bonus_value.flatten().map_or(CorrectionValue::Null, CorrectionValue::Integer))
}

fn item_effect_names(transaction: &Transaction, item_id: i64) -> Result<Vec<String>> {
    let mut statement = transaction.prepare(
        "SELECT effects.name FROM item_effects JOIN effects ON effects.id = item_effects.effect_id
          WHERE item_effects.item_id = ?1 ORDER BY item_effects.sort_order",
    )?;
    let effect_names = statement.query_map(params![item_id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    Ok(effect_names)
}

fn augment_bonus_type_on_stat(
    transaction: &Transaction,
    augment_id: i64,
    stat_id: i64,
    expected_bonus_type_id: i64,
) -> Result<CorrectionValue> {
    let mut statement = transaction.prepare(
        "SELECT bonuses.bonus_type_id, bonus_types.name FROM augment_bonuses JOIN bonuses ON bonuses.id = augment_bonuses.bonus_id
           LEFT JOIN bonus_types ON bonus_types.id = bonuses.bonus_type_id
          WHERE augment_bonuses.augment_id = ?1 AND bonuses.stat_id = ?2 ORDER BY augment_bonuses.sort_order",
    )?;
    let bonus_types_on_stat: Vec<(Option<i64>, Option<String>)> = statement
        .query_map(params![augment_id, stat_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let matching_type = bonus_types_on_stat.iter().find(|(type_id, _)| *type_id == Some(expected_bonus_type_id));
    Ok(match matching_type.or(bonus_types_on_stat.first()) {
        Some((_, Some(type_name))) => CorrectionValue::Text(type_name.clone()),
        _ => CorrectionValue::Null,
    })
}

fn repoint_augment_bonuses(
    transaction: &Transaction,
    augment_ids: &[i64],
    stat_id: i64,
    bonus_type_id: i64,
    new_bonus_type_id: i64,
    new_value: Option<i64>,
) -> Result<()> {
    for augment_id in augment_ids {
        let mut statement = transaction.prepare(
            "SELECT augment_bonuses.sort_order, bonuses.value, bonuses.value2 FROM augment_bonuses
               JOIN bonuses ON bonuses.id = augment_bonuses.bonus_id
              WHERE augment_bonuses.augment_id = ?1 AND bonuses.stat_id = ?2 AND bonuses.bonus_type_id = ?3",
        )?;
        let matching_bonuses: Vec<(i64, Option<i64>, Option<i64>)> = statement
            .query_map(params![augment_id, stat_id, bonus_type_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (sort_order, value, second_value) in matching_bonuses {
            let bonus_id =
                ensure_bonus_row(transaction, stat_id, new_bonus_type_id, new_value.or(value), second_value)?;
            transaction.execute(
                "UPDATE augment_bonuses SET bonus_id = ?3 WHERE augment_id = ?1 AND sort_order = ?2",
                params![augment_id, sort_order, bonus_id],
            )?;
        }
    }
    Ok(())
}

fn add_bonus(
    transaction: &Transaction,
    link_table: BonusLinkTable,
    owner_ids: &[i64],
    bonus: &BonusAddition,
) -> Result<()> {
    let stat_id = id_named(transaction, "stats", &bonus.stat)?
        .with_context(|| format!("stat {:?} is not in the stats table; use its exact name", bonus.stat))?;
    let bonus_type_id = bonus_type_id_named(transaction, &bonus.bonus_type)?;
    let bonus_id = ensure_bonus_row(transaction, stat_id, bonus_type_id, Some(bonus.value), None)?;
    let BonusLinkTable { table_name, owner_column } = link_table;
    for owner_id in owner_ids {
        transaction.execute(
            &format!(
                "INSERT INTO {table_name} ({owner_column}, bonus_id, sort_order)
                 SELECT ?1, ?2, COALESCE(MAX(sort_order) + 1, 0) FROM {table_name} WHERE {owner_column} = ?1"
            ),
            params![owner_id, bonus_id],
        )?;
    }
    Ok(())
}

fn add_item_effect(transaction: &Transaction, item_ids: &[i64], effect_name: &str) -> Result<()> {
    let effect_id = match matching_effect_id(transaction, effect_name)? {
        Some(effect_id) => effect_id,
        None => {
            transaction.execute("INSERT INTO effects (name) VALUES (?1)", params![effect_name])?;
            transaction.last_insert_rowid()
        }
    };
    for item_id in item_ids {
        transaction.execute(
            "INSERT INTO item_effects (item_id, effect_id, sort_order)
             SELECT ?1, ?2, COALESCE(MAX(sort_order) + 1, 0) FROM item_effects WHERE item_id = ?1",
            params![item_id, effect_id],
        )?;
    }
    Ok(())
}

fn matching_effect_id(transaction: &Transaction, effect_name: &str) -> Result<Option<i64>> {
    if let Some(effect_id) = id_named(transaction, "effects", effect_name)? {
        return Ok(Some(effect_id));
    }
    let folded_name = folded_effect_name(effect_name);
    let mut statement = transaction.prepare("SELECT id, name FROM effects ORDER BY id")?;
    let effects: Vec<(i64, String)> =
        statement.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
    Ok(effects
        .into_iter()
        .find(|(_, existing_name)| folded_effect_name(existing_name) == folded_name)
        .map(|(effect_id, _)| effect_id))
}

fn ensure_bonus_row(
    transaction: &Transaction,
    stat_id: i64,
    bonus_type_id: i64,
    value: Option<i64>,
    second_value: Option<i64>,
) -> Result<i64> {
    let existing_bonus_id: Option<i64> = transaction
        .query_row(
            "SELECT id FROM bonuses WHERE stat_id = ?1 AND bonus_type_id = ?2
                AND COALESCE(value, -1) = COALESCE(?3, -1) AND COALESCE(value2, -1) = COALESCE(?4, -1)",
            params![stat_id, bonus_type_id, value, second_value],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(bonus_id) = existing_bonus_id {
        return Ok(bonus_id);
    }
    let stat_name: String =
        transaction.query_row("SELECT name FROM stats WHERE id = ?1", params![stat_id], |r| r.get(0))?;
    transaction.execute(
        "INSERT INTO bonuses (name, stat_id, bonus_type_id, value, value2) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![bonus_name(&stat_name, value), stat_id, bonus_type_id, value, second_value],
    )?;
    Ok(transaction.last_insert_rowid())
}

fn add_item_socket(transaction: &Transaction, item_ids: &[i64], socket_label: &str) -> Result<()> {
    let slot_type_id = socket_label_id(transaction, socket_label)?.with_context(|| {
        format!("to {socket_label:?} is not a socket label in Maetrim's files; use one /v1/augment-slot-types lists")
    })?;
    for item_id in item_ids {
        transaction.execute(
            "INSERT INTO item_augment_slots (item_id, sort_order, slot_id)
             SELECT ?1, COALESCE(MAX(sort_order) + 1, 0), ?2 FROM item_augment_slots WHERE item_id = ?1",
            params![item_id, slot_type_id],
        )?;
    }
    Ok(())
}

fn remove_rows(transaction: &Transaction, kind: CorrectionKind, row_ids: &[i64]) -> Result<()> {
    let modifier_source = match kind {
        CorrectionKind::Item => ModifierSource::Item,
        CorrectionKind::Augment => ModifierSource::Augment,
        _ => bail!("only an item or an augment can be removed"),
    };
    for row_id in row_ids {
        transaction.execute(
            "DELETE FROM modifiers WHERE source_kind = ?1 AND source_id = ?2",
            params![modifier_source.as_str(), row_id],
        )?;
        transaction.execute(&format!("DELETE FROM {} WHERE id = ?1", kind.table_name()), params![row_id])?;
    }
    Ok(())
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

fn rename_rows(transaction: &Transaction, correction: &Correction, row_ids: &[i64]) -> Result<()> {
    let new_name = correction.to.as_text().context("a row's name cannot be null")?;
    match correction.kind {
        CorrectionKind::SocketLabel => return rename_socket_label(transaction, row_ids[0], new_name),
        CorrectionKind::Augment => refuse_an_augment_name_taken_in_its_families(transaction, row_ids, new_name)?,
        _ => {
            if id_named(transaction, correction.kind.table_name(), new_name)?.is_some() {
                bail!(
                    "to {new_name:?} is already the name of another {}; a rename cannot merge two rows",
                    correction.kind.as_str()
                );
            }
        }
    }
    write_column(transaction, correction.kind.table_name(), "name", row_ids, SqlValue::Text(new_name.to_string()))?;
    match correction.kind {
        CorrectionKind::Item => {
            write_column(transaction, "items", "wiki_url", row_ids, SqlValue::Text(item_wiki_url(new_name)))?;
        }
        CorrectionKind::SetBonus => {
            for set_naming_table in ["items", "augments"] {
                transaction.execute(
                    &format!("UPDATE {set_naming_table} SET set_bonus = ?2 WHERE set_bonus = ?1"),
                    params![correction.name, new_name],
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn refuse_an_augment_name_taken_in_its_families(
    transaction: &Transaction,
    augment_ids: &[i64],
    new_name: &str,
) -> Result<()> {
    for augment_id in augment_ids {
        let taken_family: Option<String> = transaction
            .query_row(
                "SELECT other.family FROM augments AS renamed JOIN augments AS other ON other.family = renamed.family
                  WHERE renamed.id = ?1 AND other.name = ?2",
                params![augment_id, new_name],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(family) = taken_family {
            bail!("to {new_name:?} is already the name of another augment in family {family:?}; a rename cannot merge two rows");
        }
    }
    Ok(())
}

fn rename_socket_label(transaction: &Transaction, slot_type_id: i64, new_label: &str) -> Result<()> {
    if let Some(merged_slot_type_id) = socket_label_id(transaction, new_label)? {
        for (table_name, column) in
            [("item_augment_slots", "slot_id"), ("crafting_recipes", "slot_id"), ("crafting_recipes", "grants_slot_id")]
        {
            transaction.execute(
                &format!("UPDATE {table_name} SET {column} = ?2 WHERE {column} = ?1"),
                params![slot_type_id, merged_slot_type_id],
            )?;
        }
        transaction.execute(
            "UPDATE OR IGNORE augment_slots SET slot_id = ?2 WHERE slot_id = ?1",
            params![slot_type_id, merged_slot_type_id],
        )?;
        transaction.execute("DELETE FROM augment_slots WHERE slot_id = ?1", params![slot_type_id])?;
        transaction.execute("DELETE FROM augment_slot_types WHERE id = ?1", params![slot_type_id])?;
        return Ok(());
    }
    let (old_label, qualifier): (String, Option<String>) = transaction.query_row(
        "SELECT label, qualifier FROM augment_slot_types WHERE id = ?1",
        params![slot_type_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if qualifier.is_some() {
        bail!("{old_label:?} carries a qualifier; rename it only into a label his files already use");
    }
    let new_variant = match old_label.split_once(": ") {
        Some((label_prefix, _)) => new_label
            .strip_prefix(&format!("{label_prefix}: "))
            .with_context(|| format!("to {new_label:?} must keep the {label_prefix:?} prefix of {old_label:?}"))?,
        None => new_label,
    };
    transaction.execute(
        "UPDATE augment_slot_types SET label = ?2, variant = ?3 WHERE id = ?1",
        params![slot_type_id, new_label, new_variant],
    )?;
    Ok(())
}

fn socket_label_id(transaction: &Transaction, label: &str) -> Result<Option<i64>> {
    Ok(transaction
        .query_row("SELECT id FROM augment_slot_types WHERE label = ?1", params![label], |r| r.get(0))
        .optional()?)
}

fn id_named(transaction: &Transaction, table_name: &str, name: &str) -> Result<Option<i64>> {
    Ok(transaction
        .query_row(&format!("SELECT id FROM {table_name} WHERE name = ?1"), params![name], |r| r.get(0))
        .optional()?)
}
