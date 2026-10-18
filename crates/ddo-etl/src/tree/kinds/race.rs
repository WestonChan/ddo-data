use crate::tree::tables::Skill;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use anyhow::{ensure, Context, Result};
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::path::Path;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct AbilityModifier in "race_ability_modifiers" {
        keys { race_id: i64, sort_order: i64 }
        fields {
            modifier: i64 => modifier,
            stat: String => stat_id [reference "effects"],
        }
    }
}

pub(crate) type RaceCitation = super::super::Citation;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct RaceFeatSlot in "race_feat_slots" {
        keys { race_id: i64 }
        fields {
            feat_type: String => feat_type,
            level: i64 => level,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            update_list: Vec<String> => update_list [json],
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct RaceGrantedFeat in "race_granted_feats" {
        context { name: &str }
        keys { race_id: i64, sort_order: i64, feat_id: Option<i64> }
        fields {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            feat_name: Option<String> => feat_name [fallback name],
        }
        projected {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            #[schemars(extend("x-reference-kind" = "feats"))]
            feat: Option<String> => "(SELECT name FROM feats WHERE id = race_granted_feats.feat_id)",
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct RaceFile in "races" {
        keys { id: i64, slug: &str }
        fields {
            name: String => name,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            build_points: Vec<i64> => build_points [json],
            description: String => description [correctable] [default_text],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            iconic_class: Option<String> => iconic_class,
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            is_construct: bool => is_construct,
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            no_past_life: bool => no_past_life,
            short_name: String => short_name [default_text],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            skill_points: Option<i64> => skill_points,
            starting_world: String => starting_world [default_text],
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            ability_modifier: Vec<AbilityModifier>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            auto_buy_skill: Vec<Skill>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            citation: Vec<RaceCitation>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            feat_slot: Vec<RaceFeatSlot>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            granted_feat: Vec<RaceGrantedFeat>,
        }
    }
}

impl RaceFile {
    pub(in crate::tree) fn file_text(&self) -> anyhow::Result<String> {
        super::super::typed_file_text(self)
    }

    pub(in crate::tree) fn all_from_rows(
        db: &Connection,
        annotations: &super::super::annotations::EntityAnnotations,
    ) -> Result<BTreeMap<i64, Self>> {
        let mut files = BTreeMap::new();
        let mut select = db.prepare(&format!("SELECT {} FROM races ORDER BY id", Self::select_columns()))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("id")?, Self::from_row(row)?)))? {
            let (id, mut file) = row?;
            file.attach_citations(annotations, id);
            files.insert(id, file);
        }
        let mut abilities = db.prepare(&format!(
            "SELECT {} FROM race_ability_modifiers ORDER BY race_id, sort_order",
            AbilityModifier::select_columns()
        ))?;
        for row in
            abilities.query_map([], |row| Ok((row.get::<_, i64>("race_id")?, AbilityModifier::from_row(row)?)))?
        {
            let (id, record) = row?;
            files
                .get_mut(&id)
                .with_context(|| format!("ability_modifier belongs to missing race {id}"))?
                .ability_modifier
                .push(record);
        }
        let mut skills = db.prepare(&format!(
            "SELECT {} FROM race_auto_buy_skills ORDER BY race_id, rowid",
            Skill::select_columns("race_auto_buy_skills", "race_id")
        ))?;
        for row in skills.query_map([], |row| Ok((row.get::<_, i64>("owner_id")?, Skill::from_row(row)?)))? {
            let (id, record) = row?;
            files
                .get_mut(&id)
                .with_context(|| format!("auto_buy_skill belongs to missing race {id}"))?
                .auto_buy_skill
                .push(record);
        }
        let mut slots = db.prepare(&format!(
            "SELECT {} FROM race_feat_slots ORDER BY race_id, rowid",
            RaceFeatSlot::select_columns()
        ))?;
        for row in slots.query_map([], |row| Ok((row.get::<_, i64>("race_id")?, RaceFeatSlot::from_row(row)?)))? {
            let (id, record) = row?;
            files
                .get_mut(&id)
                .with_context(|| format!("feat_slot belongs to missing race {id}"))?
                .feat_slot
                .push(record);
        }
        let mut select = db.prepare(&format!(
            "SELECT {} FROM race_granted_feats ORDER BY race_id, sort_order",
            RaceGrantedFeat::select_columns()
        ))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("race_id")?, RaceGrantedFeat::from_row(row)?)))? {
            let (id, mut feat) = row?;
            if feat.feat.is_some() {
                feat.feat_name = None;
            }
            files.get_mut(&id).context("linked feat has no race")?.granted_feat.push(feat);
        }
        Ok(files)
    }

    pub(in crate::tree) fn write_race(&self, db: &Connection, path: &Path, id: i64, identity: &str) -> Result<()> {
        ensure!(
            id == super::super::ids::stable_hash(&["races", identity]),
            "{}: race id does not match its slug",
            path.display()
        );
        self.insert(db, id, identity).with_context(|| format!("{}: inserting race {:?}", path.display(), self.name))?;
        Ok(())
    }

    pub(in crate::tree) fn insert_children(&self, db: &Connection, path: &Path, id: i64) -> Result<usize> {
        for (position, ability) in self.ability_modifier.iter().enumerate() {
            ability.insert(db, id, position as i64)?;
        }
        for skill in &self.auto_buy_skill {
            skill.insert(db, "race_auto_buy_skills", "race_id", id)?;
        }
        for slot in &self.feat_slot {
            slot.insert(db, id)?;
        }
        let mut feat_lookup = db.prepare_cached("SELECT id FROM feats WHERE name = ?1")?;
        for (position, granted) in self.granted_feat.iter().enumerate() {
            let name = granted
                .feat_name
                .as_deref()
                .or(granted.feat.as_deref())
                .with_context(|| format!("{}: race granted feat has no name", path.display()))?;
            let feat_id = if let Some(feat) = &granted.feat {
                let matches =
                    feat_lookup.query_map([feat], |row| row.get::<_, i64>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
                ensure!(matches.len() == 1, "{}: race feat {feat:?} matches {} rows", path.display(), matches.len());
                Some(matches[0])
            } else {
                None
            };
            granted.insert(db, name, id, position as i64, feat_id)?;
        }
        Ok(self.ability_modifier.len() + self.auto_buy_skill.len() + self.feat_slot.len() + self.granted_feat.len())
    }
}
