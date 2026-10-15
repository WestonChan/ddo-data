use anyhow::{Context, Result};
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct EffectTierFile in "effect_tier_groups" {
        keys { id: i64 }
        fields {
            name: String => name,
        }
    }
}

impl EffectTierFile {
    pub(in crate::tree) fn all_from_rows(db: &Connection) -> Result<BTreeMap<i64, Self>> {
        let mut files = BTreeMap::new();
        let mut select =
            db.prepare(&format!("SELECT {} FROM effect_tier_groups ORDER BY id", Self::select_columns()))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>(0)?, Self::from_row(row)?)))? {
            let (id, file) = row?;
            files.insert(id, file);
        }
        Ok(files)
    }

    pub(in crate::tree) fn write_effect_tier(&self, db: &Connection, path: &Path, id: i64) -> Result<()> {
        self.insert(db, id).with_context(|| format!("{}: inserting effect family {:?}", path.display(), self.name))?;
        Ok(())
    }
}
