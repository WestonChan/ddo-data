use crate::tree::tables::Modifier;
use anyhow::{Context, Result};
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct Filigree in "filigrees" {
        keys { id: i64, slug: &str }
        fields {
            name: String => name,
            description: String => description,
            icon: String => icon [image ""],
            menu: String => menu,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            set: Option<String> => set_id [optional_reference "set_bonuses"],
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            ddobuilder_correction: Vec<super::super::DdobuilderCorrection>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            modifier: Vec<Modifier>,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FiligreeSetFile {
    pub(crate) row: Vec<Filigree>,
}

impl FiligreeSetFile {
    pub(in crate::tree) fn write_filigree_set(&self, db: &Connection, path: &Path) -> Result<usize> {
        let mut count = 0;
        for filigree in &self.row {
            let identity = super::super::slug(&filigree.name);
            let id = super::super::ids::stable_hash(&["filigrees", &identity]);
            filigree
                .insert(db, id, &identity)
                .with_context(|| format!("{}: filigree {:?}", path.display(), filigree.name))?;
            count += 1;
            for (position, modifier) in filigree.modifier.iter().enumerate() {
                count += modifier.write_modifier(db, "filigree", id, position as i64)?;
            }
        }
        Ok(count)
    }

    pub(in crate::tree) fn all_from_rows(
        db: &Connection,
        modifiers: &mut super::super::ModifierRowsByOwner,
        notes: &super::super::export::CorrectionNoteIndex,
    ) -> Result<BTreeMap<i64, Filigree>> {
        let mut filigrees = BTreeMap::new();
        let mut select = db.prepare(&format!("SELECT {} FROM filigrees ORDER BY id", Filigree::select_columns()))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("id")?, Filigree::from_row(row)?)))? {
            let (id, mut filigree) = row?;
            filigree.modifier = modifiers.remove(&("filigree".to_string(), id)).unwrap_or_default();
            filigree.ddobuilder_correction = notes.for_entity("filigrees", &filigree.name);
            filigrees.insert(id, filigree);
        }
        Ok(filigrees)
    }
}
