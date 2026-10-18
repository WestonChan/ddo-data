use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use anyhow::{ensure, Context, Result};
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::path::Path;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct EffectBonus in "effect_bonuses" {
        keys { effect_id: i64, sort_order: i64 }
        fields {
            amount_from: i64 => amount_from,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            bonus_type: Option<String> => bonus_type_id [optional_reference "bonus_types"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            constant: Option<i64> => constant,
            rounding: String => rounding,
            scale: f64 => scale,
            target: String => target_effect_id [reference "effects"],
        }
        derived_corrections {
            SCALING_CORRECTIONS { scale => scale [StatScale false], rounding => rounding [StatRounding false] }
        }
    }

}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct EffectDamage in "effect_damage" {
        keys { effect_id: i64, sort_order: i64 }
        fields {
            amount_from: i64 => amount_from,
            damage_type: String => damage_type_id [reference "damage_types"],
            dice_bonus: i64 => dice_bonus,
            dice_number: i64 => dice_number,
            dice_sides: i64 => dice_sides,
            scale: f64 => scale,
            trigger: String => trigger_id [reference "triggers"],
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct EffectFile in "effects" {
        keys { id: i64, slug: &str, is_stat: bool, is_group: bool }
        fields {
            name: String => name [correctable],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            variant: Option<String> => variant,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            default_value: Option<i64> => default_value [correctable],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            description_template: Option<String> => description_template [correctable],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            home_bonus_type: Option<String> => home_bonus_type_id [optional_reference "bonus_types"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            set_bonus_line_template: Option<String> => set_bonus_line_template,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            tier: Option<i64> => tier,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            tier_group: Option<String> => tier_group_id [optional_reference "effect_tier_groups"],
            verbose_name_template: String => verbose_name_template [correctable],
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            bonus: Vec<EffectBonus>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            damage: Vec<EffectDamage>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            citation: Vec<super::super::Citation>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            ddobuilder_correction: Vec<super::super::DdobuilderCorrection>,
        }
    }
}

impl EffectFile {
    pub(in crate::tree) fn all_from_rows(
        db: &Connection,
        annotations: &crate::tree::annotations::EntityAnnotations,
        notes: &super::super::export::CorrectionNoteIndex,
    ) -> Result<BTreeMap<i64, Self>> {
        let mut files = BTreeMap::new();
        let mut select = db.prepare(&format!(
            "SELECT {} FROM effects WHERE is_stat = 0 AND is_group = 0 ORDER BY id",
            Self::select_columns()
        ))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("id")?, Self::from_row(row)?)))? {
            let (id, mut file) = row?;
            file.attach_citations(annotations, id);
            file.ddobuilder_correction = notes.for_entity("effects", &file.name);
            files.insert(id, file);
        }
        let mut bonuses = db.prepare(&format!(
            "SELECT {} FROM effect_bonuses ORDER BY effect_id, sort_order",
            EffectBonus::select_columns()
        ))?;
        for row in bonuses.query_map([], |row| Ok((row.get::<_, i64>("effect_id")?, EffectBonus::from_row(row)?)))? {
            let (id, bonus) = row?;
            if let Some(file) = files.get_mut(&id) {
                file.bonus.push(bonus);
            }
        }
        let mut damages = db.prepare(&format!(
            "SELECT {} FROM effect_damage ORDER BY effect_id, sort_order",
            EffectDamage::select_columns()
        ))?;
        for row in damages.query_map([], |row| Ok((row.get::<_, i64>("effect_id")?, EffectDamage::from_row(row)?)))? {
            let (id, damage) = row?;
            if let Some(file) = files.get_mut(&id) {
                file.damage.push(damage);
            }
        }
        Ok(files)
    }

    pub(in crate::tree) fn file_text(&self) -> anyhow::Result<String> {
        super::super::typed_file_text(self)
    }

    pub(in crate::tree) fn write_effect(&self, db: &Connection, path: &Path, id: i64, identity: &str) -> Result<()> {
        ensure!(
            id == super::super::ids::stable_hash(&["effects", identity]),
            "{}: effect id does not match its slug",
            path.display()
        );
        self.insert(db, id, identity, false, false)
            .with_context(|| format!("{}: inserting effect {:?}", path.display(), self.name))?;
        Ok(())
    }

    pub(in crate::tree) fn insert_bonuses(&self, db: &Connection, path: &Path, id: i64) -> Result<usize> {
        for (position, bonus) in self.bonus.iter().enumerate() {
            bonus.insert(db, id, position as i64).with_context(|| format!("{}: effect bonus", path.display()))?;
        }
        Ok(self.bonus.len())
    }

    pub(in crate::tree) fn insert_damage(&self, db: &Connection, path: &Path, id: i64) -> Result<usize> {
        for (position, damage) in self.damage.iter().enumerate() {
            damage.insert(db, id, position as i64).with_context(|| format!("{}: effect damage", path.display()))?;
        }
        Ok(self.damage.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::trace::{TraceEvent, TraceEventCodes};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn named_effect_round_trips_through_its_table() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        let file: EffectFile = toml::from_str("name = 'Test Effect'\nverbose_name_template = 'Test Effect'\n").unwrap();
        let id = super::super::super::ids::stable_hash(&["effects", "test-effect"]);
        file.write_effect(&db, Path::new("effects/test-effect.toml"), id, "test-effect").unwrap();
        let notes = super::super::super::export::CorrectionNoteIndex::from_database(&db).unwrap();
        let annotations = crate::tree::annotations::EntityAnnotations::from_database(&db).unwrap();
        let all = EffectFile::all_from_rows(&db, &annotations, &notes).unwrap();
        let restored = &all[&id];
        assert_eq!(restored.name, "Test Effect");
        assert!(restored.bonus.is_empty());
        assert!(restored.damage.is_empty());
        assert_eq!(restored.file_text().unwrap(), "name = \"Test Effect\"\nverbose_name_template = \"Test Effect\"\n");
    }

    #[test]
    fn effect_export_query_count_does_not_grow_with_effects() {
        static QUERY_COUNT: AtomicUsize = AtomicUsize::new(0);
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        let notes = super::super::super::export::CorrectionNoteIndex::from_database(&db).unwrap();
        let annotations = crate::tree::annotations::EntityAnnotations::from_database(&db).unwrap();
        db.trace_v2(
            TraceEventCodes::SQLITE_TRACE_STMT,
            Some(|event| {
                if let TraceEvent::Stmt(_, _) = event {
                    QUERY_COUNT.fetch_add(1, Ordering::SeqCst);
                }
            }),
        );
        let mut counts = Vec::new();
        for name in ["First Effect", "Second Effect"] {
            let file: EffectFile =
                toml::from_str(&format!("name = {name:?}\nverbose_name_template = {name:?}\n")).unwrap();
            let identity = super::super::super::slug(name);
            let id = super::super::super::ids::stable_hash(&["effects", &identity]);
            file.write_effect(&db, Path::new("effects/test.toml"), id, &identity).unwrap();
            QUERY_COUNT.store(0, Ordering::SeqCst);
            assert_eq!(EffectFile::all_from_rows(&db, &annotations, &notes).unwrap().len(), counts.len() + 1);
            counts.push(QUERY_COUNT.load(Ordering::SeqCst));
        }
        assert_eq!(counts, vec![3, 3]);
    }
}
