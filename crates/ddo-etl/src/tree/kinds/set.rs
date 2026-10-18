use crate::tree::tables::{EffectGrant, Modifier};
use anyhow::{ensure, Context, Result};
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct SetAugment in "set_bonus_augments" {
        context { augment_id: i64 }
        keys { set_id: i64 }
        fields {
            name: String => augment_id [resolved_reference "augments" augment_id],
        }
        projected {
            family: String => "(SELECT family FROM augments WHERE id = set_bonus_augments.augment_id)",
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct SetItem in "set_bonus_items" {
        keys { set_id: i64 }
        fields {
            name: String => item_id [reference "items"],
        }
        attached {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            primary: Option<bool>,
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct SetTier in "set_bonus_tiers" {
        keys { id: Option<i64>, set_id: i64 }
        fields {
            equipped_count: i64 => equipped_count,
        }
        attached {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            description: Option<String> [correctable],
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            effect: Vec<EffectGrant>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            modifier: Vec<Modifier>,
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct SetFile in "set_bonuses" {
        keys { id: i64, slug: &str }
        fields {
            name: String => name [correctable],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            icon: Option<String> => icon [correctable] [image ""],
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            is_filigree_set: bool => is_filigree_set,
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            citation: Vec<crate::tree::Citation>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            augment: Vec<SetAugment>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            item: Vec<SetItem>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            ddobuilder_correction: Vec<super::super::DdobuilderCorrection>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            tier: Vec<SetTier>,
        }
    }
}

impl SetFile {
    pub(in crate::tree) fn insert_tiers(&self, db: &Connection, path: &Path, id: i64) -> Result<usize> {
        let mut count = 0;
        for tier in &self.tier {
            tier.insert(db, None, id)?;
            let tier_id = db.last_insert_rowid();
            count += 1;
            for (position, effect) in tier.effect.iter().enumerate() {
                effect
                    .insert(db, "set_bonus_tier_effects", "tier_id", tier_id, position as i64)
                    .with_context(|| format!("{}: set tier effect", path.display()))?;
                count += 1;
            }
            for (position, modifier) in tier.modifier.iter().enumerate() {
                count += modifier.write_modifier(db, "set_bonus_tier", tier_id, position as i64)?;
            }
        }
        Ok(count)
    }

    pub(in crate::tree) fn file_text(&self) -> anyhow::Result<String> {
        super::super::typed_file_text(self)
    }

    pub(in crate::tree) fn all_from_rows(
        db: &Connection,
        notes: &super::super::export::CorrectionNoteIndex,
        modifier_rows: &mut super::super::ModifierRowsByOwner,
    ) -> Result<BTreeMap<i64, Self>> {
        let mut files = BTreeMap::new();
        let mut select = db.prepare(&format!("SELECT {} FROM set_bonuses ORDER BY id", Self::select_columns()))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("id")?, Self::from_row(row)?)))? {
            let (id, mut file) = row?;
            file.ddobuilder_correction = notes.for_entity("set_bonuses", &file.name);
            files.insert(id, file);
        }
        let mut descriptions = BTreeMap::new();
        let mut select = db.prepare(&format!(
            "SELECT {} FROM corrections WHERE kind = 'set_tier' AND field = 'description' ORDER BY id",
            crate::tree::annotations::AppliedCorrection::select_columns()
        ))?;
        for correction in select.query_map([], crate::tree::annotations::AppliedCorrection::from_row)? {
            let crate::tree::annotations::AppliedCorrection { name, qualifier, to_value: value, .. } = correction?;
            let count: i64 = qualifier
                .strip_prefix("equipped_count ")
                .with_context(|| format!("set {name:?} has invalid tier qualifier {qualifier:?}"))?
                .parse()?;
            descriptions.insert((name, count), serde_json::from_str::<String>(&value)?);
        }
        let mut tier_ids = BTreeMap::new();
        let mut tiers = db.prepare(&format!(
            "SELECT {} FROM set_bonus_tiers ORDER BY set_id, equipped_count",
            SetTier::select_columns()
        ))?;
        for row in tiers.query_map([], |row| {
            Ok((row.get::<_, i64>("id")?, row.get::<_, i64>("set_id")?, SetTier::from_row(row)?))
        })? {
            let (tier_id, set_id, mut tier) = row?;
            let file = files.get_mut(&set_id).with_context(|| format!("tier {tier_id} has no set {set_id}"))?;
            tier.description = descriptions.remove(&(file.name.clone(), tier.equipped_count));
            tier_ids.insert(tier_id, (set_id, file.tier.len()));
            file.tier.push(tier);
        }
        let mut effects = db.prepare(&format!(
            "SELECT {} FROM set_bonus_tier_effects ORDER BY tier_id, sort_order",
            EffectGrant::select_columns("set_bonus_tier_effects", "tier_id")
        ))?;
        for row in effects.query_map([], |row| Ok((row.get::<_, i64>("owner_id")?, EffectGrant::from_row(row)?)))? {
            let (id, effect) = row?;
            let &(set_id, position) = tier_ids.get(&id).with_context(|| format!("effect has no set tier {id}"))?;
            files
                .get_mut(&set_id)
                .with_context(|| format!("effect tier {id} has no set {set_id}"))?
                .tier
                .get_mut(position)
                .with_context(|| format!("effect tier {id} has no position {position}"))?
                .effect
                .push(effect);
        }
        let mut item_members = Vec::new();
        let mut select = db.prepare(&format!("SELECT {}, item_id AS member_id, (SELECT set_bonus FROM items WHERE id = set_bonus_items.item_id) AS primary_set FROM set_bonus_items ORDER BY set_id, rowid", SetItem::select_columns()))?;
        for row in select.query_map([], |row| {
            Ok((
                row.get::<_, i64>("set_id")?,
                row.get::<_, i64>("member_id")?,
                SetItem::from_row(row)?,
                row.get::<_, Option<String>>("primary_set")?,
            ))
        })? {
            item_members.push(row?);
        }
        let mut first_set_by_item = BTreeMap::<i64, String>::new();
        for (set_id, item_id, _, _) in &item_members {
            let set_name = &files.get(set_id).with_context(|| format!("member {item_id} has no set {set_id}"))?.name;
            first_set_by_item
                .entry(*item_id)
                .and_modify(|first| {
                    if set_name < first {
                        first.clone_from(set_name);
                    }
                })
                .or_insert_with(|| set_name.clone());
        }
        for (set_id, item_id, mut item, primary_set) in item_members {
            let file = files.get_mut(&set_id).with_context(|| format!("member {item_id} has no set {set_id}"))?;
            let first_set = first_set_by_item.get(&item_id).context("set member lost its first set")?;
            item.primary =
                (primary_set.as_deref() == Some(file.name.as_str()) && first_set != &file.name).then_some(true);
            file.item.push(item);
        }
        let mut augments = db.prepare(&format!(
            "SELECT {} FROM set_bonus_augments ORDER BY set_id, rowid",
            SetAugment::select_columns()
        ))?;
        for row in augments.query_map([], |row| Ok((row.get::<_, i64>("set_id")?, SetAugment::from_row(row)?)))? {
            let (id, augment) = row?;
            files.get_mut(&id).context("augment member has no set")?.augment.push(augment);
        }
        for (tier_id, (set_id, position)) in tier_ids {
            let Some(entries) = modifier_rows.remove(&("set_bonus_tier".to_string(), tier_id)) else { continue };
            let file = files.get_mut(&set_id).with_context(|| format!("tier {tier_id} has no set {set_id}"))?;
            for record in entries {
                file.tier[position].modifier.push(record);
            }
        }
        Ok(files)
    }

    pub(in crate::tree) fn write_set(&self, db: &Connection, path: &Path, id: i64, identity: &str) -> Result<()> {
        ensure!(
            id == super::super::ids::stable_hash(&["set_bonuses", identity]),
            "{}: set id does not match its slug",
            path.display()
        );
        self.insert(db, id, identity).with_context(|| format!("{}: inserting set {:?}", path.display(), self.name))?;
        Ok(())
    }

    pub(in crate::tree) fn insert_members(&self, db: &Connection, path: &Path, id: i64) -> Result<usize> {
        for item in &self.item {
            item.insert(db, id).with_context(|| format!("{}: set item {:?}", path.display(), item.name))?;
        }
        let mut augment_lookup = db.prepare_cached("SELECT id FROM augments WHERE name = ?1 AND family = ?2")?;
        for augment in &self.augment {
            let augment_id: i64 =
                augment_lookup.query_row([&augment.name, &augment.family], |row| row.get(0)).with_context(|| {
                    format!("{}: unknown set augment {:?} in {:?}", path.display(), augment.name, augment.family)
                })?;
            augment.insert(db, augment_id, id)?;
        }
        Ok(self.item.len() + self.augment.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_parent_fields_round_trip_through_typed_rows() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        let file: SetFile = toml::from_str("name = 'Test Set'\nicon = 'TestSet'\n").unwrap();
        let id = super::super::super::ids::stable_hash(&["set_bonuses", "test-set"]);
        file.write_set(&db, Path::new("packs/shared/sets/test-set.toml"), id, "test-set").unwrap();
        let notes = super::super::super::export::CorrectionNoteIndex::from_database(&db).unwrap();
        let mut modifiers = super::super::super::modifier_rows_by_owner(&db).unwrap();
        let restored = SetFile::all_from_rows(&db, &notes, &mut modifiers).unwrap();
        assert_eq!(restored[&id].name, "Test Set");
        assert_eq!(restored[&id].icon.as_deref(), Some("TestSet"));
        assert!(restored[&id].tier.is_empty());
    }
}
