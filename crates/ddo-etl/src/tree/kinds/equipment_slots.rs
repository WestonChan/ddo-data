use anyhow::Result;
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct EquipmentSlot in "equipment_slots" {
        keys { id: i64, slug: &str, sort_order: i64 }
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
pub(crate) struct EquipmentSlotsFile {
    pub(crate) row: Vec<EquipmentSlot>,
}

impl EquipmentSlotsFile {
    pub(in crate::tree) fn write_equipment_slots(
        &self,
        db: &Connection,
        path: &Path,
        ids: &mut super::super::ids::StableIds,
    ) -> Result<usize> {
        for (position, row) in self.row.iter().enumerate() {
            let identity = super::super::slug(&row.name);
            let id = ids.issue("equipment_slots", &identity, path)?;
            row.insert(db, id, &identity, position as i64 + 1)?;
        }
        Ok(self.row.len())
    }

    pub(in crate::tree) fn from_rows(
        db: &Connection,
        annotations: &crate::tree::annotations::EntityAnnotations,
    ) -> Result<Self> {
        let mut select = db.prepare(&format!(
            "SELECT {} FROM equipment_slots ORDER BY sort_order, rowid",
            EquipmentSlot::select_columns()
        ))?;
        let row = select
            .query_map([], |row| {
                let mut record = EquipmentSlot::from_row(row)?;
                record.attach_citations(annotations, row.get("id")?);
                Ok(record)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Self { row })
    }
}
