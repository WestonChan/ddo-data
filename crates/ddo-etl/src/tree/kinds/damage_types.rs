use anyhow::Result;
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct DamageType in "damage_types" {
        keys { id: i64, slug: &str }
        fields {
            category: String => category,
            name: String => name,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DamageTypesFile {
    pub(crate) row: Vec<DamageType>,
}

impl DamageTypesFile {
    pub(in crate::tree) fn write_damage_types(
        &self,
        db: &Connection,
        path: &Path,
        ids: &mut super::super::ids::StableIds,
    ) -> Result<usize> {
        for row in &self.row {
            let identity = super::super::slug(&row.name);
            let id = ids.issue("damage_types", &identity, path)?;
            row.insert(db, id, &identity)?;
        }
        Ok(self.row.len())
    }

    pub(in crate::tree) fn from_rows(db: &Connection) -> Result<Self> {
        let mut select =
            db.prepare(&format!("SELECT {} FROM damage_types ORDER BY name, rowid", DamageType::select_columns()))?;
        let row = select.query_map([], DamageType::from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Self { row })
    }
}
