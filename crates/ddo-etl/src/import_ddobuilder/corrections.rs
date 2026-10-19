use crate::build::{build_database, StaleCorrection, StaleCorrectionCause};
use crate::corrections::{Correction, CorrectionValue, Corrections};
use crate::wiki::WikiOverrides;
use anyhow::{ensure, Result};
use ddo_model::DatasetVersion;
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::path::Path;

pub(super) struct HisDatabase {
    pub(super) db: Connection,
    pub(super) first_stale: Vec<StaleCorrection>,
    pub(super) skipped_wiki_facts: Vec<crate::build::SkippedWikiFact>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct CorrectionKey {
    pub(super) kind: String,
    pub(super) name: String,
    pub(super) qualifier: String,
    pub(super) field: String,
}

impl CorrectionKey {
    fn from_correction(correction: &Correction) -> Self {
        Self {
            kind: correction.kind.as_str().into(),
            name: correction.name.clone(),
            qualifier: correction.qualifier(),
            field: correction.field.clone(),
        }
    }

    fn matches(&self, stale: &StaleCorrection) -> bool {
        self.kind == stale.kind
            && self.name == stale.name
            && self.qualifier == stale.qualifier
            && self.field == stale.field
    }
}

#[derive(Debug)]
pub(super) enum CorrectionEvent {
    ResolvedCorrection { key: CorrectionKey, previous: String, new: String },
    Conflict { key: CorrectionKey, previous: String, new: String, ours: String },
}

pub(super) fn build_database_matching_his_values(
    source: &Path,
    wiki: &WikiOverrides,
    corrections: &Corrections,
    version: &DatasetVersion,
) -> Result<HisDatabase> {
    build_matching_values(source, wiki, corrections, version, false)
}

pub(super) fn build_previous_matching_his_values(
    source: &Path,
    wiki: &WikiOverrides,
    corrections: &Corrections,
    version: &DatasetVersion,
) -> Result<HisDatabase> {
    build_matching_values(source, wiki, corrections, version, true)
}

fn build_matching_values(
    source: &Path,
    wiki: &WikiOverrides,
    corrections: &Corrections,
    version: &DatasetVersion,
    is_previous: bool,
) -> Result<HisDatabase> {
    let builder = if is_previous { crate::build::build_previous_database } else { build_database };
    let mut db = Connection::open_in_memory()?;
    let first_report = builder(source, wiki, corrections, &mut db, version)?;
    let first_stale = first_report.stale_corrections;
    let mut skipped_wiki_facts = first_report.skipped_wiki_facts;
    let mut rebased = corrections.clone();
    let mut changed = false;
    for correction in &mut rebased.entries {
        let key = CorrectionKey::from_correction(correction);
        let Some(stale) = first_stale.iter().find(|stale| key.matches(stale)) else { continue };
        if let StaleCorrectionCause::ValueChanged { maetrim_value, .. } = &stale.cause {
            ensure!(!maetrim_value.contains(" / "), "{}: his rows disagree: {maetrim_value}", correction.label());
            if *maetrim_value != correction.to.to_json() {
                correction.from = CorrectionValue::from_json(maetrim_value)?;
                changed = true;
            }
        }
    }
    if changed {
        db = Connection::open_in_memory()?;
        let report = builder(source, wiki, &rebased, &mut db, version)?;
        skipped_wiki_facts.extend(report.skipped_wiki_facts);
        for stale in report.stale_corrections {
            let resolved = rebased.entries.iter().any(|correction| {
                CorrectionKey::from_correction(correction).matches(&stale)
                    && match &stale.cause {
                        StaleCorrectionCause::ValueChanged { maetrim_value, .. } => {
                            *maetrim_value == correction.to.to_json()
                        }
                        StaleCorrectionCause::RenameDoneUpstream { .. } => true,
                    }
            });
            ensure!(resolved, "correction remains stale after rebasing: {stale:?}");
        }
    }
    skipped_wiki_facts.sort();
    skipped_wiki_facts.dedup();
    Ok(HisDatabase { db, first_stale, skipped_wiki_facts })
}

fn upstream_value(correction: &Correction, stale: &[StaleCorrection]) -> String {
    let key = CorrectionKey::from_correction(correction);
    match stale.iter().find(|stale| key.matches(stale)).map(|stale| &stale.cause) {
        Some(StaleCorrectionCause::ValueChanged { maetrim_value, .. }) => maetrim_value.clone(),
        Some(StaleCorrectionCause::RenameDoneUpstream { .. }) => "renamed upstream".into(),
        None => correction.from.to_json(),
    }
}

pub(super) fn correction_events(
    previous_stale: &[StaleCorrection],
    new_stale: &[StaleCorrection],
    corrections: &Corrections,
    ours: &Connection,
) -> Result<Vec<CorrectionEvent>> {
    let mut statement = ours.prepare("SELECT kind, name, qualifier, field, to_value FROM corrections")?;
    let values = statement
        .query_map([], |row| {
            Ok((
                CorrectionKey { kind: row.get(0)?, name: row.get(1)?, qualifier: row.get(2)?, field: row.get(3)? },
                row.get::<_, String>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
    let mut events = Vec::new();
    for correction in &corrections.entries {
        let key = CorrectionKey::from_correction(correction);
        let previous = upstream_value(correction, previous_stale);
        let new = upstream_value(correction, new_stale);
        if previous == new {
            continue;
        }
        if new == correction.to.to_json() || new == "renamed upstream" {
            events.push(CorrectionEvent::ResolvedCorrection { key, previous, new });
        } else {
            let ours = values.get(&key).cloned().unwrap_or_else(|| correction.to.to_json());
            events.push(CorrectionEvent::Conflict { key, previous, new, ours });
        }
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn corrections_with_disagreeing_rows_stop_the_import() {
        let scratch = tempfile::tempdir().unwrap();
        let original = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataFiles");
        let source = scratch.path().join("source");
        for entry in walkdir::WalkDir::new(&original) {
            let entry = entry.unwrap();
            if !entry.file_type().is_file() {
                continue;
            }
            let target = source.join(entry.path().strip_prefix(&original).unwrap());
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::copy(entry.path(), target).unwrap();
        }
        let augment_path = source.join("Augments/DinosaurBone.Augments.xml");
        let contents = std::fs::read_to_string(&augment_path).unwrap();
        let start = contents.find("<Augment>").unwrap();
        let end = start + contents[start..].find("</Augment>").unwrap() + "</Augment>".len();
        let duplicate = contents[start..end].replace("<Description>", "<Description>Different source prose: ");
        std::fs::write(&augment_path, contents.replace("</Augments>", &format!("{duplicate}</Augments>"))).unwrap();
        let wiki = WikiOverrides::default();
        let corrections = Corrections {
            entries: vec![crate::corrections::Correction {
                kind: ddo_model::enums::CorrectionKind::Augment,
                name: "Voidscale".into(),
                field: "description".into(),
                from: crate::corrections::CorrectionValue::Text("Previous text".into()),
                to: crate::corrections::CorrectionValue::Text("Reviewed text".into()),
                family: None,
                selection: None,
                stance: None,
                dc: None,
                effect: None,
                stat: None,
                bonus_type: None,
                bonus_value: None,
                equipped_count: None,
                reason: "Test disagreement between two XML augment records.".into(),
                source: "https://ddowiki.com/page/Augment:Voidscale".to_string().into(),
                read: "2026-10-17".into(),
                file_name: "attack.toml".into(),
            }],
        };
        let error = build_database_matching_his_values(
            &source,
            &wiki,
            &corrections,
            &ddo_model::DatasetVersion { upstream_sha: "fixture".into(), built_at: "2026-10-17T00:00:00Z".into() },
        )
        .err()
        .expect("variant descriptions must disagree");
        assert!(format!("{error:#}").contains("his rows disagree"), "{error:#}");
    }
}
