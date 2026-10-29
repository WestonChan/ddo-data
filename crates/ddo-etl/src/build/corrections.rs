use super::effects::{EffectCache, EffectOwner};
use super::items::item_wiki_url;
use super::wiki::folded_effect_name;
use super::{BuildReport, StaleCorrection, StaleCorrectionCause};
use crate::corrections::{
    BonusAddition, CorrectableField, Correction, CorrectionValue, Corrections, DamageRow, FieldShape, TierAddition,
    NO_SUCH_BONUS, NULL_SPELLING,
};
use crate::map::buff::BuffResolver;
use anyhow::{bail, ensure, Context, Result};
use ddo_model::enums::{CorrectionKind, ModifierSource, Provenance};
use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, OptionalExtension, Transaction};
use std::collections::{HashMap, HashSet};

pub(super) struct CorrectionContext<'a> {
    pub set_tier_descriptions_by_id: &'a HashMap<i64, String>,
    pub buff_resolver: &'a BuffResolver,
    pub groupings: &'a crate::groupings::Groupings,
    pub grouping_answers: &'a HashSet<i64>,
    pub socket_options: Option<&'a SocketOptionTargets>,
}

pub(super) fn apply_quest_corrections(
    transaction: &Transaction,
    corrections: &Corrections,
    report: &mut BuildReport,
) -> Result<()> {
    apply_corrections_where(transaction, None, corrections, None, report, |correction| {
        correction.kind == CorrectionKind::Quest
    })
}

pub(super) fn apply_non_quest_corrections(
    transaction: &Transaction,
    effects: &mut EffectCache<'_>,
    corrections: &Corrections,
    corrections_applied_while_writing: &[&Correction],
    context: &CorrectionContext<'_>,
    report: &mut BuildReport,
) -> Result<()> {
    apply_corrections_where(transaction, Some(effects), corrections, Some(context), report, |correction| {
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
    mut effects: Option<&mut EffectCache<'_>>,
    corrections: &Corrections,
    context: Option<&CorrectionContext<'_>>,
    report: &mut BuildReport,
    is_applied_in_this_pass: impl Fn(&Correction) -> bool,
) -> Result<()> {
    let (mut renames, field_corrections): (Vec<&Correction>, Vec<&Correction>) =
        corrections.entries.iter().filter(|correction| is_applied_in_this_pass(correction)).partition(|correction| {
            correction.correctable_field().is_ok_and(|field| field.shape == FieldShape::RowName)
        });
    renames.sort_by_key(|correction| correction.selection.is_none());
    for correction in field_corrections.into_iter().chain(renames) {
        apply_correction(transaction, effects.as_deref_mut(), correction, context, report)
            .with_context(|| format!("correction file {}: {}", correction.file_name, correction.label()))?;
    }
    Ok(())
}

fn apply_correction(
    transaction: &Transaction,
    mut effects: Option<&mut EffectCache<'_>>,
    correction: &Correction,
    context: Option<&CorrectionContext<'_>>,
    report: &mut BuildReport,
) -> Result<()> {
    let set_tier_descriptions_by_id = context.map(|context| context.set_tier_descriptions_by_id);
    let field = correction.correctable_field()?;
    let mut row_ids = Vec::new();
    for spelling in correction.upstream_names() {
        row_ids.extend(maetrim_row_ids_named(
            transaction,
            correction,
            spelling,
            context.and_then(|context| context.socket_options),
        )?);
    }
    row_ids.sort_unstable();
    row_ids.dedup();
    if row_ids.is_empty() {
        if correction.targets_socket_option() && id_named(transaction, "items", &correction.name)?.is_some() {
            if correction.from.as_text() == Some(NO_SUCH_BONUS) {
                return record_applied_correction(transaction, correction, report);
            }
            record_stale_correction(
                report,
                correction,
                StaleCorrectionCause::ValueChanged {
                    expected_value: correction.from.to_json(),
                    maetrim_value: CorrectionValue::Text(NO_SUCH_BONUS.to_string()).to_json(),
                },
            );
            return Ok(());
        }
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
            let renamed_ids = if correction.selection.is_some() {
                let mut renamed = correction.clone();
                renamed.selection = Some(new_name.to_string());
                maetrim_row_ids_named(transaction, &renamed, &correction.name, None)?
            } else {
                maetrim_row_ids_named(transaction, correction, new_name, None)?
            };
            if !renamed_ids.is_empty() {
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
            correction.upstream_names().join(" or "),
            correction.kind.table_name(),
            correction.kind.name_column()
        );
    }
    if let (CorrectionValue::Bonus(bonus), Some(context), Some(cache)) =
        (&correction.to, context, effects.as_deref_mut())
    {
        if let Some(grouping) = context.groupings.grouping_named(&bonus.stat) {
            cache.ensure_indexed_effect(grouping)?;
        }
    }
    if correction.kind.corrects_a_bonus() && !matches!(field.shape, FieldShape::BonusAddition | FieldShape::BonusLines)
    {
        if let Some(context) = context {
            reject_group_member_correction(transaction, context.groupings, correction, &row_ids)?;
        }
    }
    let mut maetrim_values: Vec<CorrectionValue> = Vec::new();
    for row_id in &row_ids {
        let maetrim_value = current_value(
            transaction,
            correction,
            field,
            *row_id,
            set_tier_descriptions_by_id,
            context.map(|context| context.groupings),
        )?;
        if !maetrim_values.contains(&maetrim_value) {
            maetrim_values.push(maetrim_value);
        }
    }
    let merged_spellings = match (&correction.from, field.shape) {
        (CorrectionValue::Texts(spellings), FieldShape::RowName) => Some(spellings),
        _ => None,
    };
    let still_matches_his_value = match merged_spellings {
        Some(spellings) => maetrim_values
            .iter()
            .all(|value| matches!(value, CorrectionValue::Text(current_name) if spellings.contains(current_name))),
        None => maetrim_values == [correction.from.clone()],
    };
    if !still_matches_his_value {
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
    if let Some(context) = context {
        reject_answer_definition_correction(transaction, context.grouping_answers, correction, field, &row_ids)?;
    }
    write_correction(transaction, effects, correction, field, &row_ids, context)?;
    record_applied_correction(transaction, correction, report)
}

fn record_applied_correction(
    transaction: &Transaction,
    correction: &Correction,
    report: &mut BuildReport,
) -> Result<()> {
    let mut corrected = correction.to.clone();
    if correction.field == "lines" {
        if let CorrectionValue::Texts(lines) = &mut corrected {
            lines.sort();
        }
    }
    transaction.execute(
        "INSERT INTO corrections (kind, name, qualifier, field, from_value, to_value, reason, source, read, in_game)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            correction.kind.as_str(),
            correction.name,
            correction.qualifier(),
            correction.field,
            correction.from.to_json(),
            corrected.to_json(),
            correction.reason,
            correction.source.page_url(),
            correction.read,
            correction.source.is_in_game()
        ],
    )?;
    report.correction_applied_count += 1;
    Ok(())
}

fn record_stale_correction(report: &mut BuildReport, correction: &Correction, cause: StaleCorrectionCause) {
    report.stale_corrections.push(StaleCorrection {
        qualifier: correction.qualifier(),
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
    context: Option<&CorrectionContext<'_>>,
) -> Result<()> {
    let buff_resolver = context.map(|context| context.buff_resolver);
    let table_name = correction.target_table_name();
    match field.shape {
        FieldShape::EffectReplacement => {
            let destination = correction.effect.as_deref().context("effect replacement names its destination")?;
            let source = correction.from.as_text().context("effect replacement names his effect")?;
            let context = context.context("effect replacement needs the canonical index")?;
            let canonical = context
                .groupings
                .effect_named(destination)
                .with_context(|| format!("effect replacement names unknown effect {destination:?}"))?;
            let effects = effects.context("effect replacement needs the effect cache")?;
            let effect_id = effects.ensure_indexed_effect(canonical)?;
            let family = effects.family(effect_id).expect("replacement effect is cached");
            for owner_id in row_ids {
                transaction.execute("UPDATE item_effects SET effect_id=?3, value=CASE WHEN ?4>=1 THEN value END, value2=CASE WHEN ?4>=2 THEN value2 END, bonus_type_id=CASE WHEN ?5 THEN bonus_type_id END, alias=CASE WHEN EXISTS (SELECT 1 FROM effect_aliases WHERE effect_id=?3 AND name=item_effects.alias) THEN alias END WHERE item_id=?1 AND effect_id=(SELECT id FROM effects WHERE name=?2)", params![owner_id,source,effect_id,family.amount_count,family.uses_link_type])?;
            }
        }
        FieldShape::EffectBonusRemoval => {
            let stat_name = correction.stat.as_deref().context("effect bonus correction names a stat")?;
            let effects = effects.context("effect corrections need the family cache")?;
            for effect_id in row_ids {
                if let Some(row_id) = optional_effect_bonus_row_id(transaction, *effect_id, stat_name)? {
                    transaction.execute("DELETE FROM effect_bonuses WHERE rowid = ?1", [row_id])?;
                    effects.refresh_family(*effect_id)?;
                }
            }
        }
        FieldShape::BonusAlias => {
            let alias_name = correction.to.as_text().context("alias correction names an alias")?;
            let bonus = CorrectedBonus::of(transaction, correction)?;
            let table_name = bonus.link_table.table_name;
            let matched_links = bonus.matching_bonuses_sql(&format!("{table_name}.rowid"));
            for owner_id in row_ids {
                transaction.execute(
                    &format!("UPDATE {table_name} SET alias = ?5 WHERE rowid IN ({matched_links})"),
                    params![owner_id, bonus.stat_id, bonus.bonus_type_id, bonus.bonus_value, alias_name],
                )?;
            }
        }
        FieldShape::EffectBonusConstant
        | FieldShape::EffectBonusType
        | FieldShape::StatScale
        | FieldShape::StatRounding => {
            if correction.targets_effect_bonus() {
                let stat_name = correction.stat.as_deref().context("effect bonus correction names a stat")?;
                let effects = effects.context("effect corrections need the family cache")?;
                let corrected_value = if field.shape == FieldShape::EffectBonusType {
                    let name = correction.to.as_text().context("a bonus type cannot be null")?;
                    SqlValue::Integer(bonus_type_id_named(transaction, name)?)
                } else {
                    correction.to.to_sql()
                };
                for effect_id in row_ids {
                    let bonus_row_id = effect_bonus_row_id(transaction, *effect_id, stat_name)?;
                    transaction.execute(
                        &format!("UPDATE effect_bonuses SET {} = ?2 WHERE rowid = ?1", field.column),
                        params![bonus_row_id, corrected_value],
                    )?;
                    effects.refresh_family(*effect_id)?;
                }
            } else {
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
        }
        FieldShape::Integer if correction.kind == CorrectionKind::ItemEffect => {
            let effect_name = correction.effect.as_deref().context("item_effect value names an effect")?;
            let effect_id = id_named(transaction, "effects", effect_name)?
                .with_context(|| format!("unknown effect {effect_name:?}"))?;
            for item_id in row_ids {
                let changed = transaction.execute(
                    &format!("UPDATE item_effects SET {} = ?3 WHERE item_id = ?1 AND effect_id = ?2", field.column),
                    params![item_id, effect_id, correction.to.to_sql()],
                )?;
                ensure!(changed == 1, "item {item_id} has no unique effect {effect_name:?}");
            }
        }
        FieldShape::BonusTypeName if correction.kind == CorrectionKind::ItemEffect => {
            let effect_name = correction.effect.as_deref().context("item_effect bonus_type names an effect")?;
            let bonus_type_name = correction.to.as_text().context("item_effect bonus_type needs a type")?;
            let bonus_type_id = bonus_type_id_named(transaction, bonus_type_name)?;
            for item_id in row_ids {
                let changed = transaction.execute(
                    "UPDATE item_effects SET bonus_type_id = ?3 WHERE item_id = ?1
                     AND effect_id = (SELECT id FROM effects WHERE name = ?2)",
                    params![item_id, effect_name, bonus_type_id],
                )?;
                ensure!(changed == 1, "item {item_id} has no unique effect {effect_name:?}");
            }
        }
        FieldShape::Removal if correction.kind == CorrectionKind::ItemEffect => {
            let effect_name = correction.effect.as_deref().context("item_effect remove names an effect")?;
            let effect_id = id_named(transaction, "effects", effect_name)?
                .with_context(|| format!("unknown effect {effect_name:?}"))?;
            for item_id in row_ids {
                let changed = transaction.execute(
                    "DELETE FROM item_effects WHERE item_id = ?1 AND effect_id = ?2",
                    params![item_id, effect_id],
                )?;
                ensure!(changed == 1, "item {item_id} has no unique effect {effect_name:?}");
            }
            effects.context("item effect removal needs the family cache")?.delete_unowned(effect_id)?;
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
                context.context("bonus corrections need the canonical grouping index")?.groupings,
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
                context.context("bonus corrections need the canonical grouping index")?.groupings,
                &corrected_bonus,
                row_ids,
                Some(new_type_id),
                None,
            )?;
        }
        FieldShape::BonusAddition => {
            let CorrectionValue::Bonus(bonus) = &correction.to else { bail!("an add names the bonus in to") };
            add_bonus(
                transaction,
                effects.context("bonus corrections need the family cache")?,
                context.context("bonus corrections need the canonical grouping index")?.groupings,
                BonusLinkTable::of(correction.kind)?,
                row_ids,
                bonus,
            )?;
        }
        FieldShape::BonusRemoval => {
            let corrected_bonus = CorrectedBonus::of(transaction, correction)?;
            remove_bonuses(transaction, &corrected_bonus, row_ids)?;
        }
        FieldShape::BonusLines => {
            ensure!(
                correction.from.as_text() == Some(NO_SUCH_BONUS),
                "lines corrections must be applied before inserting links"
            );
        }
        FieldShape::BonusDedupe => {
            let corrected_bonus = CorrectedBonus::of(transaction, correction)?;
            let BonusLinkTable { table_name, owner_column, .. } = corrected_bonus.link_table;
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
        FieldShape::Removal if correction.kind == CorrectionKind::Quest => {
            for row_id in row_ids {
                transaction.execute("DELETE FROM quests WHERE id = ?1", [row_id])?;
            }
        }
        FieldShape::Removal => remove_rows(
            transaction,
            effects.context("owner removals need the family cache")?,
            correction.kind,
            row_ids,
        )?,
        FieldShape::Integer | FieldShape::Flag | FieldShape::Text | FieldShape::IntegerList | FieldShape::TextList => {
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
            if correction.kind == CorrectionKind::Effect
                && id_named(transaction, "effects", correction.to.as_text().context("an effect rename has a name")?)?
                    .is_some()
            {
                let effects = effects.context("effect corrections need the family cache")?;
                for family_id in row_ids {
                    effects
                        .merge_identical_text_family(*family_id, correction.to.as_text().expect("validated name"))?;
                }
            } else {
                rename_rows(transaction, correction, row_ids)?;
                if correction.kind == CorrectionKind::Effect {
                    let effects = effects.context("effect corrections need the family cache")?;
                    for family_id in row_ids {
                        effects.refresh_family(*family_id)?;
                    }
                }
            }
        }
    }
    Ok(())
}

struct SocketOptionLocation {
    id: i64,
    socket_order: i64,
    option_order: i64,
}

pub(crate) struct SocketOptionTargets {
    options_by_names: HashMap<(String, String, String), Vec<SocketOptionLocation>>,
    item_names: HashMap<i64, String>,
}

impl SocketOptionTargets {
    pub(crate) fn from_database(db: &rusqlite::Connection) -> Result<Self> {
        let mut targets = Self { options_by_names: HashMap::new(), item_names: HashMap::new() };
        let mut statement = db.prepare(
            "SELECT item.id, item.name, slot_type.label, option.name, option.id, option.slot_order, option.option_order
             FROM item_augment_slot_options option JOIN items item ON item.id = option.item_id
             JOIN item_augment_slots socket ON socket.item_id = option.item_id AND socket.sort_order = option.slot_order
             JOIN augment_slot_types slot_type ON slot_type.id = socket.slot_id
             ORDER BY item.id, option.slot_order, option.option_order",
        )?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            targets.item_names.insert(row.get(0)?, name.clone());
            targets.options_by_names.entry((name, row.get(2)?, row.get(3)?)).or_default().push(SocketOptionLocation {
                id: row.get(4)?,
                socket_order: row.get(5)?,
                option_order: row.get(6)?,
            });
        }
        Ok(targets)
    }

    pub(crate) fn option_id(&self, correction: &Correction, item_name: &str) -> Result<Option<i64>> {
        let socket = correction.socket.as_deref().context("socket-option correction needs a socket label")?;
        let option = correction.option.as_deref().context("socket-option correction needs an option name")?;
        let matches = self
            .options_by_names
            .get(&(item_name.to_string(), socket.to_string(), option.to_string()))
            .map(Vec::as_slice)
            .unwrap_or_default();
        ensure!(
            matches.len() <= 1,
            "{}: {} has an ambiguous socket-option target on {item_name:?}: {}",
            correction.file_name,
            correction.label(),
            matches
                .iter()
                .map(|SocketOptionLocation { id, socket_order, option_order }| format!(
                    "socket order {socket_order}, option order {option_order} (option id {id})"
                ))
                .collect::<Vec<_>>()
                .join("; ")
        );
        Ok(matches.first().map(|location| location.id))
    }

    pub(crate) fn item_name(&self, item_id: i64) -> Result<&str> {
        self.item_names.get(&item_id).map(String::as_str).context("socket-option annotation's item is absent")
    }
}

fn maetrim_row_ids_named(
    transaction: &Transaction,
    correction: &Correction,
    row_name: &str,
    socket_options: Option<&SocketOptionTargets>,
) -> Result<Vec<i64>> {
    let kind = correction.kind;

    if correction.targets_socket_option() {
        return Ok(socket_options
            .context("socket-option targets must be loaded before resolving corrections")?
            .option_id(correction, row_name)?
            .into_iter()
            .collect());
    }

    if correction.selection.is_some() || correction.stance.is_some() || correction.dc.is_some() {
        let owner_table = kind.table_name();
        let selection_join = if correction.selection.is_some() {
            "JOIN enhancement_selections selection ON selection.enhancement_id = owner.id AND selection.name = ?2"
        } else {
            ""
        };
        let ability_name = correction.stance.as_ref().or(correction.dc.as_ref());
        let (target, ability_join, no_ability) = if ability_name.is_some() {
            let (owner_kind, owner_id) = if correction.selection.is_some() {
                ("enhancement_selection", "selection.id")
            } else {
                (kind.as_str(), "owner.id")
            };
            let ability_table = correction.target_table_name();
            ("ability.id", format!("JOIN {ability_table} ability ON ability.owner_kind = '{owner_kind}' AND ability.owner_id = {owner_id} AND ability.name = ?3"), "")
        } else {
            ("selection.id", String::new(), " AND ?3 IS NULL")
        };
        return Ok(transaction.prepare(&format!("SELECT {target} FROM {owner_table} owner {selection_join} {ability_join} WHERE owner.name = ?1{no_ability} ORDER BY {target}"))?
            .query_map(params![row_name, correction.selection, ability_name], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?);
    }

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LineBonus {
    pub(crate) order: usize,
    pub(crate) effect_name: String,
    pub(crate) stat_name: String,
    pub(crate) bonus_type: Option<String>,
    pub(crate) amount: Option<i64>,
}

impl LineBonus {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        let order = row.get::<_, i64>(0)?;
        Ok(Self {
            order: usize::try_from(order).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(0, order))?,
            effect_name: row.get(1)?,
            stat_name: row.get(2)?,
            bonus_type: row.get(3)?,
            amount: row.get(4)?,
        })
    }
}

pub(super) struct LineMerge {
    pub(super) orders: std::collections::BTreeSet<usize>,
    pub(super) stats: std::collections::BTreeSet<String>,
}

pub(super) fn line_bonus_rows(
    transaction: &Transaction,
    kind: CorrectionKind,
    owner_id: i64,
    pending: Option<&[super::modifiers::DerivedEffectLink]>,
) -> Result<Vec<LineBonus>> {
    let table = BonusLinkTable::of(kind)?;
    let selected = table.line_bonus_columns();
    let mut query = format!("{} AND (e.is_stat=1 OR es.effect_id IS NOT NULL)", table.bonus_rows_sql(&selected));
    let mut arguments = vec![SqlValue::Integer(owner_id), SqlValue::Null, SqlValue::Null, SqlValue::Null];
    if let Some(pending) = pending {
        let links = pending
            .iter()
            .map(|link| (link.effect_id, link.bonus_type.map(ddo_model::enums::BonusType::id), link.value, link.value2))
            .collect::<Vec<_>>();
        query = format!(
            "WITH {} ({},effect_id,bonus_type_id,value,value2,sort_order) AS
             (SELECT ?1,json_extract(value,'$[0]'),json_extract(value,'$[1]'),json_extract(value,'$[2]'),json_extract(value,'$[3]'),key FROM json_each(?5)) {query}",
            table.table_name, table.owner_column
        );
        arguments.push(SqlValue::Text(serde_json::to_string(&links)?));
    }
    let mut statement = transaction.prepare(&query)?;
    let rows =
        statement.query_map(params_from_iter(arguments), LineBonus::from_row)?.collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

pub(crate) fn line_bonuses_by_owner(
    db: &rusqlite::Connection,
    kind: CorrectionKind,
) -> Result<std::collections::BTreeMap<i64, Vec<LineBonus>>> {
    let table = BonusLinkTable::of(kind)?;
    let selected = format!("{},{}.{}", table.line_bonus_columns(), table.table_name, table.owner_column);
    let mut statement = db.prepare(&table.bonus_rows_sql(&selected))?;
    let mut owners = std::collections::BTreeMap::<i64, Vec<LineBonus>>::new();
    for row in statement.query_map([SqlValue::Null, SqlValue::Null, SqlValue::Null, SqlValue::Null], |row| {
        Ok((row.get::<_, i64>(5)?, LineBonus::from_row(row)?))
    })? {
        let (owner, bonus) = row?;
        owners.entry(owner).or_default().push(bonus);
    }
    Ok(owners)
}

pub(super) fn line_merge_selection(
    correction: &Correction,
    groupings: &crate::groupings::Groupings,
    rows: &[LineBonus],
) -> Result<Option<LineMerge>> {
    let mut orders = std::collections::BTreeSet::new();
    for stats in correction.upstream_line_stats(groupings)? {
        let possible = rows
            .iter()
            .filter(|row| stats.contains(&row.stat_name))
            .map(|row| row.order)
            .collect::<std::collections::BTreeSet<_>>();
        let matches = possible
            .into_iter()
            .filter(|order| {
                let members = rows.iter().filter(|row| row.order == *order).collect::<Vec<_>>();
                members.iter().map(|row| row.stat_name.clone()).collect::<std::collections::BTreeSet<_>>() == stats
                    && members.iter().all(|row| {
                        row.bonus_type == correction.bonus_type
                            && correction.bonus_value.is_none_or(|value| row.amount == Some(value))
                    })
            })
            .collect::<std::collections::BTreeSet<_>>();
        if matches.is_empty() {
            return Ok(None);
        }
        ensure!(matches.len() == 1, "{}: merge source {stats:?} matches several links", correction.label());
        orders.extend(matches);
    }
    let selected = rows.iter().filter(|row| orders.contains(&row.order)).collect::<Vec<_>>();
    let stats = selected.iter().map(|row| row.stat_name.clone()).collect::<std::collections::BTreeSet<_>>();
    ensure!(
        selected.len() == stats.len()
            && selected.iter().all(|row| row.amount.is_some() && row.bonus_type == correction.bonus_type),
        "{}: merged lines have overlapping stats or different types or values",
        correction.label()
    );
    Ok(Some(LineMerge { orders, stats }))
}

pub(super) fn merged_line_amounts(
    groupings: &crate::groupings::Groupings,
    correction: &Correction,
    rows: &[&LineBonus],
) -> Result<(Option<i64>, Option<i64>)> {
    let target = groupings
        .effect_named(correction.to.as_text().context("merge target missing")?)
        .context("merge target is not indexed")?;
    if target.has_constants_at_type(correction.bonus_type.as_deref().context("merge type missing")?) {
        let mut constants = std::collections::BTreeMap::new();
        for bonus in &target.bonuses {
            let member = &groupings.by_id[&bonus.target_id];
            for stat in groupings.expanded_stat_names(&member.name)? {
                ensure!(
                    constants.insert(stat, bonus.constant).is_none(),
                    "{}: merge target repeats a stat",
                    correction.label()
                );
            }
        }
        ensure!(
            rows.len() == constants.len() && rows.iter().all(|row| constants.get(&row.stat_name) == Some(&row.amount)),
            "{}: merged values disagree with the target constants",
            correction.label()
        );
        Ok((None, None))
    } else {
        let mut amounts = [None, None];
        for bonus in &target.bonuses {
            let slot = usize::try_from(bonus.amount_from - 1).context("merge target has no amount slot")?;
            let slot_amount = amounts.get_mut(slot).context("merge target has an unknown amount slot")?;
            let member = &groupings.by_id[&bonus.target_id];
            for stat in groupings.expanded_stat_names(&member.name)? {
                let amount =
                    rows.iter().find(|row| row.stat_name == stat).and_then(|row| row.amount).with_context(|| {
                        format!("{}: merged lines lack {stat:?} for slot {}", correction.label(), slot + 1)
                    })?;
                ensure!(
                    slot_amount.is_none_or(|previous| previous == amount),
                    "{}: merged lines have different values for slot {}",
                    correction.label(),
                    slot + 1
                );
                *slot_amount = Some(amount);
            }
        }
        let required_slot_count = usize::try_from(super::amount_count(
            &target.verbose_name_template,
            target.description_template.as_deref(),
        ))?;
        ensure!(
            amounts.iter().take(required_slot_count).all(Option::is_some)
                && (amounts[1].is_none() || amounts[0].is_some()),
            "{}: merged lines lack a value for every target slot",
            correction.label()
        );
        Ok((amounts[0], amounts[1]))
    }
}

fn corrected_lines_value(
    transaction: &Transaction,
    groupings: &crate::groupings::Groupings,
    correction: &Correction,
    owner_id: i64,
) -> Result<CorrectionValue> {
    if correction.from.as_text() == Some(NO_SUCH_BONUS) {
        return Ok(CorrectionValue::Text(NO_SUCH_BONUS.to_string()));
    }
    let rows = line_bonus_rows(transaction, correction.kind, owner_id, None)?;
    let expected_stats =
        correction.upstream_line_stats(groupings)?.into_iter().flatten().collect::<std::collections::BTreeSet<_>>();
    let selected_orders = rows
        .iter()
        .filter(|row| {
            expected_stats.contains(&row.stat_name)
                && row.bonus_type == correction.bonus_type
                && correction.bonus_value.is_none_or(|value| row.amount == Some(value))
        })
        .map(|row| row.order)
        .collect::<std::collections::BTreeSet<_>>();
    let selected = rows.iter().filter(|row| selected_orders.contains(&row.order)).collect::<Vec<_>>();
    if correction.is_line_merge() {
        let grouping =
            selected.first().map(|row| row.effect_name.as_str()).context("a merge must leave one grouping link")?;
        let mut resolved = correction.clone();
        resolved.to = CorrectionValue::Text(grouping.to_string());
        resolved.line_partition_stats(groupings)?;
        let target = groupings
            .effect_named(grouping)
            .with_context(|| format!("{}: merge target is not indexed", correction.label()))?;
        ensure!(
            selected.iter().map(|row| row.stat_name.clone()).collect::<std::collections::BTreeSet<_>>()
                == groupings.expanded_stat_names(&target.name)?
                && selected.iter().map(|row| row.order).collect::<std::collections::BTreeSet<_>>().len() == 1
                && selected.iter().all(|row| row.amount.is_some() && row.bonus_type == correction.bonus_type),
            "{}: a merge must leave one grouping link with all its stats at one type",
            correction.label()
        );
        merged_line_amounts(groupings, &resolved, &selected)?;
        return Ok(resolved.to);
    }
    let names = selected.iter().map(|row| row.effect_name.clone()).collect::<std::collections::BTreeSet<_>>();
    let split_value = selected.first().and_then(|row| row.amount);
    ensure!(
        selected.len() == expected_stats.len()
            && selected.iter().all(|row| {
                expected_stats.contains(&row.stat_name)
                    && row.bonus_type == correction.bonus_type
                    && row.amount == split_value
            }),
        "{}: split lines have different stats, types or values",
        correction.label()
    );
    let mut resolved = correction.clone();
    resolved.to = CorrectionValue::Texts(names.into_iter().collect());
    resolved.line_partition_stats(groupings)?;
    Ok(resolved.to)
}

fn current_value(
    transaction: &Transaction,
    correction: &Correction,
    field: &CorrectableField,
    row_id: i64,
    set_tier_descriptions_by_id: Option<&HashMap<i64, String>>,
    groupings: Option<&crate::groupings::Groupings>,
) -> Result<CorrectionValue> {
    let table_name = correction.target_table_name();
    let value_sql = match field.shape {
        FieldShape::EffectReplacement => {
            let source = correction.from.as_text().context("effect replacement names his effect")?;
            let destination = correction.effect.as_deref().context("effect replacement names its destination")?;
            let (source_count, destination_count): (i64, i64) = transaction.query_row("SELECT COUNT(CASE WHEN e.name=?2 THEN 1 END),COUNT(CASE WHEN e.name=?3 THEN 1 END) FROM item_effects l JOIN effects e ON e.id=l.effect_id WHERE l.item_id=?1 AND e.name IN (?2,?3)", params![row_id,source,destination], |row| Ok((row.get(0)?,row.get(1)?)))?;
            ensure!(source_count <= 1 && destination_count <= 1, "item {row_id} has {source_count} links to {source:?} and {destination_count} to {destination:?}; an effect replacement needs one");
            return Ok(CorrectionValue::Text(
                if source_count == 1 { source } else if destination_count == 1 { destination } else { NO_SUCH_BONUS }
                    .into(),
            ));
        }
        FieldShape::EffectBonusRemoval => {
            let stat_name = correction.stat.as_deref().context("effect bonus correction names a stat")?;
            ensure!(
                transaction.query_row("SELECT EXISTS(SELECT 1 FROM effects WHERE name = ?1 AND is_stat = 1)", [stat_name], |row| row.get::<_, bool>(0))?,
                "unknown stat {stat_name:?} in effect member removal"
            );
            let Some(bonus_row_id) = optional_effect_bonus_row_id(transaction, row_id, stat_name)? else {
                return Ok(CorrectionValue::Text(NO_SUCH_BONUS.to_string()));
            };
            let bonus = crate::tree::EffectBonus::from_database(transaction, bonus_row_id)?;
            if matches!(correction.from, CorrectionValue::Integer(_)) {
                if let Some(constant) = bonus.constant {
                    return Ok(CorrectionValue::Integer(constant));
                }
            }
            return Ok(CorrectionValue::EffectBonus(bonus));
        }
        FieldShape::BonusAlias => {
            let Some(bonus) = CorrectedBonus::from_correction_if_effect_exists(transaction, correction)? else {
                return Ok(CorrectionValue::Text(NO_SUCH_BONUS.to_string()));
            };
            let table_name = bonus.link_table.table_name;
            let values = transaction.prepare(&bonus.matching_bonuses_sql(&format!("DISTINCT {table_name}.alias")))?
                .query_map(params![row_id, bonus.stat_id, bonus.bonus_type_id, bonus.bonus_value], |row| row.get::<_, Option<String>>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            ensure!(values.len() <= 1, "alias correction matches links with different aliases");
            return Ok(match values.into_iter().next() {
                None => CorrectionValue::Text(NO_SUCH_BONUS.to_string()),
                Some(None) => CorrectionValue::Null,
                Some(Some(name)) => CorrectionValue::Text(name),
            });
        }
        FieldShape::BonusLines => {
            if correction.from.as_text() == Some(NO_SUCH_BONUS) {
                return Ok(CorrectionValue::Text(NO_SUCH_BONUS.to_string()));
            }
            let rows = line_bonus_rows(transaction, correction.kind, row_id, None)?;
            return Ok(if line_merge_selection(correction, groupings.context("lines need the canonical stat index")?, &rows)?.is_some() {
                correction.from.clone()
            } else {
                CorrectionValue::Text(NO_SUCH_BONUS.to_string())
            });
        }
        _ if correction.targets_effect_bonus() => {
            let stat_name = correction.stat.as_deref().context("effect bonus correction names a stat")?;
            let bonus_row_id = effect_bonus_row_id(transaction, row_id, stat_name)?;
            let expression = if field.shape == FieldShape::EffectBonusType {
                "(SELECT name FROM bonus_types WHERE id = effect_bonuses.bonus_type_id)"
            } else {
                field.column
            };
            let value: SqlValue = transaction.query_row(
                &format!("SELECT {expression} FROM effect_bonuses WHERE rowid = ?1"),
                [bonus_row_id],
                |row| row.get(0),
            )?;
            return Ok(CorrectionValue::from_sql(value));
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
                &format!("SELECT ie.{} FROM item_effects ie JOIN effects e ON e.id = ie.effect_id
                  WHERE ie.item_id = ?1 AND e.name = ?2", field.column),
            )?;
            let values: Vec<Option<i64>> = statement
                .query_map(params![row_id, effect_name], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            ensure!(values.len() == 1, "item {row_id} must have one effect {effect_name:?}");
            return Ok(values[0].map_or(CorrectionValue::Null, CorrectionValue::Integer));
        }
        FieldShape::BonusTypeName if correction.kind == CorrectionKind::ItemEffect => {
            let effect_name = correction.effect.as_deref().context("item_effect bonus_type names an effect")?;
            let bonus_type_name: Option<Option<String>> = transaction
                .query_row(
                    "SELECT b.name FROM item_effects ie JOIN effects e ON e.id = ie.effect_id
                     LEFT JOIN bonus_types b ON b.id = ie.bonus_type_id
                     WHERE ie.item_id = ?1 AND e.name = ?2",
                    params![row_id, effect_name],
                    |row| row.get(0),
                )
                .optional()?;
            return Ok(match bonus_type_name {
                Some(Some(name)) => CorrectionValue::Text(name),
                Some(None) => CorrectionValue::Null,
                None => CorrectionValue::Text(NO_SUCH_BONUS.to_string()),
            });
        }
        FieldShape::Removal if correction.kind == CorrectionKind::ItemEffect => {
            let effect_name = correction.effect.as_deref().context("item_effect remove names an effect")?;
            let present: bool = transaction.query_row(
                "SELECT EXISTS (SELECT 1 FROM item_effects ie JOIN effects e ON e.id = ie.effect_id
                 WHERE ie.item_id = ?1 AND e.name = ?2)",
                params![row_id, effect_name],
                |row| row.get(0),
            )?;
            return Ok(if present { CorrectionValue::Integer(0) } else { CorrectionValue::Text(NO_SUCH_BONUS.to_string()) });
        }
        FieldShape::Removal => return Ok(CorrectionValue::Integer(0)),
        FieldShape::Integer | FieldShape::BonusRemoval if correction.kind.corrects_a_bonus() => {
            let Some(corrected_bonus) = CorrectedBonus::from_correction_if_effect_exists(transaction, correction)? else {
                let stat_name = correction.bonus_key().context("a bonus correction names its bonus")?.0;
                ensure!(groupings.is_some_and(|index| index.effect_named(stat_name).is_some()), "stat {stat_name:?} is not an effect; use its exact name");
                return Ok(CorrectionValue::Text(NO_SUCH_BONUS.to_string()));
            };
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
    if matches!(field.shape, FieldShape::IntegerList | FieldShape::TextList) {
        if let SqlValue::Text(encoded) = sql_value {
            return CorrectionValue::from_json(&encoded);
        }
    }
    Ok(CorrectionValue::from_sql(sql_value))
}

pub(super) fn record_final_set_names(transaction: &Transaction) -> Result<()> {
    let mut statement = transaction.prepare(
        "SELECT correction.id, item.set_bonus FROM corrections correction
         JOIN items item ON item.name = correction.name
         WHERE correction.kind = 'item' AND correction.field = 'set_bonus'",
    )?;
    let rows = statement
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (correction_id, set_name) in rows {
        transaction.execute(
            "UPDATE corrections SET to_value = ?2 WHERE id = ?1",
            params![correction_id, serde_json::to_string(&set_name)?],
        )?;
    }
    Ok(())
}

pub(super) fn record_built_item_effect_values(transaction: &Transaction) -> Result<()> {
    let mut statement = transaction.prepare(
        "SELECT id, name, qualifier, to_value FROM corrections WHERE kind = 'item_effect' AND field = 'add'",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (correction_id, item_name, effect_name, to_value) in rows {
        if !serde_json::from_str::<serde_json::Value>(&to_value)?.is_object() {
            continue;
        }
        let applied_effect: Option<(String, Option<String>, Option<i64>)> = transaction
            .query_row(
                "SELECT effect.verbose_name_template, effect.description_template, link.value
               FROM item_effects link JOIN items item ON item.id = link.item_id
               JOIN effects effect ON effect.id = link.effect_id
              WHERE item.name = ?1 AND effect.name = ?2",
                params![item_name, effect_name],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((verbose, description, value)) = applied_effect else { continue };
        let description = description.unwrap_or_default();
        let description = if let Some((_, suffix)) = verbose.split_once(" — ") {
            format!("{suffix}: {description}")
        } else {
            description
        };
        let mut applied = serde_json::json!({"name": effect_name, "description": description});
        if let Some(value) = value {
            applied["value"] = serde_json::json!(value);
        }
        transaction.execute(
            "UPDATE corrections SET to_value = ?1 WHERE id = ?2",
            params![applied.to_string(), correction_id],
        )?;
    }
    Ok(())
}

pub(crate) fn data_correction_value(
    transaction: &Transaction,
    correction: &Correction,
    current_name: &str,
    groupings: &crate::groupings::Groupings,
    socket_options: &SocketOptionTargets,
) -> Result<CorrectionValue> {
    let field = correction.correctable_field()?;
    let row_ids = maetrim_row_ids_named(transaction, correction, current_name, Some(socket_options))?;
    if row_ids.is_empty()
        && correction.targets_socket_option()
        && correction.from.as_text() == Some(NO_SUCH_BONUS)
        && id_named(transaction, "items", current_name)?.is_some()
    {
        return Ok(CorrectionValue::Text(NO_SUCH_BONUS.to_string()));
    }
    ensure!(!row_ids.is_empty(), "no {} named {current_name:?} for data correction", correction.kind.as_str());
    let mut values = Vec::new();
    for row_id in row_ids {
        let mut value = if field.shape == FieldShape::BonusLines {
            corrected_lines_value(transaction, groupings, correction, row_id)?
        } else {
            current_value(transaction, correction, field, row_id, None, Some(groupings))?
        };
        if field.shape == FieldShape::EffectBonusRemoval {
            ensure!(
                value.as_text() == Some(NO_SUCH_BONUS),
                "removed effect member is still present: {}",
                correction.label()
            );
            value = CorrectionValue::Null;
        }
        if value == CorrectionValue::Null && matches!(field.shape, FieldShape::IntegerList | FieldShape::TextList) {
            value = CorrectionValue::Integers(Vec::new());
        }
        if !values.contains(&value) {
            values.push(value);
        }
    }
    ensure!(values.len() == 1, "data correction on {current_name:?} resolves to {} values", values.len());
    Ok(values.remove(0))
}

fn effect_bonus_row_id(transaction: &Transaction, effect_id: i64, stat_name: &str) -> Result<i64> {
    optional_effect_bonus_row_id(transaction, effect_id, stat_name)?
        .with_context(|| format!("effect {effect_id} must grant {stat_name:?} exactly once"))
}

fn optional_effect_bonus_row_id(transaction: &Transaction, effect_id: i64, stat_name: &str) -> Result<Option<i64>> {
    let mut statement = transaction.prepare(
        "SELECT eb.rowid FROM effect_bonuses eb JOIN effects target ON target.id = eb.target_effect_id
          WHERE eb.effect_id = ?1 AND target.name = ?2",
    )?;
    let rows: Vec<i64> =
        statement.query_map(params![effect_id, stat_name], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?;
    ensure!(rows.len() <= 1, "effect {effect_id} must grant {stat_name:?} at most once");
    Ok(rows.first().copied())
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
    fn line_bonus_columns(&self) -> String {
        format!(
            "{}.sort_order,e.name,resolved_stat.name,bonus_types.name,{}",
            self.table_name,
            self.effective_value_sql()
        )
    }
    const AUGMENT: Self = Self { table_name: "augment_effects", owner_column: "augment_id" };
    const FEAT: Self = Self { table_name: "feat_effects", owner_column: "feat_id" };
    const ITEM: Self = Self { table_name: "item_effects", owner_column: "item_id" };
    const SET_TIER: Self = Self { table_name: "set_bonus_tier_effects", owner_column: "tier_id" };
    const SOCKET_OPTION: Self = Self { table_name: "item_augment_slot_option_effects", owner_column: "option_id" };

    fn of(kind: CorrectionKind) -> Result<Self> {
        match kind {
            CorrectionKind::AugmentBonus => Ok(Self::AUGMENT),
            CorrectionKind::FeatBonus => Ok(Self::FEAT),
            CorrectionKind::ItemBonus => Ok(Self::ITEM),
            CorrectionKind::SetTierBonus => Ok(Self::SET_TIER),
            CorrectionKind::SocketOptionBonus => Ok(Self::SOCKET_OPTION),
            _ => bail!("{} is not a bonus correction kind", kind.as_str()),
        }
    }
    fn bonus_rows_sql(&self, selected_columns: &str) -> String {
        let BonusLinkTable { table_name, owner_column, .. } = *self;
        let effective_value = self.effective_value_sql();
        format!(
            "SELECT {selected_columns} FROM {table_name}
              JOIN effects e ON e.id = {table_name}.effect_id
              LEFT JOIN effect_bonuses es ON es.effect_id = e.id AND (?2 IS NULL OR e.id != ?2 OR es.sort_order = 0)
              LEFT JOIN effect_bonuses expanded ON expanded.effect_id = es.target_effect_id AND (?2 IS NULL OR (e.id != ?2 AND es.target_effect_id != ?2))
              LEFT JOIN bonus_types ON bonus_types.id = COALESCE(es.bonus_type_id, {table_name}.bonus_type_id)
              JOIN effects resolved_stat ON resolved_stat.id = COALESCE(expanded.target_effect_id, es.target_effect_id, e.id)
              WHERE (?1 IS NULL OR {table_name}.{owner_column} IN (SELECT value FROM json_each(?1))) AND (?2 IS NULL OR es.target_effect_id = ?2 OR expanded.target_effect_id = ?2 OR e.id = ?2)
                AND (?4 IS NULL OR {effective_value} = ?4)"
        )
    }

    fn effective_value_sql(&self) -> String {
        let table_name = self.table_name;
        let source = format!(
            "CASE WHEN e.is_stat = 1 THEN COALESCE({table_name}.value, e.default_value)
             WHEN es.amount_from = 0 THEN es.constant WHEN es.amount_from = 1 THEN COALESCE({table_name}.value, e.default_value)
             ELSE COALESCE({table_name}.value2, e.default_value2) END"
        );
        let scaled = ddo_model::effect_amount::rounded_amount_sql(&source, "es.scale", "es.rounding");
        format!("CASE WHEN e.is_stat = 1 THEN {source} ELSE {scaled} END")
    }
}

struct CorrectedBonus {
    link_table: BonusLinkTable,
    stat_id: i64,
    bonus_type_id: Option<i64>,
    bonus_value: Option<i64>,
}

fn reject_answer_definition_correction(
    transaction: &Transaction,
    grouping_answers: &HashSet<i64>,
    correction: &Correction,
    field: &CorrectableField,
    owner_ids: &[i64],
) -> Result<()> {
    let affected_effects = match field.shape {
        _ if correction.targets_effect_bonus() => {
            let mut statement =
                transaction.prepare("SELECT id, name FROM effects WHERE id IN (SELECT value FROM json_each(?1))")?;
            let affected_effects = statement
                .query_map([serde_json::to_string(owner_ids)?], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            affected_effects
        }
        FieldShape::StatScale | FieldShape::StatRounding => {
            let corrected_bonus = CorrectedBonus::of(transaction, correction)?;
            let mut statement = transaction.prepare(&corrected_bonus.matching_bonuses_sql("DISTINCT e.id, e.name"))?;
            let affected_effects = statement
                .query_map(
                    params![
                        serde_json::to_string(owner_ids)?,
                        corrected_bonus.stat_id,
                        corrected_bonus.bonus_type_id,
                        corrected_bonus.bonus_value
                    ],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            affected_effects
        }
        _ => return Ok(()),
    };
    for (effect_id, name) in affected_effects {
        if grouping_answers.contains(&effect_id) {
            bail!("definition correction {} cannot change grouping lookup answer {name:?}", correction.label());
        }
    }
    Ok(())
}

pub(super) fn correction_owner_matches(
    transaction: &Transaction,
    correction: &Correction,
    owner_id: i64,
) -> Result<bool> {
    Ok(maetrim_row_ids_named(transaction, correction, &correction.name, None)?.contains(&owner_id))
}

pub(super) fn removed_derived_bonus_indices(
    transaction: &Transaction,
    correction: &Correction,
    owner_id: i64,
    bonuses: &[(i64, i64, i64)],
) -> Result<HashSet<usize>> {
    let corrected_bonus = CorrectedBonus::of(transaction, correction)?;
    let BonusLinkTable { table_name, owner_column, .. } = corrected_bonus.link_table;
    let query = format!(
        "WITH {table_name} ({owner_column},effect_id,bonus_type_id,value,value2,sort_order) AS
          (SELECT ?5,json_extract(value, '$[0]'),json_extract(value, '$[1]'),json_extract(value, '$[2]'),NULL,key FROM json_each(?6)) {}",
        corrected_bonus.matching_bonuses_sql(&format!("{table_name}.sort_order, {}", corrected_bonus.link_table.effective_value_sql()))
    );
    let mut statement = transaction.prepare(&query)?;
    let matches = statement
        .query_map(
            params![
                owner_id,
                corrected_bonus.stat_id,
                corrected_bonus.bonus_type_id,
                corrected_bonus.bonus_value,
                owner_id,
                serde_json::to_string(bonuses)?
            ],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(matches
        .into_iter()
        .filter(|(_, amount)| correction.from == CorrectionValue::Integer(*amount))
        .map(|(index, _)| usize::try_from(index).expect("JSON array index is nonnegative"))
        .collect())
}

fn reject_group_member_correction(
    transaction: &Transaction,
    groupings: &crate::groupings::Groupings,
    correction: &Correction,
    owner_ids: &[i64],
) -> Result<()> {
    let Some(stat) = correction.stat.as_deref().and_then(ddo_model::stats::Stat::by_name) else {
        return Ok(());
    };
    let corrected_bonus = CorrectedBonus::of(transaction, correction)?;
    let mut statement = transaction.prepare(&corrected_bonus.matching_bonuses_sql(
        "DISTINCT e.name, e.is_group AND (SELECT COUNT(DISTINCT target_effect_id) FROM effect_bonuses WHERE effect_id = e.id) > 1",
    ))?;
    for row in statement.query_map(
        params![
            serde_json::to_string(owner_ids)?,
            corrected_bonus.stat_id,
            corrected_bonus.bonus_type_id,
            corrected_bonus.bonus_value
        ],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)),
    )? {
        let (effect_name, is_multi_stat_group) = row?;
        ensure!(
            !is_multi_stat_group,
            "stat {:?} is behind grouping {:?}; name the grouping in the correction",
            stat.name,
            effect_name
        );
        let Some(grouping) = groupings.grouping_named(&effect_name) else {
            continue;
        };
        let indexed_stat = groupings
            .effect_named(stat.name)
            .with_context(|| format!("stat {:?} is absent from the canonical grouping definitions", stat.name))?;
        ensure!(
            !grouping.stat_ids.contains(&indexed_stat.id),
            "stat {:?} is behind grouping {:?}; name the grouping in the correction",
            stat.name,
            grouping.name
        );
    }
    Ok(())
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
        Self::from_correction_if_effect_exists(transaction, correction)?.with_context(|| {
            format!(
                "stat {:?} is not an effect; use its exact name",
                correction.bonus_key().map_or("", |(stat, _)| stat)
            )
        })
    }

    fn from_correction_if_effect_exists(transaction: &Transaction, correction: &Correction) -> Result<Option<Self>> {
        let (stat_name, bonus_type_name) = correction.bonus_key().context("a bonus correction names its bonus")?;
        let bonus_type_id = match bonus_type_name {
            NULL_SPELLING => None,
            _ => Some(bonus_type_id_named(transaction, bonus_type_name)?),
        };
        let Some(stat_id) = id_named(transaction, "effects", stat_name)? else {
            return Ok(None);
        };
        Ok(Some(Self {
            link_table: BonusLinkTable::of(correction.kind)?,
            stat_id,
            bonus_type_id,
            bonus_value: correction.bonus_value,
        }))
    }

    fn matching_bonuses_sql(&self, selected_columns: &str) -> String {
        let table_name = self.link_table.table_name;
        format!(
            "{} AND COALESCE(es.bonus_type_id, {table_name}.bonus_type_id) IS ?3 ORDER BY {table_name}.sort_order",
            self.link_table.bonus_rows_sql(selected_columns)
        )
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
        let selected_value = self.link_table.effective_value_sql();
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
        let table_name = self.link_table.table_name;
        let selected = format!("COALESCE(es.bonus_type_id, {table_name}.bonus_type_id), bonus_types.name");
        let mut statement = transaction
            .prepare(&format!("{} ORDER BY {table_name}.sort_order", self.link_table.bonus_rows_sql(&selected)))?;
        let bonus_types_on_stat: Vec<(Option<i64>, Option<String>)> = statement
            .query_map(params![owner_id, self.stat_id, self.bonus_type_id, self.bonus_value], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<rusqlite::Result<_>>()?;
        let matching_type = bonus_types_on_stat.iter().find(|(type_id, _)| *type_id == self.bonus_type_id);
        Ok(match matching_type.or(bonus_types_on_stat.first()) {
            Some((_, Some(type_name))) => CorrectionValue::Text(type_name.clone()),
            Some((_, None)) => CorrectionValue::Null,
            None => CorrectionValue::Text(NO_SUCH_BONUS.to_string()),
        })
    }
}

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
    groupings: &crate::groupings::Groupings,
    corrected_bonus: &CorrectedBonus,
    owner_ids: &[i64],
    new_bonus_type_id: Option<i64>,
    new_value: Option<i64>,
) -> Result<()> {
    let BonusLinkTable { table_name, owner_column, .. } = corrected_bonus.link_table;
    if let Some(family) = effects.family(corrected_bonus.stat_id).filter(|family| !family.is_stat) {
        let requested_type = new_bonus_type_id.or(corrected_bonus.bonus_type_id);
        ensure!(
            groupings.grouping_named(&family.name).is_some_and(|grouping| requested_type
                .and_then(|id| ddo_model::enums::BonusType::ALL.iter().find(|bonus_type| bonus_type.id() == id))
                .is_some_and(|bonus_type| grouping.accepts_value_at_type(bonus_type.name()))),
            "effect {:?} cannot take one value at the corrected bonus type",
            effects.family(corrected_bonus.stat_id).map(|family| &family.name)
        );
    }
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
            if effect_id == corrected_bonus.stat_id
                || effects.family(effect_id).is_some_and(|family| family.is_stat || family.is_group)
            {
                let link_type = effects
                    .family(effect_id)
                    .filter(|family| family.is_stat || family.uses_link_type)
                    .and(new_bonus_type_id);
                transaction.execute(
                    &format!("UPDATE {table_name} SET bonus_type_id = COALESCE(?3, bonus_type_id), value = COALESCE(?4, value) WHERE {owner_column} = ?1 AND sort_order = ?2"),
                    params![owner_id, sort_order, link_type, new_value],
                )?;
                continue;
            }
            let amount_slot_count: i64 = transaction.query_row(
                "SELECT COUNT(*) FROM effect_bonuses WHERE effect_id = ?1 AND amount_from = ?2",
                params![effect_id, amount_from],
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
                && amount_slot_count == 1
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
    groupings: &crate::groupings::Groupings,
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
    let bonus_type = ddo_model::enums::BonusType::parse(bonus_type_name).context("validated bonus type exists")?;
    if let Some(stat) = ddo_model::stats::Stat::by_name(&bonus.stat) {
        effects.ensure_stat(effect_id, stat, Some(bonus_type), 1, None, 0)?;
    } else {
        ensure!(
            groupings
                .grouping_named(&bonus.stat)
                .is_some_and(|grouping| grouping.accepts_value_at_type(bonus_type.name())),
            "added effect {:?} cannot take one value at {}",
            bonus.stat,
            bonus_type.name()
        );
    }
    let BonusLinkTable { table_name, owner_column, .. } = link_table;
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
            (owner, *owner_id),
            effect_id,
            None,
            effects
                .family(effect_id)
                .is_some_and(|family| family.is_stat || family.uses_link_type)
                .then_some(bonus_type),
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
    let BonusLinkTable { table_name, owner_column, .. } = corrected_bonus.link_table;
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
        effects.insert_link((EffectOwner::SetBonusTier, tier_id), effect_id, None, None, (None, None), 0)?;
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
                (EffectOwner::SetBonusTier, *tier_id),
                effect_id,
                None,
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
        effects.insert_link(
            (EffectOwner::Item, *item_id),
            effect_id,
            None,
            None,
            (amount, None),
            sort_order as usize,
        )?;
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
    if correction.selection.is_some() {
        for row_id in row_ids {
            let exists: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM enhancement_selections other JOIN enhancement_selections current ON other.enhancement_id = current.enhancement_id WHERE current.id = ?1 AND other.name = ?2 AND other.id != current.id)",
                params![row_id, new_name], |row| row.get(0),
            )?;
            ensure!(!exists, "to {new_name:?} is already the name of another selection");
        }
        return write_column(
            transaction,
            "enhancement_selections",
            "name",
            row_ids,
            SqlValue::Text(new_name.to_string()),
        );
    }
    match correction.kind {
        CorrectionKind::SocketLabel => {
            for row_id in row_ids {
                rename_socket_label(transaction, *row_id, new_name)?;
            }
            return Ok(());
        }
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

pub(super) fn rename_socket_label(transaction: &Transaction, slot_type_id: i64, new_label: &str) -> Result<()> {
    let merged_slot_type_id = canonical_socket_label_id(transaction, slot_type_id, new_label)?;
    if merged_slot_type_id != slot_type_id {
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

pub(super) fn canonical_socket_label_id(transaction: &Transaction, slot_type_id: i64, new_label: &str) -> Result<i64> {
    Ok(socket_label_id(transaction, new_label)?.unwrap_or(slot_type_id))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn bonus_correction(kind: CorrectionKind, stat: &str, bonus_type: &str, field: &str) -> Correction {
        Correction {
            kind,
            name: String::new(),
            field: field.into(),
            from: CorrectionValue::Text(stat.into()),
            to: CorrectionValue::Texts(vec![]),
            family: None,
            socket: None,
            option: None,
            selection: None,
            stance: None,
            dc: None,
            effect: None,
            stat: Some(stat.into()),
            bonus_type: Some(bonus_type.into()),
            bonus_value: None,
            equipped_count: None,
            reason: "Test".into(),
            source: "https://ddowiki.com/page/Test".to_string().into(),
            read: "2026-10-21".into(),
            file_name: "test.toml".into(),
        }
    }

    #[test]
    fn a_group_member_missing_from_the_canonical_tree_returns_an_error() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data");
        let scratch = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(scratch.path().join("upstream")).unwrap();
        std::fs::write(scratch.path().join("upstream/source-aliases.toml"), "").unwrap();
        std::fs::copy(fixture.join("upstream/commit.toml"), scratch.path().join("upstream/commit.toml")).unwrap();
        std::fs::create_dir_all(scratch.path().join("effects/stats")).unwrap();
        for slug in ["fire-spell-power", "electric-spell-power", "sonic-spell-power"] {
            std::fs::copy(
                fixture.join(format!("effects/stats/{slug}.toml")),
                scratch.path().join(format!("effects/stats/{slug}.toml")),
            )
            .unwrap();
        }
        std::fs::copy(fixture.join("effects/bonus-types.toml"), scratch.path().join("effects/bonus-types.toml"))
            .unwrap();
        let contents = std::fs::read_to_string(fixture.join("effects/power-of-the-frozen-thunderstorm.toml")).unwrap();
        std::fs::write(
            scratch.path().join("effects/power-of-the-frozen-thunderstorm.toml"),
            contents.replace("Cold Spell Power", "Fire Spell Power"),
        )
        .unwrap();
        let mut canonical = rusqlite::Connection::open_in_memory().unwrap();
        crate::tree::build_from_data(scratch.path(), &mut canonical).unwrap();
        let groupings = crate::groupings::Groupings::from_database(&canonical).unwrap();
        assert!(groupings.effect_named("Cold Spell Power").is_none());
        let mut upstream = rusqlite::Connection::open_in_memory().unwrap();
        crate::tree::build_from_data(&fixture, &mut upstream).unwrap();
        let owner = upstream.query_row(
            "SELECT t.id FROM set_bonus_tiers t JOIN set_bonuses s ON s.id=t.set_id WHERE s.name='Anger of the Avalanche'",
            [], |row| row.get::<_, i64>(0),
        ).unwrap();
        let transaction = upstream.transaction().unwrap();
        let correction = bonus_correction(CorrectionKind::SetTierBonus, "Cold Spell Power", "Artifact", "value");
        let error = reject_group_member_correction(&transaction, &groupings, &correction, &[owner]).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("Cold Spell Power") && message.contains("canonical"), "{message}");
    }

    #[test]
    fn bonus_type_reader_agrees_with_matching_for_every_expanded_fixture_bonus() {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::tree::build_from_data(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data"),
            &mut db,
        )
        .unwrap();
        let rows = db
            .prepare(
                "SELECT DISTINCT b.owner_id, stat.name, bt.name FROM owner_bonuses b
             JOIN effects stat ON stat.id=b.stat_id JOIN bonus_types bt ON bt.id=b.bonus_type_id
             WHERE b.owner_kind='item'",
            )
            .unwrap()
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        let transaction = db.transaction().unwrap();
        for (owner, stat, bonus_type) in rows {
            let correction = bonus_correction(CorrectionKind::ItemBonus, &stat, &bonus_type, "bonus_type");
            let bonus = CorrectedBonus::of(&transaction, &correction).unwrap();
            assert!(bonus.is_present_on(&transaction, owner).unwrap(), "{owner}: {stat} / {bonus_type}");
            assert_eq!(
                bonus.bonus_type_on(&transaction, owner).unwrap(),
                CorrectionValue::Text(bonus_type),
                "{owner}: {stat}"
            );
        }
    }

    #[test]
    fn pending_line_bonuses_match_stored_lines_and_owner_bonuses_in_one_query() {
        use rusqlite::trace::{TraceEvent, TraceEventCodes};
        use std::cell::Cell;
        thread_local! { static READS: Cell<usize> = const { Cell::new(0) }; }
        fn trace(event: TraceEvent<'_>) {
            if matches!(event, TraceEvent::Stmt(_, _)) {
                READS.set(READS.get() + 1);
            }
        }
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let mut canonical = rusqlite::Connection::open_in_memory().unwrap();
        crate::tree::build_from_data(&fixture.join("data"), &mut canonical).unwrap();
        let groupings = crate::groupings::Groupings::from_database(&canonical).unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::build::build_database(
            &groupings,
            &fixture.join("DataFiles"),
            &crate::wiki::WikiOverrides::default(),
            &crate::corrections::Corrections::default(),
            &mut db,
            &ddo_model::DatasetVersion { upstream_sha: "test".into(), built_at: "2026-10-23T00:00:00Z".into() },
        )
        .unwrap();
        db.trace_v2(TraceEventCodes::SQLITE_TRACE_STMT, Some(trace));
        let transaction = db.transaction().unwrap();
        for (kind, owner_kind) in [
            (CorrectionKind::ItemBonus, "item"),
            (CorrectionKind::AugmentBonus, "augment"),
            (CorrectionKind::SetTierBonus, "set_bonus_tier"),
        ] {
            let table = BonusLinkTable::of(kind).unwrap();
            let owners = transaction
                .prepare(&format!("SELECT DISTINCT {} FROM {}", table.owner_column, table.table_name))
                .unwrap()
                .query_map([], |row| row.get::<_, i64>(0))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            for owner in owners {
                let pending = transaction.prepare(&format!("SELECT l.effect_id,b.name,l.value,l.value2 FROM {} l LEFT JOIN bonus_types b ON b.id=l.bonus_type_id WHERE l.{}=?1 ORDER BY l.sort_order", table.table_name, table.owner_column)).unwrap()
                    .query_map([owner], |row| Ok(super::super::modifiers::DerivedEffectLink { alias: None,
                        effect_id: row.get(0)?, bonus_type: row.get::<_, Option<String>>(1)?.map(|name| ddo_model::enums::BonusType::parse(&name).unwrap()), value: row.get(2)?, value2: row.get(3)?, is_split_line: false,
                    })).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
                READS.set(0);
                let rows = line_bonus_rows(&transaction, kind, owner, Some(&pending)).unwrap();
                assert_eq!(READS.get(), 1);
                let stored = line_bonus_rows(&transaction, kind, owner, None).unwrap();
                assert_eq!(rows, stored, "{} {owner}", owner_kind);
                let granted = transaction.prepare("SELECT b.effect_link_order,s.name,t.name,b.amount FROM owner_bonuses b JOIN effects s ON s.id=b.stat_id LEFT JOIN bonus_types t ON t.id=b.bonus_type_id WHERE b.owner_kind=?1 AND b.owner_id=?2").unwrap()
                    .query_map(params![owner_kind,owner], |row| Ok((row.get::<_, i64>(0)? as usize,row.get::<_, String>(1)?,row.get::<_, Option<String>>(2)?,row.get::<_, Option<i64>>(3)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
                let mut unmatched = granted.clone();
                for row in &rows {
                    let granted_row = (row.order, row.stat_name.clone(), row.bonus_type.clone(), row.amount);
                    if row.amount.is_none_or(|amount| amount == 0) {
                        assert!(!granted.contains(&granted_row));
                    } else {
                        let position = unmatched
                            .iter()
                            .position(|bonus| *bonus == granted_row)
                            .expect("resolved line bonus is granted");
                        unmatched.remove(position);
                    }
                }
                assert!(unmatched.is_empty(), "{unmatched:?}");
                for count in [1, 50] {
                    let repeated = pending.iter().cycle().take(count).cloned().collect::<Vec<_>>();
                    READS.set(0);
                    line_bonus_rows(&transaction, kind, owner, Some(&repeated)).unwrap();
                    assert_eq!(READS.get(), 1);
                }
            }
        }
    }

    #[test]
    fn fixed_type_grouping_is_present_to_the_lines_reader() {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data");
        let scratch = tempfile::tempdir().unwrap();
        for entry in
            walkdir::WalkDir::new(&source).into_iter().map(Result::unwrap).filter(|entry| entry.file_type().is_file())
        {
            let target = scratch.path().join(entry.path().strip_prefix(&source).unwrap());
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::copy(entry.path(), target).unwrap();
        }
        let path = scratch.path().join("packs/shared/sets/anger-of-the-avalanche.toml");
        let contents = std::fs::read_to_string(&path).unwrap();
        std::fs::write(
            path,
            contents.replace(
                "bonus_type = \"Artifact\"\neffect = \"Power of the Frozen Thunderstorm\"",
                "effect = \"Power of the Firestorm\"",
            ),
        )
        .unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::tree::build_from_data(scratch.path(), &mut db).unwrap();
        let owner = db.query_row(
            "SELECT t.id FROM set_bonus_tiers t JOIN set_bonuses s ON s.id=t.set_id WHERE s.name='Anger of the Avalanche'",
            [], |row| row.get::<_, i64>(0),
        ).unwrap();
        let correction = bonus_correction(CorrectionKind::SetTierBonus, "Power of the Firestorm", "Equipment", "lines");
        let groupings = crate::groupings::Groupings::from_database(&db).unwrap();
        let transaction = db.transaction().unwrap();
        assert_eq!(
            current_value(
                &transaction,
                &correction,
                correction.correctable_field().unwrap(),
                owner,
                None,
                Some(&groupings)
            )
            .unwrap(),
            CorrectionValue::Text("Power of the Firestorm".into())
        );
    }
}
