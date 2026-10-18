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
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            citation: Vec<crate::tree::Citation>,
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

    pub(in crate::tree) fn from_rows(
        db: &Connection,
        annotations: &crate::tree::annotations::EntityAnnotations,
    ) -> Result<Self> {
        let mut select =
            db.prepare(&format!("SELECT {} FROM damage_types ORDER BY name, rowid", DamageType::select_columns()))?;
        let row = select
            .query_map([], |row| {
                let mut record = DamageType::from_row(row)?;
                record.attach_citations(annotations, row.get("id")?);
                Ok(record)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Self { row })
    }
}
