use super::effects::{EffectCache, EffectOwner};
use super::items::item_wiki_url;
use super::wiki::folded_effect_name;
use super::{BuildReport, StaleCorrection, StaleCorrectionCause};
use crate::corrections::{
    BonusAddition, CorrectableField, Correction, CorrectionValue, Corrections, DamageRow, FieldShape, TierAddition,
    NULL_SPELLING,
};
use crate::map::buff::BuffResolver;
use anyhow::{bail, ensure, Context, Result};
use ddo_model::enums::{CorrectionKind, ModifierSource, Provenance};
use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, OptionalExtension, Transaction};
use std::collections::HashMap;

pub(super) fn apply_quest_corrections(
    transaction: &Transaction,
    corrections: &Corrections,
    report: &mut BuildReport,
) -> Result<()> {
    apply_corrections_where(transaction, None, corrections, None, None, report, |correction| {
        correction.kind == CorrectionKind::Quest
    })
}

pub(super) fn apply_non_quest_corrections(
    transaction: &Transaction,
    effects: &mut EffectCache<'_>,
    corrections: &Corrections,
    corrections_applied_while_writing: &[&Correction],
    set_tier_descriptions_by_id: &HashMap<i64, String>,
    buff_resolver: &BuffResolver,
    report: &mut BuildReport,
) -> Result<()> {
    apply_corrections_where(
        transaction,
        Some(effects),
        corrections,
        Some(set_tier_descriptions_by_id),
        Some(buff_resolver),
        report,
        |correction| {
            correction.kind != CorrectionKind::Quest
                && !corrections_applied_while_writing
                    .iter()
                    .any(|applied_correction| std::ptr::eq(*applied_correction, correction))
        },
    )
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
    mut effects: Option<&mut EffectCache<'_>>,
    corrections: &Corrections,
    set_tier_descriptions_by_id: Option<&HashMap<i64, String>>,
    buff_resolver: Option<&BuffResolver>,
    report: &mut BuildReport,
    is_applied_in_this_pass: impl Fn(&Correction) -> bool,
) -> Result<()> {
    let (renames, field_corrections): (Vec<&Correction>, Vec<&Correction>) =
        corrections.entries.iter().filter(|correction| is_applied_in_this_pass(correction)).partition(|correction| {
            correction.correctable_field().is_ok_and(|field| field.shape == FieldShape::RowName)
        });
    for correction in field_corrections.into_iter().chain(renames) {
        apply_correction(
            transaction,
            effects.as_deref_mut(),
            correction,
            set_tier_descriptions_by_id,
            buff_resolver,
            report,
        )
        .with_context(|| format!("correction file {}: {}", correction.file_name, correction.label()))?;
    }
    Ok(())
}

fn apply_correction(
    transaction: &Transaction,
    effects: Option<&mut EffectCache<'_>>,
    correction: &Correction,
    set_tier_descriptions_by_id: Option<&HashMap<i64, String>>,
    buff_resolver: Option<&BuffResolver>,
    report: &mut BuildReport,
) -> Result<()> {
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
        let maetrim_value = current_value(transaction, correction, field, *row_id, set_tier_descriptions_by_id)?;
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
    write_correction(transaction, effects, correction, field, &row_ids, buff_resolver)?;
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
    effects: Option<&mut EffectCache<'_>>,
    correction: &Correction,
    field: &CorrectableField,
    row_ids: &[i64],
    buff_resolver: Option<&BuffResolver>,
) -> Result<()> {
    let table_name = correction.kind.table_name();
    match field.shape {
        FieldShape::EffectBonusConstant => {
            let stat_name = correction.stat.as_deref().context("effect constant names a stat")?;
            let effects = effects.context("effect corrections need the family cache")?;
            for effect_id in row_ids {
                let bonus_row_id = effect_bonus_row_id(transaction, *effect_id, stat_name)?;
                transaction.execute(
                    "UPDATE effect_bonuses SET constant = ?2 WHERE rowid = ?1",
                    params![bonus_row_id, correction.to.to_sql()],
                )?;
                effects.refresh_family(*effect_id)?;
            }
        }
        FieldShape::Integer if correction.kind == CorrectionKind::ItemEffect => {
            let effect_name = correction.effect.as_deref().context("item_effect value names an effect")?;
            let effect_id = id_named(transaction, "effects", effect_name)?
                .with_context(|| format!("unknown effect {effect_name:?}"))?;
            for item_id in row_ids {
                let changed = transaction.execute(
                    "UPDATE item_effects SET value = ?3 WHERE item_id = ?1 AND effect_id = ?2",
                    params![item_id, effect_id, correction.to.to_sql()],
                )?;
                ensure!(changed == 1, "item {item_id} has no unique effect {effect_name:?}");
            }
        }
        FieldShape::Integer if correction.kind.corrects_a_bonus() => {
            let new_value = match correction.to {
                CorrectionValue::Integer(number) => number,
                _ => bail!("a bonus value is an integer"),
            };
            let corrected_bonus = CorrectedBonus::of(transaction, correction)?;
            repoint_bonuses(
                transaction,
                effects.context("bonus corrections need the family cache")?,
                &corrected_bonus,
                row_ids,
                corrected_bonus.bonus_type_id,
                Some(new_value),
            )?;
        }
        FieldShape::BonusTypeName => {
            let new_type_name = correction.to.as_text().context("a bonus type cannot be null")?;
            let new_type_id = bonus_type_id_named(transaction, new_type_name)?;
            let corrected_bonus = CorrectedBonus::of(transaction, correction)?;
            repoint_bonuses(
                transaction,
                effects.context("bonus corrections need the family cache")?,
                &corrected_bonus,
                row_ids,
                Some(new_type_id),
                None,
            )?;
        }
        FieldShape::StatScale | FieldShape::StatRounding => {
            let corrected_bonus = CorrectedBonus::of(transaction, correction)?;
            for owner_id in row_ids {
                let stat_row_ids = corrected_bonus.stat_row_ids_on(transaction, *owner_id)?;
                ensure!(stat_row_ids.len() == 1, "scale or rounding correction must identify one stat row");
                transaction.execute(
                    &format!("UPDATE effect_bonuses SET {} = ?1 WHERE rowid = ?2", field.column),
                    params![correction.to.to_sql(), stat_row_ids[0]],
                )?;
            }
        }
        FieldShape::BonusAddition => {
            let CorrectionValue::Bonus(bonus) = &correction.to else { bail!("an add names the bonus in to") };
            add_bonus(
                transaction,
                effects.context("bonus corrections need the family cache")?,
                BonusLinkTable::of(correction.kind)?,
                row_ids,
                bonus,
            )?;
        }
        FieldShape::BonusRemoval => {
            let corrected_bonus = CorrectedBonus::of(transaction, correction)?;
            remove_bonuses(transaction, &corrected_bonus, row_ids)?;
        }
        FieldShape::BonusDedupe => {
            let corrected_bonus = CorrectedBonus::of(transaction, correction)?;
            let BonusLinkTable { table_name, owner_column } = corrected_bonus.link_table;
            for owner_id in row_ids {
                let mut statement =
                    transaction.prepare(&corrected_bonus.matching_bonuses_sql(&format!("{table_name}.sort_order")))?;
                let mut orders: Vec<i64> = statement
                    .query_map(
                        params![
                            owner_id,
                            corrected_bonus.stat_id,
                            corrected_bonus.bonus_type_id,
                            corrected_bonus.bonus_value
                        ],
                        |row| row.get(0),
                    )?
                    .collect::<rusqlite::Result<_>>()?;
                orders.sort_unstable();
                orders.dedup();
                ensure!(orders.len() == 2, "augment {owner_id} must have two matching bonuses to dedupe");
                transaction.execute(
                    &format!("DELETE FROM {table_name} WHERE {owner_column} = ?1 AND sort_order = ?2"),
                    params![owner_id, orders[1]],
                )?;
            }
        }
        FieldShape::TierAddition => {
            let CorrectionValue::Tier(tier) = &correction.to else { bail!("a tier add names the tier in to") };
            add_set_tier(
                transaction,
                effects.context("tier corrections need the family cache")?,
                row_ids,
                tier,
                buff_resolver.context("set tier resolver")?,
            )?;
        }
        FieldShape::EffectAddition => {
            let effect_name = correction.to.added_effect_name().context("an add names the effect in to")?;
            add_item_effect(
                transaction,
                effects.context("effect corrections need the family cache")?,
                row_ids,
                effect_name,
                correction.to.added_effect_description(),
                correction.to.added_effect_value(),
            )?;
        }
        FieldShape::DamageAddition => {
            let CorrectionValue::Damage(damage) = &correction.to else { bail!("damage add needs a row") };
            let trigger_id = id_named(transaction, "triggers", &damage.trigger)?
                .with_context(|| format!("unknown trigger {:?}", damage.trigger))?;
            let damage_type_id = id_named(transaction, "damage_types", &damage.damage_type)?
                .with_context(|| format!("unknown damage type {:?}", damage.damage_type))?;
            for effect_id in row_ids {
                transaction.execute(
                    "INSERT INTO effect_damage (effect_id, trigger_id, damage_type_id, dice_number, dice_sides,
                     dice_bonus, amount_from, scale, sort_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        effect_id,
                        trigger_id,
                        damage_type_id,
                        damage.dice_number,
                        damage.dice_sides,
                        damage.dice_bonus,
                        damage.amount_from,
                        damage.scale,
                        damage.sort_order
                    ],
                )?;
            }
        }
        FieldShape::DamageRemoval => {
            let CorrectionValue::Damage(damage) = &correction.from else { bail!("damage remove needs a row") };
            for effect_id in row_ids {
                transaction.execute(
                    "DELETE FROM effect_damage WHERE effect_id = ?1 AND sort_order = ?2",
                    params![effect_id, damage.sort_order],
                )?;
            }
        }
        FieldShape::SocketAddition => {
            let socket_label = correction.to.as_text().context("an add names the socket label in to")?;
            add_item_socket(transaction, row_ids, socket_label)?;
        }
        FieldShape::Text if correction.kind == CorrectionKind::SetTier && field.name == "description" => {
            write_set_tier_descriptions(
                transaction,
                effects.context("tier corrections need the family cache")?,
                row_ids,
                correction.to.as_text(),
                buff_resolver.context("set tier resolver")?,
            )?;
        }
        FieldShape::Removal => remove_rows(
            transaction,
            effects.context("owner removals need the family cache")?,
            correction.kind,
            row_ids,
        )?,
        FieldShape::Integer | FieldShape::Flag | FieldShape::Text => {
            write_column(transaction, table_name, field.column, row_ids, correction.to.to_sql())?;
            if correction.kind == CorrectionKind::Effect {
                let effects = effects.context("effect corrections need the family cache")?;
                for family_id in row_ids {
                    effects.refresh_family(*family_id)?;
                }
            }
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
        FieldShape::RowName => {
            rename_rows(transaction, correction, row_ids)?;
            if correction.kind == CorrectionKind::Effect {
                let effects = effects.context("effect corrections need the family cache")?;
                for family_id in row_ids {
                    effects.refresh_family(*family_id)?;
                }
            }
        }
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

fn damage_row_is_present(transaction: &Transaction, effect_id: i64, damage: &DamageRow) -> Result<bool> {
    let present = transaction.query_row(
        "SELECT EXISTS (
           SELECT 1 FROM effect_damage ed JOIN triggers t ON t.id = ed.trigger_id
           JOIN damage_types dt ON dt.id = ed.damage_type_id
          WHERE ed.effect_id = ?1 AND t.name = ?2 AND dt.name = ?3
            AND ed.dice_number = ?4 AND ed.dice_sides = ?5 AND ed.dice_bonus = ?6
            AND ed.amount_from = ?7 AND ed.scale = ?8 AND ed.sort_order = ?9)",
        params![
            effect_id,
            damage.trigger,
            damage.damage_type,
            damage.dice_number,
            damage.dice_sides,
            damage.dice_bonus,
            damage.amount_from,
            damage.scale,
            damage.sort_order
        ],
        |row| row.get(0),
    )?;
    Ok(present)
}

fn current_value(
    transaction: &Transaction,
    correction: &Correction,
    field: &CorrectableField,
    row_id: i64,
    set_tier_descriptions_by_id: Option<&HashMap<i64, String>>,
) -> Result<CorrectionValue> {
    let table_name = correction.kind.table_name();
    let value_sql = match field.shape {
        FieldShape::EffectBonusConstant => {
            let stat_name = correction.stat.as_deref().context("effect constant names a stat")?;
            let bonus_row_id = effect_bonus_row_id(transaction, row_id, stat_name)?;
            let constant: Option<i64> = transaction.query_row(
                "SELECT constant FROM effect_bonuses WHERE rowid = ?1",
                [bonus_row_id],
                |row| row.get(0),
            )?;
            return Ok(constant.map_or(CorrectionValue::Null, CorrectionValue::Integer));
        }
        FieldShape::BonusDedupe => {
            let corrected_bonus = CorrectedBonus::of(transaction, correction)?;
            let mut statement = transaction.prepare(
                &corrected_bonus.matching_bonuses_sql(&format!("{}.sort_order", corrected_bonus.link_table.table_name)),
            )?;
            let mut orders: Vec<i64> = statement
                .query_map(
                    params![row_id, corrected_bonus.stat_id, corrected_bonus.bonus_type_id, corrected_bonus.bonus_value],
                    |row| row.get(0),
                )?
                .collect::<rusqlite::Result<_>>()?;
            orders.sort_unstable();
            orders.dedup();
            return Ok(CorrectionValue::Integer(orders.len() as i64));
        }
        FieldShape::Integer if correction.kind == CorrectionKind::ItemEffect => {
            let effect_name = correction.effect.as_deref().context("item_effect value names an effect")?;
            let mut statement = transaction.prepare(
                "SELECT ie.value FROM item_effects ie JOIN effects e ON e.id = ie.effect_id
                  WHERE ie.item_id = ?1 AND e.name = ?2",
            )?;
            let values: Vec<Option<i64>> = statement
                .query_map(params![row_id, effect_name], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            ensure!(values.len() == 1, "item {row_id} must have one effect {effect_name:?}");
            return Ok(values[0].map_or(CorrectionValue::Null, CorrectionValue::Integer));
        }
        FieldShape::Removal => return Ok(CorrectionValue::Integer(0)),
        FieldShape::Integer | FieldShape::BonusRemoval if correction.kind.corrects_a_bonus() => {
            let corrected_bonus = CorrectedBonus::of(transaction, correction)?;
            let matching_values = corrected_bonus.values_on(transaction, row_id)?;
            if matching_values.is_empty() {
                return Ok(CorrectionValue::Text(NO_SUCH_BONUS.to_string()));
            }
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
        FieldShape::StatScale | FieldShape::StatRounding => {
            let corrected_bonus = CorrectedBonus::of(transaction, correction)?;
            let stat_row_ids = corrected_bonus.stat_row_ids_on(transaction, row_id)?;
            ensure!(stat_row_ids.len() <= 1, "scale or rounding correction must identify one stat row");
            let Some(stat_row_id) = stat_row_ids.first() else { return Ok(CorrectionValue::Null) };
            let current: rusqlite::types::Value = transaction.query_row(
                &format!("SELECT {} FROM effect_bonuses WHERE rowid = ?1", field.column),
                [stat_row_id],
                |row| row.get(0),
            )?;
            return Ok(CorrectionValue::from_sql(current));
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
        FieldShape::DamageAddition | FieldShape::DamageRemoval => {
            let damage = match field.shape {
                FieldShape::DamageAddition => &correction.to,
                FieldShape::DamageRemoval => &correction.from,
                _ => unreachable!(),
            };
            let CorrectionValue::Damage(damage_row) = damage else { bail!("damage correction needs a row") };
            return Ok(if damage_row_is_present(transaction, row_id, damage_row)? {
                damage.clone()
            } else {
                CorrectionValue::Null
            });
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
        FieldShape::Text if correction.kind == CorrectionKind::SetTier && field.name == "description" => {
            if let Some(source_descriptions) = set_tier_descriptions_by_id {
                return Ok(source_descriptions
                    .get(&row_id)
                    .cloned()
                    .map_or(CorrectionValue::Null, CorrectionValue::Text));
            }
            let description = transaction.query_row(
                "SELECT e.verbose_name_template || CASE WHEN e.description_template IS NULL THEN '' ELSE ': ' || e.description_template END
                 FROM set_bonus_tier_effects ste
                 JOIN effects e ON e.id = ste.effect_id
                 WHERE ste.tier_id = ?1 AND NOT EXISTS
                   (SELECT 1 FROM effect_bonuses es WHERE es.effect_id = e.id)
                 ORDER BY ste.sort_order LIMIT 1",
                [row_id],
                |row| row.get::<_, String>(0),
            ).optional()?;
            return Ok(description.map_or(CorrectionValue::Null, CorrectionValue::Text));
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

fn effect_bonus_row_id(transaction: &Transaction, effect_id: i64, stat_name: &str) -> Result<i64> {
    let mut statement = transaction.prepare(
        "SELECT eb.rowid FROM effect_bonuses eb JOIN effects target ON target.id = eb.target_effect_id
          WHERE eb.effect_id = ?1 AND target.name = ?2",
    )?;
    let rows: Vec<i64> =
        statement.query_map(params![effect_id, stat_name], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?;
    ensure!(rows.len() == 1, "effect {effect_id} must grant {stat_name:?} exactly once");
    Ok(rows[0])
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
    const AUGMENT: Self = Self { table_name: "augment_effects", owner_column: "augment_id" };
    const ITEM: Self = Self { table_name: "item_effects", owner_column: "item_id" };
    const SET_TIER: Self = Self { table_name: "set_bonus_tier_effects", owner_column: "tier_id" };

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

struct BonusToRepoint {
    sort_order: i64,
    effect_id: i64,
    link_bonus_type_id: Option<i64>,
    value: Option<i64>,
    second_value: Option<i64>,
    amount_from: i64,
}

impl CorrectedBonus {
    fn of(transaction: &Transaction, correction: &Correction) -> Result<Self> {
        let (stat_name, bonus_type_name) = correction.bonus_key().context("a bonus correction names its bonus")?;
        let stat_id = id_named(transaction, "effects", stat_name)?
            .with_context(|| format!("stat {stat_name:?} is not an effect; use its exact name"))?;
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
        let effective_value = self.effective_value_sql();
        format!(
            "SELECT {selected_columns} FROM {table_name}
              JOIN effects e ON e.id = {table_name}.effect_id
              LEFT JOIN effect_bonuses es ON es.effect_id = e.id
              WHERE {table_name}.{owner_column} = ?1 AND (es.target_effect_id = ?2 OR (e.is_stat = 1 AND e.id = ?2))
                AND COALESCE(es.bonus_type_id, {table_name}.bonus_type_id) IS ?3
                AND (?4 IS NULL OR {effective_value} = ?4)
              ORDER BY {table_name}.sort_order"
        )
    }

    fn effective_value_sql(&self) -> String {
        let table_name = self.link_table.table_name;
        let source = format!(
            "CASE WHEN e.is_stat = 1 THEN COALESCE({table_name}.value, e.default_value)
             WHEN es.amount_from = 0 THEN es.constant WHEN es.amount_from = 1 THEN COALESCE({table_name}.value, e.default_value)
             ELSE COALESCE({table_name}.value2, e.default_value2) END"
        );
        let scaled = ddo_model::effect_amount::rounded_amount_sql(&source, "es.scale", "es.rounding");
        format!("CASE WHEN e.is_stat = 1 THEN {source} ELSE {scaled} END")
    }

    fn stat_row_ids_on(&self, transaction: &Transaction, owner_id: i64) -> Result<Vec<i64>> {
        let mut statement = transaction.prepare(&self.matching_bonuses_sql("es.rowid"))?;
        let stat_row_ids = statement
            .query_map(params![owner_id, self.stat_id, self.bonus_type_id, self.bonus_value], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(stat_row_ids)
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
        let selected_value = self.effective_value_sql();
        let mut statement = transaction.prepare(&self.matching_bonuses_sql(&selected_value))?;
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
        let effective_value = self.effective_value_sql();
        let mut statement = transaction.prepare(&format!(
            "SELECT COALESCE(es.bonus_type_id, {table_name}.bonus_type_id), bonus_types.name FROM {table_name}
               JOIN effects e ON e.id = {table_name}.effect_id
               LEFT JOIN effect_bonuses es ON es.effect_id = e.id
               LEFT JOIN bonus_types ON bonus_types.id = COALESCE(es.bonus_type_id, {table_name}.bonus_type_id)
              WHERE {table_name}.{owner_column} = ?1 AND (es.target_effect_id = ?2 OR (e.is_stat = 1 AND e.id = ?2))
                AND (?3 IS NULL OR {effective_value} = ?3)
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
        "SELECT e.name FROM item_effects ie JOIN effects e ON e.id = ie.effect_id
          WHERE ie.item_id = ?1 AND e.is_stat = 0 AND NOT EXISTS (SELECT 1 FROM effect_bonuses s WHERE s.effect_id = e.id)
          ORDER BY ie.sort_order",
    )?;
    let effect_names = statement.query_map(params![item_id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    Ok(effect_names)
}

fn repoint_bonuses(
    transaction: &Transaction,
    effects: &mut EffectCache<'_>,
    corrected_bonus: &CorrectedBonus,
    owner_ids: &[i64],
    new_bonus_type_id: Option<i64>,
    new_value: Option<i64>,
) -> Result<()> {
    let BonusLinkTable { table_name, owner_column } = corrected_bonus.link_table;
    for owner_id in owner_ids {
        let mut statement = transaction.prepare(&corrected_bonus.matching_bonuses_sql(&format!(
            "{table_name}.sort_order, {table_name}.effect_id, {table_name}.bonus_type_id, {table_name}.value, {table_name}.value2, COALESCE(es.amount_from, 1)"
        )))?;
        let matching_bonuses: Vec<BonusToRepoint> = statement
            .query_map(
                params![owner_id, corrected_bonus.stat_id, corrected_bonus.bonus_type_id, corrected_bonus.bonus_value],
                |r| {
                    Ok(BonusToRepoint {
                        sort_order: r.get(0)?,
                        effect_id: r.get(1)?,
                        link_bonus_type_id: r.get(2)?,
                        value: r.get(3)?,
                        second_value: r.get(4)?,
                        amount_from: r.get(5)?,
                    })
                },
            )?
            .collect::<rusqlite::Result<_>>()?;
        for BonusToRepoint { sort_order, effect_id, link_bonus_type_id, value, second_value, amount_from } in
            matching_bonuses
        {
            if effects.family(effect_id).is_some_and(|family| family.is_stat) {
                transaction.execute(
                    &format!("UPDATE {table_name} SET bonus_type_id = COALESCE(?3, bonus_type_id), value = COALESCE(?4, value) WHERE {owner_column} = ?1 AND sort_order = ?2"),
                    params![owner_id, sort_order, new_bonus_type_id, new_value],
                )?;
                continue;
            }
            let stat_count: i64 = transaction.query_row(
                "SELECT COUNT(*) FROM effect_bonuses WHERE effect_id = ?1",
                [effect_id],
                |row| row.get(0),
            )?;
            if link_bonus_type_id.is_some() && new_bonus_type_id != corrected_bonus.bonus_type_id {
                transaction.execute(
                    &format!(
                        "UPDATE {table_name} SET bonus_type_id = ?3 WHERE {owner_column} = ?1 AND sort_order = ?2"
                    ),
                    params![owner_id, sort_order, new_bonus_type_id],
                )?;
                continue;
            }
            if new_bonus_type_id == corrected_bonus.bonus_type_id
                && new_value.is_some()
                && amount_from > 0
                && stat_count == 1
            {
                let changed_column = if amount_from == 1 { "value" } else { "value2" };
                transaction.execute(
                    &format!(
                        "UPDATE {table_name} SET {changed_column} = ?3 WHERE {owner_column} = ?1 AND sort_order = ?2"
                    ),
                    params![owner_id, sort_order, new_value],
                )?;
                continue;
            }
            let new_effect_id = corrected_effect(
                transaction,
                effects,
                effect_id,
                corrected_bonus.stat_id,
                new_bonus_type_id,
                new_value,
            )?;
            transaction.execute(
                &format!("UPDATE {table_name} SET effect_id = ?3, value = ?4, value2 = ?5 WHERE {owner_column} = ?1 AND sort_order = ?2"),
                params![owner_id, sort_order, new_effect_id, value, second_value],
            )?;
        }
    }
    Ok(())
}

fn add_bonus(
    transaction: &Transaction,
    effects: &mut EffectCache<'_>,
    link_table: BonusLinkTable,
    owner_ids: &[i64],
    bonus: &BonusAddition,
) -> Result<()> {
    let _stat_id = id_named(transaction, "effects", &bonus.stat)?
        .with_context(|| format!("stat {:?} is not an effect; use its exact name", bonus.stat))?;
    let _bonus_type_id = bonus_type_id_named(transaction, &bonus.bonus_type)?;
    let bonus_type_name = &bonus.bonus_type;
    let effect_id = match effects.family_named(&bonus.stat) {
        Some(family) if !family.verbose_name_template.is_empty() => family.id,
        _ => {
            let template = format!("%b1 {} +{{1}}", bonus.stat);
            effects.ensure_family(&bonus.stat, &template, None, 1)?
        }
    };
    let stat = ddo_model::stats::Stat::by_name(&bonus.stat).context("validated stat exists")?;
    let bonus_type = ddo_model::enums::BonusType::parse(bonus_type_name).context("validated bonus type exists")?;
    effects.ensure_stat(effect_id, stat, Some(bonus_type), 1, None, 0)?;
    let BonusLinkTable { table_name, owner_column } = link_table;
    for owner_id in owner_ids {
        let next_order: i64 = transaction.query_row(
            &format!("SELECT COALESCE(MAX(sort_order) + 1, 0) FROM {table_name} WHERE {owner_column} = ?1"),
            [owner_id],
            |row| row.get(0),
        )?;
        let owner = match link_table.table_name {
            "augment_effects" => EffectOwner::Augment,
            "item_effects" => EffectOwner::Item,
            "set_bonus_tier_effects" => EffectOwner::SetBonusTier,
            _ => unreachable!(),
        };
        effects.insert_link(
            owner,
            *owner_id,
            effect_id,
            Some(bonus_type),
            (Some(bonus.value), None),
            next_order as usize,
        )?;
    }
    Ok(())
}

fn corrected_effect(
    transaction: &Transaction,
    effects: &mut EffectCache<'_>,
    original_id: i64,
    corrected_stat_id: i64,
    new_bonus_type_id: Option<i64>,
    new_value: Option<i64>,
) -> Result<i64> {
    struct ExistingStatRow {
        stat_id: i64,
        bonus_type_id: Option<i64>,
        amount_from: i64,
        constant: Option<i64>,
        sort_order: i64,
    }
    let original = effects.family(original_id).context("corrected family is cached")?;
    let (old_name, verbose_name_template, description_template, amount_count) = (
        original.name.clone(),
        original.verbose_name_template.clone(),
        original.description_template.clone(),
        original.amount_count,
    );
    let mut statement = transaction.prepare(
        "SELECT target_effect_id, bonus_type_id, amount_from, constant, sort_order FROM effect_bonuses
         WHERE effect_id = ?1 ORDER BY sort_order",
    )?;
    let stat_rows: Vec<ExistingStatRow> = statement
        .query_map([original_id], |row| {
            Ok(ExistingStatRow {
                stat_id: row.get(0)?,
                bonus_type_id: row.get(1)?,
                amount_from: row.get(2)?,
                constant: row.get(3)?,
                sort_order: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let current_type_id = stat_rows
        .iter()
        .find(|stat_row| stat_row.stat_id == corrected_stat_id)
        .map(|stat_row| stat_row.bonus_type_id)
        .context("corrected stat belongs to its effect")?;
    let stat_name: String = transaction.query_row(
        "SELECT name FROM effects WHERE id = ?1 AND is_stat = 1",
        [corrected_stat_id],
        |row| row.get(0),
    )?;
    let mut corrected_name = old_name.clone();
    let mut corrected_text = verbose_name_template;
    let mut corrected_description = description_template;
    if let Some(new_type_id) = new_bonus_type_id.filter(|type_id| Some(*type_id) != current_type_id) {
        let old_type_name: String = transaction.query_row(
            "SELECT name FROM bonus_types WHERE id = ?1",
            [current_type_id.context("fixed family type exists")?],
            |row| row.get(0),
        )?;
        let new_type_name: String =
            transaction.query_row("SELECT name FROM bonus_types WHERE id = ?1", [new_type_id], |row| row.get(0))?;
        corrected_name = if let Some(remainder) = old_name.strip_prefix(&format!("{old_type_name} ")) {
            format!("{new_type_name} {remainder}")
        } else if old_name == stat_name {
            format!("{new_type_name} {stat_name}")
        } else {
            format!("{new_type_name} {old_name}")
        };
        corrected_text = corrected_text
            .strip_prefix(&old_type_name)
            .map(|suffix| format!("{new_type_name}{suffix}"))
            .or_else(|| corrected_text.strip_prefix(&old_name).map(|suffix| format!("{corrected_name}{suffix}")))
            .unwrap_or(corrected_text);
        corrected_description = corrected_description.map(|template| {
            template
                .strip_prefix(&old_type_name)
                .map_or_else(|| template.clone(), |suffix| format!("{new_type_name}{suffix}"))
        });
    }
    if let Some(new_value) = new_value {
        corrected_name = format!("{corrected_name} ({stat_name} {new_value})");
    }
    let corrected_id =
        effects.ensure_family(&corrected_name, &corrected_text, corrected_description.as_deref(), amount_count)?;
    for ExistingStatRow { stat_id, bonus_type_id, amount_from, constant, sort_order } in stat_rows {
        let stat = ddo_model::stats::STATS.iter().find(|stat| stat.id == stat_id).context("seeded stat exists")?;
        let resolved_type_id =
            if stat_id == corrected_stat_id { new_bonus_type_id.or(bonus_type_id) } else { bonus_type_id };
        let bonus_type = resolved_type_id
            .map(|type_id| {
                ddo_model::enums::BonusType::ALL
                    .iter()
                    .find(|bonus_type| bonus_type.id() == type_id)
                    .copied()
                    .context("seeded bonus type exists")
            })
            .transpose()?;
        let resolved_amount_from = if stat_id == corrected_stat_id && new_value.is_some() { 0 } else { amount_from };
        let resolved_constant = if stat_id == corrected_stat_id { new_value.or(constant) } else { constant };
        effects.ensure_stat(
            corrected_id,
            stat,
            bonus_type,
            resolved_amount_from,
            resolved_constant,
            sort_order as usize,
        )?;
    }
    Ok(corrected_id)
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

fn add_set_tier(
    transaction: &Transaction,
    effects: &mut EffectCache<'_>,
    set_ids: &[i64],
    tier: &TierAddition,
    buff_resolver: &BuffResolver,
) -> Result<()> {
    for set_id in set_ids {
        let set_name: String =
            transaction.query_row("SELECT name FROM set_bonuses WHERE id = ?1", [set_id], |row| row.get(0))?;
        transaction.execute(
            "INSERT INTO set_bonus_tiers (set_id, equipped_count) VALUES (?1, ?2)",
            params![set_id, tier.equipped_count],
        )?;
        let tier_id = transaction.last_insert_rowid();
        let family_name = buff_resolver.set_tier_prose_name(&set_name, tier.equipped_count, &tier.description)?;
        let effect_id = effects.ensure_family(&family_name, &tier.description, None, 0)?;
        effects.insert_link(EffectOwner::SetBonusTier, tier_id, effect_id, None, (None, None), 0)?;
    }
    Ok(())
}

fn write_set_tier_descriptions(
    transaction: &Transaction,
    effects: &mut EffectCache<'_>,
    tier_ids: &[i64],
    description: Option<&str>,
    buff_resolver: &BuffResolver,
) -> Result<()> {
    for tier_id in tier_ids {
        let has_structured: bool = transaction.query_row(
            "SELECT EXISTS (SELECT 1 FROM set_bonus_tier_effects l JOIN effects e ON e.id = l.effect_id
             WHERE l.tier_id = ?1 AND (e.is_stat = 1 OR EXISTS (SELECT 1 FROM effect_bonuses s WHERE s.effect_id = e.id)))",
            [tier_id],
            |row| row.get(0),
        )?;
        if has_structured {
            continue;
        }
        let old_link = transaction
            .query_row(
                "SELECT l.sort_order, l.effect_id, e.name FROM set_bonus_tier_effects l
             JOIN effects e ON e.id = l.effect_id WHERE l.tier_id = ?1 ORDER BY l.sort_order LIMIT 1",
                [tier_id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?)),
            )
            .optional()?;
        if let Some(description) = description {
            let (set_name, equipped_count): (String, i64) = transaction.query_row(
                "SELECT s.name, t.equipped_count FROM set_bonus_tiers t JOIN set_bonuses s ON s.id = t.set_id WHERE t.id = ?1",
                [tier_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            let family_name = buff_resolver.set_tier_prose_name(&set_name, equipped_count, description)?;
            if let Some((_, old_id, old_name)) = &old_link {
                if *old_name == family_name {
                    let owner_count: i64 = transaction.query_row(
                        "SELECT (SELECT COUNT(*) FROM item_effects WHERE effect_id = ?1)
                              + (SELECT COUNT(*) FROM augment_effects WHERE effect_id = ?1)
                              + (SELECT COUNT(*) FROM set_bonus_tier_effects WHERE effect_id = ?1)
                              + (SELECT COUNT(*) FROM feat_effects WHERE effect_id = ?1)
                              + (SELECT COUNT(*) FROM item_augment_slot_option_effects WHERE effect_id = ?1)",
                        [old_id],
                        |row| row.get(0),
                    )?;
                    if owner_count == 1 {
                        effects.replace_family_text(*old_id, description)?;
                        continue;
                    }
                }
            }
            if let Some((old_order, old_id, _)) = &old_link {
                transaction.execute(
                    "DELETE FROM set_bonus_tier_effects WHERE tier_id = ?1 AND sort_order = ?2",
                    params![tier_id, old_order],
                )?;
                effects.delete_unowned(*old_id)?;
            }
            let existing_family =
                effects.family_named(&family_name).map(|family| (family.id, family.verbose_name_template.clone()));
            let effect_id = match existing_family {
                Some((id, text)) if text == description => id,
                Some(_) => anyhow::bail!(
                    "set {set_name:?} tier {equipped_count} prose needs a distinct [names] entry for {description:?}"
                ),
                None => effects.ensure_family(&family_name, description, None, 0)?,
            };
            let next_order = match old_link {
                Some((old_order, _, _)) => old_order,
                None => transaction.query_row(
                    "SELECT COALESCE(MAX(sort_order) + 1, 0) FROM set_bonus_tier_effects WHERE tier_id = ?1",
                    [tier_id],
                    |row| row.get(0),
                )?,
            };
            effects.insert_link(
                EffectOwner::SetBonusTier,
                *tier_id,
                effect_id,
                None,
                (None, None),
                next_order as usize,
            )?;
        } else if let Some((old_order, old_id, _)) = old_link {
            transaction.execute(
                "DELETE FROM set_bonus_tier_effects WHERE tier_id = ?1 AND sort_order = ?2",
                params![tier_id, old_order],
            )?;
            effects.delete_unowned(old_id)?;
        }
    }
    Ok(())
}

fn add_item_effect(
    transaction: &Transaction,
    effects: &mut EffectCache<'_>,
    item_ids: &[i64],
    effect_name: &str,
    description_of_new_effect: Option<&str>,
    amount: Option<i64>,
) -> Result<()> {
    let effect_id = match matching_effect_id(effects, effect_name) {
        Some(effect_id) => effect_id,
        None => {
            let rendered_text = match description_of_new_effect {
                Some(description) => format!("{effect_name}: {description}"),
                None => effect_name.to_string(),
            };
            effects.ensure_text(effect_name, &rendered_text)?
        }
    };
    for item_id in item_ids {
        let sort_order: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(sort_order) + 1, 0) FROM item_effects WHERE item_id = ?1",
            [item_id],
            |row| row.get(0),
        )?;
        effects.insert_link(EffectOwner::Item, *item_id, effect_id, None, (amount, None), sort_order as usize)?;
    }
    Ok(())
}

fn matching_effect_id(effects: &EffectCache<'_>, effect_name: &str) -> Option<i64> {
    if let Some(family) = effects.family_named(effect_name) {
        return Some(family.id);
    }
    let folded_name = folded_effect_name(effect_name);
    effects
        .families()
        .filter(|family| !effects.has_stats(family.id))
        .filter(|family| folded_effect_name(&family.name) == folded_name)
        .map(|family| family.id)
        .min()
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

fn remove_rows(
    transaction: &Transaction,
    effects: &mut EffectCache<'_>,
    kind: CorrectionKind,
    row_ids: &[i64],
) -> Result<()> {
    let modifier_source = match kind {
        CorrectionKind::Item => ModifierSource::Item,
        CorrectionKind::Augment => ModifierSource::Augment,
        CorrectionKind::SetTier => ModifierSource::SetBonusTier,
        _ => bail!("only an item, augment or set tier can be removed"),
    };
    let owner_values = vec!["(?)"; row_ids.len()].join(", ");
    let link_query = match kind {
        CorrectionKind::Item => {
            "SELECT effect_id FROM item_effects WHERE item_id IN (SELECT id FROM removed_owner)
             UNION SELECT oe.effect_id FROM item_augment_slot_option_effects oe
             JOIN item_augment_slot_options o ON o.id = oe.option_id WHERE o.item_id IN (SELECT id FROM removed_owner)"
        }
        CorrectionKind::Augment => {
            "SELECT effect_id FROM augment_effects WHERE augment_id IN (SELECT id FROM removed_owner)"
        }
        CorrectionKind::SetTier => {
            "SELECT effect_id FROM set_bonus_tier_effects WHERE tier_id IN (SELECT id FROM removed_owner)"
        }
        _ => unreachable!(),
    };
    let linked_family_ids: Vec<i64> = transaction
        .prepare(&format!("WITH removed_owner(id) AS (VALUES {owner_values}) {link_query}"))?
        .query_map(params_from_iter(row_ids.iter()), |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
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
    for family_id in linked_family_ids {
        effects.delete_unowned(family_id)?;
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
