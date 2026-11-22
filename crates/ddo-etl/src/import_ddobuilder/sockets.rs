use super::ImportReportEntry;
use crate::delta::MergeOutcome;
use crate::tree::DataFilePaths;
use anyhow::Result;
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct RetiredItemSockets {
    pub(super) retained_items: BTreeSet<String>,
    added_sockets: BTreeSet<(String, String)>,
    retired_labels: BTreeSet<String>,
}

impl RetiredItemSockets {
    pub(super) fn from_databases(ours: &Connection, incoming: &Connection) -> Result<Self> {
        let retired_labels = crate::wiki::WikiOverrides::from_database(ours)?.retired_socket_labels;
        let existing_items = ours
            .prepare("SELECT name FROM items")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<BTreeSet<_>>>()?;
        let mut sockets = Self { retained_items: BTreeSet::new(), added_sockets: BTreeSet::new(), retired_labels };
        let mut statement = incoming.prepare("SELECT DISTINCT i.name,t.label FROM item_augment_slots s JOIN items i ON i.id=s.item_id JOIN augment_slot_types t ON t.id=s.slot_id")?;
        for row in statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))? {
            let (name, label) = row?;
            if sockets.retired_labels.contains(&label) {
                if existing_items.contains(&name) {
                    sockets.retained_items.insert(name);
                } else {
                    sockets.added_sockets.insert((name, label));
                }
            }
        }
        Ok(sockets)
    }

    pub(super) fn report_entries(&self, db: &Connection) -> Result<Vec<ImportReportEntry>> {
        if self.added_sockets.is_empty() {
            return Ok(Vec::new());
        }
        let paths = DataFilePaths::from_database(db)?;
        let items = db
            .prepare("SELECT name,id FROM items")?
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?
            .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
        self.added_sockets.iter().map(|(name, label)| Ok(ImportReportEntry {
            path: paths.path("items", items[name])?,
            category: "NEEDS SLOT TYPE",
            detail: format!("{name} socket {label:?}: this socket uses a retired label; add an item_socket label correction, choosing the system from its recipe or source; settle before the next import"),
        })).collect()
    }

    pub(super) fn discard_unreferenced_additions(&self, db: &mut Connection, outcome: &mut MergeOutcome) -> Result<()> {
        if self.retained_items.is_empty() {
            return Ok(());
        }
        let inserted = outcome.inserted.iter().cloned().collect::<BTreeSet<_>>();
        let transaction = db.transaction()?;
        let uncarried = transaction
            .prepare(crate::build::CARRIERLESS_SOCKET_OPTION_SETS)?
            .query_map([], |row| Ok((row.get::<_, i64>(1)?, row.get::<_, bool>(3)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut discarded = BTreeSet::new();
        for (set_id, referenced) in uncarried {
            if referenced || !inserted.contains(&("socket_option_sets".into(), set_id)) {
                continue;
            }
            let effects = crate::build::delete_socket_option_set(&transaction, set_id)?;
            discarded.insert(("socket_option_sets".to_string(), set_id));
            for effect_id in effects {
                if inserted.contains(&("effects".into(), effect_id))
                    && crate::build::delete_unowned_effect(&transaction, effect_id)?
                {
                    discarded.insert(("effects".into(), effect_id));
                }
            }
        }
        let references = transaction.prepare("SELECT m.name,f.\"from\" FROM sqlite_schema m JOIN pragma_foreign_key_list(m.name) f WHERE m.type='table' AND f.\"table\"='augment_slot_types' AND f.\"to\"='id'")?
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let unreferenced = references
            .iter()
            .map(|(table, column)| format!("NOT EXISTS (SELECT 1 FROM \"{table}\" WHERE \"{column}\"=?1)"))
            .collect::<Vec<_>>()
            .join(" AND ");
        let labels = transaction
            .prepare("SELECT id,label FROM augment_slot_types")?
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
        for (table, id) in &inserted {
            if table == "augment_slot_types"
                && labels.get(id).is_some_and(|label| self.retired_labels.contains(label))
                && transaction
                    .execute(&format!("DELETE FROM augment_slot_types WHERE id=?1 AND {unreferenced}"), [id])?
                    > 0
            {
                discarded.insert((table.clone(), *id));
            }
        }
        transaction.commit()?;
        for (table, _) in &discarded {
            outcome.applied.get_mut(table).expect("an inserted entity has an applied count").added -= 1;
        }
        outcome.inserted.retain(|entity| !discarded.contains(entity));
        Ok(())
    }
}
