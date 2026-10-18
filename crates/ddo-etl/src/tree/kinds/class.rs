use super::super::file_types::*;
use crate::tree::tables::Skill;
use anyhow::{ensure, Context, Result};
use rusqlite::{Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct ClassAutoFeat in "class_auto_feats" {
        context { name: &str }
        keys { class_id: i64, feat_id: Option<i64> }
        fields {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            feat_name: Option<String> => feat_name [fallback name],
            level: i64 => level,
        }
        projected {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            #[schemars(extend("x-reference-kind" = "feats"))]
            feat: Option<String> => "(SELECT name FROM feats WHERE id = class_auto_feats.feat_id)",
            #[serde(default, skip_serializing_if = "Option::is_none")]
            feat_description: Option<String> => "(SELECT description FROM feats WHERE id = class_auto_feats.feat_id AND (SELECT COUNT(*) FROM feats duplicate WHERE duplicate.name = class_auto_feats.feat_name) > 1)",
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct ClassFeatSlot in "class_feat_slots" {
        keys { class_id: i64, sort_order: i64 }
        fields {
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            auto_populate: bool => auto_populate,
            feat_type: String => feat_type,
            level: i64 => level,
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            singular: bool => singular,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            update_list: Vec<String> => update_list [json],
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct ClassSpell in "class_spells" {
        keys { class_id: i64 }
        fields {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            cost: Option<i64> => cost,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            max_caster_level: Option<i64> => max_caster_level,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            spell: Option<String> => spell_id [optional_reference "spells"],
            spell_level: i64 => spell_level,
            spell_name: String => spell_name,
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct ClassSpellSlot in "class_spell_slots" {
        keys { class_id: i64 }
        fields {
            class_level: i64 => class_level,
            slots: i64 => slots,
            spell_level: i64 => spell_level,
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct ClassFile in "classes" {
        keys { id: i64, slug: &str, base_class_id: Option<i64> }
        fields {
            name: String => name,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            alignments: Vec<String> => alignments [json],
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            bab: Vec<FileNumber> => bab [json],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            base_class: Option<String> => base_class,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            casting_stats: Vec<String> => casting_stats [json],
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            class_specific_feat_types: Vec<String> => class_specific_feat_types [json],
            description: String => description,
            fortitude: SaveProgressionFile => fortitude [enum_text],
            hit_points: i64 => hit_points,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            large_icon: Option<String> => large_icon [image "classes"],
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            not_heroic: bool => not_heroic,
            reflex: SaveProgressionFile => reflex [enum_text],
            skill_points: i64 => skill_points,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            small_icon: Option<String> => small_icon [image "classes"],
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            spell_points_per_level: Vec<i64> => spell_points_per_level [json],
            will: SaveProgressionFile => will [enum_text],
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            citation: Vec<crate::tree::Citation>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            auto_buy_skill: Vec<Skill>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            auto_feat: Vec<ClassAutoFeat>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            feat_slot: Vec<ClassFeatSlot>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            skill: Vec<Skill>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            spell: Vec<ClassSpell>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            spell_slot: Vec<ClassSpellSlot>,
        }
    }
}

impl ClassFile {
    pub(in crate::tree) fn insert_children(&self, db: &Connection, path: &Path, id: i64) -> Result<usize> {
        for skill in &self.skill {
            skill.insert(db, "class_skills", "class_id", id)?;
        }
        for skill in &self.auto_buy_skill {
            skill.insert(db, "class_auto_buy_skills", "class_id", id)?;
        }
        let mut feat_lookup =
            db.prepare_cached("SELECT id FROM feats WHERE name = ?1 AND (?2 IS NULL OR description IS ?2)")?;
        for feat in &self.auto_feat {
            let name = feat
                .feat
                .as_deref()
                .or(feat.feat_name.as_deref())
                .with_context(|| format!("{}: class auto feat has no name", path.display()))?;
            let feat_id = if feat.feat.is_some() {
                let ids = feat_lookup
                    .query_map((name, feat.feat_description.as_deref()), |row| row.get::<_, i64>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                ensure!(ids.len() == 1, "{}: linked feat {name:?} matches {} rows", path.display(), ids.len());
                Some(ids[0])
            } else {
                None
            };
            feat.insert(db, name, id, feat_id)?;
        }
        for (position, slot) in self.feat_slot.iter().enumerate() {
            slot.insert(db, id, position as i64)?;
        }
        for spell in &self.spell {
            spell.insert(db, id)?;
        }
        for slot in &self.spell_slot {
            slot.insert(db, id)?;
        }
        Ok(self.skill.len()
            + self.auto_buy_skill.len()
            + self.auto_feat.len()
            + self.feat_slot.len()
            + self.spell.len()
            + self.spell_slot.len())
    }

    pub(in crate::tree) fn file_text(&self) -> anyhow::Result<String> {
        super::super::typed_file_text(self)
    }

    pub(in crate::tree) fn all_from_rows(db: &Connection) -> Result<BTreeMap<i64, Self>> {
        let mut files = BTreeMap::new();
        let mut select = db.prepare(&format!("SELECT {} FROM classes ORDER BY id", Self::select_columns()))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("id")?, Self::from_row(row)?)))? {
            let (id, file) = row?;
            files.insert(id, file);
        }
        let mut buy_skills = db.prepare(&format!(
            "SELECT {} FROM class_auto_buy_skills ORDER BY class_id, rowid",
            Skill::select_columns("class_auto_buy_skills", "class_id")
        ))?;
        for row in buy_skills.query_map([], |row| Ok((row.get::<_, i64>("owner_id")?, Skill::from_row(row)?)))? {
            let (id, record) = row?;
            files
                .get_mut(&id)
                .with_context(|| format!("auto_buy_skill belongs to missing class {id}"))?
                .auto_buy_skill
                .push(record);
        }
        let mut select = db.prepare(&format!(
            "SELECT {} FROM class_auto_feats ORDER BY class_id, rowid",
            ClassAutoFeat::select_columns()
        ))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("class_id")?, ClassAutoFeat::from_row(row)?)))? {
            let (id, mut feat) = row?;
            if feat.feat.is_some() {
                feat.feat_name = None;
            }
            files.get_mut(&id).context("linked feat has no class")?.auto_feat.push(feat);
        }
        let mut feat_slots = db.prepare(&format!(
            "SELECT {} FROM class_feat_slots ORDER BY class_id, sort_order, id",
            ClassFeatSlot::select_columns()
        ))?;
        for row in
            feat_slots.query_map([], |row| Ok((row.get::<_, i64>("class_id")?, ClassFeatSlot::from_row(row)?)))?
        {
            let (id, record) = row?;
            files
                .get_mut(&id)
                .with_context(|| format!("feat_slot belongs to missing class {id}"))?
                .feat_slot
                .push(record);
        }
        let mut skills = db.prepare(&format!(
            "SELECT {} FROM class_skills ORDER BY class_id, rowid",
            Skill::select_columns("class_skills", "class_id")
        ))?;
        for row in skills.query_map([], |row| Ok((row.get::<_, i64>("owner_id")?, Skill::from_row(row)?)))? {
            let (id, record) = row?;
            files.get_mut(&id).with_context(|| format!("skill belongs to missing class {id}"))?.skill.push(record);
        }
        let mut spells =
            db.prepare(&format!("SELECT {} FROM class_spells ORDER BY class_id, rowid", ClassSpell::select_columns()))?;
        for row in spells.query_map([], |row| Ok((row.get::<_, i64>("class_id")?, ClassSpell::from_row(row)?)))? {
            let (id, record) = row?;
            files.get_mut(&id).with_context(|| format!("spell belongs to missing class {id}"))?.spell.push(record);
        }
        let mut slots = db.prepare(&format!(
            "SELECT {} FROM class_spell_slots ORDER BY class_id, rowid",
            ClassSpellSlot::select_columns()
        ))?;
        for row in slots.query_map([], |row| Ok((row.get::<_, i64>("class_id")?, ClassSpellSlot::from_row(row)?)))? {
            let (id, record) = row?;
            files
                .get_mut(&id)
                .with_context(|| format!("spell_slot belongs to missing class {id}"))?
                .spell_slot
                .push(record);
        }
        Ok(files)
    }

    pub(in crate::tree) fn write_class(&self, db: &Connection, path: &Path, id: i64, identity: &str) -> Result<()> {
        ensure!(
            id == super::super::ids::stable_hash(&["classes", identity]),
            "{}: class id does not match its slug",
            path.display()
        );
        let base_class_id = self
            .base_class
            .as_deref()
            .map(|name| {
                db.query_row("SELECT id FROM classes WHERE name = ?1", [name], |row| row.get::<_, i64>(0)).optional()
            })
            .transpose()?
            .flatten();
        self.insert(db, id, identity, base_class_id)
            .with_context(|| format!("{}: inserting class {:?}", path.display(), self.name))?;
        Ok(())
    }
}
