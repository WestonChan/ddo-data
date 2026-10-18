use anyhow::Result;
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct SentientGemFile in "sentient_gems" {
        keys { id: i64, slug: &str }
        fields {
            description: String => description,
            icon: String => icon [image ""],
            name: String => name,
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            citation: Vec<crate::tree::Citation>,
        }
    }
}

impl SentientGemFile {
    pub(in crate::tree) fn write_sentient_gem(
        &self,
        db: &Connection,
        path: &Path,
        ids: &mut super::super::ids::StableIds,
    ) -> Result<i64> {
        let identity = super::super::slug(&self.name);
        super::super::build::validate_entity_file_name(path, &identity, false, None)?;
        let id = ids.issue("sentient_gems", &identity, path)?;
        self.insert(db, id, &identity)?;
        Ok(id)
    }

    pub(in crate::tree) fn all_from_rows(db: &Connection) -> Result<Vec<(i64, Self)>> {
        let mut select =
            db.prepare(&format!("SELECT {} FROM sentient_gems ORDER BY name, rowid", Self::select_columns()))?;
        let rows =
            select.query_map([], |row| Ok((row.get("id")?, Self::from_row(row)?)))?.collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }
}
