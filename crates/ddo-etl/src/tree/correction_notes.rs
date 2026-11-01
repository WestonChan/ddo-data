use super::DdobuilderCorrection;
use crate::build::data_correction_value;
use crate::corrections::{Correction, CorrectionValue, FieldShape, NO_SUCH_BONUS};
use anyhow::{bail, ensure, Context, Result};
use ddo_model::enums::{CorrectionKind, EffectOwner};
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

pub fn correction_for_note(note: &DdobuilderCorrection, current_name: &str) -> Result<Correction> {
    let kind = note.kind;
    let mut correction = Correction {
        kind,
        name: current_name.to_string(),
        field: note.field.clone(),
        from: CorrectionValue::Null,
        to: CorrectionValue::Null,
        family: None,
        socket: None,
        option: None,
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
    crate::corrections::set_correction_qualifiers(&mut correction, &note.qualifier)?;
    Ok(correction)
}

fn owner_ids(transaction: &Transaction, kind: CorrectionKind, name: &str, correction: &Correction) -> Result<Vec<i64>> {
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
    ensure!(!ids.is_empty(), "{} {name:?} matches no rows", kind.as_str());
    Ok(ids)
}

fn owner_id(transaction: &Transaction, kind: CorrectionKind, name: &str, correction: &Correction) -> Result<i64> {
    let ids = owner_ids(transaction, kind, name, correction)?;
    ensure!(ids.len() == 1, "{} {name:?} matches {} rows", kind.as_str(), ids.len());
    Ok(ids[0])
}

fn added_bonus_value(
    transaction: &Transaction,
    kind: CorrectionKind,
    current_name: &str,
    correction: &Correction,
) -> Result<Value> {
    let owners = owner_ids(transaction, kind, current_name, correction)?;
    let stat = correction.stat.as_deref().context("added bonus has no stat")?;
    let bonus_type = correction.bonus_type.as_deref().context("added bonus has no bonus type")?;
    let owner_kind = match kind {
        CorrectionKind::ItemBonus => "item",
        CorrectionKind::AugmentBonus => "augment",
        CorrectionKind::FeatBonus => "feat",
        CorrectionKind::SetTierBonus => "set_bonus_tier",
        _ => bail!("{} cannot add a bonus", kind.as_str()),
    };
    let mut statement = transaction.prepare(
        "SELECT DISTINCT bonus.amount FROM owner_bonuses bonus
           JOIN effects stat ON stat.id = bonus.stat_id
           LEFT JOIN effects linked ON linked.id = bonus.via_effect_id
           LEFT JOIN bonus_types bonus_type ON bonus_type.id = bonus.bonus_type_id
          WHERE bonus.owner_kind = ?1 AND bonus.owner_id = ?2 AND (stat.name = ?3 OR linked.name = ?3)
            AND bonus_type.name IS ?4",
    )?;
    let mut owner_amounts = std::collections::BTreeSet::new();
    for owner_id in owners {
        let amounts = statement
            .query_map(params![owner_kind, owner_id, stat, bonus_type], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ensure!(amounts.len() == 1, "added bonus {stat:?} on {current_name:?} has {} values", amounts.len());
        owner_amounts.insert(amounts[0]);
    }
    ensure!(owner_amounts.len() == 1, "added bonus {stat:?} on {current_name:?} disagrees across owners");
    Ok(json!({"stat": stat, "bonus_type": bonus_type, "value": owner_amounts.first()}))
}

pub(super) fn validate_adds_against_splits<'a>(
    groupings: &crate::groupings::Groupings,
    notes: impl Iterator<Item = (&'a std::path::Path, &'a str, &'a DdobuilderCorrection)>,
) -> Result<()> {
    let corrections = notes
        .filter(|(_, _, note)| note.kind.corrects_a_bonus() && matches!(note.field.as_str(), "add" | "lines"))
        .map(|(path, name, note)| {
            let mut correction = correction_for_note(note, name).with_context(|| path.display().to_string())?;
            correction.from = CorrectionValue::from_json(&note.upstream.as_ref().unwrap_or(&Value::Null).to_string())?;
            Ok((path, correction))
        })
        .collect::<Result<Vec<_>>>()?;
    for (path, addition) in corrections.iter().filter(|(_, correction)| correction.field == "add") {
        (|| -> Result<()> {
            let stat = addition.stat.as_deref().context("added bonus has no stat")?;
            let added_stats = groupings.expanded_stat_names(stat)?;
            for (_, split) in corrections.iter().filter(|(_, correction)| {
                correction.kind == addition.kind
                    && correction.name == addition.name
                    && correction.equipped_count == addition.equipped_count
                    && correction.socket == addition.socket
                    && correction.option == addition.option
                    && (correction.family.is_none()
                        || addition.family.is_none()
                        || correction.family == addition.family)
                    && correction.field == "lines"
                    && !correction.is_line_merge()
                    && correction.bonus_type == addition.bonus_type
                    && matches!(correction.from, CorrectionValue::StatSets(_))
            }) {
                let split_stats = split.upstream_line_stats()?.into_iter().flatten().collect();
                ensure!(
                    added_stats.is_disjoint(&split_stats),
                    "{}: add overlaps the same owner's split {}",
                    addition.label(),
                    split.label()
                );
            }
            Ok(())
        })()
        .with_context(|| path.display().to_string())?;
    }
    Ok(())
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
    groupings: &crate::groupings::Groupings,
    socket_options: &crate::build::SocketOptionTargets,
) -> Result<(String, String)> {
    let mut correction = correction_for_note(note, current_name)?;
    let mut upstream = note.upstream.clone().unwrap_or(Value::Null);
    correction.from = CorrectionValue::from_json(&upstream.to_string())?;
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
                    &data_correction_value(transaction, &correction, current_name, groupings, socket_options)?
                        .to_json(),
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
        (CorrectionKind::ItemSocket, "remove") => {
            let current = data_correction_value(transaction, &correction, current_name, groupings, socket_options)?;
            ensure!(
                current.as_text() == Some(NO_SUCH_BONUS),
                "removed socket is still present: {}",
                correction.label()
            );
            json!(1)
        }
        (CorrectionKind::ItemEffect, "effect") => {
            let owner = owner_id(transaction, CorrectionKind::ItemEffect, current_name, &correction)?;
            let count: i64 = transaction.query_row("SELECT COUNT(*) FROM item_effects l JOIN effects e ON e.id=l.effect_id WHERE l.item_id=?1 AND e.name=?2", params![owner,note.qualifier], |row| row.get(0))?;
            ensure!(
                count == 1 || (count == 0 && correction.from.as_text() == Some(NO_SUCH_BONUS)),
                "item {current_name:?} must link destination effect {:?} exactly once, found {count}",
                note.qualifier
            );
            json!(note.qualifier)
        }
        (CorrectionKind::ItemEffect, "add") if note.added_as_name == Some(true) => json!(note.qualifier),
        (CorrectionKind::ItemEffect, "add") => added_item_effect_value(transaction, current_name, note, &correction)?,
        (CorrectionKind::EffectDamage, "add") => added_damage_value(transaction, current_name, &correction)?,
        (_, "add") if correction.correctable_field()?.shape == FieldShape::BonusAddition => {
            added_bonus_value(transaction, correction.kind, current_name, &correction)?
        }
        (_, "remove") if correction.correctable_field()?.shape == FieldShape::BonusRemoval => {
            if correction.from == CorrectionValue::Null {
                json!(1)
            } else {
                Value::Null
            }
        }
        (CorrectionKind::Effect, "remove") => serde_json::from_str::<Value>(
            &data_correction_value(transaction, &correction, current_name, groupings, socket_options)?.to_json(),
        )?,
        (_, "remove") => json!(1),
        _ => serde_json::from_str::<Value>(
            &data_correction_value(transaction, &correction, current_name, groupings, socket_options)?.to_json(),
        )?,
    };
    correction.from = CorrectionValue::from_json(&upstream.to_string())?;
    correction.to = CorrectionValue::from_json(&applied.to_string())?;
    correction.validate_values(correction.correctable_field()?)?;
    Ok((correction.from.to_json(), correction.to.to_json()))
}

pub(super) fn validate_game_bug_corrections(
    transaction: &Transaction,
    groupings: &crate::groupings::Groupings,
    socket_options: &crate::build::SocketOptionTargets,
    layout: &super::ExportLayout,
) -> Result<()> {
    let mut statement = transaction.prepare(&format!(
        "SELECT annotation.*, identity.name AS owner_name FROM
         (SELECT {} FROM game_bug_corrections ORDER BY id) annotation
         LEFT JOIN entity_identity identity ON identity.kind = annotation.owner_kind AND identity.entity_id = annotation.owner_id",
        super::GameBugCorrection::select_columns()
    ))?;
    let annotations = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>("owner_kind")?,
                row.get::<_, i64>("owner_id")?,
                super::GameBugCorrection::from_row(row)?,
                row.get::<_, Option<String>>("owner_name")?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (owner_kind, owner_id, annotation, owner_name) in annotations {
        let path = layout.path(&owner_kind, owner_id)?;
        (|| -> Result<()> {
            ensure!(
                !annotation.skip_description_check
                    || matches!(annotation.field.as_str(), "value" | "bonus_type" | "lines"),
                "skip_description_check requires a served line (value, bonus_type or lines)"
            );
            let target_owner_kind = annotation_owner_kind(annotation.kind);
            ensure!(owner_kind == target_owner_kind, "game-bug kind {} does not address {owner_kind}", annotation.kind);
            let owner_name = owner_name.context("game-bug target owner is absent")?;
            let note = DdobuilderCorrection {
                kind: annotation.kind,
                original_name: None,
                field: annotation.field.clone(),
                added_as_name: None,
                upstream: Some(annotation.intended),
                reason: annotation.reason,
                source: annotation.source,
                read: annotation.read,
                qualifier: annotation.qualifier,
            };
            let mut correction = correction_for_note(&note, &owner_name)?;
            correction.from = CorrectionValue::from_json(&note.upstream.unwrap().to_string())?;
            let field = correction.correctable_field()?;
            let current = data_correction_value(transaction, &correction, &owner_name, groupings, socket_options)?;
            if field.shape == FieldShape::BonusRemoval {
                ensure!(
                    current.as_text() == Some(NO_SUCH_BONUS),
                    "game-bug remove target is present: {}",
                    correction.label()
                );
                correction.to = CorrectionValue::Null;
            } else {
                ensure!(current.as_text() != Some(NO_SUCH_BONUS), "game-bug target is absent: {}", correction.label());
                correction.to = current;
            }
            correction.validate_values(field)?;
            Ok(())
        })()
        .with_context(|| format!("{}: game-bug correction", path.display()))?;
    }
    Ok(())
}

fn annotation_owner_kind(kind: CorrectionKind) -> &'static str {
    match kind {
        CorrectionKind::SetTier | CorrectionKind::SetTierBonus => "set_bonuses",
        kind => kind.table_name(),
    }
}

pub(super) fn validate_verification_requests(
    transaction: &Transaction,
    groupings: &crate::groupings::Groupings,
    socket_options: &crate::build::SocketOptionTargets,
    layout: &super::ExportLayout,
) -> Result<()> {
    let mut statement = transaction.prepare(&format!(
        "SELECT request.*, identity.name AS owner_name, source.url, source.in_game FROM
         (SELECT id, {} FROM verification_requests) request
         LEFT JOIN entity_identity identity ON identity.kind = request.owner_kind AND identity.entity_id = request.owner_id JOIN verification_request_sources source ON source.request_id = request.id AND source.sort_order = 0",
        super::VerificationRequest::select_columns()
    ))?;
    let requests = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>("owner_kind")?,
                row.get::<_, i64>("owner_id")?,
                super::VerificationRequest::from_row(row)?,
                row.get::<_, Option<String>>("owner_name")?,
                crate::AnnotationSource::from_columns(row.get("url")?, row.get("in_game")?)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (owner_kind, owner_id, request, owner_name, source) in requests {
        let path = layout.path(&owner_kind, owner_id)?;
        (|| -> Result<()> {
            let Some(kind) = request.kind else {
                ensure!(request.assumed.is_string(), "a whole-entity request's assumed value must be free text");
                return Ok(());
            };
            ensure!(
                owner_kind == annotation_owner_kind(kind),
                "verification kind {kind} does not address {owner_kind}"
            );
            let owner_name = owner_name.context("verification target owner is absent")?;
            let note = DdobuilderCorrection {
                kind,
                field: request.field,
                qualifier: request.qualifier,
                original_name: None,
                added_as_name: None,
                upstream: Some(request.assumed.clone()),
                reason: request.question,
                source,
                read: request.raised,
            };
            let mut correction = correction_for_note(&note, &owner_name)?;
            let field = correction.correctable_field()?;
            if field.shape == FieldShape::BonusLines {
                ensure!(request.assumed.is_string(), "a line-structure request's assumed value must be free text");
            } else {
                correction.from = CorrectionValue::from_json(&request.assumed.to_string())?;
                correction.to =
                    data_correction_value(transaction, &correction, &owner_name, groupings, socket_options)?;
                ensure!(
                    correction.to.as_text() != Some(NO_SUCH_BONUS),
                    "verification target is absent: {}",
                    correction.label()
                );
                correction.validate_values(field)?;
                ensure!(
                    correction.from == correction.to,
                    "{}: assumed value differs from the file; resolve the request or update assumed",
                    correction.label()
                );
            }
            Ok(())
        })()
        .with_context(|| format!("{}: verification request", path.display()))?;
    }
    Ok(())
}

pub(super) fn rederive_note_lines(db: &rusqlite::Connection) -> Result<()> {
    let mut statement = db.prepare(
        "SELECT 'game_bug' AS note_kind, id, owner_kind, owner_id, kind, field, qualifier, reason, url, in_game, read, intended_value FROM game_bug_corrections
         UNION ALL
         SELECT 'unverified', request.id, request.owner_kind, request.owner_id, request.kind, request.field, request.qualifier, request.question, source.url, source.in_game, request.raised, NULL
         FROM verification_requests request JOIN verification_request_sources source ON source.request_id=request.id AND source.sort_order=0 WHERE request.kind IS NOT NULL"
    )?;
    let notes = statement
        .query_map([], |row| {
            let kind: String = row.get("kind")?;
            Ok((
                row.get::<_, String>("note_kind")?,
                row.get::<_, i64>("id")?,
                row.get::<_, String>("owner_kind")?,
                row.get::<_, i64>("owner_id")?,
                DdobuilderCorrection {
                    kind: CorrectionKind::parse(&kind).ok_or(rusqlite::Error::InvalidQuery)?,
                    field: row.get("field")?,
                    qualifier: row.get("qualifier")?,
                    original_name: None,
                    added_as_name: None,
                    upstream: None,
                    reason: row.get("reason")?,
                    source: crate::AnnotationSource::from_columns(row.get("url")?, row.get("in_game")?)?,
                    read: row.get("read")?,
                },
                row.get::<_, Option<String>>("intended_value")?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if notes.is_empty() {
        return Ok(());
    }
    let groupings = crate::groupings::Groupings::from_database(db)?;
    let mut bonuses = BTreeMap::new();
    for kind in [
        CorrectionKind::ItemBonus,
        CorrectionKind::AugmentBonus,
        CorrectionKind::FeatBonus,
        CorrectionKind::SetTierBonus,
        CorrectionKind::SocketOptionBonus,
    ] {
        for (owner, rows) in crate::build::line_bonuses_by_owner(db, kind)? {
            bonuses.insert((kind.as_str(), owner), rows);
        }
    }
    let mut statement = db.prepare("SELECT set_id, equipped_count, id FROM set_bonus_tiers")?;
    let tiers = statement
        .query_map([], |row| Ok(((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?), row.get::<_, i64>(2)?)))?
        .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
    let layout = super::ExportLayout::from_database(db)?;
    let socket_options = crate::build::SocketOptionTargets::from_database(db)?;
    for (note_kind, note_id, owner_kind, owner_id, note, intended) in notes {
        if note.field == "remove" || owner_kind != annotation_owner_kind(note.kind) {
            continue;
        }
        let (bonus_kind, link_owner_kind) = match note.kind {
            CorrectionKind::ItemBonus | CorrectionKind::ItemEffect => {
                (CorrectionKind::ItemBonus, EffectOwner::Item.as_str())
            }
            CorrectionKind::AugmentBonus => (CorrectionKind::AugmentBonus, EffectOwner::Augment.as_str()),
            CorrectionKind::FeatBonus => (CorrectionKind::FeatBonus, EffectOwner::Feat.as_str()),
            CorrectionKind::SetTierBonus => (CorrectionKind::SetTierBonus, EffectOwner::SetBonusTier.as_str()),
            CorrectionKind::SocketOptionBonus => {
                (CorrectionKind::SocketOptionBonus, EffectOwner::ItemAugmentSlotOption.as_str())
            }
            _ => continue,
        };
        (|| -> Result<()> {
            let mut correction = correction_for_note(&note, "")?;
            let link_owner = if note.kind == CorrectionKind::SetTierBonus {
                let count = correction.equipped_count.context("set annotation needs a piece count")?;
                *tiers.get(&(owner_id, count)).context("set annotation's tier is absent")?
            } else if note.kind == CorrectionKind::SocketOptionBonus {
                socket_options.option_id(&correction, socket_options.item_name(owner_id)?)?
                    .context("socket-option annotation's option is absent")?
            } else { owner_id };
            let target = correction.stat.as_deref().or(correction.effect.as_deref()).context("bonus annotation must name its target")?;
            let expected = if note_kind == "game_bug" && note.field == "lines" {
                correction.from = CorrectionValue::from_json(intended.as_deref().context("game bug has no intended value")?)?;
                correction.upstream_line_stats()?.into_iter().flatten().collect()
            } else {
                groupings.expanded_stat_names(target)?
            };
            let matching = bonuses.get(&(bonus_kind.as_str(), link_owner)).into_iter().flatten().filter(|row| {
                if note.kind == CorrectionKind::ItemEffect { return row.effect_name == target; }
                (expected.contains(&row.stat_name) || row.effect_name == target)
                    && row.bonus_type.as_deref() == correction.bonus_type.as_deref().filter(|name| *name != crate::corrections::NULL_SPELLING)
                    && correction.bonus_value.is_none_or(|amount| row.amount == Some(amount))
            }).collect::<Vec<_>>();
            if note.field == "lines" {
                let actual = matching.iter().map(|row| row.stat_name.clone()).collect::<std::collections::BTreeSet<_>>();
                ensure!(!expected.is_empty() && actual == expected, "verification line target {target:?} is absent or incomplete");
            }
            let orders = matching.iter().map(|row| row.order).collect::<std::collections::BTreeSet<_>>();
            for order in orders {
                db.execute("INSERT INTO note_lines (note_kind,note_id,owner_kind,owner_id,link_sort_order) VALUES (?1,?2,?3,?4,?5)", params![note_kind, note_id, link_owner_kind, link_owner, order as i64])?;
            }
            Ok(())
        })().with_context(|| format!("{}: note target", layout.path(&owner_kind, owner_id).map(|path| path.display().to_string()).unwrap_or(owner_kind)))?;
    }
    Ok(())
}

#[cfg(test)]
mod add_split_tests {
    use super::*;
    use rusqlite::trace::{TraceEvent, TraceEventCodes};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn add_split_validation_uses_no_queries_for_one_or_fifty_corrections() {
        static SELECTS: AtomicUsize = AtomicUsize::new(0);
        fn trace(event: TraceEvent<'_>) {
            if let TraceEvent::Stmt(statement, _) = event {
                if statement.sql().trim_start().to_uppercase().starts_with("SELECT") {
                    SELECTS.fetch_add(1, Ordering::SeqCst);
                }
            }
        }
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::tree::build_from_data(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data"),
            &mut db,
        )
        .unwrap();
        let groupings = crate::groupings::Groupings::from_database(&db).unwrap();
        let owner: String = db
            .query_row("SELECT name FROM augments GROUP BY name HAVING COUNT(*)=1 LIMIT 1", [], |row| row.get(0))
            .unwrap();
        let note: DdobuilderCorrection = toml::from_str("kind = 'augment_bonus'\nfield = 'add'\nqualifier = 'Strength / Quality'\nreason = 'The source supplies this bonus.'\nurl = 'https://ddowiki.com/page/Augment_Slot'\nread = '2026-10-29'\n").unwrap();
        db.trace_v2(TraceEventCodes::SQLITE_TRACE_STMT, Some(trace));
        for count in [0, 1, 50] {
            SELECTS.store(0, Ordering::SeqCst);
            validate_adds_against_splits(
                &groupings,
                (0..count).map(|_| (std::path::Path::new("augment.toml"), owner.as_str(), &note)),
            )
            .unwrap();
            assert_eq!(SELECTS.load(Ordering::SeqCst), 0, "{count} corrections");
        }
        db.trace_v2(TraceEventCodes::empty(), None);
    }
}
