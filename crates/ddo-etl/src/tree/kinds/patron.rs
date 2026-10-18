use anyhow::{ensure, Context, Result};
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct PatronFile in "patrons" {
        keys { id: i64, slug: &str }
        fields { name: String => name, }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            citation: Vec<crate::tree::Citation>, #[serde(default, skip_serializing_if = "Vec::is_empty")]
            ddobuilder_correction: Vec<super::super::DdobuilderCorrection>, }
    }
}

impl PatronFile {
    pub(in crate::tree) fn all_from_rows(
        db: &Connection,
        notes: &super::super::export::CorrectionNoteIndex,
    ) -> Result<BTreeMap<i64, Self>> {
        let mut files = BTreeMap::new();
        let mut select = db.prepare(&format!("SELECT {} FROM patrons ORDER BY id", Self::select_columns()))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("id")?, Self::from_row(row)?)))? {
            let (id, mut file) = row?;
            file.ddobuilder_correction = notes.for_entity("patrons", &file.name);
            files.insert(id, file);
        }
        Ok(files)
    }

    pub(in crate::tree) fn write_patron(&self, db: &Connection, path: &Path, id: i64, identity: &str) -> Result<()> {
        ensure!(
            id == super::super::ids::stable_hash(&["patrons", identity]),
            "{}: patron id does not match its slug",
            path.display()
        );
        self.insert(db, id, identity)
            .with_context(|| format!("{}: inserting patron {:?}", path.display(), self.name))?;
        Ok(())
    }
}
