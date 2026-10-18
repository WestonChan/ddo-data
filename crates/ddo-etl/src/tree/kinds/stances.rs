use crate::tree::tables::Stance;
use anyhow::Result;
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct StancesFile {
    pub(crate) row: Vec<Stance>,
}

impl StancesFile {
    pub(in crate::tree) fn write_stances(
        &self,
        db: &Connection,
        path: &Path,
        ids: &mut super::super::ids::StableIds,
    ) -> Result<usize> {
        let mut count = 0;
        for (position, stance) in self.row.iter().enumerate() {
            let identity = super::super::slug(&stance.name);
            let id = ids.issue("stances", &identity, path)?;
            count += stance.write_stance(db, Some(id), &identity, "standalone", 0, position as i64)?;
        }
        Ok(count)
    }

    pub(in crate::tree) fn from_rows(
        db: &Connection,
        annotations: &crate::tree::annotations::EntityAnnotations,
    ) -> Result<Self> {
        let mut requirements = super::super::requirement_rows_by_owner(db)?;
        let mut stances = super::super::stances_by_owner(db, annotations, &mut requirements)?;
        Ok(Self { row: stances.remove(&("standalone".to_string(), 0)).unwrap_or_default() })
    }
}
