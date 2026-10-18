use anyhow::{Context, Result};
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct StatGroupFile in "effects" {
        keys { id: i64, slug: &str, is_stat: bool, is_group: bool }
        fields {
            name: String => name,
            description_template: String => description_template,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            home_bonus_type: Option<String> => home_bonus_type_id [optional_reference "bonus_types"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            set_bonus_line_template: Option<String> => set_bonus_line_template,
            verbose_name_template: String => verbose_name_template,
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            citation: Vec<crate::tree::Citation>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            bonus: Vec<super::effect::EffectBonus>,
        }
    }
}

impl StatGroupFile {
    pub(in crate::tree) fn all_from_rows(db: &Connection) -> Result<BTreeMap<i64, Self>> {
        let mut files = BTreeMap::new();
        let mut select =
            db.prepare(&format!("SELECT {} FROM effects WHERE is_group = 1 ORDER BY id", Self::select_columns()))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("id")?, Self::from_row(row)?)))? {
            let (id, file) = row?;
            files.insert(id, file);
        }
        let mut select = db.prepare(&format!(
            "SELECT {} FROM effect_bonuses ORDER BY effect_id, sort_order",
            super::effect::EffectBonus::select_columns()
        ))?;
        for row in select
            .query_map([], |row| Ok((row.get::<_, i64>("effect_id")?, super::effect::EffectBonus::from_row(row)?)))?
        {
            let (id, bonus) = row?;
            if let Some(file) = files.get_mut(&id) {
                file.bonus.push(bonus);
            }
        }
        Ok(files)
    }

    pub(in crate::tree) fn file_text(&self) -> anyhow::Result<String> {
        super::super::typed_file_text(self)
    }

    pub(in crate::tree) fn write_stat_group(
        &self,
        db: &Connection,
        path: &Path,
        ids: &mut super::super::ids::StableIds,
    ) -> Result<i64> {
        let identity = super::super::slug(&self.name);
        let id = ids.issue("effects", &identity, path)?;
        self.insert(db, id, &identity, false, true)
            .with_context(|| format!("{}: inserting group {:?}", path.display(), self.name))?;
        Ok(id)
    }

    pub(in crate::tree) fn insert_bonuses(&self, db: &Connection, path: &Path, id: i64) -> Result<usize> {
        for (position, bonus) in self.bonus.iter().enumerate() {
            bonus.insert(db, id, position as i64).with_context(|| format!("{}: group bonus", path.display()))?;
        }
        Ok(self.bonus.len())
    }
}
