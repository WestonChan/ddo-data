use anyhow::{Context, Result};
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct WeaponType in "weapon_types" {
        keys { id: i64, slug: &str }
        fields {
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            is_shield: bool => is_shield,
            name: String => name,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            proficiency: Option<String> => proficiency_id [optional_reference "weapon_proficiencies"],
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            citation: Vec<crate::tree::Citation>,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WeaponTypesFile {
    pub(crate) row: Vec<WeaponType>,
}

impl WeaponTypesFile {
    pub(in crate::tree) fn write_weapon_types(
        &self,
        db: &Connection,
        path: &Path,
        ids: &mut super::super::ids::StableIds,
    ) -> Result<usize> {
        for row in &self.row {
            let identity = super::super::slug(&row.name);
            let id = ids.issue("weapon_types", &identity, path)?;
            row.insert(db, id, &identity).with_context(|| format!("{}: weapon type {:?}", path.display(), row.name))?;
        }
        Ok(self.row.len())
    }

    pub(in crate::tree) fn from_rows(
        db: &Connection,
        annotations: &crate::tree::annotations::EntityAnnotations,
    ) -> Result<Self> {
        let mut select =
            db.prepare(&format!("SELECT {} FROM weapon_types ORDER BY name, rowid", WeaponType::select_columns()))?;
        let row = select
            .query_map([], |row| {
                let mut record = WeaponType::from_row(row)?;
                record.attach_citations(annotations, row.get("id")?);
                Ok(record)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Self { row })
    }
}
