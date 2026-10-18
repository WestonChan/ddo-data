use crate::tree::tables::{Modifier, Stance};
use anyhow::{Context, Result};
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct SpellDamage in "spell_damage" {
        keys { spell_id: i64, sort_order: i64 }
        fields {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            base_dice_bonus: Option<i64> => base_dice_bonus,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            base_dice_number: Option<i64> => base_dice_number,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            base_dice_sides: Option<i64> => base_dice_sides,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            bonus_dice_bonus: Option<i64> => bonus_dice_bonus,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            bonus_dice_number: Option<i64> => bonus_dice_number,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            bonus_dice_sides: Option<i64> => bonus_dice_sides,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            damage: Option<String> => damage,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            per_caster_levels: Option<i64> => per_caster_levels,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            spell_power: Option<String> => spell_power,
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct SpellDifficultyClass in "spell_dcs" {
        keys { spell_id: i64, sort_order: i64 }
        fields {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            amount: Vec<i64> => amount [json],
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            casting_stat_mod: bool => casting_stat_mod,
            dc_type: String => dc_type,
            dc_versus: String => dc_versus,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            mod_abilities: Vec<String> => mod_abilities [json],
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            schools: Vec<String> => schools [json],
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct Spell in "spells" {
        keys { id: i64, slug: &str }
        fields {
            name: String => name,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            cost: Option<i64> => cost,
            description: String => description,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            icon: Option<String> => icon [image ""],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            max_caster_level: Option<i64> => max_caster_level,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            metamagics: Vec<String> => metamagics [json],
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            schools: Vec<String> => schools [json],
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            damage: Vec<SpellDamage>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            dc: Vec<SpellDifficultyClass>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            modifier: Vec<Modifier>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            ddobuilder_correction: Vec<super::super::DdobuilderCorrection>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            stance: Vec<Stance>,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SpellListFile {
    pub(crate) row: Vec<Spell>,
}

impl SpellListFile {
    pub(in crate::tree) fn all_from_rows(
        db: &Connection,
        notes: &super::super::export::CorrectionNoteIndex,
        modifiers: &mut super::super::ModifierRowsByOwner,
    ) -> Result<BTreeMap<i64, Spell>> {
        let mut spells = BTreeMap::new();
        let mut select = db.prepare(&format!("SELECT {} FROM spells ORDER BY id", Spell::select_columns()))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("id")?, Spell::from_row(row)?)))? {
            let (id, mut spell) = row?;
            spell.ddobuilder_correction = notes.for_entity("spells", &spell.name);
            spell.modifier = modifiers.remove(&("spell".to_string(), id)).unwrap_or_default();
            spells.insert(id, spell);
        }
        let mut select = db.prepare(&format!(
            "SELECT {} FROM spell_damage ORDER BY spell_id, sort_order",
            SpellDamage::select_columns()
        ))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("spell_id")?, SpellDamage::from_row(row)?)))? {
            let (id, damage) = row?;
            spells.get_mut(&id).context("damage has no spell")?.damage.push(damage);
        }
        let mut select = db.prepare(&format!(
            "SELECT {} FROM spell_dcs ORDER BY spell_id, sort_order",
            SpellDifficultyClass::select_columns()
        ))?;
        for row in
            select.query_map([], |row| Ok((row.get::<_, i64>("spell_id")?, SpellDifficultyClass::from_row(row)?)))?
        {
            let (id, dc) = row?;
            spells.get_mut(&id).context("dc has no spell")?.dc.push(dc);
        }
        let mut requirements = super::super::requirement_rows_by_owner(db)?;
        for ((kind, id), mut stances) in super::super::stances_by_owner(db, &mut requirements)? {
            if kind != "spell" {
                continue;
            }
            let spell = spells.get_mut(&id).context("stance has no spell")?;
            for stance in &mut stances {
                stance.ddobuilder_correction =
                    notes.for_named_child("spells", &spell.name, &[("stance", &stance.name)]);
            }
            spell.stance = stances;
        }
        Ok(spells)
    }

    pub(in crate::tree) fn insert_damage_and_dcs(&self, db: &Connection) -> Result<usize> {
        let mut count = 0;
        for spell in &self.row {
            let identity = super::super::slug(&spell.name);
            let spell_id = super::super::ids::stable_hash(&["spells", &identity]);
            for (position, damage) in spell.damage.iter().enumerate() {
                count += damage.insert(db, spell_id, position as i64)?;
            }
            for (position, dc) in spell.dc.iter().enumerate() {
                count += dc.insert(db, spell_id, position as i64)?;
            }
        }
        Ok(count)
    }

    pub(in crate::tree) fn insert_modifiers_and_stances(&self, db: &Connection, path: &Path) -> Result<usize> {
        let mut count = 0;
        for spell in &self.row {
            let identity = super::super::slug(&spell.name);
            let spell_id = super::super::ids::stable_hash(&["spells", &identity]);
            for (position, modifier) in spell.modifier.iter().enumerate() {
                count += modifier
                    .write_modifier(db, "spell", spell_id, position as i64)
                    .with_context(|| format!("{}: spell {:?} modifier", path.display(), spell.name))?;
            }
            for (position, stance) in spell.stance.iter().enumerate() {
                count += stance.write_stance(
                    db,
                    None,
                    &ddo_model::identity::nested_slug(&identity, &stance.name),
                    "spell",
                    spell_id,
                    position as i64,
                )?;
            }
        }
        Ok(count)
    }

    pub(in crate::tree) fn write_spell_list(&self, db: &Connection, path: &Path) -> Result<usize> {
        for spell in &self.row {
            let identity = super::super::slug(&spell.name);
            let id = super::super::ids::stable_hash(&["spells", &identity]);
            spell.insert(db, id, &identity).with_context(|| format!("{}: spell {:?}", path.display(), spell.name))?;
        }
        Ok(self.row.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::trace::{TraceEvent, TraceEventCodes};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn spell_export_query_count_does_not_grow_with_spells() {
        static QUERY_COUNT: AtomicUsize = AtomicUsize::new(0);
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        db.trace_v2(
            TraceEventCodes::SQLITE_TRACE_STMT,
            Some(|event| {
                if let TraceEvent::Stmt(_, _) = event {
                    QUERY_COUNT.fetch_add(1, Ordering::SeqCst);
                }
            }),
        );
        let mut counts = Vec::new();
        for name in ["First Spell", "Second Spell"] {
            let file: SpellListFile =
                toml::from_str(&format!("[[row]]\nname = {name:?}\ndescription = 'A spell'\n")).unwrap();
            file.write_spell_list(&db, Path::new("characters/spells/test.toml")).unwrap();
            let notes = super::super::super::export::CorrectionNoteIndex::from_database(&db).unwrap();
            let mut modifiers = super::super::super::modifier_rows_by_owner(&db).unwrap();
            QUERY_COUNT.store(0, Ordering::SeqCst);
            let restored = SpellListFile::all_from_rows(&db, &notes, &mut modifiers).unwrap();
            assert_eq!(restored.len(), counts.len() + 1);
            counts.push(QUERY_COUNT.load(Ordering::SeqCst));
        }
        assert_eq!(counts, vec![5, 5]);
    }
}
