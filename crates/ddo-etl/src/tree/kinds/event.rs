use anyhow::{Context, Result};
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct EventFile in "events" {
        keys { id: i64, slug: &str, provenance: &str }
        fields { name: String => name,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            wiki_url: Option<String> => wiki_url [wiki "events"], }
        attached { citation: Vec<super::super::Citation>, }
    }
}

impl EventFile {
    pub(in crate::tree) fn all_from_rows(
        db: &Connection,
        annotations: &super::super::annotations::EntityAnnotations,
    ) -> Result<BTreeMap<i64, Self>> {
        let mut files = BTreeMap::new();
        let mut select = db.prepare(&format!("SELECT {} FROM events ORDER BY id", Self::select_columns()))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("id")?, Self::from_row(row)?)))? {
            let (id, mut file) = row?;
            annotations.entity("events", id).context("entity has no citation")?;
            file.attach_citations(annotations, id);
            files.insert(id, file);
        }
        Ok(files)
    }

    pub(in crate::tree) fn write_event(&self, db: &Connection, path: &Path, id: i64, identity: &str) -> Result<()> {
        self.insert(db, id, identity, "wiki")
            .with_context(|| format!("{}: inserting event {:?}", path.display(), self.name))?;
        Ok(())
    }
}
