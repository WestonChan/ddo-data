use super::super::file_types::*;
use crate::tree::tables::Modifier;
use anyhow::{ensure, Context, Result};
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub(crate) type AugmentCitation = super::super::Citation;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct AugmentEffect in "augment_effects" {
        keys { augment_id: i64, sort_order: i64 }
        fields {
            name: String => effect_id [reference "effects"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            bonus_type: Option<String> => bonus_type_id [optional_reference "bonus_types"],
            value: i64 => value,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            value2: Option<i64> => value2,
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct AugmentSlot in "augment_slots" {
        keys { augment_id: i64 }
        fields {
            slot: String => slot_id [reference "augment_slot_types" "label"],
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("x-source-reference" = true))]
pub(crate) struct AugmentSource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) chest: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) is_rare: bool,
    pub(crate) kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) loot_type: Option<String>,
    pub(crate) name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tier: Option<String>,
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct AugmentFile in "augments" {
        keys { id: i64, slug: &str }
        fields {
            name: String => name,
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            is_wiki_only: bool => provenance [wiki_only],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            adds_augment: Option<String> => adds_augment,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            variant: Option<String> => variant,
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            choose_level: bool => choose_level,
            description: String => description,
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            dual_values: bool => dual_values,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            effect_description: Option<String> => effect_description,
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            enter_value: bool => enter_value,
            family: String => family,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            grants_augment: Option<String> => grants_augment,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            icon: Option<String> => icon [image ""],
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            level_values: Vec<FileNumber> => level_values [json],
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            level_values2: Vec<i64> => level_values2 [json],
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            levels: Vec<i64> => levels [json],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            min_level: Option<i64> => min_level,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            set_bonus: Option<String> => set_bonus,
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            suppress_set_bonus: bool => suppress_set_bonus,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            weapon_class: Option<String> => weapon_class,
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            citation: Vec<AugmentCitation>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            effect: Vec<AugmentEffect>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            modifier: Vec<Modifier>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            ddobuilder_correction: Vec<super::super::DdobuilderCorrection>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            slot: Vec<AugmentSlot>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            source: Vec<AugmentSource>,
        }
    }
}

impl AugmentFile {
    pub(in crate::tree) fn insert_modifiers(&self, db: &Connection, path: &Path, id: i64) -> Result<usize> {
        self.modifier.iter().enumerate().try_fold(0, |count, (position, modifier)| {
            modifier
                .write_modifier(db, "augment", id, position as i64)
                .map(|written| count + written)
                .with_context(|| format!("{}: augment modifier", path.display()))
        })
    }

    pub(in crate::tree) fn file_text(&self) -> anyhow::Result<String> {
        super::super::typed_file_text(self)
    }

    pub(in crate::tree) fn all_from_rows(
        db: &Connection,
        annotations: &super::super::annotations::EntityAnnotations,
        notes: &super::super::export::CorrectionNoteIndex,
        source_rows: &mut super::super::sources::SourcesByOwner,
        modifier_rows: &mut super::super::ModifierRowsByOwner,
    ) -> Result<BTreeMap<i64, Self>> {
        let mut files = BTreeMap::new();
        let mut select = db.prepare(&format!("SELECT {} FROM augments ORDER BY id", Self::select_columns()))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("id")?, Self::from_row(row)?)))? {
            let (id, mut file) = row?;
            file.attach_citations(annotations, id);
            file.ddobuilder_correction.extend(notes.for_entity("augments", &file.name));
            files.insert(id, file);
        }
        let mut select = db.prepare(&format!(
            "SELECT {} FROM augment_effects ORDER BY augment_id, sort_order",
            AugmentEffect::select_columns()
        ))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("augment_id")?, AugmentEffect::from_row(row)?)))? {
            let (id, effect) = row?;
            files.get_mut(&id).with_context(|| format!("effect link has no augment {id}"))?.effect.push(effect);
        }
        let mut slots = db.prepare(&format!(
            "SELECT {} FROM augment_slots ORDER BY augment_id, rowid",
            AugmentSlot::select_columns()
        ))?;
        for row in slots.query_map([], |row| Ok((row.get::<_, i64>("augment_id")?, AugmentSlot::from_row(row)?)))? {
            let (id, slot) = row?;
            files.get_mut(&id).context("slot has no augment")?.slot.push(slot);
        }
        for (id, entries) in std::mem::take(&mut source_rows.augments) {
            let file = files.get_mut(&id).with_context(|| format!("sources have no augment {id}"))?;
            file.source = entries
                .into_iter()
                .map(|source| {
                    ensure!(source.price.is_empty(), "augment {:?} has a vendor cost", file.name);
                    Ok(AugmentSource {
                        chest: source.chest,
                        is_rare: source.is_rare,
                        kind: source.kind,
                        tier: source.tier,
                        loot_type: source.loot_type,
                        name: source.name,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
        }
        for key in modifier_rows.keys().filter(|(kind, _)| kind == "augment").cloned().collect::<Vec<_>>() {
            let entries = modifier_rows.remove(&key).unwrap();
            let file = files.get_mut(&key.1).with_context(|| format!("modifiers have no augment {}", key.1))?;
            for record in entries {
                file.modifier.push(record);
            }
        }
        Ok(files)
    }

    pub(in crate::tree) fn write_augment(&self, db: &Connection, path: &Path, id: i64, identity: &str) -> Result<()> {
        ensure!(
            id == super::super::ids::stable_hash(&["augments", identity]),
            "{}: augment id does not match its slug",
            path.display()
        );
        self.insert(db, id, identity)
            .with_context(|| format!("{}: inserting augment {:?}", path.display(), self.name))?;
        Ok(())
    }

    pub(in crate::tree) fn insert_effect_links(&self, db: &Connection, path: &Path, id: i64) -> Result<usize> {
        for (position, effect) in self.effect.iter().enumerate() {
            effect.insert(db, id, position as i64).with_context(|| format!("{}: augment effect", path.display()))?;
        }
        Ok(self.effect.len())
    }

    pub(in crate::tree) fn insert_slots(&self, db: &Connection, path: &Path, id: i64) -> Result<usize> {
        for slot in &self.slot {
            slot.insert(db, id).with_context(|| format!("{}: augment slot {:?}", path.display(), slot.slot))?;
        }
        Ok(self.slot.len())
    }
}

impl AugmentFile {
    pub(in crate::tree) fn insert_sources(
        &self,
        db: &Connection,
        id: i64,
        quest_epic_names: &BTreeMap<String, String>,
        aliases: &crate::map::source_alias::SourceAliases,
    ) -> Result<usize> {
        let owner = super::super::sources::SourceOwner {
            drop_text: super::super::sources::augment_drop_text(&self.description),
            minimum_level: self.min_level,
            explicit_wiki_sources: self.citation.iter().any(|citation| citation.field == "sources"),
        };
        for source in &self.source {
            let source = super::super::sources::SourceInput {
                kind: &source.kind,
                tier: source.tier.as_deref(),
                name: &source.name,
                loot_type: source.loot_type.as_deref(),
                chest: source.chest.as_deref(),
                is_rare: source.is_rare,
                price: &[],
            };
            super::super::sources::insert_source(db, "augments", id, &owner, &source, quest_epic_names, aliases)?;
        }
        Ok(self.source.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn augment_parent_fields_round_trip_through_typed_rows() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        let file: AugmentFile =
            toml::from_str("name = 'Test Augment'\nfamily = 'Test Family'\ndescription = 'A test.'\nmin_level = 17\n")
                .unwrap();
        let id = super::super::super::ids::stable_hash(&["augments", "test-augment"]);
        file.write_augment(&db, Path::new("test-augment.toml"), id, "test-augment").unwrap();
        let annotations = super::super::super::annotations::EntityAnnotations::from_database(&db).unwrap();
        let notes = super::super::super::export::CorrectionNoteIndex::from_database(&db).unwrap();
        let mut sources = super::super::super::sources::sources_by_owner(&db).unwrap();
        let mut modifiers = super::super::super::modifier_rows_by_owner(&db).unwrap();
        let restored = AugmentFile::all_from_rows(&db, &annotations, &notes, &mut sources, &mut modifiers).unwrap();
        assert_eq!(restored[&id].name, "Test Augment");
        assert_eq!(restored[&id].min_level, Some(17));
        assert_eq!(restored[&id].family, "Test Family");
    }
}
