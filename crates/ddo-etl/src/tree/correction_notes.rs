use super::DdobuilderCorrection;
use crate::build::data_correction_value;
use crate::corrections::{Correction, CorrectionValue};
use anyhow::{bail, ensure, Context, Result};
use ddo_model::enums::CorrectionKind;
use rusqlite::{params, Transaction};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(super) fn named_row_qualifier(keys: &[(&str, &str)]) -> String {
    keys.iter().map(|(key, name)| format!("{key} {name:?}")).collect::<Vec<_>>().join(", ")
}

pub(super) fn is_selection_rename(field: &str, qualifier: &str) -> bool {
    field == "name" && qualifier.starts_with("selection ")
}

pub(super) fn renamed_qualifier<'a>(qualifier: &str, renames: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    renames
        .into_iter()
        .find_map(|(before, after)| {
            let suffix = qualifier.strip_prefix(before)?;
            (suffix.is_empty() || suffix.starts_with(", ")).then(|| format!("{after}{suffix}"))
        })
        .unwrap_or_else(|| qualifier.to_string())
}

pub(super) fn nested_notes(
    owner_name: &str,
    notes: &[DdobuilderCorrection],
    keys: &[(&str, &str)],
) -> Result<Vec<(String, DdobuilderCorrection)>> {
    let qualifier = named_row_qualifier(keys);
    notes
        .iter()
        .map(|note| {
            ensure!(note.qualifier.is_empty(), "{owner_name:?}: nested notes take their row key from their location");
            let mut note = note.clone();
            note.qualifier.clone_from(&qualifier);
            Ok((owner_name.to_string(), note))
        })
        .collect()
}

fn quoted_name(text: &str, prefix: &str) -> Result<Option<String>> {
    text.strip_prefix(prefix)
        .map(|quoted| serde_json::from_str::<String>(quoted).with_context(|| format!("invalid quoted name {quoted:?}")))
        .transpose()
}

fn correction_for_note(note: &DdobuilderCorrection, current_name: &str) -> Result<Correction> {
    let kind = note.kind;
    let mut correction = Correction {
        kind,
        name: current_name.to_string(),
        field: note.field.clone(),
        from: CorrectionValue::Null,
        to: CorrectionValue::Null,
        family: None,
        selection: None,
        stance: None,
        dc: None,
        effect: None,
        stat: None,
        bonus_type: None,
        bonus_value: None,
        equipped_count: None,
        reason: note.reason.clone(),
        source: note.source.clone(),
        read: note.read.clone(),
        file_name: String::new(),
    };
    for part in note.qualifier.split(", ").filter(|part| !part.is_empty()) {
        if let Some(selection) = quoted_name(part, "selection ")? {
            correction.selection = Some(selection);
        } else if let Some(dc) = quoted_name(part, "dc ")? {
            correction.dc = Some(dc);
        } else if let Some(stance) = quoted_name(part, "stance ")? {
            correction.stance = Some(stance);
        } else if let Some(family) = quoted_name(part, "family ")? {
            correction.family = Some(family);
        } else if let Some(effect) = quoted_name(part, "effect ")? {
            correction.effect = Some(effect);
        } else if let Some(stat) = quoted_name(part, "stat ")? {
            correction.stat = Some(stat);
        } else if let Some(equipped_count) = part.strip_prefix("equipped_count ") {
            correction.equipped_count = Some(equipped_count.parse()?);
        } else if let Some((stat, bonus)) = part.split_once(" / ") {
            let (bonus_type, bonus_value) = match bonus.rsplit_once(" / ") {
                Some((bonus_type, value)) => (bonus_type, Some(value.parse::<i64>()?)),
                None => (bonus, None),
            };
            correction.stat = Some(stat.to_string());
            correction.bonus_type = Some(bonus_type.to_string());
            correction.bonus_value = bonus_value;
        }
    }
    Ok(correction)
}

fn owner_id(transaction: &Transaction, kind: CorrectionKind, name: &str, correction: &Correction) -> Result<i64> {
    let ids = if matches!(kind, CorrectionKind::SetTier | CorrectionKind::SetTierBonus) {
        let count = correction.equipped_count.context("set correction has no piece count")?;
        let mut statement = transaction.prepare(
            "SELECT tier.id FROM set_bonus_tiers tier JOIN set_bonuses set_bonus ON set_bonus.id = tier.set_id
              WHERE set_bonus.name = ?1 AND tier.equipped_count = ?2",
        )?;
        let rows = statement
            .query_map(params![name, count], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    } else if matches!(kind, CorrectionKind::Augment | CorrectionKind::AugmentBonus) {
        let mut statement =
            transaction.prepare("SELECT id FROM augments WHERE name = ?1 AND (?2 IS NULL OR family = ?2)")?;
        let rows = statement
            .query_map(params![name, correction.family], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    } else {
        let table = kind.table_name();
        let name_column = kind.name_column();
        let mut statement = transaction.prepare(&format!("SELECT id FROM {table} WHERE {name_column} = ?1"))?;
        let rows = statement.query_map([name], |row| row.get::<_, i64>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    ensure!(ids.len() == 1, "{} {name:?} matches {} rows", kind.as_str(), ids.len());
    Ok(ids[0])
}

fn added_bonus_value(
    transaction: &Transaction,
    kind: CorrectionKind,
    current_name: &str,
    correction: &Correction,
) -> Result<Value> {
    let owner_id = owner_id(transaction, kind, current_name, correction)?;
    let stat = correction.stat.as_deref().context("added bonus has no stat")?;
    let bonus_type = correction.bonus_type.as_deref().context("added bonus has no bonus type")?;
    let owner_kind = match kind {
        CorrectionKind::ItemBonus => "item",
        CorrectionKind::AugmentBonus => "augment",
        CorrectionKind::SetTierBonus => "set_bonus_tier",
        _ => bail!("{} cannot add a bonus", kind.as_str()),
    };
    let mut statement = transaction.prepare(
        "SELECT DISTINCT bonus.amount FROM owner_bonuses bonus
           JOIN effects stat ON stat.id = bonus.stat_id
           LEFT JOIN bonus_types bonus_type ON bonus_type.id = bonus.bonus_type_id
          WHERE bonus.owner_kind = ?1 AND bonus.owner_id = ?2 AND stat.name = ?3
            AND bonus_type.name IS ?4",
    )?;
    let amounts = statement
        .query_map(params![owner_kind, owner_id, stat, bonus_type], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(amounts.len() == 1, "added bonus {stat:?} on {current_name:?} has {} values", amounts.len());
    Ok(json!({"stat": stat, "bonus_type": bonus_type, "value": amounts[0]}))
}

fn added_item_effect_value(
    transaction: &Transaction,
    current_name: &str,
    note: &DdobuilderCorrection,
    correction: &Correction,
) -> Result<Value> {
    let item_id = owner_id(transaction, CorrectionKind::ItemEffect, current_name, correction)?;
    let effect_name = note.qualifier.as_str();
    let (verbose, description, value): (String, Option<String>, Option<i64>) = transaction.query_row(
        "SELECT effect.verbose_name_template, effect.description_template, link.value
           FROM item_effects link JOIN effects effect ON effect.id = link.effect_id
          WHERE link.item_id = ?1 AND effect.name = ?2",
        params![item_id, effect_name],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let description = description.unwrap_or_default();
    let description = if let Some((_, suffix)) = verbose.split_once(" — ") {
        format!("{suffix}: {description}")
    } else {
        description
    };
    let mut added = json!({"name": effect_name, "description": description});
    if let Some(value) = value {
        added["value"] = json!(value);
    }
    Ok(added)
}

fn added_damage_value(transaction: &Transaction, current_name: &str, correction: &Correction) -> Result<Value> {
    let effect_id = owner_id(transaction, CorrectionKind::EffectDamage, current_name, correction)?;
    let mut statement = transaction.prepare(
        "SELECT trigger.name, damage_type.name, damage.dice_number, damage.dice_sides,
                damage.dice_bonus, damage.amount_from, damage.scale, damage.sort_order
           FROM effect_damage damage JOIN triggers trigger ON trigger.id = damage.trigger_id
           JOIN damage_types damage_type ON damage_type.id = damage.damage_type_id
          WHERE damage.effect_id = ?1",
    )?;
    let rows = statement
        .query_map([effect_id], |row| {
            Ok(json!({
                "trigger": row.get::<_, String>(0)?,
                "damage_type": row.get::<_, String>(1)?,
                "dice_number": row.get::<_, i64>(2)?,
                "dice_sides": row.get::<_, i64>(3)?,
                "dice_bonus": row.get::<_, i64>(4)?,
                "amount_from": row.get::<_, i64>(5)?,
                "scale": row.get::<_, f64>(6)?,
                "sort_order": row.get::<_, i64>(7)?,
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(rows.len() == 1, "effect {current_name:?} has {} damage rows", rows.len());
    Ok(rows.into_iter().next().unwrap())
}

pub(super) fn correction_values(
    transaction: &Transaction,
    current_name: &str,
    note: &DdobuilderCorrection,
    tier_descriptions: &BTreeMap<(String, i64), String>,
) -> Result<(String, String)> {
    let mut correction = correction_for_note(note, current_name)?;
    let mut upstream = note.upstream.clone().unwrap_or(Value::Null);
    if note.field == "is_legacy" {
        if let Some(value) = upstream.as_bool() {
            upstream = json!(i64::from(value));
        }
    }
    let applied = match (correction.kind, note.field.as_str()) {
        (CorrectionKind::Item, "set_bonus") => {
            let set_name: String = transaction.query_row(
                "SELECT set_bonus.name FROM set_bonus_items piece
                 JOIN items item ON item.id = piece.item_id
                 JOIN set_bonuses set_bonus ON set_bonus.id = piece.set_id
                 WHERE item.name = ?1 ORDER BY set_bonus.name LIMIT 1",
                [current_name],
                |row| row.get(0),
            )?;
            json!(set_name)
        }
        (CorrectionKind::ItemBonus, "bonus_type") if correction.bonus_value.is_some() => {
            let stat_name = correction.stat.as_deref().context("bonus correction has no stat")?;
            let amount = correction.bonus_value.context("bonus correction has no amount")?;
            let mut statement = transaction.prepare(
                "SELECT DISTINCT bonus_type.name FROM item_augment_slot_options option
                 JOIN items item ON item.id = option.item_id
                 JOIN item_augment_slot_option_effects link ON link.option_id = option.id
                 JOIN effects effect ON effect.id = link.effect_id
                 LEFT JOIN bonus_types bonus_type ON bonus_type.id = link.bonus_type_id
                 WHERE item.name = ?1 AND effect.name = ?2 AND link.value = ?3",
            )?;
            let values = statement
                .query_map(params![current_name, stat_name, amount], |row| row.get::<_, Option<String>>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            ensure!(
                values.len() <= 1,
                "bonus {stat_name:?} {amount} on {current_name:?} has {} slot options",
                values.len()
            );
            if let Some(value) = values.first() {
                json!(value)
            } else {
                serde_json::from_str::<Value>(
                    &data_correction_value(transaction, &correction, current_name)?.to_json(),
                )?
            }
        }
        (CorrectionKind::SetTier, "description") => {
            let piece_count = correction.equipped_count.context("tier description has no piece count")?;
            let description = tier_descriptions
                .get(&(current_name.to_string(), piece_count))
                .with_context(|| format!("set {current_name:?} has no description for {piece_count} pieces"))?;
            json!(description)
        }
        (CorrectionKind::ItemSocket, "add") => json!(note.qualifier),
        (CorrectionKind::ItemEffect, "add") if note.added_as_name == Some(true) => json!(note.qualifier),
        (CorrectionKind::ItemEffect, "add") => added_item_effect_value(transaction, current_name, note, &correction)?,
        (CorrectionKind::EffectDamage, "add") => added_damage_value(transaction, current_name, &correction)?,
        (CorrectionKind::ItemBonus | CorrectionKind::AugmentBonus | CorrectionKind::SetTierBonus, "add") => {
            added_bonus_value(transaction, correction.kind, current_name, &correction)?
        }
        (CorrectionKind::SetTierBonus, "remove") => Value::Null,
        (_, "remove") => json!(1),
        _ => serde_json::from_str::<Value>(&data_correction_value(transaction, &correction, current_name)?.to_json())?,
    };
    correction.from = CorrectionValue::from_json(&upstream.to_string())?;
    correction.to = CorrectionValue::from_json(&applied.to_string())?;
    correction.validate_values(correction.correctable_field()?)?;
    Ok((correction.from.to_json(), correction.to.to_json()))
}
