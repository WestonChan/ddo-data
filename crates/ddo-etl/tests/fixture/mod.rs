#![allow(dead_code)]

use ddo_etl::corrections::{Correction, CorrectionValue, Corrections};
use ddo_model::enums::CorrectionKind;
use std::ops::Add;

pub(crate) fn database(edit: impl FnOnce(&std::path::Path)) -> rusqlite::Connection {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data");
    let scratch = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(&source).into_iter().map(Result::unwrap) {
        if entry.file_type().is_file() {
            let destination = scratch.path().join(entry.path().strip_prefix(&source).unwrap());
            std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
            std::fs::copy(entry.path(), destination).unwrap();
        }
    }
    edit(scratch.path());
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(scratch.path(), &mut db).unwrap();
    db
}

pub(crate) fn fixture_icon_corrections() -> Corrections {
    let mut corrections = Corrections::from_database(&database(|_| {})).unwrap();
    corrections.entries.retain(|entry| entry.field == "icon");
    corrections
}

#[derive(Clone, Debug, Default)]
pub(crate) struct CorrectionSet(pub(crate) Vec<Correction>);

impl CorrectionSet {
    pub(crate) fn sourced(mut self, source: &str) -> Self {
        for correction in &mut self.0 {
            correction.source = source.to_string().into();
        }
        self
    }

    pub(crate) fn cited(mut self, reason: &str, source: &str, read: &str) -> Self {
        for correction in &mut self.0 {
            correction.reason = reason.into();
            correction.source = source.to_string().into();
            correction.read = read.into();
        }
        self
    }
}

impl FromIterator<CorrectionSet> for CorrectionSet {
    fn from_iter<Sets: IntoIterator<Item = CorrectionSet>>(sets: Sets) -> Self {
        CorrectionSet(sets.into_iter().flat_map(|set| set.0).collect())
    }
}

impl Add<&CorrectionSet> for CorrectionSet {
    type Output = CorrectionSet;

    fn add(mut self, other: &CorrectionSet) -> CorrectionSet {
        self.0.extend(other.0.iter().cloned());
        self
    }
}

impl Add<&str> for CorrectionSet {
    type Output = CorrectionSet;

    fn add(mut self, qualifier_keys: &str) -> CorrectionSet {
        let correction = self.0.last_mut().expect("qualifiers belong to a correction");
        set_qualifiers(correction, qualifier_keys);
        self
    }
}

pub(crate) fn correction_value(literal: &str) -> CorrectionValue {
    let table: toml::Table = toml::from_str(&format!("value = {literal}")).unwrap();
    let mut json = serde_json::to_value(&table["value"]).unwrap();
    if json == serde_json::json!("null") {
        json = serde_json::Value::Null;
    }
    CorrectionValue::from_json(&json.to_string()).unwrap()
}

pub(crate) fn new_correction(kind: &str, name: &str, field: &str, from: &str, to: &str) -> CorrectionSet {
    correction_between(kind, name, field, correction_value(from), correction_value(to))
}

pub(crate) fn correction_between(
    kind: &str,
    name: &str,
    field: &str,
    from: CorrectionValue,
    to: CorrectionValue,
) -> CorrectionSet {
    CorrectionSet(vec![Correction {
        kind: CorrectionKind::parse(kind).unwrap_or_else(|| panic!("unknown correction kind {kind:?}")),
        name: name.into(),
        field: field.into(),
        from,
        to,
        family: None,
        selection: None,
        stance: None,
        dc: None,
        effect: None,
        stat: None,
        bonus_type: None,
        bonus_value: None,
        equipped_count: None,
        reason: "Test reason.".into(),
        source: "https://ddowiki.com/page/Test".to_string().into(),
        read: "2026-09-29".into(),
        file_name: "corrections.toml".into(),
    }])
}

pub(crate) fn new_qualified_correction(
    kind: &str,
    name: &str,
    qualifier_keys: &str,
    field: &str,
    from: &str,
    to: &str,
) -> CorrectionSet {
    new_correction(kind, name, field, from, to) + qualifier_keys
}

fn set_qualifiers(correction: &mut Correction, qualifier_keys: &str) {
    let keys: toml::Table = toml::from_str(qualifier_keys).unwrap();
    for (key, value) in keys {
        let text = || value.as_str().unwrap().to_string();
        match key.as_str() {
            "family" => correction.family = Some(text()),
            "selection" => correction.selection = Some(text()),
            "stance" => correction.stance = Some(text()),
            "dc" => correction.dc = Some(text()),
            "effect" => correction.effect = Some(text()),
            "stat" => correction.stat = Some(text()),
            "bonus_type" => correction.bonus_type = Some(text()),
            "bonus_value" => correction.bonus_value = value.as_integer(),
            "equipped_count" => correction.equipped_count = value.as_integer(),
            unknown => panic!("unknown correction qualifier {unknown:?}"),
        }
    }
}

pub(crate) fn corrections_from(files: &[(&str, &CorrectionSet)]) -> Result<Corrections, String> {
    let mut entries = Vec::new();
    for (file_name, set) in files {
        for correction in &set.0 {
            let mut correction = correction.clone();
            correction.file_name = (*file_name).into();
            correction
                .validate()
                .map_err(|error| format!("correction file {file_name}: {}: {error:#}", correction.label()))?;
            entries.push(correction);
        }
    }
    Ok(Corrections { entries })
}

pub(crate) fn data_build_error(edit: impl FnOnce(&std::path::Path)) -> String {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data");
    let scratch = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(&source).into_iter().map(Result::unwrap) {
        if entry.file_type().is_file() {
            let destination = scratch.path().join(entry.path().strip_prefix(&source).unwrap());
            std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
            std::fs::copy(entry.path(), destination).unwrap();
        }
    }
    edit(scratch.path());
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    match ddo_etl::tree::build_from_data(scratch.path(), &mut db) {
        Ok(_) => String::new(),
        Err(error) => format!("{error:#}").replace(&scratch.path().display().to_string(), "<data>"),
    }
}
