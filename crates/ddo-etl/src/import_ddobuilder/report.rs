use super::corrections::{CorrectionEvent, CorrectionKey};
use super::icons::IconMerge;
use crate::delta::{ConflictSubject, IdentityIndex, MergeOutcome, RemovalDeclineReason, TABLE_MERGES};
use crate::tree::{DataFileChanges, DataFilePaths};
use anyhow::{Context, Result};
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::fmt::{self, Display};
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct ImportTableCounts {
    pub added: usize,
    pub removed: usize,
    pub changed: usize,
    pub conflicts: usize,
    pub declined_removals: usize,
    pub resolved_notes: usize,
}

#[derive(Debug)]
pub struct ImportReportEntry {
    pub path: PathBuf,
    pub category: &'static str,
    pub detail: String,
}

#[derive(Debug)]
pub struct DdobuilderImportReport {
    pub previous_sha: String,
    pub new_sha: String,
    pub tables: BTreeMap<String, ImportTableCounts>,
    pub entries: Vec<ImportReportEntry>,
    pub files: DataFileChanges,
    conflict_count: usize,
}

impl DdobuilderImportReport {
    pub fn conflict_count(&self) -> usize {
        self.conflict_count
    }
    pub fn has_conflicts(&self) -> bool {
        self.conflict_count() != 0
    }
}

fn quoted(value: &str) -> String {
    serde_json::to_string(value).expect("string serialization cannot fail")
}

fn report_value(encoded: &str) -> String {
    serde_json::from_str::<serde_json::Value>(encoded).map_or_else(|_| quoted(encoded), |value| value.to_string())
}

fn subject_name(subject: &ConflictSubject) -> &str {
    match subject {
        ConflictSubject::Column(column) => column,
        ConflictSubject::Array(array) => array,
    }
}

pub(super) fn report(
    revisions: (String, String),
    outcome: MergeOutcome,
    events: Vec<CorrectionEvent>,
    icons: IconMerge,
    rebuilt: &Connection,
    incoming: &Connection,
    old_paths: &DataFilePaths,
) -> Result<DdobuilderImportReport> {
    let paths = DataFilePaths::from_database(rebuilt)?;
    let incoming_paths = DataFilePaths::from_database(incoming)?;
    let mut tables: BTreeMap<String, ImportTableCounts> =
        TABLE_MERGES.iter().map(|(table, _)| ((*table).to_string(), ImportTableCounts::default())).collect();
    for (table, applied) in outcome.applied {
        let counts = tables.entry(table).or_default();
        counts.added = applied.added;
        counts.removed = applied.removed;
        counts.changed = applied.changed + applied.arrays_replaced;
    }
    let mut entries = Vec::new();
    let mut conflict_count = 0;
    for conflict in outcome.conflicts {
        tables.entry(conflict.table.clone()).or_default().conflicts += 1;
        conflict_count += 1;
        let conflict_paths = if conflict.identity == "dataset" {
            let mut files = std::collections::BTreeSet::new();
            if conflict.table == "pack_exceptions" {
                for encoded in [&conflict.previous, &conflict.new, &conflict.ours] {
                    let rows: serde_json::Value = serde_json::from_str(encoded)?;
                    for row in rows.as_array().context("pack exception conflict is not an array")? {
                        let name = row["fields"]["pack_name"].as_str().context("pack exception has no owner")?;
                        let path = paths
                            .pack_path(name)
                            .or_else(|| incoming_paths.pack_path(name))
                            .or_else(|| old_paths.pack_path(name))
                            .context("pack exception owner has no path")?;
                        files.insert(path.clone());
                    }
                }
            } else {
                files.extend(paths.dataset_paths(&conflict.table));
                files.extend(incoming_paths.dataset_paths(&conflict.table));
                files.extend(old_paths.dataset_paths(&conflict.table));
            }
            anyhow::ensure!(!files.is_empty(), "dataset conflict {} has no file", conflict.table);
            files
        } else {
            std::collections::BTreeSet::from([paths.path(&conflict.table, conflict.ours_id)?])
        };
        let mut conflict_paths = conflict_paths.into_iter();
        if let Some(path) = conflict_paths.next() {
            entries.push(ImportReportEntry {
                path,
                category: "CONFLICT",
                detail: format!(
                    "{} {} previous={} new={} ours={}{}",
                    conflict.identity,
                    subject_name(&conflict.subject),
                    report_value(&conflict.previous),
                    report_value(&conflict.new),
                    report_value(&conflict.ours),
                    {
                        let others = conflict_paths.map(|path| path.display().to_string()).collect::<Vec<_>>();
                        if others.is_empty() {
                            String::new()
                        } else {
                            format!("; files={}", quoted(&others.join(", ")))
                        }
                    }
                ),
            });
        }
    }
    for removal in outcome.declined_removals {
        tables.entry(removal.table.clone()).or_default().declined_removals += 1;
        let reason = match removal.reason {
            RemovalDeclineReason::Annotated => "annotated".to_string(),
            RemovalDeclineReason::EditedByUs => "edited by us".to_string(),
            RemovalDeclineReason::ReferencedBy(identity) => format!("referenced by {identity}"),
            RemovalDeclineReason::OwnerKept(identity) => format!("owner kept {identity}"),
        };
        entries.push(ImportReportEntry {
            path: paths.path(&removal.table, removal.ours_id)?,
            category: "DECLINED REMOVAL",
            detail: format!(
                "{}: {}{}",
                removal.identity,
                reason,
                if removal.position_renumbered { "; position was renumbered" } else { "" }
            ),
        });
    }
    let mut correction_statement = rebuilt.prepare("SELECT id, kind, name, qualifier, field FROM corrections")?;
    let correction_ids = correction_statement
        .query_map([], |row| {
            Ok((
                CorrectionKey { kind: row.get(1)?, name: row.get(2)?, qualifier: row.get(3)?, field: row.get(4)? },
                row.get::<_, i64>(0)?,
            ))
        })?
        .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
    let mut retained_index = IdentityIndex::from_db(rebuilt)?;
    for (key, id) in &correction_ids {
        if outcome.kept_correction_notes.contains(&retained_index.identity("corrections", *id)?) {
            entries.push(ImportReportEntry {
                path: paths.path("corrections", *id)?,
                category: "KEPT NOTE",
                detail: format!("{} {} [{}].{}", key.kind, quoted(&key.name), key.qualifier, key.field),
            });
        }
    }
    for event in events {
        let (key, previous, new, ours) = match event {
            CorrectionEvent::ResolvedNote { key, previous, new } => (key, previous, new, None),
            CorrectionEvent::Conflict { key, previous, new, ours } => (key, previous, new, Some(ours)),
        };
        let id = correction_ids.get(&key).with_context(|| format!("correction has no retained note: {key:?}"))?;
        let counts = tables.entry("corrections".into()).or_default();
        let (category, suffix) = if let Some(ours) = ours {
            conflict_count += 1;
            counts.conflicts += 1;
            ("CONFLICT", format!(" ours={}", report_value(&ours)))
        } else {
            counts.resolved_notes += 1;
            ("RESOLVED NOTE", String::new())
        };
        entries.push(ImportReportEntry {
            path: paths.path("corrections", *id)?,
            category,
            detail: format!(
                "{} {} [{}].{} previous={} new={}{}",
                key.kind,
                quoted(&key.name),
                key.qualifier,
                key.field,
                report_value(&previous),
                report_value(&new),
                suffix
            ),
        });
    }
    let mut incoming_index = IdentityIndex::from_db(incoming)?;
    let mut incoming_ids = BTreeMap::new();
    for table in
        outcome.absent_in_ours.iter().map(|change| change.table.clone()).collect::<std::collections::BTreeSet<_>>()
    {
        if !crate::database_tables::table_columns(incoming, &table)?.iter().any(|column| column == "id") {
            continue;
        }
        let mut statement = incoming.prepare(&format!("SELECT id FROM \"{table}\""))?;
        for id in statement.query_map([], |row| row.get::<_, i64>(0))? {
            let id = id?;
            incoming_ids.insert(incoming_index.identity(&table, id)?, (table.clone(), id));
        }
    }
    for absent in outcome.absent_in_ours {
        let (table, id) = incoming_ids.get(&absent.identity).context("absent change has no incoming identity")?;
        entries.push(ImportReportEntry {
            path: incoming_paths.path(table, *id)?,
            category: "CHANGED UPSTREAM, ABSENT IN OURS",
            detail: format!(
                "{} {} previous={} new={}",
                absent.identity,
                subject_name(&absent.subject),
                quoted(&absent.previous),
                quoted(&absent.new)
            ),
        });
    }
    let mut packs =
        rebuilt.prepare("SELECT id, name FROM adventure_packs WHERE [update] IS NULL AND released IS NULL")?;
    let packs = packs
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
    for (table, id) in outcome.inserted {
        if table == "adventure_packs" {
            if let Some(name) = packs.get(&id) {
                entries.push(ImportReportEntry {
                    path: paths.path(&table, id)?,
                    category: "NEEDS RESEARCH",
                    detail: name.clone(),
                });
            }
        }
    }
    for (table, id) in outcome.deleted {
        entries.push(ImportReportEntry {
            path: old_paths.path(&table, id)?,
            category: "REMOVED",
            detail: format!("{table}/{id}"),
        });
    }
    let icon_counts = tables.entry("icons".into()).or_default();
    icon_counts.added = icons.added;
    icon_counts.changed = icons.replaced;
    icon_counts.removed = icons.removed;
    icon_counts.conflicts = icons.conflicts.len();
    icon_counts.declined_removals = icons.declined.len();
    conflict_count += icons.conflicts.len();
    for (category, keys) in [("ICON CONFLICT", icons.conflicts), ("ICON DECLINED", icons.declined)] {
        for ((kind, name), reason) in keys {
            entries.push(ImportReportEntry {
                path: icons
                    .paths
                    .get(&(kind.clone(), name.clone()))
                    .cloned()
                    .unwrap_or_else(|| PathBuf::from("icons").join(kind).join(name)),
                category,
                detail: reason.into(),
            });
        }
    }
    entries.sort_by(|left, right| {
        (&left.category, &left.path, &left.detail).cmp(&(&right.category, &right.path, &right.detail))
    });
    Ok(DdobuilderImportReport {
        previous_sha: revisions.0,
        new_sha: revisions.1,
        tables,
        entries,
        files: DataFileChanges::default(),
        conflict_count,
    })
}

impl Display for DdobuilderImportReport {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(output, "import-ddobuilder {} → {}", self.previous_sha, self.new_sha)?;
        writeln!(output, "table | added | removed | changed | conflicts | declined removals | resolved notes")?;
        for (table, counts) in &self.tables {
            writeln!(
                output,
                "{table} | {} | {} | {} | {} | {} | {}",
                counts.added,
                counts.removed,
                counts.changed,
                counts.conflicts,
                counts.declined_removals,
                counts.resolved_notes
            )?;
        }
        for entry in &self.entries {
            writeln!(output, "{}: {} {}", entry.path.display(), entry.category, entry.detail)?;
        }
        writeln!(output, "files: {} written, {} removed", self.files.written, self.files.removed)?;
        writeln!(output, "{} conflicts", self.conflict_count())
    }
}
