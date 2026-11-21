use super::bonus_types::{BonusOrigin, BonusOwner, BonusOwnerKind};
use super::drop_text::DroppedLoot;
use super::effects::{amount_count, split_template, BonusRule, EffectOwner};
use super::modifiers::DerivedEffectLink;
use super::quest_series::QuestSeriesTable;
use super::{joined_non_empty, trimmed_non_empty, BuildReport, TableWriter};
use crate::map::buff::{BuffResolutionSource, ResolvedBuff};
use crate::map::item_version::names_legacy_version;
use crate::map::material;
use crate::map::placement::placement_of;
use crate::xml::items::{AugmentSlotOption, Item};
use anyhow::{bail, ensure, Context, Result};
use ddo_model::enums::{ArmorType, EquipmentSlot, Handedness, ItemCategory, ModifierSource, Provenance};
use rusqlite::params;

pub(super) struct ItemRow<'a> {
    pub(super) name: &'a str,
    pub(super) equipment_slot: EquipmentSlot,
    pub(super) category: ItemCategory,
    pub(super) item_type: Option<&'a str>,
    pub(super) minimum_level: Option<i64>,
    pub(super) enhancement_bonus: Option<i64>,
    pub(super) material_id: Option<i64>,
    pub(super) race_required: Option<String>,
    pub(super) icon: Option<&'a str>,
    pub(super) description: Option<&'a str>,
    pub(super) drop_location: Option<&'a str>,
    pub(super) set_bonus: Option<&'a str>,
    pub(super) accepts_sentience: bool,
    pub(super) is_minor_artifact: bool,
    pub(super) wiki_url: String,
    pub(super) provenance: Provenance,
}

pub(super) struct WeaponStatsRow<'a> {
    pub(super) weapon_type_id: i64,
    pub(super) base_dice_count: Option<i64>,
    pub(super) base_dice_sides: Option<i64>,
    pub(super) base_dice_bonus: Option<i64>,
    pub(super) damage_multiplier: Option<f64>,
    pub(super) critical_threat_range: Option<i64>,
    pub(super) critical_multiplier: Option<i64>,
    pub(super) attack_modifier: Option<String>,
    pub(super) damage_modifier: Option<String>,
    pub(super) handedness: Option<Handedness>,
    pub(super) enhancement_bonus: Option<i64>,
    pub(super) dr_bypasses: &'a [String],
}

pub(super) struct ArmorStatsRow {
    pub(super) armor_type: ArmorType,
    pub(super) armor_bonus: Option<i64>,
    pub(super) max_dex_bonus: Option<i64>,
    pub(super) arcane_spell_failure: Option<i64>,
    pub(super) armor_check_penalty: Option<i64>,
    pub(super) shield_bonus: Option<i64>,
    pub(super) damage_reduction: Option<i64>,
    pub(super) mithral_body: Option<i64>,
    pub(super) adamantine_body: Option<i64>,
}

impl ArmorStatsRow {
    fn from_item(item: &Item, armor_type: ArmorType) -> Self {
        Self {
            armor_type,
            armor_bonus: item.armor_bonus,
            max_dex_bonus: item.maximum_dexterity_bonus,
            arcane_spell_failure: item.arcane_spell_failure,
            armor_check_penalty: item.armor_check_penalty,
            shield_bonus: item.shield_bonus,
            damage_reduction: item.damage_reduction,
            mithral_body: item.mithral_body,
            adamantine_body: item.adamantine_body,
        }
    }
}

impl<'a> TableWriter<'a> {
    pub(super) fn write_item(
        &mut self,
        item: &Item,
        drop_location: Option<&str>,
        report: &mut BuildReport,
    ) -> Result<()> {
        let placement = match placement_of(&item.equipment_slots, item.weapon.as_deref(), item.armor.as_deref())? {
            Ok(placement) => placement,
            Err(cosmetic_exclusion) => {
                self.transaction.execute(
                    "INSERT OR REPLACE INTO excluded_items (name, reason) VALUES (?1, ?2)",
                    params![item.name.trim(), cosmetic_exclusion.reason()],
                )?;
                report.skipped_cosmetic_item_count += 1;
                return Ok(());
            }
        };
        let placement = &placement;
        let mut enhancement_bonus: Option<i64> = None;
        let mut resolved_buffs = Vec::with_capacity(item.buffs.len());
        for buff in &item.buffs {
            match self.buff_resolver.resolved(buff)? {
                ResolvedBuff::EnhancementBonus(value) => {
                    enhancement_bonus = Some(value);
                    report.family_buff_count += 1;
                }
                resolved_buff => {
                    match &resolved_buff {
                        ResolvedBuff::Bonuses { source: BuffResolutionSource::Family, .. } => {
                            report.family_buff_count += 1;
                        }
                        ResolvedBuff::Bonuses { source: BuffResolutionSource::EffectFallback, .. } => {
                            report.effect_fallback_buff_count += 1;
                        }
                        ResolvedBuff::Effect { .. } | ResolvedBuff::NamedEffect { .. } => report.effect_buff_count += 1,
                        ResolvedBuff::EnhancementBonus(_) => unreachable!("matched above"),
                    }
                    resolved_buffs.push((buff, resolved_buff));
                }
            }
        }

        let material_id = match material::normalized_name(item.material.as_deref()) {
            Some(material_name) => Some(self.ensure_material(&material_name)?),
            None => None,
        };
        let item_name = item.name.trim();
        let item_id = self.insert_item_row(&ItemRow {
            name: item_name,
            equipment_slot: placement.equipment_slot,
            category: placement.category,
            item_type: placement.item_type.as_deref(),
            minimum_level: item.minimum_level,
            enhancement_bonus,
            material_id,
            race_required: item.race_required(),
            icon: item.icon.as_deref().map(str::trim),
            description: trimmed_non_empty(item.description.as_deref()),
            drop_location: trimmed_non_empty(drop_location),
            set_bonus: item.set_bonus_names.first().map(|s| s.trim()),
            accepts_sentience: item.accepts_sentience,
            is_minor_artifact: item.is_minor_artifact,
            wiki_url: item_wiki_url(item_name),
            provenance: Provenance::Maetrim,
        })?;

        if let Some(weapon_type) = placement.weapon_type() {
            let base_dice = item.base_dice.as_ref();
            self.insert_weapon_stats(
                item_id,
                &WeaponStatsRow {
                    weapon_type_id: weapon_type.id,
                    base_dice_count: base_dice.and_then(|d| d.count),
                    base_dice_sides: base_dice.and_then(|d| d.sides),
                    base_dice_bonus: base_dice.and_then(|d| d.bonus),
                    damage_multiplier: item.damage_multiplier,
                    critical_threat_range: item.critical_threat_range,
                    critical_multiplier: item.critical_multiplier,
                    attack_modifier: joined_non_empty(&item.attack_modifiers, ", "),
                    damage_modifier: joined_non_empty(&item.damage_modifiers, ", "),
                    handedness: placement.handedness,
                    enhancement_bonus,
                    dr_bypasses: &item.dr_bypasses,
                },
            )?;
            if weapon_type.is_shield {
                self.insert_armor_stats(item_id, &ArmorStatsRow::from_item(item, ArmorType::Shield))?;
            }
        }
        if let Some(armor_name) = item.armor.as_deref() {
            let Some(armor_type) = ArmorType::parse(armor_name.trim()) else {
                bail!("unknown <Armor> type {armor_name:?}");
            };
            self.insert_armor_stats(item_id, &ArmorStatsRow::from_item(item, armor_type))?;
        }

        let mut links = Vec::new();
        let item_owner = BonusOwner { kind: BonusOwnerKind::Item, name: item_name, family: None, option: None };
        for (buff, resolved_buff) in resolved_buffs {
            let first_link = links.len();
            match resolved_buff {
                ResolvedBuff::NamedEffect { name } => {
                    let family = self.effects.family_named(&name).with_context(|| {
                        format!("item {item_name:?} buff {:?} routes to missing effect {name:?}", buff.kind)
                    })?;
                    let effect_id = family.id;
                    let (value, value2) = self.buff_resolver.link_amounts(buff, family.amount_count);
                    let bonus_type =
                        if family.uses_link_type { self.buff_resolver.link_bonus_type(buff)? } else { None };
                    links.push(DerivedEffectLink {
                        alias: None,
                        effect_id,
                        bonus_type,
                        value,
                        value2,
                        is_split_line: false,
                    });
                }
                ResolvedBuff::Bonuses { source: _, stats } => {
                    let mut typed_stats = Vec::with_capacity(stats.len());
                    for resolved_stat in &stats {
                        let stat = resolved_stat.stat;
                        let value = resolved_stat.amount(buff);
                        let bonus_type = self.bonus_type_of(
                            &BonusOrigin {
                                owner: &item_owner,
                                source_name: buff.kind.trim(),
                                target_name: crate::map::effect_map::EFFECT_MAP
                                    .family
                                    .groups
                                    .get(buff.kind.trim())
                                    .map(String::as_str)
                                    .unwrap_or(stat.name),
                                value,
                            },
                            resolved_stat.bonus_type,
                        )?;
                        typed_stats.push((resolved_stat, bonus_type));
                    }
                    let first_type = typed_stats.first().map(|(_, bonus_type)| *bonus_type);
                    if buff.kind.trim() == "SkillBonusAbility" {
                        ensure!(
                            typed_stats.iter().all(|(_, bonus_type)| Some(*bonus_type) == first_type),
                            "item {item_name:?} skill group has multiple bonus types"
                        );
                        let ability =
                            buff.target.as_deref().expect("resolved skill group has an ability target").trim();
                        let group_id = self.effects.ensure_group(&format!("{ability} Skills"))?;
                        links.push(DerivedEffectLink {
                            alias: None,
                            effect_id: group_id,
                            bonus_type: first_type,
                            value: buff.value,
                            value2: None,
                            is_split_line: false,
                        });
                    } else {
                        let family_name =
                            self.buff_resolver.family_name(buff, stats.first().map(|stat| stat.stat.name));
                        let family_text = self.buff_resolver.family_template(buff);
                        let (mut verbose_name_template, description_template) = if family_text.is_empty() {
                            (family_name.clone(), None)
                        } else {
                            split_template(&family_text)
                        };
                        let constant_amounts: std::collections::BTreeSet<i64> = stats
                            .iter()
                            .filter_map(|stat| match stat.amount_from {
                                crate::map::buff::AmountFrom::Constant(amount) => Some(amount),
                                _ => None,
                            })
                            .collect();
                        if constant_amounts.len() == 1
                            && !verbose_name_template.chars().any(|character| character.is_ascii_digit())
                            && !verbose_name_template.contains("{1}")
                            && !verbose_name_template.contains("{2}")
                        {
                            verbose_name_template
                                .push_str(&format!(" {:+}", constant_amounts.first().expect("one constant")));
                        }
                        let required_count = stats
                            .iter()
                            .map(|stat| match stat.amount_from {
                                crate::map::buff::AmountFrom::ItemValue1 => 1,
                                crate::map::buff::AmountFrom::ItemValue2 => 2,
                                crate::map::buff::AmountFrom::Constant(_) => 0,
                            })
                            .max()
                            .unwrap_or(0);
                        for slot in 1..=required_count {
                            let placeholder = format!("{{{slot}}}");
                            if !verbose_name_template.contains(&placeholder)
                                && !description_template.as_deref().is_some_and(|text| text.contains(&placeholder))
                            {
                                verbose_name_template.push_str(&format!(" {placeholder}"));
                            }
                        }
                        let count = amount_count(&verbose_name_template, description_template.as_deref());
                        let effect_id = self.effects.ensure_family(
                            &family_name,
                            &verbose_name_template,
                            description_template.as_deref(),
                            count,
                        )?;
                        let single_stat = typed_stats.len() == 1;
                        let link_types = typed_stats
                            .iter()
                            .filter_map(|(resolved, bonus_type)| {
                                (single_stat || self.effects.stat_uses_link_type(effect_id, resolved.stat))
                                    .then_some(*bonus_type)
                            })
                            .collect::<Vec<_>>();
                        let link_type = link_types.first().copied();
                        if link_type.is_some() {
                            ensure!(link_types.iter().all(|bonus_type| Some(*bonus_type) == link_type),
                            "item {item_name:?} family {family_name:?} resolves to multiple bonus types for one %b1 slot");
                        }
                        let rules: Vec<BonusRule> = typed_stats
                            .into_iter()
                            .map(|(resolved_stat, bonus_type)| {
                                let (amount_from, constant) = match resolved_stat.amount_from {
                                    crate::map::buff::AmountFrom::ItemValue1 => (1, None),
                                    crate::map::buff::AmountFrom::ItemValue2 => (2, None),
                                    crate::map::buff::AmountFrom::Constant(amount) => (0, Some(amount)),
                                };
                                BonusRule {
                                    stat: resolved_stat.stat,
                                    bonus_type: (!(single_stat
                                        || self.effects.stat_uses_link_type(effect_id, resolved_stat.stat)))
                                    .then_some(bonus_type),
                                    amount_from,
                                    constant,
                                }
                            })
                            .collect();
                        self.effects.ensure_bonus_rules(effect_id, &rules)?;
                        let family = self.effects.family(effect_id).expect("effect cached");
                        let defaults = self.buff_resolver.definition_defaults(buff.kind.trim(), family.amount_count);
                        let (mut value, mut value2) = self.buff_resolver.link_amounts(buff, family.amount_count);
                        if family.is_group {
                            value = value.or(defaults.0);
                            value2 = value2.or(defaults.1);
                        } else if defaults != (None, None)
                            && self.written.written_effect_defaults.insert((effect_id, defaults.0, defaults.1))
                        {
                            self.effects.set_defaults(effect_id, defaults)?;
                        }
                        links.push(DerivedEffectLink {
                            alias: None,
                            effect_id,
                            bonus_type: link_type,
                            value,
                            value2,
                            is_split_line: false,
                        });
                    }
                }
                ResolvedBuff::Effect { .. } => {
                    let family_name = self.buff_resolver.family_name(buff, None);
                    let family_text = self.buff_resolver.family_template(buff);
                    let (verbose_name_template, description_template) =
                        split_template(if family_text.is_empty() { &family_name } else { &family_text });
                    let count = amount_count(&verbose_name_template, description_template.as_deref());
                    let effect_id = self.effects.ensure_family(
                        &family_name,
                        &verbose_name_template,
                        description_template.as_deref(),
                        count,
                    )?;
                    let defaults = self.buff_resolver.definition_defaults(buff.kind.trim(), count);
                    if defaults != (None, None)
                        && self.written.written_effect_defaults.insert((effect_id, defaults.0, defaults.1))
                    {
                        self.effects.set_defaults(effect_id, defaults)?;
                    }
                    let uses_link_type = self.effects.family(effect_id).expect("effect cached").uses_link_type;
                    let bonus_type = if uses_link_type { self.buff_resolver.link_bonus_type(buff)? } else { None };
                    let family_amount_count = self.effects.family(effect_id).expect("effect cached").amount_count;
                    let (value, value2) = self.buff_resolver.link_amounts(buff, family_amount_count);
                    links.push(DerivedEffectLink {
                        alias: None,
                        effect_id,
                        bonus_type,
                        value,
                        value2,
                        is_split_line: false,
                    });
                }
                ResolvedBuff::EnhancementBonus(_) => unreachable!("filtered above"),
            }
            let source_family = self.buff_resolver.source_family_name(buff);
            for link in &mut links[first_link..] {
                link.alias = self.effects.alias_named(link.effect_id, &source_family).map(str::to_string);
            }
        }

        self.queue_effect_links(&item_owner, item_id, links);

        for (slot_order, augment_slot) in item.augment_slots.iter().enumerate() {
            let (label, spelling_correction) = self.effective_socket_label(&augment_slot.kind)?;
            if let Some(removal) = self.retired_socket_removal(item, &label) {
                self.record_item_socket_correction(removal, spelling_correction);
                continue;
            }
            let relabel = self.socket_relabel(item, augment_slot, &label);
            let slot_type_id = if let Some(correction) = relabel {
                let target = correction.to.as_text().context("a socket relabel has a target label")?;
                let id = super::corrections::required_socket_label_id(self.transaction, target)?;
                self.record_item_socket_correction(correction, spelling_correction);
                id
            } else {
                self.ensure_augment_slot_type(&augment_slot.kind)?
            };
            self.insert_item_augment_slot(item_id, slot_order, slot_type_id)?;
            if !augment_slot.options.is_empty() {
                let label = self.transaction.query_row(
                    "SELECT label FROM augment_slot_types WHERE id=?1",
                    [slot_type_id],
                    |row| row.get(0),
                )?;
                self.pending_socket_options.push(super::SocketOptionList {
                    label,
                    options: augment_slot.options.clone(),
                    socket: (item_id, slot_order as i64),
                    item_slug: crate::tree::xml_entity_slug("items", None, item_name, None),
                    item_name: item_name.to_owned(),
                });
            }
        }

        self.write_modifiers(ModifierSource::Item, item_id, &item.effects)?;
        for (clickie_order, effect) in item.effects.iter().filter(|e| e.types[0] == "ItemClickie").enumerate() {
            let Some(clickie_name) = effect.targets.first().map(|s| s.trim()) else {
                bail!("{item_name}: ItemClickie effect without an <Item>");
            };
            let clickie_id = self.written.clickie_ids_by_name.get(clickie_name).copied();
            self.transaction.execute(
                "INSERT INTO item_clickies (item_id, sort_order, name, clickie_id) VALUES (?1, ?2, ?3, ?4)",
                params![item_id, clickie_order as i64, clickie_name, clickie_id],
            )?;
        }
        for set_name in item.set_bonus_names.iter().filter_map(|s| trimmed_non_empty(Some(s))) {
            self.pending_set_item_links.push((item_id, set_name.to_string()));
        }

        if let Some(drop_location) = drop_location {
            self.flag_item_naming_only_legacy_sources(item_id, drop_location, report)?;
            self.link_item_to_quests(item_id, drop_location, report)?;
        }
        report.written_item_count += 1;
        Ok(())
    }

    pub(super) fn prepare_item_socket_types(&mut self, item: &Item) -> Result<()> {
        if placement_of(&item.equipment_slots, item.weapon.as_deref(), item.armor.as_deref())?.is_err() {
            return Ok(());
        }
        let sockets = item
            .augment_slots
            .iter()
            .map(|socket| Ok((socket, self.effective_socket_label(&socket.kind)?.0)))
            .collect::<Result<Vec<_>>>()?;
        for correction in self.retired_socket_removals.iter().filter(|correction| correction.name == item.name.trim()) {
            let label = correction.socket.as_deref().context("a socket removal names a label")?;
            let socket_count = sockets.iter().filter(|(_, effective)| effective == label).count();
            ensure!(
                socket_count <= 1,
                "{}: socket {label:?} matches {socket_count} sockets on {:?}",
                correction.file_name,
                correction.name
            );
        }
        for correction in self.socket_relabels.iter().filter(|correction| correction.name == item.name.trim()) {
            let option_name = correction.option.as_deref().context("a socket relabel names an option")?;
            let socket_count = sockets
                .iter()
                .filter(|(socket, label)| {
                    self.retired_socket_removal(item, label).is_none()
                        && self.socket_has_authored_option(item, socket, option_name)
                })
                .count();
            ensure!(
                socket_count <= 1,
                "{}: option {option_name:?} matches {} sockets on {:?}",
                correction.file_name,
                socket_count,
                correction.name
            );
        }
        for (socket, label) in sockets {
            if self.retired_socket_removal(item, &label).is_none()
                && self.socket_relabel(item, socket, &label).is_none()
            {
                self.ensure_augment_slot_type(&socket.kind)?;
            }
        }
        Ok(())
    }

    fn record_item_socket_correction(
        &mut self,
        correction: &'a crate::corrections::Correction,
        spelling: Option<&'a crate::corrections::Correction>,
    ) {
        self.applied_item_socket_corrections.push(correction);
        if let Some(spelling) = spelling {
            if !self.applied_socket_label_corrections.iter().any(|applied| std::ptr::eq(*applied, spelling)) {
                self.applied_socket_label_corrections.push(spelling);
            }
        }
    }

    fn retired_socket_removal(&self, item: &Item, label: &str) -> Option<&'a crate::corrections::Correction> {
        self.retired_socket_removals.iter().copied().find(|correction| {
            correction.name == item.name.trim()
                && correction.socket.as_deref() == Some(label)
                && matches!(correction.from, crate::corrections::CorrectionValue::Integer(0))
        })
    }

    fn effective_socket_label(&self, upstream: &str) -> Result<(String, Option<&'a crate::corrections::Correction>)> {
        let label = crate::map::augment_slot::AugmentSlotType::parse(upstream).label;
        let spelling = self.socket_label_corrections.get(label.as_str()).copied();
        let effective = match spelling {
            Some(correction) => correction.to.as_text().context("a socket label rename has a target")?.to_owned(),
            None => label,
        };
        Ok((effective, spelling))
    }

    fn socket_relabel(
        &self,
        item: &Item,
        socket: &crate::xml::items::ItemAugmentSlot,
        label: &str,
    ) -> Option<&'a crate::corrections::Correction> {
        self.socket_relabels.iter().copied().find(|correction| {
            correction.name == item.name.trim()
                && correction.from.as_text() == Some(label)
                && correction.option.as_deref().is_some_and(|name| self.socket_has_authored_option(item, socket, name))
        })
    }

    fn socket_has_authored_option(
        &self,
        item: &Item,
        socket: &crate::xml::items::ItemAugmentSlot,
        option_name: &str,
    ) -> bool {
        let item_slug = crate::tree::xml_entity_slug("items", None, item.name.trim(), None);
        socket.options.iter().any(|option| {
            option.name.trim() == option_name
                || self.socket_option_name_corrections.iter().any(|correction| {
                    correction.from.as_text() == Some(option.name.trim())
                        && correction.to.as_text() == Some(option_name)
                        && self.existing_socket_option_sets.item_has_set(&item_slug, &correction.name)
                })
        })
    }

    pub(super) fn apply_socket_label_corrections(&mut self) -> Result<()> {
        let mut label_corrections = self.socket_label_corrections.values().copied().collect::<Vec<_>>();
        label_corrections.sort_by(|left, right| left.name.cmp(&right.name));
        for correction in label_corrections {
            if self.applied_socket_label_corrections.iter().any(|applied| std::ptr::eq(*applied, correction)) {
                continue;
            }
            let target = correction.to.as_text().context("a socket label rename has a target")?;
            let mut applied = false;
            for spelling in correction.upstream_names() {
                let Some(id) = self.written.augment_slot_type_ids_by_label.get(spelling).copied() else {
                    continue;
                };
                let destination = super::corrections::canonical_socket_label_id(self.transaction, id, target)?;
                if id == destination && self.written.augment_slot_type_ids_by_label.get(target) == Some(&id) {
                    continue;
                }
                super::corrections::rename_socket_label(self.transaction, id, target)?;
                for known_id in self.written.augment_slot_type_ids_by_label.values_mut() {
                    if *known_id == id {
                        *known_id = destination;
                    }
                }
                self.written.augment_slot_type_ids_by_label.insert(target.to_string(), destination);
                applied = true;
            }
            if applied {
                self.applied_socket_label_corrections.push(correction);
            }
        }
        Ok(())
    }

    pub(super) fn write_socket_option_sets(
        &mut self,
        existing: &super::ExistingSocketOptionSets,
        references: &super::wiki::OverlayReferences,
    ) -> Result<()> {
        self.apply_socket_label_corrections()?;
        let socket_labels = self.transaction.prepare("SELECT s.item_id,s.sort_order,t.label FROM item_augment_slots s JOIN augment_slot_types t ON t.id=s.slot_id")?
            .query_map([], |row| Ok(((row.get::<_, i64>(0)?,row.get::<_, i64>(1)?),row.get::<_, String>(2)?)))?
            .collect::<rusqlite::Result<std::collections::BTreeMap<_,_>>>()?;
        for options in &mut self.pending_socket_options {
            options.label = socket_labels[&options.socket].clone();
            if let Some(name) = references.corrected_name("items", options.socket.0) {
                options.item_slug = crate::tree::xml_entity_slug("items", None, &name, None);
            }
        }
        for group in super::socket_option_sets::matched_socket_option_sets(
            std::mem::take(&mut self.pending_socket_options),
            existing,
        )? {
            self.transaction.execute("INSERT INTO socket_option_sets(slug,name,variant,slot_id) VALUES(?1,?2,?3,(SELECT id FROM augment_slot_types WHERE label=?4))",params![group.slug,group.name,group.variant,group.label])?;
            let set_id = self.transaction.last_insert_rowid();
            for (item_id, position) in group.sockets {
                self.transaction.execute(
                    "UPDATE item_augment_slots SET option_set_id=?1 WHERE item_id=?2 AND sort_order=?3",
                    params![set_id, item_id, position],
                )?;
            }
            for (position, option) in group.options.iter().enumerate() {
                self.write_socket_option(&group.slug, set_id, position, option)?;
            }
        }
        Ok(())
    }

    fn write_socket_option(
        &mut self,
        set_slug: &str,
        set_id: i64,
        option_order: usize,
        slot_option: &AugmentSlotOption,
    ) -> Result<()> {
        self.transaction.execute(
            "INSERT INTO socket_options (set_id, option_order, name, description, min_level, icon)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                set_id,
                option_order as i64,
                slot_option.name,
                trimmed_non_empty(Some(&slot_option.description)),
                slot_option.minimum_level,
                trimmed_non_empty(slot_option.icon.as_deref()),
            ],
        )?;
        let option_id = self.transaction.last_insert_rowid();
        for (grant_order, slot_type_name) in slot_option.granted_augments.iter().enumerate() {
            let granted_slot_type_id = self.ensure_augment_slot_type(slot_type_name)?;
            self.transaction.execute(
                "INSERT INTO socket_option_grants (option_id, sort_order, slot_id) VALUES (?1, ?2, ?3)",
                params![option_id, grant_order as i64, granted_slot_type_id],
            )?;
        }
        for set_name in slot_option.set_bonus_names.iter().filter_map(|s| trimmed_non_empty(Some(s))) {
            self.pending_set_option_links.push((option_id, set_name.to_string()));
        }
        self.write_modifiers(ModifierSource::SocketOption, option_id, &slot_option.effects)?;
        self.pending_derived_effects.push(super::PendingDerivedEffects {
            owner_kind: EffectOwner::SocketOption,
            owner_id: option_id,
            owner_name: set_slug.to_string(),
            family: None,
            source: super::PendingEffectSource::Effects {
                option_name: Some(slot_option.name.clone()),
                effects: slot_option.effects.clone(),
            },
        });
        Ok(())
    }

    pub(super) fn insert_item_row(&self, row: &ItemRow) -> Result<i64> {
        self.transaction.execute(
            "INSERT INTO items (name, slot_id, item_category, item_type, minimum_level, enhancement_bonus, material_id, race_required,
                                icon, description, drop_location, set_bonus, accepts_sentience, is_minor_artifact, is_legacy, wiki_url, provenance, slug, id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
            params![row.name, row.equipment_slot.id(), row.category.as_str(), row.item_type, row.minimum_level, row.enhancement_bonus, row.material_id, row.race_required, row.icon, row.description, row.drop_location, row.set_bonus, row.accepts_sentience, row.is_minor_artifact, names_legacy_version(row.name), row.wiki_url, row.provenance.as_str(), crate::tree::xml_entity_slug("items", None, row.name, None), (row.provenance == Provenance::Wiki).then(|| crate::tree::xml_overlay_id("items", &crate::tree::xml_entity_slug("items", None, row.name, None))).transpose()?],
        )?;
        Ok(self.transaction.last_insert_rowid())
    }

    pub(super) fn insert_weapon_stats(&self, item_id: i64, row: &WeaponStatsRow) -> Result<()> {
        let damage_text = weapon_damage_text(
            row.damage_multiplier,
            row.base_dice_count.zip(row.base_dice_sides).map(|(count, sides)| (count, sides, row.base_dice_bonus)),
            row.enhancement_bonus,
            row.dr_bypasses,
        );
        let critical_text = match (row.critical_threat_range, row.critical_multiplier) {
            (Some(threat_range), Some(multiplier)) if threat_range <= 1 => Some(format!("20 / x{multiplier}")),
            (Some(threat_range), Some(multiplier)) => Some(format!("{}-20 / x{multiplier}", 21 - threat_range)),
            _ => None,
        };
        self.transaction.execute(
            "INSERT INTO item_weapon_stats (item_id, weapon_type_id, base_dice_count, base_dice_sides, base_dice_bonus, damage_multiplier,
                                            critical_threat_range, critical_multiplier, attack_modifier, damage_modifier, handedness, damage, critical)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                item_id,
                row.weapon_type_id,
                row.base_dice_count,
                row.base_dice_sides,
                row.base_dice_bonus,
                row.damage_multiplier,
                row.critical_threat_range,
                row.critical_multiplier,
                row.attack_modifier,
                row.damage_modifier,
                row.handedness.map(|h| h.as_str()),
                damage_text,
                critical_text,
            ],
        )?;
        for bypass in row.dr_bypasses {
            let bypass = bypass.trim();
            if bypass.is_empty() || bypass == "-" {
                continue;
            }
            self.transaction.execute(
                "INSERT OR IGNORE INTO item_dr_bypass (item_id, bypass) VALUES (?1, ?2)",
                params![item_id, bypass],
            )?;
        }
        Ok(())
    }

    pub(super) fn insert_armor_stats(&self, item_id: i64, row: &ArmorStatsRow) -> Result<()> {
        self.transaction.execute(
            "INSERT OR REPLACE INTO item_armor_stats (item_id, armor_type, armor_bonus, max_dex_bonus, arcane_spell_failure, armor_check_penalty,
                                                      shield_bonus, damage_reduction, mithral_body, adamantine_body)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                item_id,
                row.armor_type.as_str(),
                row.armor_bonus,
                row.max_dex_bonus,
                row.arcane_spell_failure,
                row.armor_check_penalty,
                row.shield_bonus,
                row.damage_reduction,
                row.mithral_body,
                row.adamantine_body,
            ],
        )?;
        Ok(())
    }

    pub(super) fn insert_item_augment_slot(&self, item_id: i64, sort_order: usize, slot_type_id: i64) -> Result<()> {
        self.transaction.execute(
            "INSERT INTO item_augment_slots (item_id, sort_order, slot_id) VALUES (?1, ?2, ?3)",
            params![item_id, sort_order as i64, slot_type_id],
        )?;
        Ok(())
    }

    fn flag_item_naming_only_legacy_sources(
        &self,
        item_id: i64,
        drop_location: &str,
        report: &mut BuildReport,
    ) -> Result<()> {
        let item_minimum_level = DroppedLoot::Item(item_id).minimum_level(self.transaction)?;
        if self.drop_text_linker.legacy_texts_naming_every_segment(drop_location, item_minimum_level).is_none() {
            return Ok(());
        }
        self.transaction.execute("UPDATE items SET is_legacy = 1 WHERE id = ?1", params![item_id])?;
        report.legacy_source_flagged_count += 1;
        Ok(())
    }

    fn link_item_to_quests(&self, item_id: i64, drop_location: &str, report: &mut BuildReport) -> Result<()> {
        for linked_quest in self.link_to_drop_text_quests(DroppedLoot::Item(item_id), drop_location)? {
            report.quest_loot_link_count += 1;
            if linked_quest.is_wiki_quest {
                report.drop_text_wiki_quest_link_count += 1;
            }
            if linked_quest.is_newly_rare {
                report.drop_text_rare_link_count += 1;
            }
        }
        self.link_to_sources_named_in_drop_text(DroppedLoot::Item(item_id), drop_location, report)?;
        for linked_table in self.link_item_to_drop_text_reward_givers(item_id, drop_location)? {
            match linked_table {
                QuestSeriesTable::QuestChains => report.drop_text_quest_chain_reward_count += 1,
                QuestSeriesTable::Sagas => report.drop_text_saga_reward_count += 1,
            }
        }
        Ok(())
    }
}

pub(crate) fn weapon_damage_text(
    damage_multiplier: Option<f64>,
    base_dice: Option<(i64, i64, Option<i64>)>,
    enhancement_bonus: Option<i64>,
    dr_bypasses: &[String],
) -> Option<String> {
    let (dice_count, dice_sides, dice_bonus) = base_dice?;
    let mut text = String::new();
    if let Some(multiplier) = damage_multiplier {
        text.push_str(&multiplier.to_string());
    }
    text.push_str(&format!("[{dice_count}d{dice_sides}"));
    if let Some(bonus) = dice_bonus.filter(|b| *b != 0) {
        text.push_str(&format!("{bonus:+}"));
    }
    text.push(']');
    if let Some(enhancement) = enhancement_bonus {
        text.push_str(&format!(" + {enhancement}"));
    }
    let bypasses: Vec<&str> = dr_bypasses.iter().map(|s| s.trim()).filter(|s| !s.is_empty() && *s != "-").collect();
    if !bypasses.is_empty() {
        text.push(' ');
        text.push_str(&bypasses.join(", "));
    }
    Some(text)
}

pub(super) fn item_wiki_url(item_name: &str) -> String {
    format!("https://ddowiki.com/page/Item:{}", item_name.replace(' ', "_").replace('+', "%2B"))
}
