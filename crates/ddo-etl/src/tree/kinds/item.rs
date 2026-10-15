use super::super::file_types::*;
use crate::tree::tables::Modifier;
use anyhow::{ensure, Context, Result};
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct SocketOptionEffect in "item_augment_slot_option_effects" {
        keys { option_id: i64, sort_order: i64 }
        fields {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            bonus_type: Option<String> => bonus_type_id [optional_reference "bonus_types"],
            effect: String => effect_id [reference "effects"],
            value: i64 => value,
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct SocketGrant in "item_augment_slot_option_grants" {
        keys { option_id: i64, sort_order: i64 }
        fields {
            slot: String => slot_id [reference "augment_slot_types" "label"],
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct SocketSet in "item_augment_slot_option_sets" {
        keys { option_id: i64 }
        fields {
            set: String => set_id [reference "set_bonuses"],
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct Armor in "item_armor_stats" {
        keys { item_id: i64 }
        fields {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            adamantine_body: Option<i64> => adamantine_body,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            arcane_spell_failure: Option<i64> => arcane_spell_failure,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            armor_bonus: Option<i64> => armor_bonus,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            armor_check_penalty: Option<i64> => armor_check_penalty,
            armor_type: String => armor_type,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            damage_reduction: Option<i64> => damage_reduction,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            max_dex_bonus: Option<i64> => max_dex_bonus,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            mithral_body: Option<i64> => mithral_body,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            shield_bonus: Option<i64> => shield_bonus,
        }
    }
}

pub(crate) type ItemCitation = super::super::Citation;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct ItemClickie in "item_clickies" {
        keys { item_id: i64, sort_order: i64 }
        fields {
            name: String => name,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            clickie: Option<String> => clickie_id [optional_reference "clickies"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            spell: Option<String> => spell_id [optional_reference "spells"],
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct ItemDrBypass in "item_dr_bypass" {
        keys { item_id: i64 }
        fields {
            bypass: String => bypass,
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct ItemEffect in "item_effects" {
        keys { item_id: i64, sort_order: i64 }
        fields {
            name: String => effect_id [reference "effects"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            bonus_type: Option<String> => bonus_type_id [optional_reference "bonus_types"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            value: Option<i64> => value,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            value2: Option<i64> => value2,
        }
    }
}

pub(crate) type ItemNote = super::super::EntityNote;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct SocketOption in "item_augment_slot_options" {
        keys { id: Option<i64>, item_id: i64, slot_order: i64, option_order: i64 }
        fields {
            name: String => name,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            description: Option<String> => description,
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

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct Socket in "item_augment_slots" {
        keys { item_id: i64, sort_order: i64 }
        fields {
            slot: String => slot_id [reference "augment_slot_types" "label"],
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            option: Vec<SocketOption>,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("x-source-reference" = true))]
pub(crate) struct ItemSource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) chest: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) price: Vec<crate::vendor_prices::VendorPrice>,
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
    pub(crate) struct Weapon in "item_weapon_stats" {
        keys { item_id: i64, damage: Option<String>, critical: Option<String> }
        fields {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            attack_modifier: Option<String> => attack_modifier,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            base_dice_bonus: Option<i64> => base_dice_bonus,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            base_dice_count: Option<i64> => base_dice_count,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            base_dice_sides: Option<i64> => base_dice_sides,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            critical_multiplier: Option<i64> => critical_multiplier,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            critical_threat_range: Option<i64> => critical_threat_range,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            damage_modifier: Option<String> => damage_modifier,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            damage_multiplier: Option<f64> => damage_multiplier,
            handedness: String => handedness,
            weapon_type: String => weapon_type_id [reference "weapon_types"],
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct ItemFile in "items" {
        keys { id: i64, slug: &str, provenance: &str }
        fields {
            name: String => name,
            #[serde(default, skip_serializing_if = "is_false")]
            accepts_sentience: bool => accepts_sentience,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            description: Option<String> => description,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            drop_location: Option<String> => drop_location [trimmed],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            enhancement_bonus: Option<i64> => enhancement_bonus,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            icon: Option<String> => icon [image ""],
            #[serde(default, skip_serializing_if = "is_false")]
            is_legacy: bool => is_legacy,
            #[serde(default, skip_serializing_if = "is_false")]
            is_minor_artifact: bool => is_minor_artifact,
            item_category: ItemCategoryFile => item_category [enum_text],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            item_type: Option<String> => item_type,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            material: Option<String> => material_id [optional_reference "item_materials"],
            minimum_level: i64 => minimum_level,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            race_required: Option<String> => race_required,
            slot: String => slot_id [reference "equipment_slots"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            wiki_url: Option<String> => wiki_url [wiki "items"],
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            armor: Vec<Armor>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            citation: Vec<ItemCitation>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            clickie: Vec<ItemClickie>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            dr_bypass: Vec<ItemDrBypass>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            effect: Vec<ItemEffect>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            modifier: Vec<Modifier>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            removed: Vec<super::super::RemovedEntry>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            note: Vec<ItemNote>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            socket: Vec<Socket>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            source: Vec<ItemSource>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            weapon: Vec<Weapon>,
        }
    }
}

fn socket_option_for_id<'a>(
    files: &'a mut BTreeMap<i64, ItemFile>,
    locations: &BTreeMap<i64, (i64, usize, usize)>,
    option_id: i64,
) -> Result<&'a mut SocketOption> {
    let &(item_id, socket_index, option_index) =
        locations.get(&option_id).with_context(|| format!("socket option {option_id} has no parent"))?;
    files
        .get_mut(&item_id)
        .with_context(|| format!("socket option {option_id} has no item {item_id}"))?
        .socket
        .get_mut(socket_index)
        .with_context(|| format!("socket option {option_id} has no socket {socket_index}"))?
        .option
        .get_mut(option_index)
        .with_context(|| format!("socket option {option_id} has no entry {option_index}"))
}

impl ItemFile {
    pub(in crate::tree) fn insert_modifiers(&self, db: &Connection, path: &Path, id: i64) -> Result<usize> {
        self.modifier.iter().enumerate().try_fold(0, |count, (position, modifier)| {
            modifier
                .write_modifier(db, "item", id, position as i64)
                .map(|written| count + written)
                .with_context(|| format!("{}: item modifier", path.display()))
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
        let mut select = db.prepare(&format!("SELECT {} FROM items ORDER BY id", Self::select_columns()))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("id")?, Self::from_row(row)?)))? {
            let (id, mut file) = row?;
            if let Some(annotation) = annotations.entity("items", id) {
                file.citation = annotation.citation.clone();
                file.removed.clone_from(&annotation.removed);
            }
            file.note.extend(notes.for_entity("items", &file.name));
            if let Some(annotation) = annotations.entity("items", id) {
                file.note.extend(annotation.reviewed_notes.iter().cloned());
            }
            files.insert(id, file);
        }
        let mut select =
            db.prepare(&format!("SELECT {} FROM item_armor_stats ORDER BY item_id", Armor::select_columns()))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("item_id")?, Armor::from_row(row)?)))? {
            let (id, armor) = row?;
            files.get_mut(&id).with_context(|| format!("armor has no item {id}"))?.armor.push(armor);
        }
        let mut bypasses = db.prepare(&format!(
            "SELECT {} FROM item_dr_bypass ORDER BY item_id, rowid",
            ItemDrBypass::select_columns()
        ))?;
        for row in bypasses.query_map([], |row| Ok((row.get::<_, i64>("item_id")?, ItemDrBypass::from_row(row)?)))? {
            let (id, bypass) = row?;
            files.get_mut(&id).context("bypass has no item")?.dr_bypass.push(bypass);
        }
        let mut select =
            db.prepare(&format!("SELECT {} FROM item_weapon_stats ORDER BY item_id", Weapon::select_columns()))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("item_id")?, Weapon::from_row(row)?)))? {
            let (id, weapon) = row?;
            files.get_mut(&id).with_context(|| format!("weapon has no item {id}"))?.weapon.push(weapon);
        }
        let mut select = db.prepare(&format!(
            "SELECT {} FROM item_effects ORDER BY item_id, sort_order",
            ItemEffect::select_columns()
        ))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("item_id")?, ItemEffect::from_row(row)?)))? {
            let (id, effect) = row?;
            files.get_mut(&id).with_context(|| format!("effect link has no item {id}"))?.effect.push(effect);
        }
        let mut select = db.prepare(&format!(
            "SELECT {} FROM item_clickies ORDER BY item_id, sort_order",
            ItemClickie::select_columns()
        ))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("item_id")?, ItemClickie::from_row(row)?)))? {
            let (id, clickie) = row?;
            files.get_mut(&id).with_context(|| format!("clickie has no item {id}"))?.clickie.push(clickie);
        }
        let mut sockets = db.prepare(&format!(
            "SELECT {} FROM item_augment_slots ORDER BY item_id, sort_order",
            Socket::select_columns()
        ))?;
        for row in sockets.query_map([], |row| Ok((row.get::<_, i64>("item_id")?, Socket::from_row(row)?)))? {
            let (id, socket) = row?;
            files.get_mut(&id).context("socket has no item")?.socket.push(socket);
        }
        let mut option_locations = BTreeMap::new();
        let mut options = db.prepare(&format!(
            "SELECT {} FROM item_augment_slot_options ORDER BY item_id, slot_order, option_order",
            SocketOption::select_columns()
        ))?;
        for row in options.query_map([], |row| {
            Ok((
                row.get::<_, i64>("id")?,
                row.get::<_, i64>("item_id")?,
                row.get::<_, i64>("slot_order")?,
                row.get::<_, i64>("option_order")?,
                SocketOption::from_row(row)?,
            ))
        })? {
            let (option_id, item_id, socket_index, option_index, option) = row?;
            let socket_index = usize::try_from(socket_index)?;
            let option_index = usize::try_from(option_index)?;
            let socket = files
                .get_mut(&item_id)
                .with_context(|| format!("option {option_id} has no item {item_id}"))?
                .socket
                .get_mut(socket_index)
                .with_context(|| format!("option {option_id} has no socket {socket_index}"))?;
            ensure!(socket.option.len() == option_index, "option {option_id} has a gap in array order");
            socket.option.push(option);
            option_locations.insert(option_id, (item_id, socket_index, option_index));
        }
        let mut select = db.prepare(&format!(
            "SELECT {} FROM item_augment_slot_option_grants ORDER BY option_id, sort_order",
            SocketGrant::select_columns()
        ))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("option_id")?, SocketGrant::from_row(row)?)))? {
            let (id, record) = row?;
            socket_option_for_id(&mut files, &option_locations, id)?.grant.push(record);
        }
        let mut select = db.prepare(&format!(
            "SELECT {} FROM item_augment_slot_option_sets ORDER BY option_id, rowid",
            SocketSet::select_columns()
        ))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("option_id")?, SocketSet::from_row(row)?)))? {
            let (id, record) = row?;
            socket_option_for_id(&mut files, &option_locations, id)?.set.push(record);
        }
        let mut select = db.prepare(&format!(
            "SELECT {} FROM item_augment_slot_option_effects ORDER BY option_id, sort_order",
            SocketOptionEffect::select_columns()
        ))?;
        for row in
            select.query_map([], |row| Ok((row.get::<_, i64>("option_id")?, SocketOptionEffect::from_row(row)?)))?
        {
            let (id, record) = row?;
            socket_option_for_id(&mut files, &option_locations, id)?.effect.push(record);
        }
        for (id, entries) in std::mem::take(&mut source_rows.items) {
            let file = files.get_mut(&id).with_context(|| format!("sources have no item {id}"))?;
            if let Some(drop_location) = &file.drop_location {
                if let Some(raw_segment) = entries
                    .iter()
                    .filter_map(|source| source.drop_text_segment.as_deref())
                    .find(|segment| segment.trim_end() == drop_location && segment.len() > drop_location.len())
                {
                    file.drop_location = Some(raw_segment.to_string());
                }
            }
            file.source = entries
                .into_iter()
                .map(|source| ItemSource {
                    chest: source.chest,
                    price: source.price,
                    is_rare: source.is_rare,
                    kind: source.kind,
                    tier: source.tier,
                    loot_type: source.loot_type,
                    name: source.name,
                })
                .collect();
        }
        let item_modifier_keys = modifier_rows.keys().filter(|(kind, _)| kind == "item").cloned().collect::<Vec<_>>();
        for key in item_modifier_keys {
            let entries = modifier_rows.remove(&key).context("item modifier group disappeared")?;
            let file = files.get_mut(&key.1).with_context(|| format!("modifiers have no item {}", key.1))?;
            for record in entries {
                file.modifier.push(record);
            }
        }
        for option_id in option_locations.keys().copied().collect::<Vec<_>>() {
            let Some(entries) = modifier_rows.remove(&("item_augment_slot_option".to_string(), option_id)) else {
                continue;
            };
            let option = socket_option_for_id(&mut files, &option_locations, option_id)?;
            for record in entries {
                option.modifier.push(record);
            }
        }
        Ok(files)
    }

    pub(in crate::tree) fn insert_sockets(&self, db: &Connection, path: &Path, id: i64) -> Result<usize> {
        for (position, socket) in self.socket.iter().enumerate() {
            socket
                .insert(db, id, position as i64)
                .with_context(|| format!("{}: socket {:?}", path.display(), socket.slot))?;
        }
        Ok(self.socket.len())
    }

    pub(in crate::tree) fn insert_clickies(&self, db: &Connection, path: &Path, id: i64) -> Result<usize> {
        for (position, clickie) in self.clickie.iter().enumerate() {
            clickie.insert(db, id, position as i64).with_context(|| format!("{}: item clickie", path.display()))?;
        }
        Ok(self.clickie.len())
    }

    pub(in crate::tree) fn insert_effect_links(&self, db: &Connection, path: &Path, id: i64) -> Result<usize> {
        for (position, effect) in self.effect.iter().enumerate() {
            effect.insert(db, id, position as i64).with_context(|| format!("{}: item effect", path.display()))?;
        }
        Ok(self.effect.len())
    }

    pub(in crate::tree) fn insert_weapon_stats(&self, db: &Connection, path: &Path, id: i64) -> Result<usize> {
        ensure!(self.weapon.len() <= 1, "{}: item has more than one weapon row", path.display());
        let Some(weapon) = self.weapon.first() else { return Ok(0) };
        let bypasses = self.dr_bypass.iter().map(|entry| entry.bypass.clone()).collect::<Vec<_>>();
        let base_dice = weapon
            .base_dice_count
            .zip(weapon.base_dice_sides)
            .map(|(count, sides)| (count, sides, weapon.base_dice_bonus));
        let damage =
            crate::build::weapon_damage_text(weapon.damage_multiplier, base_dice, self.enhancement_bonus, &bypasses);
        let critical = weapon.critical_threat_range.zip(weapon.critical_multiplier).map(|(range, multiplier)| {
            if range <= 1 {
                format!("20 / x{multiplier}")
            } else {
                format!("{}-20 / x{multiplier}", 21 - range)
            }
        });
        weapon.insert(db, id, damage, critical).with_context(|| format!("{}: item weapon", path.display()))?;
        Ok(1)
    }

    pub(in crate::tree) fn write_item(&self, db: &Connection, path: &Path, id: i64, identity: &str) -> Result<usize> {
        ensure!(
            id == super::super::ids::stable_hash(&["items", identity]),
            "{}: item id does not match its slug",
            path.display()
        );
        self.insert(
            db,
            id,
            identity,
            if self.citation.iter().any(|citation| citation.field.is_empty()) { "wiki" } else { "maetrim" },
        )
        .with_context(|| format!("{}: inserting item {:?}", path.display(), self.name))?;
        ensure!(self.armor.len() <= 1, "{}: item has more than one armor row", path.display());
        if let Some(armor) = self.armor.first() {
            armor.insert(db, id)?;
        }
        for bypass in &self.dr_bypass {
            bypass.insert(db, id)?;
        }
        Ok(1 + self.armor.len() + self.dr_bypass.len())
    }
}

impl ItemFile {
    pub(in crate::tree) fn insert_socket_options(&self, db: &Connection, id: i64) -> Result<usize> {
        let mut count = 0;
        for (slot_order, socket) in self.socket.iter().enumerate() {
            for (option_order, option) in socket.option.iter().enumerate() {
                option.insert(db, None, id, slot_order as i64, option_order as i64)?;
                let option_id = db.last_insert_rowid();
                count += 1;
                for (position, grant) in option.grant.iter().enumerate() {
                    grant.insert(db, option_id, position as i64)?;
                    count += 1;
                }
                for set in &option.set {
                    set.insert(db, option_id)?;
                    count += 1;
                }
                for (position, effect) in option.effect.iter().enumerate() {
                    effect.insert(db, option_id, position as i64)?;
                    count += 1;
                }
                for (position, modifier) in option.modifier.iter().enumerate() {
                    count += modifier.write_modifier(db, "item_augment_slot_option", option_id, position as i64)?;
                }
            }
        }
        Ok(count)
    }
}

impl ItemFile {
    pub(in crate::tree) fn insert_sources(
        &self,
        db: &Connection,
        id: i64,
        quest_epic_names: &BTreeMap<String, String>,
        aliases: &crate::map::source_alias::SourceAliases,
    ) -> Result<usize> {
        let owner = super::super::sources::SourceOwner {
            drop_text: self.drop_location.as_deref(),
            minimum_level: Some(self.minimum_level),
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
                price: &source.price,
            };
            super::super::sources::insert_source(db, "items", id, &owner, &source, quest_epic_names, aliases)?;
        }
        Ok(self.source.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::trace::{TraceEvent, TraceEventCodes};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn weapon_labels_are_not_authorable_fields() {
        for field in ["damage", "critical"] {
            let contents = format!("weapon_type = 'Longsword'\nhandedness = 'One'\n{field} = 'invented'\n");
            let error = toml::from_str::<Weapon>(&contents).unwrap_err().to_string();
            assert!(error.contains(&format!("unknown field `{field}`")), "{error}");
        }
    }

    #[test]
    fn item_parent_fields_round_trip_through_typed_rows() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        let slots: super::super::EquipmentSlotsFile =
            toml::from_str("[[row]]\nname = 'Trinket'\ncategory = 'Equipment'\n").unwrap();
        let mut ids = super::super::super::ids::StableIds::default();
        slots.write_equipment_slots(&db, Path::new("effects/equipment-slots.toml"), &mut ids).unwrap();
        let file: ItemFile = toml::from_str(
            "name = 'Test Item'\nslot = 'Trinket'\nitem_category = 'Jewelry'\nminimum_level = 17\nis_legacy = true\n",
        )
        .unwrap();
        let id = super::super::super::ids::stable_hash(&["items", "test-item"]);
        file.write_item(&db, Path::new("test-item.toml"), id, "test-item").unwrap();
        let annotations = super::super::super::annotations::EntityAnnotations::from_database(&db).unwrap();
        let notes = super::super::super::export::CorrectionNoteIndex::from_database(&db).unwrap();
        let mut sources = super::super::super::sources::sources_by_owner(&db).unwrap();
        let mut modifiers = super::super::super::modifier_rows_by_owner(&db).unwrap();
        let restored = ItemFile::all_from_rows(&db, &annotations, &notes, &mut sources, &mut modifiers).unwrap();
        assert_eq!(restored[&id].name, "Test Item");
        assert_eq!(restored[&id].minimum_level, 17);
        assert!(restored[&id].is_legacy);
        assert_eq!(restored[&id].slot, "Trinket");
    }

    #[test]
    fn item_export_query_count_does_not_grow_with_items() {
        static QUERY_COUNT: AtomicUsize = AtomicUsize::new(0);
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        let slots: super::super::EquipmentSlotsFile =
            toml::from_str("[[row]]\nname = 'Trinket'\ncategory = 'Equipment'\n").unwrap();
        let mut ids = super::super::super::ids::StableIds::default();
        slots.write_equipment_slots(&db, Path::new("effects/equipment-slots.toml"), &mut ids).unwrap();
        db.trace_v2(
            TraceEventCodes::SQLITE_TRACE_STMT,
            Some(|event| {
                if let TraceEvent::Stmt(_, _) = event {
                    QUERY_COUNT.fetch_add(1, Ordering::SeqCst);
                }
            }),
        );
        let mut counts = Vec::new();
        for name in ["First Item", "Second Item"] {
            let file: ItemFile = toml::from_str(&format!(
                "name = {name:?}\nslot = 'Trinket'\nitem_category = 'Jewelry'\nminimum_level = 17\n"
            ))
            .unwrap();
            let identity = super::super::super::slug(name);
            let id = super::super::super::ids::stable_hash(&["items", &identity]);
            file.write_item(&db, Path::new("items/test.toml"), id, &identity).unwrap();
            let annotations = super::super::super::annotations::EntityAnnotations::from_database(&db).unwrap();
            let notes = super::super::super::export::CorrectionNoteIndex::from_database(&db).unwrap();
            let mut sources = super::super::super::sources::sources_by_owner(&db).unwrap();
            let mut modifiers = super::super::super::modifier_rows_by_owner(&db).unwrap();
            QUERY_COUNT.store(0, Ordering::SeqCst);
            let restored = ItemFile::all_from_rows(&db, &annotations, &notes, &mut sources, &mut modifiers).unwrap();
            assert_eq!(restored.len(), counts.len() + 1);
            counts.push(QUERY_COUNT.load(Ordering::SeqCst));
        }
        assert_eq!(counts, vec![11, 11]);
    }
}
