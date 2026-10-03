use super::items::item_wiki_url;
use super::wiki::folded_effect_name;
use super::{bonus_name, BuildReport, StaleCorrection, StaleCorrectionCause};
use crate::corrections::{
    BonusAddition, CorrectableField, Correction, CorrectionValue, Corrections, FieldShape, TierAddition, NULL_SPELLING,
};
use anyhow::{bail, Context, Result};
use ddo_model::enums::{CorrectionKind, ModifierSource, Provenance};
use rusqlite::types::Value as SqlValue;
use rusqlite::{params, OptionalExtension, Transaction};

pub(super) fn apply_quest_corrections(
    transaction: &Transaction,
    corrections: &Corrections,
    report: &mut BuildReport,
) -> Result<()> {
    apply_corrections_where(transaction, corrections, report, |correction| correction.kind == CorrectionKind::Quest)
}

pub(super) fn apply_non_quest_corrections(
    transaction: &Transaction,
    corrections: &Corrections,
    corrections_applied_while_writing: &[&Correction],
    report: &mut BuildReport,
) -> Result<()> {
    apply_corrections_where(transaction, corrections, report, |correction| {
        correction.kind != CorrectionKind::Quest
            && !corrections_applied_while_writing
                .iter()
                .any(|applied_correction| std::ptr::eq(*applied_correction, correction))
    })
}

pub(super) fn record_corrections_applied_while_writing(
    transaction: &Transaction,
    corrections_applied_while_writing: &[&Correction],
    report: &mut BuildReport,
) -> Result<()> {
    for correction in corrections_applied_while_writing {
        record_applied_correction(transaction, correction, report)?;
    }
    Ok(())
}

fn apply_corrections_where(
    transaction: &Transaction,
    corrections: &Corrections,
    report: &mut BuildReport,
    is_applied_in_this_pass: impl Fn(&Correction) -> bool,
) -> Result<()> {
    let (renames, field_corrections): (Vec<&Correction>, Vec<&Correction>) =
        corrections.entries.iter().filter(|correction| is_applied_in_this_pass(correction)).partition(|correction| {
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
        if matches!(correction.kind, CorrectionKind::SetTier | CorrectionKind::SetTierBonus) {
            if id_named(transaction, "set_bonuses", &correction.name)?.is_none() {
                bail!(
                    "no set in Maetrim's files is named {:?} (matched against set_bonuses.name); use his exact name",
                    correction.name
                );
            }
            bail!(
                "set {:?} has no tier with equipped_count {} in Maetrim's files",
                correction.name,
                correction.equipped_count.context("a tier correction requires equipped_count")?
            );
        }
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
    record_applied_correction(transaction, correction, report)
}

fn record_applied_correction(
    transaction: &Transaction,
    correction: &Correction,
    report: &mut BuildReport,
) -> Result<()> {
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
        FieldShape::Integer if correction.kind.corrects_a_bonus() => {
            let new_value = match correction.to {
                CorrectionValue::Integer(number) => number,
                _ => bail!("a bonus value is an integer"),
            };
            let corrected_bonus = CorrectedBonus::of(transaction, correction)?;
            repoint_bonuses(transaction, &corrected_bonus, row_ids, corrected_bonus.bonus_type_id, Some(new_value))?;
        }
        FieldShape::BonusTypeName => {
            let new_type_name = correction.to.as_text().context("a bonus type cannot be null")?;
            let new_type_id = bonus_type_id_named(transaction, new_type_name)?;
            let corrected_bonus = CorrectedBonus::of(transaction, correction)?;
            repoint_bonuses(transaction, &corrected_bonus, row_ids, Some(new_type_id), None)?;
        }
        FieldShape::BonusAddition => {
            let CorrectionValue::Bonus(bonus) = &correction.to else { bail!("an add names the bonus in to") };
            add_bonus(transaction, BonusLinkTable::of(correction.kind)?, row_ids, bonus)?;
        }
        FieldShape::BonusRemoval => {
            let corrected_bonus = CorrectedBonus::of(transaction, correction)?;
            remove_bonuses(transaction, &corrected_bonus, row_ids)?;
        }
        FieldShape::TierAddition => {
            let CorrectionValue::Tier(tier) = &correction.to else { bail!("a tier add names the tier in to") };
            add_set_tier(transaction, row_ids, tier)?;
        }
        FieldShape::EffectAddition => {
            let effect_name = correction.to.added_effect_name().context("an add names the effect in to")?;
            add_item_effect(transaction, row_ids, effect_name, correction.to.added_effect_description())?;
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
    if matches!(kind, CorrectionKind::SetTier | CorrectionKind::SetTierBonus) {
        if correction.correctable_field()?.shape == FieldShape::TierAddition {
            return Ok(transaction
                .prepare("SELECT id FROM set_bonuses WHERE name = ?1")?
                .query_map(params![row_name], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?);
        }
        return Ok(transaction
            .prepare(
                "SELECT set_bonus_tiers.id FROM set_bonuses JOIN set_bonus_tiers ON set_bonus_tiers.set_id = set_bonuses.id
                 WHERE set_bonuses.name = ?1 AND set_bonus_tiers.equipped_count = ?2",
            )?
            .query_map(params![row_name, correction.equipped_count], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?);
    }
    let maetrim_rows_only = match kind {
        CorrectionKind::Item
        | CorrectionKind::ItemBonus
        | CorrectionKind::ItemEffect
        | CorrectionKind::ItemSocket
        | CorrectionKind::Quest
        | CorrectionKind::Augment
        | CorrectionKind::AugmentBonus => {
            format!(" AND provenance = '{}'", Provenance::Maetrim.as_str())
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
        FieldShape::Integer | FieldShape::BonusRemoval if correction.kind.corrects_a_bonus() => {
            let matching_values = CorrectedBonus::of(transaction, correction)?.values_on(transaction, row_id)?;
            if field.shape == FieldShape::BonusRemoval && correction.bonus_value.is_none() && matching_values.len() > 1 {
                let value_texts: Vec<String> = matching_values.iter().map(CorrectionValue::to_json).collect();
                bail!(
                    "bonus removal matches multiple links with values [{}]; specify bonus_value",
                    value_texts.join(", ")
                );
            }
            return Ok(matching_values.into_iter().next().unwrap_or(CorrectionValue::Null));
        }
        FieldShape::BonusTypeName => {
            return CorrectedBonus::of(transaction, correction)?.bonus_type_on(transaction, row_id);
        }
        FieldShape::BonusAddition => {
            return Ok(if CorrectedBonus::of(transaction, correction)?.is_present_on(transaction, row_id)? {
                correction.to.clone()
            } else {
                CorrectionValue::Null
            });
        }
        FieldShape::TierAddition => {
            let existing_tier_count: i64 = transaction.query_row(
                "SELECT COUNT(*) FROM set_bonus_tiers WHERE set_id = ?1 AND equipped_count = ?2",
                params![row_id, correction.equipped_count],
                |row| row.get(0),
            )?;
            return Ok(if existing_tier_count == 0 { CorrectionValue::Null } else { correction.to.clone() });
        }
        FieldShape::EffectAddition => {
            let effect_name = correction.to.added_effect_name().context("an add names the effect in to")?;
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
    const SET_TIER: Self = Self { table_name: "set_bonus_tier_bonuses", owner_column: "tier_id" };

    fn of(kind: CorrectionKind) -> Result<Self> {
        match kind {
            CorrectionKind::AugmentBonus => Ok(Self::AUGMENT),
            CorrectionKind::ItemBonus => Ok(Self::ITEM),
            CorrectionKind::SetTierBonus => Ok(Self::SET_TIER),
            _ => bail!("only an augment_bonus, item_bonus or set_tier_bonus correction names a bonus"),
        }
    }
}

struct CorrectedBonus {
    link_table: BonusLinkTable,
    stat_id: i64,
    bonus_type_id: Option<i64>,
    bonus_value: Option<i64>,
}

impl CorrectedBonus {
    fn of(transaction: &Transaction, correction: &Correction) -> Result<Self> {
        let (stat_name, bonus_type_name) = correction.bonus_key().context("a bonus correction names its bonus")?;
        let stat_id = id_named(transaction, "stats", stat_name)?
            .with_context(|| format!("stat {stat_name:?} is not in the stats table; use its exact name"))?;
        let bonus_type_id = match bonus_type_name {
            NULL_SPELLING => None,
            _ => Some(bonus_type_id_named(transaction, bonus_type_name)?),
        };
        Ok(Self {
            link_table: BonusLinkTable::of(correction.kind)?,
            stat_id,
            bonus_type_id,
            bonus_value: correction.bonus_value,
        })
    }

    fn matching_bonuses_sql(&self, selected_columns: &str) -> String {
        let BonusLinkTable { table_name, owner_column } = self.link_table;
        format!(
            "SELECT {selected_columns} FROM {table_name} JOIN bonuses ON bonuses.id = {table_name}.bonus_id
              WHERE {table_name}.{owner_column} = ?1 AND bonuses.stat_id = ?2 AND bonuses.bonus_type_id IS ?3
                AND (?4 IS NULL OR bonuses.value = ?4)
              ORDER BY {table_name}.sort_order"
        )
    }

    fn is_present_on(&self, transaction: &Transaction, owner_id: i64) -> Result<bool> {
        Ok(transaction
            .query_row(
                &format!("{} LIMIT 1", self.matching_bonuses_sql("1")),
                params![owner_id, self.stat_id, self.bonus_type_id, self.bonus_value],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .is_some())
    }

    fn values_on(&self, transaction: &Transaction, owner_id: i64) -> Result<Vec<CorrectionValue>> {
        let mut statement = transaction.prepare(&self.matching_bonuses_sql("bonuses.value"))?;
        let bonus_values = statement
            .query_map(params![owner_id, self.stat_id, self.bonus_type_id, self.bonus_value], |row| {
                let bonus_value: Option<i64> = row.get(0)?;
                Ok(bonus_value.map_or(CorrectionValue::Null, CorrectionValue::Integer))
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(bonus_values)
    }

    fn bonus_type_on(&self, transaction: &Transaction, owner_id: i64) -> Result<CorrectionValue> {
        let BonusLinkTable { table_name, owner_column } = self.link_table;
        let mut statement = transaction.prepare(&format!(
            "SELECT bonuses.bonus_type_id, bonus_types.name FROM {table_name} JOIN bonuses ON bonuses.id = {table_name}.bonus_id
               LEFT JOIN bonus_types ON bonus_types.id = bonuses.bonus_type_id
              WHERE {table_name}.{owner_column} = ?1 AND bonuses.stat_id = ?2 AND (?3 IS NULL OR bonuses.value = ?3)
              ORDER BY {table_name}.sort_order"
        ))?;
        let bonus_types_on_stat: Vec<(Option<i64>, Option<String>)> = statement
            .query_map(params![owner_id, self.stat_id, self.bonus_value], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let matching_type = bonus_types_on_stat.iter().find(|(type_id, _)| *type_id == self.bonus_type_id);
        Ok(match matching_type.or(bonus_types_on_stat.first()) {
            Some((_, Some(type_name))) => CorrectionValue::Text(type_name.clone()),
            Some((_, None)) => CorrectionValue::Null,
            None => CorrectionValue::Text(NO_SUCH_BONUS.to_string()),
        })
    }
}

const NO_SUCH_BONUS: &str = "no such bonus";

fn item_effect_names(transaction: &Transaction, item_id: i64) -> Result<Vec<String>> {
    let mut statement = transaction.prepare(
        "SELECT effects.name FROM item_effects JOIN effects ON effects.id = item_effects.effect_id
          WHERE item_effects.item_id = ?1 ORDER BY item_effects.sort_order",
    )?;
    let effect_names = statement.query_map(params![item_id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    Ok(effect_names)
}

fn repoint_bonuses(
    transaction: &Transaction,
    corrected_bonus: &CorrectedBonus,
    owner_ids: &[i64],
    new_bonus_type_id: Option<i64>,
    new_value: Option<i64>,
) -> Result<()> {
    let BonusLinkTable { table_name, owner_column } = corrected_bonus.link_table;
    for owner_id in owner_ids {
        let mut statement = transaction.prepare(
            &corrected_bonus.matching_bonuses_sql(&format!("{table_name}.sort_order, bonuses.value, bonuses.value2")),
        )?;
        let matching_bonuses: Vec<(i64, Option<i64>, Option<i64>)> = statement
            .query_map(
                params![owner_id, corrected_bonus.stat_id, corrected_bonus.bonus_type_id, corrected_bonus.bonus_value],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?
            .collect::<rusqlite::Result<_>>()?;
        for (sort_order, value, second_value) in matching_bonuses {
            let bonus_type_id = new_bonus_type_id
                .or(corrected_bonus.bonus_type_id)
                .context("a corrected bonus keeps or gains a bonus type")?;
            let bonus_id = ensure_bonus_row(
                transaction,
                corrected_bonus.stat_id,
                bonus_type_id,
                new_value.or(value),
                second_value,
            )?;
            transaction.execute(
                &format!("UPDATE {table_name} SET bonus_id = ?3 WHERE {owner_column} = ?1 AND sort_order = ?2"),
                params![owner_id, sort_order, bonus_id],
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

fn remove_bonuses(transaction: &Transaction, corrected_bonus: &CorrectedBonus, owner_ids: &[i64]) -> Result<()> {
    let BonusLinkTable { table_name, owner_column } = corrected_bonus.link_table;
    for owner_id in owner_ids {
        transaction.execute(
            &format!(
                "DELETE FROM {table_name} WHERE {owner_column} = ?1 AND sort_order IN ({})",
                corrected_bonus.matching_bonuses_sql(&format!("{table_name}.sort_order"))
            ),
            params![owner_id, corrected_bonus.stat_id, corrected_bonus.bonus_type_id, corrected_bonus.bonus_value],
        )?;
    }
    Ok(())
}

fn add_set_tier(transaction: &Transaction, set_ids: &[i64], tier: &TierAddition) -> Result<()> {
    for set_id in set_ids {
        transaction.execute(
            "INSERT INTO set_bonus_tiers (set_id, equipped_count, description) VALUES (?1, ?2, ?3)",
            params![set_id, tier.equipped_count, tier.description],
        )?;
    }
    Ok(())
}

fn add_item_effect(
    transaction: &Transaction,
    item_ids: &[i64],
    effect_name: &str,
    description_of_new_effect: Option<&str>,
) -> Result<()> {
    let effect_id = match matching_effect_id(transaction, effect_name)? {
        Some(effect_id) => effect_id,
        None => {
            transaction.execute(
                "INSERT INTO effects (name, description) VALUES (?1, ?2)",
                params![effect_name, description_of_new_effect],
            )?;
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
        CorrectionKind::SetTier => ModifierSource::SetBonusTier,
        _ => bail!("only an item, augment or set tier can be removed"),
    };
    for row_id in row_ids {
        if kind == CorrectionKind::Item {
            transaction.execute(
                "DELETE FROM modifiers WHERE source_kind = ?1
                    AND source_id IN (SELECT id FROM item_augment_slot_options WHERE item_id = ?2)",
                params![ModifierSource::ItemAugmentSlotOption.as_str(), row_id],
            )?;
        }
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
        for (table_name, column) in [
            ("item_augment_slots", "slot_id"),
            ("item_augment_slot_option_grants", "slot_id"),
            ("crafting_recipes", "slot_id"),
            ("crafting_recipes", "grants_slot_id"),
        ] {
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
