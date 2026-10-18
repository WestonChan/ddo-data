use crate::tree::tables::Modifier;
use anyhow::Result;
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct OptionalBuff in "optional_buffs" {
        keys { id: i64, slug: &str }
        fields {
            name: String => name,
            description: String => description,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            icon: Option<String> => icon [image "*"],
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            ddobuilder_correction: Vec<super::super::DdobuilderCorrection>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            modifier: Vec<Modifier>,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OptionalBuffsFile {
    pub(crate) row: Vec<OptionalBuff>,
}

impl OptionalBuffsFile {
    pub(in crate::tree) fn from_rows(
        db: &Connection,
        modifiers: &mut super::super::ModifierRowsByOwner,
        notes: &super::super::export::CorrectionNoteIndex,
    ) -> Result<Self> {
        let mut select =
            db.prepare(&format!("SELECT {} FROM optional_buffs ORDER BY name, id", OptionalBuff::select_columns()))?;
        let mut rows = Vec::new();
        for record in
            select.query_map([], |record| Ok((record.get::<_, i64>("id")?, OptionalBuff::from_row(record)?)))?
        {
            let (id, mut row) = record?;
            row.ddobuilder_correction = notes.for_entity("optional_buffs", &row.name);
            row.modifier = modifiers.remove(&("optional_buff".to_string(), id)).unwrap_or_default();
            rows.push(row);
        }
        Ok(Self { row: rows })
    }
    pub(in crate::tree) fn write_optional_buffs(
        &self,
        db: &Connection,
        path: &Path,
        ids: &mut super::super::ids::StableIds,
    ) -> Result<usize> {
        let mut count = 0;
        for row in &self.row {
            let identity = super::super::slug(&row.name);
            let id = ids.issue("optional_buffs", &identity, path)?;
            row.insert(db, id, &identity)?;
            count += 1;
            for (position, modifier) in row.modifier.iter().enumerate() {
                count += modifier.write_modifier(db, "optional_buff", id, position as i64)?;
            }
        }
        Ok(count)
    }
}
