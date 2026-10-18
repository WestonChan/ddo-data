use anyhow::Result;
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct SocketType in "augment_slot_types" {
        keys { id: i64, slug: &str }
        fields {
            family: String => family,
            label: String => label,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            qualifier: Option<String> => qualifier,
            variant: String => variant,
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            ddobuilder_correction: Vec<super::super::DdobuilderCorrection>,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct AugmentSlotTypesFile {
    pub(crate) row: Vec<SocketType>,
}

impl AugmentSlotTypesFile {
    pub(in crate::tree) fn write_augment_slot_types(
        &self,
        db: &Connection,
        path: &Path,
        ids: &mut super::super::ids::StableIds,
    ) -> Result<usize> {
        for row in &self.row {
            let identity = super::super::slug(&row.label);
            let id = ids.issue("augment_slot_types", &identity, path)?;
            row.insert(db, id, &identity)?;
        }
        Ok(self.row.len())
    }

    pub(in crate::tree) fn from_rows(db: &Connection) -> Result<Self> {
        let mut select = db.prepare(&format!(
            "SELECT {} FROM augment_slot_types ORDER BY label, rowid",
            SocketType::select_columns()
        ))?;
        let mut row = select.query_map([], SocketType::from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        let notes = super::super::export::CorrectionNoteIndex::from_database(db)?;
        for socket in &mut row {
            socket.ddobuilder_correction = notes.for_entity("augment_slot_types", &socket.label);
        }
        Ok(Self { row })
    }
}
