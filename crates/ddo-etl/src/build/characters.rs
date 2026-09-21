//! Feats (standard, class, race), races and classes.

use super::{json_numbers, json_strings, nonempty, BuildReport, Ctx};
use crate::xml::classes::{self, FeatSlot};
use crate::xml::feats::{self, Feat};
use crate::xml::races;
use anyhow::{bail, Context, Result};
use ddo_model::enums::{AbilityOwner, FeatSource, ModifierSource, RequirementOwner, SaveProgression};
use rusqlite::params;
use std::path::Path;

impl Ctx<'_> {
    pub(super) fn write_standard_feats(&mut self, path: &Path, report: &mut BuildReport) -> Result<()> {
        if !path.is_file() {
            return Ok(());
        }
        for feat in feats::parse(path)? {
            self.write_feat(&feat, FeatSource::Standard, None)?;
            report.feats += 1;
        }
        Ok(())
    }

    fn write_feat(&mut self, f: &Feat, source: FeatSource, source_id: Option<i64>) -> Result<i64> {
        let (auto_reqs, ignore) = match &f.automatic_acquisition {
            Some(a) => (Some(&a.requirements), a.ignore_requirements),
            None => (None, false),
        };
        self.tx.execute(
            "INSERT INTO feats (name, source_kind, source_id, description, icon, acquire, max_times_acquire, sphere,
                                auto_acquire_ignores_requirements)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                f.name,
                source.as_str(),
                source_id,
                nonempty(f.description.as_deref()),
                nonempty(f.icon.as_deref()),
                nonempty(f.acquire.as_deref()),
                f.max_times_acquire,
                nonempty(f.sphere.as_deref()),
                ignore,
            ],
        )
        .with_context(|| format!("feat {:?} from {:?} {:?}", f.name, source, source_id))?;
        let id = self.tx.last_insert_rowid();
        self.caches.feats.entry((f.name.clone(), source, source_id)).or_insert(id);

        for g in &f.groups {
            self.tx
                .execute("INSERT OR IGNORE INTO feat_groups (feat_id, group_name) VALUES (?1, ?2)", params![id, g])?;
        }
        for cg in &f.conditional_groups {
            self.tx.execute(
                "INSERT INTO feat_conditional_groups (feat_id, groups) VALUES (?1, ?2)",
                params![id, json_strings(&cg.groups).unwrap_or_else(|| "[]".into())],
            )?;
            let cg_id = self.tx.last_insert_rowid();
            if let Some(reqs) = &cg.requirements {
                self.write_requirements(RequirementOwner::FeatConditionalGroup, cg_id, reqs)?;
            }
        }
        for (i, s) in f.sub_items.iter().enumerate() {
            self.tx.execute(
                "INSERT INTO feat_sub_items (feat_id, sort_order, name, icon, description) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, i as i64, s.name.trim(), nonempty(s.icon.as_deref()), nonempty(s.description.as_deref())],
            )?;
        }
        if let Some(reqs) = &f.requirements {
            self.write_requirements(RequirementOwner::Feat, id, reqs)?;
        }
        if let Some(reqs) = auto_reqs {
            self.write_requirements(RequirementOwner::FeatAutoAcquire, id, reqs)?;
        }
        self.write_ability_children(AbilityOwner::Feat, id, &f.stances, &f.dcs, f.attack.as_ref())?;
        self.write_modifiers(ModifierSource::Feat, id, &f.effects)?;
        for (i, bonus_id) in self.derived_bonus_ids(&f.effects)?.into_iter().enumerate() {
            self.tx.execute(
                "INSERT INTO feat_bonuses (feat_id, bonus_id, sort_order) VALUES (?1, ?2, ?3)",
                params![id, bonus_id, i as i64],
            )?;
        }
        Ok(id)
    }

    /// Stances, DCs and the attack an ability carries. Shared by feats and (later) enhancements.
    pub(super) fn write_ability_children(
        &mut self,
        owner: AbilityOwner,
        owner_id: i64,
        stances: &[feats::Stance],
        dcs: &[feats::Dc],
        attack: Option<&feats::Attack>,
    ) -> Result<()> {
        for (i, s) in stances.iter().enumerate() {
            self.tx.execute(
                "INSERT INTO stances (owner_kind, owner_id, sort_order, name, description, icon, group_name, auto_controlled, incompatible)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    owner.as_str(),
                    owner_id,
                    i as i64,
                    s.name.trim(),
                    nonempty(s.description.as_deref()),
                    nonempty(s.icon.as_deref()),
                    nonempty(s.group.as_deref()),
                    s.auto_controlled.is_some(),
                    json_strings(&s.incompatible),
                ],
            )?;
            let stance_id = self.tx.last_insert_rowid();
            if let Some(reqs) = &s.requirements {
                self.write_requirements(RequirementOwner::Stance, stance_id, reqs)?;
            }
        }
        for (i, d) in dcs.iter().enumerate() {
            let amount = d.amount.as_ref().map(|v| v.numbers()).transpose().map_err(anyhow::Error::msg)?;
            self.tx.execute(
                "INSERT INTO dcs (owner_kind, owner_id, sort_order, name, description, icon, dc_type, dc_versus, mod_ability, amount,
                                  tactical, other, skill, class_level, base_class_level)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    owner.as_str(),
                    owner_id,
                    i as i64,
                    nonempty(d.name.as_deref()),
                    nonempty(d.description.as_deref()),
                    nonempty(d.icon.as_deref()),
                    nonempty(d.dc_type.as_deref()),
                    nonempty(d.dc_versus.as_deref()),
                    json_strings(&d.mod_ability),
                    amount.as_deref().and_then(json_numbers),
                    nonempty(d.tactical.as_deref()),
                    nonempty(d.other.as_deref()),
                    nonempty(d.skill.as_deref()),
                    nonempty(d.class_level.as_deref()),
                    nonempty(d.base_class_level.as_deref()),
                ],
            )?;
        }
        if let Some(a) = attack {
            self.tx.execute(
                "INSERT INTO attacks (owner_kind, owner_id, name, description, icon) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    owner.as_str(),
                    owner_id,
                    nonempty(a.name.as_deref()),
                    nonempty(a.description.as_deref()),
                    nonempty(a.icon.as_deref())
                ],
            )?;
        }
        Ok(())
    }

    /// A feat referenced by name from a class or race: that source's own definition first, then
    /// the standard list.
    fn resolve_feat(&self, name: &str, source: FeatSource, source_id: i64) -> Option<i64> {
        let name = name.trim();
        self.caches
            .feats
            .get(&(name.to_string(), source, Some(source_id)))
            .or_else(|| self.caches.feats.get(&(name.to_string(), FeatSource::Standard, None)))
            .copied()
    }

    pub(super) fn write_race_file(&mut self, path: &Path, report: &mut BuildReport) -> Result<()> {
        let r = races::parse(path)?;
        self.tx.execute(
            "INSERT INTO races (name, short_name, description, starting_world, build_points, iconic_class, is_construct, no_past_life, skill_points)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                r.name,
                nonempty(r.short_name.as_deref()),
                nonempty(r.description.as_deref()),
                nonempty(r.starting_world.as_deref()),
                json_numbers(&r.build_points.iter().map(|v| *v as f64).collect::<Vec<_>>()),
                nonempty(r.iconic_class.as_deref()),
                r.is_construct,
                r.no_past_life,
                r.skill_points,
            ],
        )?;
        let race_id = self.tx.last_insert_rowid();
        for (ability, modifier) in &r.ability_modifiers {
            let Some(stat) = ddo_model::stat_by_name(ability) else {
                bail!("{}: unknown ability {ability:?}", r.name);
            };
            self.tx.execute(
                "INSERT OR REPLACE INTO race_ability_modifiers (race_id, stat_id, modifier) VALUES (?1, ?2, ?3)",
                params![race_id, stat.id, modifier],
            )?;
        }
        for feat in &r.feats {
            self.write_feat(feat, FeatSource::Race, Some(race_id))?;
            report.feats += 1;
        }
        for (i, name) in r.granted_feats.iter().enumerate() {
            let feat_id = self.resolve_feat(name, FeatSource::Race, race_id);
            self.tx.execute(
                "INSERT INTO race_granted_feats (race_id, sort_order, feat_name, feat_id) VALUES (?1, ?2, ?3, ?4)",
                params![race_id, i as i64, name, feat_id],
            )?;
        }
        for slot in &r.feat_slots {
            self.tx.execute(
                "INSERT OR REPLACE INTO race_feat_slots (race_id, level, feat_type, update_list) VALUES (?1, ?2, ?3, ?4)",
                params![race_id, slot.level, slot.feat_type.trim(), json_strings(&slot.update_list)],
            )?;
        }
        for skill in &r.auto_buy_skills {
            self.tx.execute(
                "INSERT OR IGNORE INTO race_auto_buy_skills (race_id, skill) VALUES (?1, ?2)",
                params![race_id, skill],
            )?;
        }
        report.races += 1;
        Ok(())
    }

    pub(super) fn write_class_file(&mut self, path: &Path, report: &mut BuildReport) -> Result<()> {
        let c = classes::parse(path)?;
        let save = |s: &Option<String>| -> Result<Option<&'static str>> {
            match s.as_deref() {
                None => Ok(None),
                Some(v) => match SaveProgression::from_upstream(v) {
                    Some(p) => Ok(Some(p.as_str())),
                    None => bail!("{}: unknown save progression {v:?}", c.name),
                },
            }
        };
        self.tx.execute(
            "INSERT INTO classes (name, base_class, not_heroic, description, small_icon, large_icon, skill_points, hit_points, alignments,
                                  fortitude, reflex, will, bab, spell_points_per_level, casting_stats, class_specific_feat_types)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                c.name,
                nonempty(c.base_class.as_deref()),
                c.not_heroic,
                nonempty(c.description.as_deref()),
                nonempty(c.small_icon.as_deref()),
                nonempty(c.large_icon.as_deref()),
                c.skill_points,
                c.hit_points,
                json_strings(&c.alignments),
                save(&c.fortitude)?,
                save(&c.reflex)?,
                save(&c.will)?,
                json_numbers(&c.bab),
                json_numbers(&c.spell_points_per_level),
                json_strings(&c.casting_stats),
                json_strings(&c.class_specific_feat_types),
            ],
        )?;
        let class_id = self.tx.last_insert_rowid();
        self.caches.classes.insert(c.name.clone(), class_id);
        for skill in &c.class_skills {
            self.tx.execute(
                "INSERT OR IGNORE INTO class_skills (class_id, skill) VALUES (?1, ?2)",
                params![class_id, skill],
            )?;
        }
        for skill in &c.auto_buy_skills {
            self.tx.execute(
                "INSERT OR IGNORE INTO class_auto_buy_skills (class_id, skill) VALUES (?1, ?2)",
                params![class_id, skill],
            )?;
        }
        for (class_level, slots) in &c.spell_slots {
            for (i, n) in slots.iter().enumerate() {
                self.tx.execute(
                    "INSERT OR REPLACE INTO class_spell_slots (class_id, class_level, spell_level, slots) VALUES (?1, ?2, ?3, ?4)",
                    params![class_id, *class_level as i64, i as i64 + 1, n],
                )?;
            }
        }
        for s in &c.class_spells {
            self.tx.execute(
                "INSERT OR REPLACE INTO class_spells (class_id, spell_name, spell_level, cost, max_caster_level) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![class_id, s.name.trim(), s.level, s.cost, s.max_caster_level],
            )?;
        }
        for slot in &c.feat_slots {
            self.write_class_feat_slot(class_id, slot)?;
        }
        for feat in &c.feats {
            self.write_feat(feat, FeatSource::Class, Some(class_id))?;
            report.feats += 1;
        }
        for auto in &c.automatic_feats {
            for name in &auto.feats {
                let feat_id = self.resolve_feat(name, FeatSource::Class, class_id);
                self.tx.execute(
                    "INSERT OR REPLACE INTO class_auto_feats (class_id, level, feat_name, feat_id) VALUES (?1, ?2, ?3, ?4)",
                    params![class_id, auto.level, name.trim(), feat_id],
                )?;
            }
        }
        report.classes += 1;
        Ok(())
    }

    fn write_class_feat_slot(&mut self, class_id: i64, slot: &FeatSlot) -> Result<()> {
        self.tx.execute(
            "INSERT INTO class_feat_slots (class_id, level, feat_type, auto_populate, singular, update_list) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                class_id,
                slot.level,
                slot.feat_type.trim(),
                slot.auto_populate.is_some(),
                slot.singular.is_some(),
                json_strings(&slot.update_list)
            ],
        )?;
        Ok(())
    }

    /// Archetypes name their parent class; link them once every class row exists.
    pub(super) fn resolve_base_classes(&mut self) -> Result<()> {
        self.tx.execute(
            "UPDATE classes SET base_class_id = (SELECT p.id FROM classes p WHERE p.name = classes.base_class) WHERE base_class IS NOT NULL",
            [],
        )?;
        Ok(())
    }
}
