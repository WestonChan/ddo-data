mod corrections;
mod icons;
mod report;

pub use report::{DdobuilderImportReport, ImportReportEntry, ImportTableCounts};

use crate::corrections::Corrections;
use crate::delta::{database_delta, merge_delta_into};
use crate::tree::{self, DataFilePaths};
use crate::wiki::WikiOverrides;
use anyhow::{ensure, Context, Result};
use rusqlite::Connection;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub struct DdobuilderImportOptions {
    pub source_dir: PathBuf,
    pub previous_dir: PathBuf,
    pub data_dir: PathBuf,
    pub sha: Option<String>,
    pub previous_sha: Option<String>,
    pub wiki: WikiOverrides,
    pub corrections: Corrections,
}

pub fn validate_previous_commit(
    data_dir: &Path,
    previous_dir: &Path,
    previous_sha: Option<String>,
) -> Result<ddo_model::DatasetVersion> {
    ensure!(data_dir.is_dir(), "data directory {} does not exist", data_dir.display());
    let path = data_dir.join("upstream/commit.toml");
    let contents = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let commit: toml::Value = toml::from_str(&contents)?;
    let recorded = commit.get("sha").and_then(toml::Value::as_str).context("upstream/commit.toml has no sha")?;
    let previous = crate::upstream::dataset_version(previous_dir, previous_sha);
    ensure!(
        previous.upstream_sha == recorded,
        "previous commit {} does not match recorded commit {recorded}",
        previous.upstream_sha
    );
    Ok(previous)
}

fn build_normalized_database(db: &Connection, source: &Path, destination: &Path) -> Result<Connection> {
    tree::export_ddobuilder_tree(db, source, destination)?;
    let mut normalized = Connection::open_in_memory()?;
    tree::build_from_data(destination, &mut normalized)?;
    Ok(normalized)
}

fn verification_differences(delta: &crate::delta::DatabaseDelta) -> Vec<String> {
    let mut differences = Vec::new();
    for (table, changes) in
        delta.entities.iter().chain(std::iter::once((&"corrections".to_string(), &delta.correction_notes)))
    {
        differences.extend(changes.added.iter().map(|identity| format!("{table}: added {identity}")));
        differences.extend(changes.removed.iter().map(|identity| format!("{table}: removed {identity}")));
        differences.extend(changes.changed.iter().map(|change| {
            format!("{}.{}: merged={:?} rebuilt={:?}", change.identity, change.column, change.previous, change.new)
        }));
    }
    for (key, change) in &delta.arrays {
        differences
            .push(format!("{} [{}]: merged={:?} rebuilt={:?}", key.owner, key.array, change.previous, change.new));
    }
    differences.truncate(50);
    differences
}

fn verify_staging(merged: &Connection, staging: &Path, check: &Path) -> Result<Connection> {
    let mut rebuilt = Connection::open_in_memory()?;
    tree::build_from_data(staging, &mut rebuilt)?;
    tree::export_database(&rebuilt, check)?;
    let staged = tree::data_file_bytes(staging)?;
    let exported = tree::data_file_bytes(check)?;
    let differences: Vec<_> = staged
        .keys()
        .chain(exported.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|path| staged.get(*path) != exported.get(*path))
        .take(50)
        .map(|path| path.display().to_string())
        .collect();
    ensure!(differences.is_empty(), "V1 export is not byte-stable:\n{}", differences.join("\n"));
    let delta = database_delta(merged, &rebuilt)?;
    ensure!(
        delta.is_empty(),
        "V2 export does not preserve merged authored rows:\n{}",
        verification_differences(&delta).join("\n")
    );
    Ok(rebuilt)
}

pub fn import_ddobuilder(options: &DdobuilderImportOptions) -> Result<DdobuilderImportReport> {
    let previous_version =
        validate_previous_commit(&options.data_dir, &options.previous_dir, options.previous_sha.clone())?;
    let new_version = crate::upstream::dataset_version(&options.source_dir, options.sha.clone());
    let scratch = tempfile::tempdir()?;
    let mut ours = Connection::open_in_memory()?;
    tree::build_from_data(&options.data_dir, &mut ours)?;
    let old_paths = DataFilePaths::from_database(&ours)?;
    let annotated = tree::annotated_entities(&ours)?;
    let previous = corrections::build_database_matching_his_values(
        &options.previous_dir,
        &options.wiki,
        &options.corrections,
        &previous_version,
    )
    .context("building the previous DataFiles")?;
    let new = corrections::build_database_matching_his_values(
        &options.source_dir,
        &options.wiki,
        &options.corrections,
        &new_version,
    )
    .context("building the new DataFiles")?;
    let previous_tree = scratch.path().join("previous");
    let new_tree = scratch.path().join("new");
    let previous_normalized = build_normalized_database(&previous.db, &options.previous_dir, &previous_tree)
        .context("exporting the previous DataFiles")?;
    let new_normalized =
        build_normalized_database(&new.db, &options.source_dir, &new_tree).context("exporting the new DataFiles")?;
    let events = corrections::correction_events(&previous.first_stale, &new.first_stale, &options.corrections, &ours)?;
    let outcome = merge_delta_into(&mut ours, &previous_normalized, &new_normalized, &annotated)?;
    ours.execute("UPDATE dataset_version SET upstream_sha = ?1", [&new_version.upstream_sha])?;
    tree::rederive_data_build_tables(&ours)?;
    let staging = scratch.path().join("staging");
    tree::export_database(&ours, &staging)?;
    let referenced = tree::referenced_data_icons(&staging)?
        .into_iter()
        .map(|reference| {
            let (kind, _, name) = crate::icons::image_reference_parts(&reference.kind, &reference.icon)?;
            Ok((kind.to_string(), format!("{name}.png")))
        })
        .collect::<Result<BTreeSet<_>>>()?;
    let mut icons = icons::merge_icon_files(&previous_tree, &new_tree, &options.data_dir, &staging, &referenced)?;
    crate::icons::place_enhancement_icons_beside_trees(&staging)?;
    ensure!(crate::icons::enhancement_icon_actions(&staging)?.is_empty(), "staged enhancement icon layout is invalid");
    icons.resolve_paths(&staging)?;
    let rebuilt = verify_staging(&ours, &staging, &scratch.path().join("check"))?;
    let mut report = report::report(
        (previous_version.upstream_sha, new_version.upstream_sha),
        outcome,
        events,
        icons,
        &rebuilt,
        &new_normalized,
        &old_paths,
    )?;
    report.files = tree::replace_data_files(&staging, &options.data_dir)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::build_database;

    fn fixture_database(scratch: &Path) -> Connection {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import_ddobuilder/ba7e18e/DataFiles");
        let mut original = Connection::open_in_memory().unwrap();
        build_database(
            &source,
            &WikiOverrides::default(),
            &Corrections::default(),
            &mut original,
            &ddo_model::DatasetVersion { upstream_sha: "fixture".into(), built_at: "2026-10-17T00:00:00Z".into() },
        )
        .unwrap();
        build_normalized_database(&original, &source, scratch).unwrap()
    }

    #[test]
    fn verification_fails_when_the_rebuild_differs() {
        let scratch = tempfile::tempdir().unwrap();
        let staging = scratch.path().join("staging");
        let merged = fixture_database(&staging);
        let changed = fixture_database(&scratch.path().join("changed"));
        changed.execute("UPDATE enhancements SET description = 'An authored value lost by the export' WHERE internal_name = 'SDDefenseBoost'", []).unwrap();
        tree::export_database(&changed, &staging).unwrap();
        let error = verify_staging(&merged, &staging, &scratch.path().join("check")).unwrap_err();
        assert!(error.to_string().contains("V2 export does not preserve merged authored rows"), "{error:#}");
    }

    #[test]
    fn verification_rejects_noncanonical_staging_before_writing() {
        let scratch = tempfile::tempdir().unwrap();
        let staging = scratch.path().join("staging");
        let merged = fixture_database(&staging);
        let path = staging.join("upstream/commit.toml");
        let original = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, format!("\n{original}")).unwrap();
        let error = verify_staging(&merged, &staging, &scratch.path().join("check")).unwrap_err();
        assert!(error.to_string().contains("V1 export is not byte-stable"), "{error:#}");
    }

    #[test]
    fn report_paths_cover_exported_entities_without_per_row_queries() {
        use crate::delta::{TableMerge, TABLE_MERGES};
        use rusqlite::trace::{TraceEvent, TraceEventCodes};
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SELECTS: AtomicUsize = AtomicUsize::new(0);
        fn trace(event: TraceEvent<'_>) {
            if let TraceEvent::Stmt(statement, _) = event {
                if statement.sql().trim_start().to_uppercase().starts_with("SELECT") {
                    SELECTS.fetch_add(1, Ordering::SeqCst);
                }
            }
        }
        let scratch = tempfile::tempdir().unwrap();
        let mut counts = Vec::new();
        for full in [false, true] {
            let exported = scratch.path().join(if full { "full" } else { "fixture" });
            let db = if full {
                let mut db = Connection::open_in_memory().unwrap();
                tree::build_from_data(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"), &mut db).unwrap();
                tree::export_database(&db, &exported).unwrap();
                db
            } else {
                fixture_database(&exported)
            };
            db.trace_v2(TraceEventCodes::SQLITE_TRACE_STMT, Some(trace));
            SELECTS.store(0, Ordering::SeqCst);
            let paths = DataFilePaths::from_database(&db).unwrap();
            counts.push(SELECTS.load(Ordering::SeqCst));
            db.trace_v2(TraceEventCodes::empty(), None);
            for (table, rule) in TABLE_MERGES {
                if !matches!(rule, TableMerge::Entity { .. } | TableMerge::CorrectionNotes) {
                    continue;
                }
                let mut statement = db.prepare(&format!("SELECT id FROM \"{table}\"")).unwrap();
                for id in statement.query_map([], |row| row.get::<_, i64>(0)).unwrap() {
                    let id = id.unwrap();
                    let path = paths.path(table, id).unwrap();
                    assert!(exported.join(&path).is_file(), "{table}/{id}: {}", path.display());
                }
            }
        }
        assert_eq!(counts[0], counts[1]);
    }
    #[test]
    fn dataset_array_conflicts_name_their_data_file() {
        let scratch = tempfile::tempdir().unwrap();
        let previous = fixture_database(&scratch.path().join("previous"));
        let edited = |folder: &str, reason: &str| {
            let root = scratch.path().join(folder);
            let db = fixture_database(&root);
            db.execute(
                "UPDATE source_aliases SET reason = ?1 WHERE rowid = (SELECT MIN(rowid) FROM source_aliases)",
                [reason],
            )
            .unwrap();
            tree::export_database(&db, &root).unwrap();
            let mut rebuilt = Connection::open_in_memory().unwrap();
            tree::build_from_data(&root, &mut rebuilt).unwrap();
            rebuilt
        };
        let new = edited("new", "His revised alias research");
        let mut ours = edited("ours", "Our revised alias research");
        let paths = DataFilePaths::from_database(&ours).unwrap();
        assert_eq!(paths.path("dataset_version", 0).unwrap(), PathBuf::from("upstream/commit.toml"));
        assert_eq!(paths.path("excluded_items", 0).unwrap(), PathBuf::from("upstream/excluded-items.toml"));
        let outcome = merge_delta_into(&mut ours, &previous, &new, &BTreeSet::new()).unwrap();
        tree::rederive_data_build_tables(&ours).unwrap();
        let report = report::report(
            ("previous".into(), "new".into()),
            outcome,
            Vec::new(),
            icons::IconMerge::default(),
            &ours,
            &new,
            &paths,
        )
        .unwrap();
        assert_eq!(report.conflict_count(), 1);
        assert_eq!(report.entries.len(), 1);
        assert_eq!(report.entries[0].path, PathBuf::from("upstream/source-aliases.toml"));
        assert!(report.entries[0].detail.contains("His revised alias research"));
        assert!(report.entries[0].detail.contains("Our revised alias research"));
        assert!(!report.entries[0].detail.contains("integer:"));
        assert!(!report.entries[0].detail.contains("text:"));
        let values = report.entries[0].detail.split("previous=").nth(1).unwrap().split(" new=").next().unwrap();
        let rows: serde_json::Value = serde_json::from_str(values).unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 1);
    }

    #[test]
    fn report_lists_kept_correction_notes() {
        let scratch = tempfile::tempdir().unwrap();
        let with_note = |folder: &str| {
            let root = scratch.path().join(folder);
            let original = fixture_database(&root);
            original.execute("INSERT INTO corrections (kind, name, field, from_value, to_value, reason, source, read) SELECT 'enhancement', name, 'description', '\"Previous prose\"', json_quote(description), 'Retained research', 'https://ddowiki.com/page/Stalwart_Defender_enhancements', '2026-10-17' FROM enhancements WHERE internal_name = 'SDDefenseBoost'", []).unwrap();
            tree::export_database(&original, &root).unwrap();
            let mut rebuilt = Connection::open_in_memory().unwrap();
            tree::build_from_data(&root, &mut rebuilt).unwrap();
            rebuilt
        };
        let previous = with_note("previous");
        let mut ours = with_note("ours");
        let new = fixture_database(&scratch.path().join("new"));
        let paths = DataFilePaths::from_database(&ours).unwrap();
        let outcome = merge_delta_into(&mut ours, &previous, &new, &BTreeSet::new()).unwrap();
        tree::rederive_data_build_tables(&ours).unwrap();
        let report = report::report(
            ("previous".into(), "new".into()),
            outcome,
            Vec::new(),
            icons::IconMerge::default(),
            &ours,
            &new,
            &paths,
        )
        .unwrap();
        assert!(report.to_string().contains("characters/enhancements/stalwart-defender/tree.toml: KEPT NOTE enhancement \"Stalwart Defender: Defense Boost\""), "{report}");
    }

    #[test]
    fn pack_exception_conflict_names_owners_once_and_omits_unchanged_rows() {
        let scratch = tempfile::tempdir().unwrap();
        let side = |folder: &str, url: &str| {
            let root = scratch.path().join(folder);
            let original = fixture_database(&root);
            original.execute_batch("INSERT INTO adventure_packs (id, slug, name) VALUES (-1, 'another-pack', 'Another Pack'); INSERT INTO pack_exceptions (pack_name, rule, row_index, entity_kind, entity_name, url) SELECT name, 'patron', 0, 'patron', name, 'https://ddowiki.com/page/Unchanged' FROM adventure_packs").unwrap();
            original.execute("UPDATE pack_exceptions SET url = ?1 WHERE pack_name = 'Another Pack'", [url]).unwrap();
            tree::rederive_data_build_tables(&original).unwrap();
            tree::export_database(&original, &root).unwrap();
            let mut rebuilt = Connection::open_in_memory().unwrap();
            tree::build_from_data(&root, &mut rebuilt).unwrap();
            rebuilt
        };
        let previous = side("previous", "https://ddowiki.com/page/Previous");
        let new = side("new", "https://ddowiki.com/page/New");
        let mut ours = side("ours", "https://ddowiki.com/page/Ours");
        let paths = DataFilePaths::from_database(&ours).unwrap();
        let outcome = merge_delta_into(&mut ours, &previous, &new, &BTreeSet::new()).unwrap();
        tree::rederive_data_build_tables(&ours).unwrap();
        let report = report::report(
            ("previous".into(), "new".into()),
            outcome,
            Vec::new(),
            icons::IconMerge::default(),
            &ours,
            &new,
            &paths,
        )
        .unwrap();
        assert_eq!(report.conflict_count(), 1);
        assert_eq!(report.entries.len(), 1);
        assert_eq!(report.entries[0].path, PathBuf::from("packs/another-pack/pack.toml"));
        assert!(!report.entries[0].detail.contains("Unchanged"));
        assert!(!report.entries[0].detail.contains("Terror of Demogorgon"));
        assert_eq!(report.to_string().lines().filter(|line| line.contains(": CONFLICT")).count(), 1);
    }
}
