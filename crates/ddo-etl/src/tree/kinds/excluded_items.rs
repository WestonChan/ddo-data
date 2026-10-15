use anyhow::Result;
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct ExcludedItem in "excluded_items" {
        keys {  }
        fields {
            name: String => name,
            reason: String => reason,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExcludedItemsFile {
    pub(crate) row: Vec<ExcludedItem>,
}

impl ExcludedItemsFile {
    pub(in crate::tree) fn write_excluded_items(&self, db: &Connection) -> Result<usize> {
        for row in &self.row {
            row.insert(db)?;
        }
        Ok(self.row.len())
    }

    pub(in crate::tree) fn from_rows(db: &Connection) -> Result<Self> {
        let mut select =
            db.prepare(&format!("SELECT {} FROM excluded_items ORDER BY name, rowid", ExcludedItem::select_columns()))?;
        let row = select.query_map([], ExcludedItem::from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Self { row })
    }
}
