use anyhow::{ensure, Context, Result};
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct PackFile in "adventure_packs" {
        keys { id: i64, slug: &str }
        fields { name: String => name,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            update: Option<String> => update,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            released: Option<String> => released,
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            is_free_to_play: bool => is_free_to_play, }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            citation: Vec<super::super::Citation>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            note: Vec<super::super::EntityNote>, }
    }
}

impl PackFile {
    pub(in crate::tree) fn all_from_rows(
        db: &Connection,
        annotations: &super::super::annotations::EntityAnnotations,
        notes: &super::super::export::CorrectionNoteIndex,
    ) -> Result<BTreeMap<i64, Self>> {
        let mut files = BTreeMap::new();
        let mut select = db.prepare(&format!("SELECT {} FROM adventure_packs ORDER BY id", Self::select_columns()))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("id")?, Self::from_row(row)?)))? {
            let (id, mut file) = row?;
            file.note = notes.for_entity("adventure_packs", &file.name);
            if let Some(annotation) = annotations.entity("adventure_packs", id) {
                file.citation = annotation.citation.clone();
                file.note.extend(annotation.reviewed_notes.iter().cloned());
            }
            files.insert(id, file);
        }
        Ok(files)
    }

    pub(in crate::tree) fn write_pack(&self, db: &Connection, path: &Path, id: i64, identity: &str) -> Result<()> {
        ensure!(
            id == super::super::ids::stable_hash(&["adventure_packs", identity]),
            "{}: pack id does not match its slug",
            path.display()
        );
        self.insert(db, id, identity).with_context(|| format!("{}: inserting pack {:?}", path.display(), self.name))?;
        Ok(())
    }
}
