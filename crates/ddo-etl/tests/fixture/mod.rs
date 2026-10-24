#![allow(dead_code)]

use ddo_etl::corrections::{Correction, CorrectionValue, Corrections};
use ddo_model::enums::CorrectionKind;
use std::ops::Add;

pub(crate) fn groupings() -> &'static ddo_etl::groupings::Groupings {
    static GROUPINGS: std::sync::OnceLock<ddo_etl::groupings::Groupings> = std::sync::OnceLock::new();
    GROUPINGS.get_or_init(|| ddo_etl::groupings::Groupings::from_database(&database(|_| {})).unwrap())
}

pub(crate) fn grouping_cases() -> &'static ddo_etl::groupings::Groupings {
    static GROUPINGS: std::sync::OnceLock<ddo_etl::groupings::Groupings> = std::sync::OnceLock::new();
    GROUPINGS.get_or_init(|| {
        let db = grouping_case_database(|_| {});
        ddo_etl::groupings::Groupings::from_database(&db).unwrap()
    })
}

pub(crate) fn grouping_case_database(edit: impl Fn(&std::path::Path)) -> rusqlite::Connection {
    database(|root| {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/effect-groups/data");
        for entry in
            walkdir::WalkDir::new(&source).into_iter().map(Result::unwrap).filter(|entry| entry.file_type().is_file())
        {
            let destination = root.join(entry.path().strip_prefix(&source).unwrap());
            std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
            std::fs::copy(entry.path(), destination).unwrap();
        }
        edit(root);
    })
}

pub(crate) fn xml_source() -> tempfile::TempDir {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataFiles");
    let scratch = tempfile::tempdir().unwrap();
    for entry in
        walkdir::WalkDir::new(&source).into_iter().map(Result::unwrap).filter(|entry| entry.file_type().is_file())
    {
        let destination = scratch.path().join(entry.path().strip_prefix(&source).unwrap());
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::copy(entry.path(), destination).unwrap();
    }
    scratch
}

pub(crate) fn write_scion_feat(source: &std::path::Path) {
    let path = source.join("Feats.xml");
    let feats = std::fs::read_to_string(&path).unwrap();
    let scion = include_str!("../fixtures/effect-groups/ScionOfTheAstralPlane.Feat.xml");
    std::fs::write(path, feats.replace("</Feats>", &format!("{scion}</Feats>"))).unwrap();
    std::fs::create_dir_all(source.join("FeatImages")).unwrap();
    std::fs::write(
        source.join("FeatImages/ScionOfTheAstralPlane.png"),
        include_bytes!("../fixtures/effect-groups/icons/ScionOfTheAstralPlane.png"),
    )
    .unwrap();
}

pub(crate) fn write_plague_wolf_feat(source: &std::path::Path) {
    let path = source.join("Feats.xml");
    let original = std::fs::read_to_string(&path).unwrap();
    let feat = include_str!("../fixtures/effect-groups/PlagueWolf.Feat.xml");
    std::fs::write(path, original.replace("</Feats>", &format!("{feat}</Feats>"))).unwrap();
    std::fs::create_dir_all(source.join("FeatImages")).unwrap();
    std::fs::write(
        source.join("FeatImages/WildShapePlagueWolf.png"),
        include_bytes!("../fixtures/effect-groups/icons/WildShapePlagueWolf.png"),
    )
    .unwrap();
}

pub(crate) fn plague_wolf_corrections() -> CorrectionSet {
    let removal = new_qualified_correction(
        "feat_bonus",
        "Wild Shape: Plague Wolf",
        "stat = 'Intelligence'\nbonus_type = 'Penalty'",
        "remove",
        "-2",
        "\"null\"",
    );
    let value = new_qualified_correction(
        "feat_bonus",
        "Wild Shape: Plague Wolf",
        "stat = 'Charisma'\nbonus_type = 'Penalty'",
        "value",
        "-2",
        "-4",
    );
    removal + &value
}

pub(crate) fn plague_wolf_penalties(db: &rusqlite::Connection) -> Vec<(String, i64, String, bool)> {
    db.prepare("SELECT e.name, l.value, bt.name, e.is_stat FROM feat_effects l JOIN feats f ON f.id=l.feat_id JOIN effects e ON e.id=l.effect_id JOIN bonus_types bt ON bt.id=l.bonus_type_id WHERE f.name='Wild Shape: Plague Wolf' AND bt.name='Penalty' ORDER BY e.name").unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap()
}

pub(crate) fn scion_dodge_split() -> CorrectionSet {
    new_qualified_correction(
        "feat_bonus",
        "Scion of the Astral Plane",
        "stat = 'Dodge and Dodge Cap'\nbonus_type = 'Feat'",
        "lines",
        "\"Dodge and Dodge Cap\"",
        "[\"Dodge\", \"Dodge Cap\"]",
    )
}

pub(crate) fn scion_dodge_lines(db: &rusqlite::Connection) -> Vec<(String, i64)> {
    db.prepare(
        "SELECT e.name,l.value FROM feat_effects l JOIN effects e ON e.id=l.effect_id
         JOIN feats f ON f.id=l.feat_id WHERE f.name='Scion of the Astral Plane'
         AND e.name LIKE '%Dodge%' ORDER BY e.name",
    )
    .unwrap()
    .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
    .unwrap()
    .collect::<rusqlite::Result<_>>()
    .unwrap()
}

pub(crate) fn write_search_and_spot_item(source: &std::path::Path, amount: i64, include_spot: bool) {
    std::fs::create_dir_all(source.join("Items")).unwrap();
    let mut item = include_str!("../fixtures/effect-groups/SearchAndSpot.item")
        .replace("<Value1>20</Value1>", &format!("<Value1>{amount}</Value1>"));
    if !include_spot {
        let target = item.find("<Item>Spot</Item>").unwrap();
        let start = item[..target].rfind("<Buff>").unwrap();
        let end = target + item[target..].find("</Buff>").unwrap() + "</Buff>".len();
        item.replace_range(start..end, "");
    }
    std::fs::write(source.join("Items/SearchAndSpot.item"), item).unwrap();
    let path = source.join("ItemBuffs.xml");
    if !ddo_etl::xml::item_buffs::parse(&path).unwrap().contains_key("SkillBonus") {
        let buffs = std::fs::read_to_string(&path).unwrap();
        let definition = include_str!("../fixtures/effect-groups/SkillBonus.Buff.xml");
        std::fs::write(path, buffs.replace("</Buffs>", &format!("{definition}</Buffs>"))).unwrap();
    }
}

pub(crate) fn search_and_spot_merge() -> CorrectionSet {
    new_qualified_correction(
        "item_bonus",
        "Legendary Keylock Ring",
        "stat = 'Search and Spot'\nbonus_type = 'Competence'",
        "lines",
        "[\"Search\", \"Spot\"]",
        "\"Search and Spot\"",
    )
}

pub(crate) fn search_and_spot_lines(db: &rusqlite::Connection) -> Vec<(String, Option<i64>)> {
    db.prepare(
        "SELECT e.name,l.value FROM item_effects l JOIN effects e ON e.id=l.effect_id
         JOIN items i ON i.id=l.item_id WHERE i.name='Legendary Keylock Ring' ORDER BY l.sort_order",
    )
    .unwrap()
    .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
    .unwrap()
    .collect::<rusqlite::Result<_>>()
    .unwrap()
}

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

pub(crate) fn verification_note_database(edit: impl FnOnce(&std::path::Path)) -> rusqlite::Connection {
    database(|root| {
        let captured = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/verification-notes/data");
        std::fs::remove_file(root.join("effects/sheltering.toml")).unwrap();
        for entry in
            walkdir::WalkDir::new(&captured).into_iter().map(Result::unwrap).filter(|entry| entry.file_type().is_file())
        {
            let destination = root.join(entry.path().strip_prefix(&captured).unwrap());
            std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
            std::fs::copy(entry.path(), destination).unwrap();
        }
        edit(root);
    })
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
