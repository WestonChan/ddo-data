use super::table_merges::is_rebuilt_column;
use super::{table_merge, IdentityIndex, RowFields, TableMerge};
use crate::database_tables::{database_tables, DatabaseTable};
use anyhow::{ensure, Context, Result};
use rusqlite::{types::Value as SqlValue, Connection};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Display};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DatabaseDelta {
    pub entities: BTreeMap<String, EntityTableDelta>,
    pub arrays: BTreeMap<ArrayKey, ArrayChange>,
    pub correction_notes: EntityTableDelta,
    pub previous_upstream_sha: String,
    pub new_upstream_sha: String,
}

impl DatabaseDelta {
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty() && self.arrays.is_empty() && self.correction_notes.is_empty()
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EntityTableDelta {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<ColumnChange>,
}

impl EntityTableDelta {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnChange {
    pub identity: String,
    pub column: String,
    pub previous: String,
    pub new: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ArrayKey {
    pub owner: String,
    pub array: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrayChange {
    pub previous: Vec<String>,
    pub new: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct RowAddress {
    pub(super) table: String,
    pub(super) row_index: usize,
}

pub(super) struct DatabaseSnapshot {
    pub(super) tables: BTreeMap<String, DatabaseTable>,
    pub(super) index: IdentityIndex,
    pub(super) entity_rows: BTreeMap<String, BTreeMap<String, RowAddress>>,
    pub(super) array_rows: BTreeMap<ArrayKey, Vec<RowAddress>>,
    pub(super) rows_by_id: BTreeMap<(String, i64), RowAddress>,
    pub(super) array_keys: BTreeMap<RowAddress, ArrayKey>,
}

impl DatabaseSnapshot {
    pub(super) fn from_database(db: &Connection) -> Result<Self> {
        let tables = database_tables(db)?;
        let index = IdentityIndex::from_tables(db, &tables)?;
        let mut snapshot = Self {
            tables,
            index,
            entity_rows: BTreeMap::new(),
            array_rows: BTreeMap::new(),
            rows_by_id: BTreeMap::new(),
            array_keys: BTreeMap::new(),
        };
        for (table, contents) in &snapshot.tables {
            table_merge(table).with_context(|| format!("table {table} has no merge rule"))?;
            if let Some(column) = contents.columns.iter().position(|name| name == "id") {
                for (row_index, (_, row)) in contents.rows.iter().enumerate() {
                    if let SqlValue::Integer(id) = row[column] {
                        snapshot.rows_by_id.insert((table.clone(), id), RowAddress { table: table.clone(), row_index });
                    }
                }
            }
        }
        let addresses = snapshot
            .tables
            .iter()
            .flat_map(|(table, contents)| {
                (0..contents.rows.len())
                    .map(|row_index| RowAddress { table: table.clone(), row_index })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        for address in addresses {
            match table_merge(&address.table).unwrap() {
                TableMerge::Entity { .. } | TableMerge::Corrections { .. } => {
                    let id = snapshot.id(&address)?;
                    let identity = snapshot.index.identity(&address.table, id)?;
                    ensure!(
                        snapshot
                            .entity_rows
                            .entry(address.table.clone())
                            .or_default()
                            .insert(identity.clone(), address)
                            .is_none(),
                        "duplicate entity identity {identity}"
                    );
                }
                TableMerge::Array { .. } | TableMerge::DatasetArray { .. } => {
                    let key = snapshot.array_key(&address, &mut BTreeSet::new())?;
                    snapshot.array_rows.entry(key).or_default().push(address);
                }
                TableMerge::Rederived | TableMerge::DatasetVersion => {}
            }
        }
        Ok(snapshot)
    }

    pub(super) fn fields(&self, address: &RowAddress) -> RowFields {
        let table = &self.tables[&address.table];
        table.columns.iter().cloned().zip(table.rows[address.row_index].1.iter().cloned()).collect()
    }

    pub(super) fn id(&self, address: &RowAddress) -> Result<i64> {
        match self.fields(address).get("id") {
            Some(SqlValue::Integer(id)) => Ok(*id),
            _ => anyhow::bail!("{} row {} has no integer id", address.table, address.row_index),
        }
    }

    pub(super) fn reference(&self, address: &RowAddress, column: &str) -> Result<Option<RowAddress>> {
        let fields = self.fields(address);
        let Some(SqlValue::Integer(id)) = fields.get(column) else { return Ok(None) };
        let Some(table) = self.index.reference_target(&address.table, column, &fields) else { return Ok(None) };
        Ok(Some(
            self.rows_by_id
                .get(&(table.clone(), *id))
                .with_context(|| format!("{}.{} references absent {table}/{id}", address.table, column))?
                .clone(),
        ))
    }

    fn array_key(&mut self, address: &RowAddress, visiting: &mut BTreeSet<RowAddress>) -> Result<ArrayKey> {
        if let Some(key) = self.array_keys.get(address) {
            return Ok(key.clone());
        }
        ensure!(visiting.insert(address.clone()), "cyclic array ownership at {}", address.table);
        let key = match table_merge(&address.table).context("array table has no merge rule")? {
            TableMerge::DatasetArray { array } => ArrayKey { owner: "dataset".to_string(), array },
            TableMerge::Array { owner_column, array } => {
                let owner = self
                    .reference(address, owner_column)?
                    .with_context(|| format!("{}.{} has no array owner", address.table, owner_column))?;
                match table_merge(&owner.table) {
                    Some(TableMerge::Entity { .. }) => {
                        let owner_id = self.id(&owner)?;
                        ArrayKey { owner: self.index.identity(&owner.table, owner_id)?, array }
                    }
                    Some(TableMerge::Array { .. }) => self.array_key(&owner, visiting)?,
                    _ => anyhow::bail!("array {} has unsupported owner {}", address.table, owner.table),
                }
            }
            _ => anyhow::bail!("{} is not an array table", address.table),
        };
        visiting.remove(address);
        self.array_keys.insert(address.clone(), key.clone());
        Ok(key)
    }

    pub(super) fn normalized_fields(&mut self, address: &RowAddress) -> Result<BTreeMap<String, String>> {
        let fields = self.fields(address);
        fields
            .iter()
            .filter(|(column, _)| column.as_str() != "id" && !is_rebuilt_column(&address.table, column))
            .map(|(column, value)| {
                Ok((column.clone(), self.index.normalized_cell(&address.table, column, value, &fields)?))
            })
            .collect()
    }

    pub(super) fn normalized_array(&mut self, key: &ArrayKey) -> Result<Vec<String>> {
        let rows = self.array_rows.get(key).cloned().unwrap_or_default();
        rows.into_iter()
            .map(|address| {
                let fields = self.normalized_fields(&address)?;
                Ok(format!(
                    "{}: {}",
                    address.table,
                    fields.into_iter().map(|(column, value)| format!("{column}={value}")).collect::<Vec<_>>().join("/")
                ))
            })
            .collect()
    }

    pub(super) fn report_cell(&mut self, address: &RowAddress, column: &str) -> Result<serde_json::Value> {
        let fields = self.fields(address);
        let cell = &fields[column];
        if matches!(cell, SqlValue::Integer(_))
            && self
                .index
                .reference_target(&address.table, column, &fields)
                .is_some_and(|target| matches!(table_merge(&target), Some(TableMerge::Entity { .. })))
        {
            return Ok(self.index.normalized_cell(&address.table, column, cell, &fields)?.into());
        }
        Ok(match cell {
            SqlValue::Null => serde_json::Value::Null,
            SqlValue::Integer(number) => (*number).into(),
            SqlValue::Real(number) => (*number).into(),
            SqlValue::Text(text) => text.clone().into(),
            SqlValue::Blob(bytes) => serde_json::to_value(bytes)?,
        })
    }

    pub(super) fn report_array(&mut self, key: &ArrayKey, differing: &BTreeSet<usize>) -> Result<String> {
        let mut rows = Vec::new();
        for (index, address) in self.array_rows.get(key).cloned().unwrap_or_default().into_iter().enumerate() {
            if !differing.contains(&index) {
                continue;
            }
            let mut fields = serde_json::Map::new();
            for column in self.normalized_fields(&address)?.keys() {
                fields.insert(column.clone(), self.report_cell(&address, column)?);
            }
            rows.push(serde_json::json!({"table": address.table, "fields": fields}));
        }
        Ok(serde_json::to_string(&rows)?)
    }

    fn version(&self, table: &str, column: &str) -> Result<String> {
        let contents = self.tables.get(table).with_context(|| format!("missing {table}"))?;
        ensure!(contents.rows.len() == 1, "{table} must have exactly one row");
        let column_index = contents
            .columns
            .iter()
            .position(|name| name == column)
            .with_context(|| format!("missing {table}.{column}"))?;
        Ok(match &contents.rows[0].1[column_index] {
            SqlValue::Text(text) => text.clone(),
            SqlValue::Integer(number) => number.to_string(),
            _ => anyhow::bail!("invalid {table}.{column}"),
        })
    }
}

pub fn database_delta(previous: &Connection, new: &Connection) -> Result<DatabaseDelta> {
    let mut previous = DatabaseSnapshot::from_database(previous)?;
    let mut new = DatabaseSnapshot::from_database(new)?;
    snapshot_delta(&mut previous, &mut new)
}

pub(super) fn snapshot_delta(previous: &mut DatabaseSnapshot, new: &mut DatabaseSnapshot) -> Result<DatabaseDelta> {
    let previous_version = previous.version("schema_version", "version")?;
    let new_version = new.version("schema_version", "version")?;
    ensure!(previous_version == new_version, "schema version differs: {previous_version} vs {new_version}");
    ensure!(previous.tables.keys().eq(new.tables.keys()), "database table sets differ");
    for (table, contents) in &previous.tables {
        ensure!(contents.columns == new.tables[table].columns, "table {table} columns differ");
    }
    let mut delta = DatabaseDelta {
        previous_upstream_sha: previous.version("dataset_version", "upstream_sha")?,
        new_upstream_sha: new.version("dataset_version", "upstream_sha")?,
        ..DatabaseDelta::default()
    };
    let tables: BTreeSet<_> = previous.entity_rows.keys().chain(new.entity_rows.keys()).cloned().collect();
    for table in tables {
        let previous_rows = previous.entity_rows.get(&table).cloned().unwrap_or_default();
        let new_rows = new.entity_rows.get(&table).cloned().unwrap_or_default();
        let identities: BTreeSet<_> = previous_rows.keys().chain(new_rows.keys()).cloned().collect();
        let mut changes = EntityTableDelta::default();
        for identity in identities {
            match (previous_rows.get(&identity), new_rows.get(&identity)) {
                (None, Some(_)) => changes.added.push(identity),
                (Some(_), None) => changes.removed.push(identity),
                (Some(before), Some(after)) => {
                    let before = previous.normalized_fields(before)?;
                    let after = new.normalized_fields(after)?;
                    for (column, value) in before {
                        if value != after[&column] {
                            changes.changed.push(ColumnChange {
                                identity: identity.clone(),
                                column: column.clone(),
                                previous: value,
                                new: after[&column].clone(),
                            });
                        }
                    }
                }
                (None, None) => unreachable!(),
            }
        }
        if table == "corrections" {
            delta.correction_notes = changes;
        } else if !changes.is_empty() {
            delta.entities.insert(table, changes);
        }
    }
    let keys: BTreeSet<_> = previous.array_rows.keys().chain(new.array_rows.keys()).cloned().collect();
    for key in keys {
        let before = previous.normalized_array(&key)?;
        let after = new.normalized_array(&key)?;
        if before != after {
            delta.arrays.insert(key, ArrayChange { previous: before, new: after });
        }
    }
    Ok(delta)
}

impl Display for DatabaseDelta {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut counts: BTreeMap<String, (usize, usize, usize, usize)> = self
            .entities
            .iter()
            .map(|(table, changes)| {
                (table.clone(), (changes.added.len(), changes.removed.len(), changes.changed.len(), 0))
            })
            .collect();
        if !self.correction_notes.is_empty() {
            let changes = &self.correction_notes;
            counts.insert("corrections".into(), (changes.added.len(), changes.removed.len(), changes.changed.len(), 0));
        }
        for key in self.arrays.keys() {
            let table =
                if key.owner == "dataset" { key.array } else { key.owner.split('/').next().unwrap_or(&key.owner) };
            counts.entry(table.to_string()).or_default().3 += 1;
        }
        for (table, (added, removed, changed, arrays)) in counts {
            writeln!(
                formatter,
                "{table}: {added} added, {removed} removed, {changed} changed, {arrays} arrays changed"
            )?;
        }
        Ok(())
    }
}
