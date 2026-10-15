use super::super::file_types::*;
use anyhow::{Context, Result};
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct StatFile in "effects" {
        keys { id: i64, slug: &str, is_stat: bool, is_group: bool }
        fields {
            name: String => name,
            category: StatCategoryFile => category [enum_text],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            default_value: Option<i64> => default_value,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            description_template: Option<String> => description_template,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            home_bonus_type: Option<String> => home_bonus_type_id [optional_reference "bonus_types"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            set_bonus_line_template: Option<String> => set_bonus_line_template,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            tier: Option<i64> => tier,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            tier_group: Option<String> => tier_group_id [optional_reference "effect_tier_groups"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            verbose_name_template: Option<String> => verbose_name_template,
        }
    }
}

impl StatFile {
    pub(in crate::tree) fn all_from_rows(db: &Connection) -> Result<BTreeMap<i64, Self>> {
        let mut files = BTreeMap::new();
        let mut select =
            db.prepare(&format!("SELECT {} FROM effects WHERE is_stat = 1 ORDER BY id", Self::select_columns()))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("id")?, Self::from_row(row)?)))? {
            let (id, file) = row?;
            files.insert(id, file);
        }
        Ok(files)
    }

    pub(in crate::tree) fn file_text(&self) -> anyhow::Result<String> {
        super::super::typed_file_text(self)
    }

    pub(in crate::tree) fn write_stat(
        &self,
        db: &Connection,
        path: &Path,
        ids: &mut super::super::ids::StableIds,
    ) -> Result<i64> {
        let identity = super::super::slug(&self.name);
        let id = ids.issue("effects", &identity, path)?;
        self.insert(db, id, &identity, true, false)
            .with_context(|| format!("{}: inserting stat {:?}", path.display(), self.name))?;
        Ok(id)
    }
}
