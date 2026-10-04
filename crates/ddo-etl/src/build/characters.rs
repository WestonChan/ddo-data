use super::{json_number_array, json_string_array, trimmed_non_empty, BuildReport, TableWriter};
use crate::xml::classes::{self, FeatSlot};
use crate::xml::feats::{self, Feat};
use crate::xml::{races, stances};
use anyhow::{bail, Context, Result};
use ddo_model::enums::{AbilityOwner, FeatSource, ModifierSource, RequirementOwner, SaveProgression};
use ddo_model::stats::Stat;
use rusqlite::params;
use std::path::Path;

impl TableWriter<'_> {
    pub(super) fn write_standard_feats(&mut self, path: &Path, report: &mut BuildReport) -> Result<()> {
        if !path.is_file() {
            return Ok(());
        }
        for feat in feats::parse(path)? {
            self.write_feat(&feat, FeatSource::Standard, None)?;
            report.feat_count += 1;
        }
        Ok(())
    }

    fn write_feat(&mut self, feat: &Feat, source: FeatSource, source_id: Option<i64>) -> Result<i64> {
        let (auto_acquire_requirements, ignores_requirements) = match &feat.automatic_acquisition {
            Some(acquisition) => (Some(&acquisition.requirements), acquisition.ignores_requirements),
            None => (None, false),
        };
        self.transaction
            .execute(
                "INSERT INTO feats (name, source_kind, source_id, description, icon, acquire, max_times_acquire, sphere,
                                auto_acquire_ignores_requirements)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    feat.name,
                    source.as_str(),
                    source_id,
                    trimmed_non_empty(feat.description.as_deref()),
                    trimmed_non_empty(feat.icon.as_deref()),
                    trimmed_non_empty(feat.acquire.as_deref()),
                    feat.maximum_times_acquired,
                    trimmed_non_empty(feat.sphere.as_deref()),
                    ignores_requirements,
                ],
            )
            .with_context(|| format!("feat {:?} from {:?} {:?}", feat.name, source, source_id))?;
        let feat_id = self.transaction.last_insert_rowid();
        self.written.feat_ids_by_key.entry((feat.name.clone(), source, source_id)).or_insert(feat_id);

        for group_name in &feat.groups {
            self.transaction.execute(
                "INSERT OR IGNORE INTO feat_groups (feat_id, group_name) VALUES (?1, ?2)",
                params![feat_id, group_name],
            )?;
        }
        for conditional_group in &feat.conditional_groups {
            self.transaction.execute(
                "INSERT INTO feat_conditional_groups (feat_id, groups) VALUES (?1, ?2)",
                params![feat_id, json_string_array(&conditional_group.groups).unwrap_or_else(|| "[]".into())],
            )?;
            let conditional_group_id = self.transaction.last_insert_rowid();
            if let Some(requirements) = &conditional_group.requirements {
                self.write_requirements(RequirementOwner::FeatConditionalGroup, conditional_group_id, requirements)?;
            }
        }
        for (sort_order, sub_item) in feat.sub_items.iter().enumerate() {
            self.transaction.execute(
                "INSERT INTO feat_sub_items (feat_id, sort_order, name, icon, description) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    feat_id,
                    sort_order as i64,
                    sub_item.name.trim(),
                    trimmed_non_empty(sub_item.icon.as_deref()),
                    trimmed_non_empty(sub_item.description.as_deref())
                ],
            )?;
        }
        if let Some(requirements) = &feat.requirements {
            self.write_requirements(RequirementOwner::Feat, feat_id, requirements)?;
        }
        if let Some(requirements) = auto_acquire_requirements {
            self.write_requirements(RequirementOwner::FeatAutoAcquire, feat_id, requirements)?;
        }
        self.write_abilities(AbilityOwner::Feat, feat_id, &feat.stances, &feat.dcs, feat.attack.as_ref())?;
        self.write_modifiers(ModifierSource::Feat, feat_id, &feat.effects)?;
        self.write_attack_bonuses(AbilityOwner::Feat, feat_id, feat.attack.as_ref())?;
        self.pending_derived_effects.push((
            super::effects::EffectOwner::Feat,
            feat_id,
            feat.name.trim().to_string(),
            feat.effects.clone(),
        ));
        Ok(feat_id)
    }

    pub(super) fn write_standalone_stances(&mut self, path: &Path, report: &mut BuildReport) -> Result<()> {
        if !path.is_file() {
            return Ok(());
        }
        let standalone_stances = stances::parse(path)?;
        self.write_stances(AbilityOwner::Standalone, 0, &standalone_stances)?;
        report.standalone_stance_count = standalone_stances.len();
        Ok(())
    }

    fn write_stances(&mut self, owner: AbilityOwner, owner_id: i64, stances: &[feats::Stance]) -> Result<()> {
        for (sort_order, stance) in stances.iter().enumerate() {
            self.transaction.execute(
                "INSERT INTO stances (owner_kind, owner_id, sort_order, name, description, icon, group_name, auto_controlled, incompatible)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    owner.as_str(),
                    owner_id,
                    sort_order as i64,
                    stance.name.trim(),
                    trimmed_non_empty(stance.description.as_deref()),
                    trimmed_non_empty(stance.icon.as_deref()),
                    trimmed_non_empty(stance.group.as_deref()),
                    stance.auto_controlled.is_some(),
                    json_string_array(&stance.incompatible_stances),
                ],
            )?;
            let stance_id = self.transaction.last_insert_rowid();
            if let Some(requirements) = &stance.requirements {
                self.write_requirements(RequirementOwner::Stance, stance_id, requirements)?;
            }
            self.write_modifiers(ModifierSource::Stance, stance_id, &stance.effects)?;
        }
        Ok(())
    }

    pub(super) fn write_abilities(
        &mut self,
        owner: AbilityOwner,
        owner_id: i64,
        stances: &[feats::Stance],
        dcs: &[feats::Dc],
        attack: Option<&feats::Attack>,
    ) -> Result<()> {
        self.write_stances(owner, owner_id, stances)?;
        for (sort_order, dc) in dcs.iter().enumerate() {
            let amounts = dc.amount.as_ref().map(|v| v.numbers()).transpose().map_err(anyhow::Error::msg)?;
            self.transaction.execute(
                "INSERT INTO dcs (owner_kind, owner_id, sort_order, name, description, icon, dc_type, dc_versus, mod_ability, amount,
                                  tactical, other, skill, class_level, base_class_level)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    owner.as_str(),
                    owner_id,
                    sort_order as i64,
                    trimmed_non_empty(dc.name.as_deref()),
                    trimmed_non_empty(dc.description.as_deref()),
                    trimmed_non_empty(dc.icon.as_deref()),
                    trimmed_non_empty(dc.dc_type.as_deref()),
                    trimmed_non_empty(dc.dc_versus.as_deref()),
                    json_string_array(&dc.modifier_abilities),
                    amounts.as_deref().and_then(json_number_array),
                    trimmed_non_empty(dc.tactical.as_deref()),
                    trimmed_non_empty(dc.other.as_deref()),
                    trimmed_non_empty(dc.skill.as_deref()),
                    trimmed_non_empty(dc.class_level.as_deref()),
                    trimmed_non_empty(dc.base_class_level.as_deref()),
                ],
            )?;
        }
        if let Some(attack) = attack {
            self.transaction.execute(
                "INSERT INTO attacks (owner_kind, owner_id, name, description, icon, cooldown_seconds, duration_seconds)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    owner.as_str(),
                    owner_id,
                    trimmed_non_empty(attack.name.as_deref()),
                    trimmed_non_empty(attack.description.as_deref()),
                    trimmed_non_empty(attack.icon.as_deref()),
                    attack.cooldown_seconds,
                    attack.duration_seconds(),
                ],
            )?;
        }
        Ok(())
    }

    pub(super) fn write_attack_bonuses(
        &mut self,
        owner: AbilityOwner,
        owner_id: i64,
        attack: Option<&feats::Attack>,
    ) -> Result<()> {
        let Some(attack) = attack else {
            return Ok(());
        };
        let (this_attack_source, follow_on_source) = match owner {
            AbilityOwner::Feat => (ModifierSource::FeatThisAttack, ModifierSource::FeatFollowOn),
            AbilityOwner::Enhancement => (ModifierSource::EnhancementThisAttack, ModifierSource::EnhancementFollowOn),
            AbilityOwner::EnhancementSelection => {
                (ModifierSource::EnhancementSelectionThisAttack, ModifierSource::EnhancementSelectionFollowOn)
            }
            AbilityOwner::Spell | AbilityOwner::Standalone => {
                if attack.follow_on_effects().is_empty() && attack.this_attack_effects().is_empty() {
                    return Ok(());
                }
                bail!("{owner:?} {owner_id}: attack bonuses have no modifier source");
            }
        };
        self.write_modifiers(follow_on_source, owner_id, attack.follow_on_effects())?;
        self.write_modifiers(this_attack_source, owner_id, attack.this_attack_effects())?;
        Ok(())
    }

    fn resolved_feat_id(&self, feat_name: &str, source: FeatSource, source_id: i64) -> Option<i64> {
        let feat_name = feat_name.trim();
        self.written
            .feat_ids_by_key
            .get(&(feat_name.to_string(), source, Some(source_id)))
            .or_else(|| self.written.feat_ids_by_key.get(&(feat_name.to_string(), FeatSource::Standard, None)))
            .copied()
    }

    pub(super) fn write_race_file(&mut self, path: &Path, report: &mut BuildReport) -> Result<()> {
        let race = races::parse(path)?;
        self.transaction.execute(
            "INSERT INTO races (name, short_name, description, starting_world, build_points, iconic_class, is_construct, no_past_life, skill_points)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                race.name,
                trimmed_non_empty(race.short_name.as_deref()),
                trimmed_non_empty(race.description.as_deref()),
                trimmed_non_empty(race.starting_world.as_deref()),
                json_number_array(&race.build_points.iter().map(|v| *v as f64).collect::<Vec<_>>()),
                trimmed_non_empty(race.iconic_class.as_deref()),
                race.is_construct,
                race.lacks_past_life,
                race.skill_points,
            ],
        )?;
        let race_id = self.transaction.last_insert_rowid();
        for (ability, modifier) in &race.ability_modifiers {
            let Some(stat) = Stat::by_name(ability) else {
                bail!("{}: unknown ability {ability:?}", race.name);
            };
            self.transaction.execute(
                "INSERT OR REPLACE INTO race_ability_modifiers (race_id, stat_id, modifier) VALUES (?1, ?2, ?3)",
                params![race_id, stat.id, modifier],
            )?;
        }
        for feat in &race.feats {
            self.write_feat(feat, FeatSource::Race, Some(race_id))?;
            report.feat_count += 1;
        }
        for (sort_order, feat_name) in race.granted_feat_names.iter().enumerate() {
            let feat_id = self.resolved_feat_id(feat_name, FeatSource::Race, race_id);
            self.transaction.execute(
                "INSERT INTO race_granted_feats (race_id, sort_order, feat_name, feat_id) VALUES (?1, ?2, ?3, ?4)",
                params![race_id, sort_order as i64, feat_name, feat_id],
            )?;
        }
        for feat_slot in &race.feat_slots {
            self.transaction.execute(
                "INSERT OR REPLACE INTO race_feat_slots (race_id, level, feat_type, update_list) VALUES (?1, ?2, ?3, ?4)",
                params![race_id, feat_slot.level, feat_slot.feat_type.trim(), json_string_array(&feat_slot.update_list)],
            )?;
        }
        for skill in &race.auto_buy_skills {
            self.transaction.execute(
                "INSERT OR IGNORE INTO race_auto_buy_skills (race_id, skill) VALUES (?1, ?2)",
                params![race_id, skill],
            )?;
        }
        report.race_count += 1;
        Ok(())
    }

    pub(super) fn write_class_file(&mut self, path: &Path, report: &mut BuildReport) -> Result<()> {
        let class = classes::parse(path)?;
        let save_progression = |code: &Option<String>| -> Result<Option<&'static str>> {
            match code.as_deref() {
                None => Ok(None),
                Some(code) => match SaveProgression::parse(code) {
                    Some(progression) => Ok(Some(progression.as_str())),
                    None => bail!("{}: unknown save progression {code:?}", class.name),
                },
            }
        };
        self.transaction.execute(
            "INSERT INTO classes (name, base_class, not_heroic, description, small_icon, large_icon, skill_points, hit_points, alignments,
                                  fortitude, reflex, will, bab, spell_points_per_level, casting_stats, class_specific_feat_types)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                class.name,
                trimmed_non_empty(class.base_class.as_deref()),
                class.is_non_heroic,
                trimmed_non_empty(class.description.as_deref()),
                trimmed_non_empty(class.small_icon.as_deref()),
                trimmed_non_empty(class.large_icon.as_deref()),
                class.skill_points,
                class.hit_points,
                json_string_array(&class.alignments),
                save_progression(&class.fortitude)?,
                save_progression(&class.reflex)?,
                save_progression(&class.will)?,
                json_number_array(&class.bab),
                json_number_array(&class.spell_points_per_level),
                json_string_array(&class.casting_stats),
                json_string_array(&class.class_specific_feat_types),
            ],
        )?;
        let class_id = self.transaction.last_insert_rowid();
        for skill in &class.class_skills {
            self.transaction.execute(
                "INSERT OR IGNORE INTO class_skills (class_id, skill) VALUES (?1, ?2)",
                params![class_id, skill],
            )?;
        }
        for skill in &class.auto_buy_skills {
            self.transaction.execute(
                "INSERT OR IGNORE INTO class_auto_buy_skills (class_id, skill) VALUES (?1, ?2)",
                params![class_id, skill],
            )?;
        }
        for (class_level, slot_counts) in &class.spell_slots_by_class_level {
            for (spell_level_index, slot_count) in slot_counts.iter().enumerate() {
                self.transaction.execute(
                    "INSERT OR REPLACE INTO class_spell_slots (class_id, class_level, spell_level, slots) VALUES (?1, ?2, ?3, ?4)",
                    params![class_id, *class_level as i64, spell_level_index as i64 + 1, slot_count],
                )?;
            }
        }
        for class_spell in &class.class_spells {
            self.transaction.execute(
                "INSERT OR REPLACE INTO class_spells (class_id, spell_name, spell_level, cost, max_caster_level) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    class_id,
                    class_spell.name.trim(),
                    class_spell.level,
                    class_spell.cost,
                    class_spell.maximum_caster_level
                ],
            )?;
        }
        for feat_slot in &class.feat_slots {
            self.write_class_feat_slot(class_id, feat_slot)?;
        }
        for feat in &class.feats {
            self.write_feat(feat, FeatSource::Class, Some(class_id))?;
            report.feat_count += 1;
        }
        for automatic_feats in &class.automatic_feats {
            for feat_name in &automatic_feats.feat_names {
                let feat_id = self.resolved_feat_id(feat_name, FeatSource::Class, class_id);
                self.transaction.execute(
                    "INSERT OR REPLACE INTO class_auto_feats (class_id, level, feat_name, feat_id) VALUES (?1, ?2, ?3, ?4)",
                    params![class_id, automatic_feats.level, feat_name.trim(), feat_id],
                )?;
            }
        }
        report.class_count += 1;
        Ok(())
    }

    fn write_class_feat_slot(&mut self, class_id: i64, feat_slot: &FeatSlot) -> Result<()> {
        self.transaction.execute(
            "INSERT INTO class_feat_slots (class_id, level, feat_type, auto_populate, singular, update_list) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                class_id,
                feat_slot.level,
                feat_slot.feat_type.trim(),
                feat_slot.auto_populate.is_some(),
                feat_slot.singular.is_some(),
                json_string_array(&feat_slot.update_list)
            ],
        )?;
        Ok(())
    }

    pub(super) fn link_base_classes(&mut self) -> Result<()> {
        self.transaction.execute(
            "UPDATE classes SET base_class_id = (SELECT p.id FROM classes p WHERE p.name = classes.base_class) WHERE base_class IS NOT NULL",
            [],
        )?;
        Ok(())
    }
}
