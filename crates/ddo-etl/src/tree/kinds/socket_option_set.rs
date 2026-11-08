use crate::tree::Modifier;
use anyhow::{Context, Result};
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct SocketOptionSetFile in "socket_option_sets" {
        keys { id: i64, slug: &str }
        fields {
            label: String => slot_id [reference "augment_slot_types" "label"],
            name: String => name,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            variant: Option<String> => variant,
        }
        attached {
            option: Vec<SocketOption>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            ddobuilder_correction: Vec<crate::tree::DdobuilderCorrection>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            citation: Vec<crate::tree::Citation>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            game_bug_correction: Vec<crate::tree::GameBugCorrection>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            verification_request: Vec<crate::tree::VerificationRequest>,
        }
    }
}

impl SocketOptionSetFile {
    pub(in crate::tree) fn write_set(
        &self,
        db: &Connection,
        path: &Path,
        ids: &mut crate::tree::ids::StableIds,
    ) -> Result<i64> {
        let identity = crate::tree::socket_option_set_slug(&self.label, &self.name, self.variant.as_deref());
        let id = ids.issue("socket_option_sets", &identity, path)?;
        self.insert(db, id, &identity)?;
        Ok(id)
    }

    pub(in crate::tree) fn all_from_rows(
        db: &Connection,
        annotations: &crate::tree::annotations::EntityAnnotations,
        notes: &crate::tree::export::CorrectionNoteIndex,
        modifiers: &mut crate::tree::ModifierRowsByOwner,
    ) -> Result<BTreeMap<i64, Self>> {
        let mut sets = BTreeMap::new();
        let mut query =
            db.prepare(&format!("SELECT {} FROM socket_option_sets ORDER BY id", Self::select_columns()))?;
        for row in query
            .query_map([], |row| Ok((row.get::<_, i64>("id")?, row.get::<_, String>("slug")?, Self::from_row(row)?)))?
        {
            let (id, slug, mut set) = row?;
            set.attach_annotations(annotations, id);
            set.ddobuilder_correction.extend(notes.for_entity("socket_option_sets", &slug));
            sets.insert(id, set);
        }
        let mut locations = BTreeMap::new();
        let mut query = db.prepare(&format!(
            "SELECT {} FROM socket_options ORDER BY set_id,option_order",
            SocketOption::select_columns()
        ))?;
        for row in query.query_map([], |row| {
            Ok((row.get::<_, i64>("id")?, row.get::<_, i64>("set_id")?, SocketOption::from_row(row)?))
        })? {
            let (id, set_id, mut option) = row?;
            option.modifier = modifiers.remove(&("socket_option".into(), id)).unwrap_or_default();
            let set = sets.get_mut(&set_id).context("socket option has no set")?;
            locations.insert(id, (set_id, set.option.len()));
            set.option.push(option);
        }
        let mut query = db.prepare(&format!(
            "SELECT {} FROM socket_option_effects ORDER BY option_id,sort_order",
            SocketOptionEffect::select_columns()
        ))?;
        for row in
            query.query_map([], |row| Ok((row.get::<_, i64>("option_id")?, SocketOptionEffect::from_row(row)?)))?
        {
            let (id, effect) = row?;
            let (set, position) = locations[&id];
            sets.get_mut(&set).unwrap().option[position].effect.push(effect);
        }
        let mut query = db.prepare(&format!(
            "SELECT {} FROM socket_option_grants ORDER BY option_id,sort_order",
            SocketGrant::select_columns()
        ))?;
        for row in query.query_map([], |row| Ok((row.get::<_, i64>("option_id")?, SocketGrant::from_row(row)?)))? {
            let (id, grant) = row?;
            let (set, position) = locations[&id];
            sets.get_mut(&set).unwrap().option[position].grant.push(grant);
        }
        let mut query = db.prepare(&format!(
            "SELECT {} FROM socket_option_set_bonuses ORDER BY option_id,rowid",
            SocketSet::select_columns()
        ))?;
        for row in query.query_map([], |row| Ok((row.get::<_, i64>("option_id")?, SocketSet::from_row(row)?)))? {
            let (id, bonus) = row?;
            let (set, position) = locations[&id];
            sets.get_mut(&set).unwrap().option[position].set.push(bonus);
        }
        Ok(sets)
    }
}

database_record! {
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct SocketOptionEffect in "socket_option_effects" {
        keys { option_id: i64, sort_order: i64 }
        fields {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            bonus_type: Option<String> => bonus_type_id [optional_reference "bonus_types"],
            effect: String => effect_id [reference "effects"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            alias: Option<String> => alias,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            value: Option<i64> => value,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            value2: Option<i64> => value2,
        }
    }
}

database_record! {
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct SocketGrant in "socket_option_grants" {
        keys { option_id: i64, sort_order: i64 }
        fields {
            slot: String => slot_id [reference "augment_slot_types" "label"],
        }
    }
}

database_record! {
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct SocketSet in "socket_option_set_bonuses" {
        keys { option_id: i64 }
        fields {
            set: String => set_bonus_id [reference "set_bonuses"],
        }
    }
}

database_record! {
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct SocketOption in "socket_options" {
        keys { id: Option<i64>, set_id: i64, option_order: i64 }
        fields {
            name: String => name [correctable],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            description: Option<String> => description [correctable],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            icon: Option<String> => icon [image ""],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            min_level: Option<i64> => min_level,
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            effect: Vec<SocketOptionEffect>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            grant: Vec<SocketGrant>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            modifier: Vec<Modifier>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            set: Vec<SocketSet>,
        }
    }
}

impl SocketOption {
    pub(crate) fn write_option(&self, db: &Connection, set_id: i64, position: i64) -> Result<i64> {
        self.insert(db, None, set_id, position)?;
        let id = db.last_insert_rowid();
        for (position, effect) in self.effect.iter().enumerate() {
            effect.insert(db, id, position as i64)?;
        }
        for (position, grant) in self.grant.iter().enumerate() {
            grant.insert(db, id, position as i64)?;
        }
        for set in &self.set {
            set.insert(db, id)?;
        }
        for (position, modifier) in self.modifier.iter().enumerate() {
            modifier.write_modifier(db, "socket_option", id, position as i64)?;
        }
        Ok(id)
    }
}
