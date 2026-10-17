use super::compute::{snapshot_delta, DatabaseSnapshot, RowAddress};
use super::{table_merge, ArrayKey, RowFields, TableMerge, TABLE_MERGES};
use anyhow::{ensure, Context, Result};
use rusqlite::{params_from_iter, types::Value as SqlValue, Connection};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default)]
pub struct MergeOutcome {
    pub applied: BTreeMap<String, AppliedCounts>,
    pub conflicts: Vec<Conflict>,
    pub declined_removals: Vec<DeclinedRemoval>,
    pub absent_in_ours: Vec<AbsentChange>,
    pub kept_correction_notes: Vec<String>,
    pub inserted: Vec<(String, i64)>,
    pub deleted: Vec<(String, i64)>,
}

#[derive(Debug, Default)]
pub struct AppliedCounts {
    pub added: usize,
    pub removed: usize,
    pub changed: usize,
    pub arrays_replaced: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictSubject {
    Column(String),
    Array(&'static str),
}

#[derive(Debug)]
pub struct Conflict {
    pub table: String,
    pub identity: String,
    pub ours_id: i64,
    pub subject: ConflictSubject,
    pub previous: String,
    pub new: String,
    pub ours: String,
}

#[derive(Debug)]
pub struct DeclinedRemoval {
    pub table: String,
    pub identity: String,
    pub ours_id: i64,
    pub reason: RemovalDeclineReason,
    pub position_renumbered: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemovalDeclineReason {
    Annotated,
    EditedByUs,
    ReferencedBy(String),
    OwnerKept(String),
}

#[derive(Debug)]
pub struct AbsentChange {
    pub table: String,
    pub identity: String,
    pub subject: ConflictSubject,
    pub previous: String,
    pub new: String,
}

#[derive(Default)]
struct MergeEdits {
    columns: BTreeMap<RowAddress, BTreeMap<String, RowAddress>>,
    inserted: BTreeSet<RowAddress>,
    deleted: BTreeSet<RowAddress>,
    removals: BTreeMap<String, RowAddress>,
    current_positions: BTreeMap<RowAddress, i64>,
    incoming_positions: BTreeMap<RowAddress, i64>,
    outcome: MergeOutcome,
}

pub(crate) fn clear_rederived_tables(db: &Connection) -> Result<()> {
    for (table, rule) in TABLE_MERGES {
        if matches!(rule, TableMerge::Rederived) {
            db.execute(&format!("DELETE FROM \"{table}\""), [])?;
        }
    }
    Ok(())
}

pub fn merge_delta_into(
    ours: &mut Connection,
    previous: &Connection,
    new: &Connection,
    annotated: &BTreeSet<(String, i64)>,
) -> Result<MergeOutcome> {
    ensure!(ours.is_autocommit(), "merge requires a connection outside a transaction");
    let mut previous = DatabaseSnapshot::from_database(previous)?;
    let mut new = DatabaseSnapshot::from_database(new)?;
    let mut current = DatabaseSnapshot::from_database(ours)?;
    let upstream_delta = snapshot_delta(&mut previous, &mut new)?;
    let local_delta = snapshot_delta(&mut previous, &mut current)?;
    let mut edits = MergeEdits::default();
    let mut changes_by_table = upstream_delta.entities;
    changes_by_table.insert("corrections".into(), upstream_delta.correction_notes);
    for (table, changes) in changes_by_table {
        for identity in changes.removed {
            if table == "corrections" {
                if entity_row(&current, &table, &identity).is_some() {
                    edits.outcome.kept_correction_notes.push(identity);
                }
            } else if let Some(address) = entity_row(&current, &table, &identity) {
                edits.removals.insert(identity, address.clone());
            }
        }
        for identity in changes.added {
            let incoming = entity_row(&new, &table, &identity).unwrap().clone();
            if let Some(address) = entity_row(&current, &table, &identity).cloned() {
                for (column, incoming_value) in new.normalized_fields(&incoming)? {
                    let local_value = current.normalized_fields(&address)?[&column].clone();
                    if local_value != incoming_value {
                        edits.outcome.conflicts.push(Conflict {
                            table: table.clone(),
                            identity: identity.clone(),
                            ours_id: current.id(&address)?,
                            subject: ConflictSubject::Column(column),
                            previous: "absent".into(),
                            new: incoming_value,
                            ours: local_value,
                        });
                    }
                }
            } else {
                edits.inserted.insert(incoming);
                edits.outcome.applied.entry(table.clone()).or_default().added += 1;
            }
        }
        for change in changes.changed {
            let Some(address) = entity_row(&current, &table, &change.identity).cloned() else {
                edits.outcome.absent_in_ours.push(AbsentChange {
                    table: table.clone(),
                    identity: change.identity,
                    subject: ConflictSubject::Column(change.column),
                    previous: change.previous,
                    new: change.new,
                });
                continue;
            };
            let local_value = current.normalized_fields(&address)?[&change.column].clone();
            if local_value == change.new {
                continue;
            }
            if local_value == change.previous {
                let incoming = entity_row(&new, &table, &change.identity).unwrap().clone();
                edits.columns.entry(address).or_default().insert(change.column, incoming);
                edits.outcome.applied.entry(table.clone()).or_default().changed += 1;
            } else {
                edits.outcome.conflicts.push(Conflict {
                    table: table.clone(),
                    identity: change.identity,
                    ours_id: current.id(&address)?,
                    subject: ConflictSubject::Column(change.column),
                    previous: change.previous,
                    new: change.new,
                    ours: local_value,
                });
            }
        }
    }
    let keys: BTreeSet<_> =
        previous.array_rows.keys().chain(new.array_rows.keys()).chain(current.array_rows.keys()).cloned().collect();
    for key in keys {
        let before = previous.normalized_array(&key)?;
        let incoming = new.normalized_array(&key)?;
        let local = current.normalized_array(&key)?;
        let table = array_table(&key).to_string();
        if key.owner != "dataset" && entity_row(&new, &table, &key.owner).is_none() {
            continue;
        }
        let added_owner = key.owner != "dataset"
            && entity_row(&previous, &table, &key.owner).is_none()
            && entity_row(&new, &table, &key.owner).is_some();
        if edits.removals.contains_key(&key.owner) || (before == incoming && !added_owner) {
            continue;
        }
        let local_owner = entity_row(&current, &table, &key.owner);
        let inserted_owner =
            entity_row(&new, &table, &key.owner).is_some_and(|address| edits.inserted.contains(address));
        if key.owner != "dataset" && local_owner.is_none() && !inserted_owner {
            edits.outcome.absent_in_ours.push(AbsentChange {
                table,
                identity: key.owner,
                subject: ConflictSubject::Array(key.array),
                previous: array_text(&before),
                new: array_text(&incoming),
            });
        } else if local == incoming {
            continue;
        } else if inserted_owner || (!added_owner && local == before) || key.owner == "dataset" && local == before {
            edits.deleted.extend(current.array_rows.get(&key).into_iter().flatten().cloned());
            edits.inserted.extend(new.array_rows.get(&key).into_iter().flatten().cloned());
            edits.outcome.applied.entry(table).or_default().arrays_replaced += 1;
        } else {
            edits.outcome.conflicts.push(Conflict {
                table,
                identity: key.owner,
                ours_id: local_owner.map(|address| current.id(address)).transpose()?.unwrap_or(0),
                subject: ConflictSubject::Array(key.array),
                previous: if added_owner { "absent".into() } else { array_text(&before) },
                new: array_text(&incoming),
                ours: array_text(&local),
            });
        }
    }
    let edited_entities: BTreeSet<_> = local_delta
        .entities
        .values()
        .flat_map(|changes| changes.changed.iter().map(|change| change.identity.clone()))
        .chain(local_delta.arrays.keys().map(|key| key.owner.clone()))
        .collect();
    decline_removals(&mut edits, &mut current, annotated, &edited_entities)?;
    assign_merged_positions(&mut edits, &mut previous, &mut current, &mut new)?;
    let foreign_keys: bool = ours.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
    ours.pragma_update(None, "foreign_keys", false)?;
    let applied = (|| {
        let transaction = ours.transaction()?;
        clear_rederived_tables(&transaction)?;
        apply_edits(&transaction, &mut edits, &mut current, &mut new)?;
        let violation: Option<String> = transaction
            .prepare("PRAGMA foreign_key_check")?
            .query_map([], |row| {
                Ok(format!(
                    "{} row {} references {}",
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?
                ))
            })?
            .next()
            .transpose()?;
        ensure!(violation.is_none(), "merge foreign key check failed: {}", violation.unwrap_or_default());
        transaction.commit()?;
        Ok(edits.outcome)
    })();
    let restored = ours.pragma_update(None, "foreign_keys", foreign_keys);
    let outcome = applied?;
    restored?;
    Ok(outcome)
}

fn entity_row<'a>(snapshot: &'a DatabaseSnapshot, table: &str, identity: &str) -> Option<&'a RowAddress> {
    snapshot.entity_rows.get(table)?.get(identity)
}

fn array_table(key: &ArrayKey) -> &str {
    if key.owner == "dataset" {
        key.array
    } else {
        key.owner.split('/').next().unwrap()
    }
}

fn array_text(rows: &[String]) -> String {
    rows.join("\n")
}

fn removal_rows(
    edits: &MergeEdits,
    current: &DatabaseSnapshot,
    declined: &BTreeMap<String, RemovalDeclineReason>,
) -> BTreeSet<RowAddress> {
    let mut removed = edits.deleted.clone();
    for (identity, address) in &edits.removals {
        if !declined.contains_key(identity) {
            removed.insert(address.clone());
            for (key, rows) in &current.array_rows {
                if &key.owner == identity {
                    removed.extend(rows.iter().cloned());
                }
            }
        }
    }
    removed
}

fn decline_removals(
    edits: &mut MergeEdits,
    current: &mut DatabaseSnapshot,
    annotated: &BTreeSet<(String, i64)>,
    edited_entities: &BTreeSet<String>,
) -> Result<()> {
    if edits.removals.is_empty() {
        return Ok(());
    }
    let mut declined = BTreeMap::new();
    let identities_by_address: BTreeMap<_, _> =
        edits.removals.iter().map(|(identity, address)| (address.clone(), identity.clone())).collect();
    for (identity, address) in &edits.removals {
        if annotated.contains(&(address.table.clone(), current.id(address)?)) {
            declined.insert(identity.clone(), RemovalDeclineReason::Annotated);
        } else if edited_entities.contains(identity) {
            declined.insert(identity.clone(), RemovalDeclineReason::EditedByUs);
        }
    }
    loop {
        let previous_count = declined.len();
        for (identity, address) in &edits.removals {
            if declined.contains_key(identity) {
                continue;
            }
            if let Some(TableMerge::Entity { owner_column: Some(column) }) = table_merge(&address.table) {
                if let Some(owner) = current.reference(address, column)? {
                    if let Some(owner_identity) = identities_by_address.get(&owner) {
                        if declined.contains_key(owner_identity) {
                            declined.insert(identity.clone(), RemovalDeclineReason::OwnerKept(owner_identity.clone()));
                        }
                    }
                }
            }
        }
        let deleted = removal_rows(edits, current, &declined);
        for (table, contents) in &current.tables {
            if matches!(table_merge(table), Some(TableMerge::Rederived | TableMerge::DatasetVersion)) {
                continue;
            }
            for row_index in 0..contents.rows.len() {
                let address = RowAddress { table: table.clone(), row_index };
                if deleted.contains(&address) {
                    continue;
                }
                for column in &contents.columns {
                    if column == "id" {
                        continue;
                    }
                    if edits.columns.get(&address).is_some_and(|columns| columns.contains_key(column)) {
                        continue;
                    }
                    if let Some(target) = current.reference(&address, column)? {
                        if let Some(identity) = identities_by_address.get(&target) {
                            let referrer = if let Some(key) = current.array_keys.get(&address) {
                                key.owner.clone()
                            } else {
                                current.index.identity(table, current.id(&address)?)?
                            };
                            declined.entry(identity.clone()).or_insert(RemovalDeclineReason::ReferencedBy(referrer));
                        }
                    }
                }
            }
        }
        if previous_count == declined.len() {
            break;
        }
    }
    edits.deleted = removal_rows(edits, current, &declined);
    for (identity, address) in &edits.removals {
        if let Some(reason) = declined.remove(identity) {
            edits.outcome.declined_removals.push(DeclinedRemoval {
                table: address.table.clone(),
                identity: identity.clone(),
                ours_id: current.id(address)?,
                reason,
                position_renumbered: false,
            });
        } else {
            edits.outcome.applied.entry(address.table.clone()).or_default().removed += 1;
            edits.outcome.deleted.push((address.table.clone(), current.id(address)?));
        }
    }
    Ok(())
}

type OrderedEntitiesByOwner = BTreeMap<(String, String), Vec<(i64, String, RowAddress)>>;

fn ordered_entities(snapshot: &mut DatabaseSnapshot) -> Result<OrderedEntitiesByOwner> {
    let mut groups: OrderedEntitiesByOwner = BTreeMap::new();
    for (table, rows) in snapshot.entity_rows.clone() {
        let Some(TableMerge::Entity { owner_column: Some(owner_column) }) = table_merge(&table) else { continue };
        if !snapshot.tables[&table].columns.iter().any(|column| column == "sort_order") {
            continue;
        }
        for (identity, address) in rows {
            let Some(owner) = snapshot.reference(&address, owner_column)? else { continue };
            let owner_identity = snapshot.index.identity(&owner.table, snapshot.id(&owner)?)?;
            let SqlValue::Integer(position) = snapshot.fields(&address)["sort_order"] else {
                anyhow::bail!("{table} position is not an integer")
            };
            groups.entry((table.clone(), owner_identity)).or_default().push((position, identity, address));
        }
    }
    for rows in groups.values_mut() {
        rows.sort();
    }
    Ok(groups)
}

fn assign_merged_positions(
    edits: &mut MergeEdits,
    previous: &mut DatabaseSnapshot,
    current: &mut DatabaseSnapshot,
    new: &mut DatabaseSnapshot,
) -> Result<()> {
    let before = ordered_entities(previous)?;
    let incoming = ordered_entities(new)?;
    let local = ordered_entities(current)?;
    let keys = before.keys().chain(incoming.keys()).cloned().collect::<BTreeSet<_>>();
    for key in keys {
        let previous_rows = before.get(&key).cloned().unwrap_or_default();
        let incoming_rows = incoming.get(&key).cloned().unwrap_or_default();
        if previous_rows
            .iter()
            .map(|(position, identity, _)| (position, identity))
            .eq(incoming_rows.iter().map(|(position, identity, _)| (position, identity)))
        {
            continue;
        }
        let local_rows = local.get(&key).cloned().unwrap_or_default();
        let incoming_by_identity = incoming_rows
            .iter()
            .map(|(position, identity, _)| (identity.clone(), *position))
            .collect::<BTreeMap<_, _>>();
        let mut positions = Vec::new();
        for (position, identity, address) in local_rows {
            if edits.deleted.contains(&address) {
                continue;
            }
            let incoming_position = incoming_by_identity.get(&identity);
            positions.push((
                incoming_position.copied().unwrap_or(position),
                incoming_position.is_none(),
                identity,
                false,
                address,
            ));
        }
        for (position, identity, address) in incoming_rows {
            if edits.inserted.contains(&address) {
                positions.push((position, false, identity, true, address));
            }
        }
        positions.sort();
        for (position, (_, _, identity, is_incoming, address)) in positions.into_iter().enumerate() {
            let position = i64::try_from(position)?;
            if is_incoming {
                edits.incoming_positions.insert(address, position);
            } else {
                let changed = current.fields(&address)["sort_order"] != SqlValue::Integer(position);
                if changed && !edits.columns.get(&address).is_some_and(|columns| columns.contains_key("sort_order")) {
                    edits.outcome.applied.entry(address.table.clone()).or_default().changed += 1;
                }
                for removal in &mut edits.outcome.declined_removals {
                    if removal.identity == identity {
                        removal.position_renumbered = changed;
                    }
                }
                edits.outcome.conflicts.retain(|conflict| {
                    conflict.identity != identity || conflict.subject != ConflictSubject::Column("sort_order".into())
                });
                edits.current_positions.insert(address, position);
            }
        }
    }
    Ok(())
}

struct TranslatedRows {
    ids: BTreeMap<RowAddress, i64>,
    entities: BTreeMap<String, i64>,
}

impl TranslatedRows {
    fn from_edits(edits: &MergeEdits, current: &mut DatabaseSnapshot, new: &mut DatabaseSnapshot) -> Result<Self> {
        let mut entities = BTreeMap::new();
        for rows in current.entity_rows.values() {
            for (identity, address) in rows {
                if !edits.deleted.contains(address) {
                    entities.insert(identity.clone(), current.id(address)?);
                }
            }
        }
        let mut maximum_ids = BTreeMap::new();
        for (table, id) in current.rows_by_id.keys() {
            let maximum = maximum_ids.entry(table.clone()).or_insert(0_i64);
            *maximum = (*maximum).max(*id);
        }
        let mut allocated = current.rows_by_id.keys().cloned().collect::<BTreeSet<_>>();
        let mut ids = BTreeMap::new();
        for address in &edits.inserted {
            let Some(SqlValue::Integer(original_id)) = new.fields(address).get("id").cloned() else { continue };
            let is_entity =
                matches!(table_merge(&address.table), Some(TableMerge::Entity { .. } | TableMerge::CorrectionNotes));
            let maximum = maximum_ids.entry(address.table.clone()).or_insert(0);
            let assigned = if is_entity && !allocated.contains(&(address.table.clone(), original_id)) {
                original_id
            } else {
                maximum.checked_add(1).context("merge exhausted integer row ids")?
            };
            *maximum = (*maximum).max(assigned);
            allocated.insert((address.table.clone(), assigned));
            ids.insert(address.clone(), assigned);
            if is_entity {
                entities.insert(new.index.identity(&address.table, original_id)?, assigned);
            }
        }
        Ok(Self { ids, entities })
    }

    fn cell(&self, address: &RowAddress, column: &str, new: &mut DatabaseSnapshot) -> Result<SqlValue> {
        if column == "id" {
            if let Some(id) = self.ids.get(address) {
                return Ok(SqlValue::Integer(*id));
            }
        }
        let Some(target) = new.reference(address, column)? else { return Ok(new.fields(address)[column].clone()) };
        let translated = if let Some(id) = self.ids.get(&target) {
            *id
        } else {
            let target_id = new.id(&target)?;
            let identity = new.index.identity(&target.table, target_id)?;
            let referrer = if let Some(key) = new.array_keys.get(address) {
                key.owner.clone()
            } else {
                new.index.identity(&address.table, new.id(address)?)?
            };
            *self.entities.get(&identity).with_context(|| {
                format!(
                    "his {referrer} ({}.{column}) references {identity}, which the data files no longer have",
                    address.table
                )
            })?
        };
        Ok(SqlValue::Integer(translated))
    }

    fn fields(&self, address: &RowAddress, new: &mut DatabaseSnapshot) -> Result<RowFields> {
        new.tables[&address.table]
            .columns
            .clone()
            .into_iter()
            .map(|column| Ok((column.clone(), self.cell(address, &column, new)?)))
            .collect()
    }
}

fn apply_edits(
    db: &Connection,
    edits: &mut MergeEdits,
    current: &mut DatabaseSnapshot,
    new: &mut DatabaseSnapshot,
) -> Result<()> {
    let translation = TranslatedRows::from_edits(edits, current, new)?;
    for (table, contents) in &current.tables {
        let mut delete = db.prepare(&format!("DELETE FROM \"{table}\" WHERE rowid = ?1"))?;
        for address in edits.deleted.iter().filter(|address| &address.table == table) {
            delete.execute([contents.rows[address.row_index].0])?;
        }
    }
    for table in current.tables.keys() {
        let addresses = edits.current_positions.keys().filter(|address| &address.table == table).collect::<Vec<_>>();
        if addresses.is_empty() {
            continue;
        }
        let minimum = current.tables[table]
            .rows
            .iter()
            .map(|(_, row)| {
                let column = current.tables[table].columns.iter().position(|column| column == "sort_order").unwrap();
                match row[column] {
                    SqlValue::Integer(position) => position,
                    _ => 0,
                }
            })
            .min()
            .unwrap_or(0);
        let mut statement = db.prepare(&format!("UPDATE \"{table}\" SET sort_order = ?1 WHERE rowid = ?2"))?;
        for (offset, address) in addresses.into_iter().enumerate() {
            let temporary =
                minimum.checked_sub(i64::try_from(offset)? + 1).context("merge exhausted temporary positions")?;
            statement.execute([temporary, current.tables[table].rows[address.row_index].0])?;
        }
    }
    let mut pending = edits.inserted.clone();
    let mut insert_statements = BTreeMap::new();
    for address in &pending {
        if !insert_statements.contains_key(&address.table) {
            let columns = &new.tables[&address.table].columns;
            let quoted = columns.iter().map(|column| format!("\"{column}\"")).collect::<Vec<_>>().join(", ");
            let placeholders = vec!["?"; columns.len()].join(", ");
            insert_statements.insert(
                address.table.clone(),
                db.prepare(&format!("INSERT INTO \"{}\" ({quoted}) VALUES ({placeholders})", address.table))?,
            );
        }
    }
    while !pending.is_empty() {
        let before = pending.len();
        let mut blocked_tables = BTreeSet::new();
        for address in pending.clone() {
            if blocked_tables.contains(&address.table) {
                continue;
            }
            let columns = new.tables[&address.table].columns.clone();
            let dependencies = columns
                .iter()
                .filter(|column| column.as_str() != "id")
                .map(|column| new.reference(&address, column))
                .collect::<Result<Vec<_>>>()?;
            if dependencies.iter().flatten().any(|target| target != &address && pending.contains(target)) {
                blocked_tables.insert(address.table.clone());
                continue;
            }
            let mut fields = translation.fields(&address, new)?;
            if let Some(position) = edits.incoming_positions.get(&address) {
                fields.insert("sort_order".into(), SqlValue::Integer(*position));
            }
            insert_statements
                .get_mut(&address.table)
                .unwrap()
                .execute(params_from_iter(columns.iter().map(|column| &fields[column])))
                .with_context(|| format!("inserting {} row {}", address.table, address.row_index))?;
            if matches!(table_merge(&address.table), Some(TableMerge::Entity { .. } | TableMerge::CorrectionNotes)) {
                edits.outcome.inserted.push((address.table.clone(), translation.ids[&address]));
            }
            pending.remove(&address);
        }
        ensure!(pending.len() < before, "cyclic references among inserted rows");
    }
    for (table, contents) in &current.tables {
        let assignments =
            contents.columns.iter().map(|column| format!("\"{column}\" = ?")).collect::<Vec<_>>().join(", ");
        let mut update = db.prepare(&format!("UPDATE \"{table}\" SET {assignments} WHERE rowid = ?"))?;
        let addresses = edits
            .columns
            .keys()
            .chain(edits.current_positions.keys())
            .filter(|address| &address.table == table)
            .cloned()
            .collect::<BTreeSet<_>>();
        for address in &addresses {
            let columns = edits.columns.get(address);
            let mut fields = current.fields(address);
            for (column, incoming) in columns.into_iter().flatten() {
                fields.insert(column.clone(), translation.cell(incoming, column, new)?);
            }
            if let Some(position) = edits.current_positions.get(address) {
                fields.insert("sort_order".into(), SqlValue::Integer(*position));
            }
            let mut values = contents.columns.iter().map(|column| fields[column].clone()).collect::<Vec<_>>();
            values.push(SqlValue::Integer(contents.rows[address.row_index].0));
            update
                .execute(params_from_iter(values))
                .with_context(|| format!("updating {table} row {}", address.row_index))?;
        }
    }
    Ok(())
}
