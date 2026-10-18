use anyhow::Result;
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct BonusType in "bonus_types" {
        keys { id: i64, slug: &str }
        fields {
            name: String => name,
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            stacks_with_self: bool => stacks_with_self,
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            citation: Vec<crate::tree::Citation>,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct BonusTypesFile {
    pub(crate) row: Vec<BonusType>,
}

impl BonusTypesFile {
    pub(in crate::tree) fn write_bonus_types(
        &self,
        db: &Connection,
        path: &Path,
        ids: &mut super::super::ids::StableIds,
    ) -> Result<usize> {
        for row in &self.row {
            let identity = super::super::slug(&row.name);
            let id = ids.issue("bonus_types", &identity, path)?;
            row.insert(db, id, &identity)?;
        }
        Ok(self.row.len())
    }

    pub(in crate::tree) fn from_rows(
        db: &Connection,
        annotations: &crate::tree::annotations::EntityAnnotations,
    ) -> Result<Self> {
        let mut select =
            db.prepare(&format!("SELECT {} FROM bonus_types ORDER BY name, rowid", BonusType::select_columns()))?;
        let row = select
            .query_map([], |row| {
                let mut record = BonusType::from_row(row)?;
                record.attach_citations(annotations, row.get("id")?);
                Ok(record)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Self { row })
    }
}

#[cfg(test)]
mod tests {
    use super::BonusTypesFile;
    use crate::tree::ids::StableIds;
    use rusqlite::Connection;
    use std::path::Path;

    #[test]
    fn bonus_types_round_trip_through_their_table() {
        let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/effects/bonus-types.toml"));
        let contents = std::fs::read_to_string(path).unwrap();
        let file: BonusTypesFile = toml::from_str(&contents).unwrap();
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        assert_eq!(file.write_bonus_types(&db, path, &mut StableIds::default()).unwrap(), file.row.len());
        let restored =
            BonusTypesFile::from_rows(&db, &crate::tree::annotations::EntityAnnotations::from_database(&db).unwrap())
                .unwrap();
        assert_eq!(toml::to_string_pretty(&restored).unwrap(), contents);
    }
}
