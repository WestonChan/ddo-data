use ddo_etl::build::build_database;
use ddo_etl::corrections::Corrections;
use ddo_etl::map::effect_map::EFFECT_MAP;
use ddo_etl::wiki::WikiOverrides;
use ddo_model::stats::STATS;
use ddo_model::DatasetVersion;
use quick_xml::events::Event;
use quick_xml::Reader;
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const STATS_KEPT_WITHOUT_A_SOURCE: &[(&str, &str)] = &[
    ("Pact Dice", "warlock pact damage dice, which no buff or effect type in his files carries"),
    ("Spellsword Dice", "eldritch knight spellsword dice, which no buff or effect type in his files carries"),
    ("Burning Ambition Dice", "the Burning Ambition damage dice, which no buff or effect type in his files carries"),
    ("Temporary Hit Points", "temporary hit points, which no buff or effect type in his files carries"),
    ("Sleep Save", "saves against sleep, which no SaveBonus in his files names"),
    ("Divination Spell Focus", "divination spell DCs, which no SpellDC or SchoolFocusNumber in his files names"),
    ("Force Resistance", "resistance to force damage, which no EnergyResistance in his files names"),
];

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ItemWordOwner {
    Buff,
    Effect,
}

struct OpenTypedElement {
    owner: ItemWordOwner,
    depth: usize,
    kind: Option<String>,
    item_words: Vec<String>,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data")
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = walkdir::WalkDir::new(dir)
        .into_iter()
        .map(|walked| walked.unwrap())
        .filter(|walked| walked.file_type().is_file())
        .map(|walked| walked.into_path())
        .collect();
    files.sort();
    files
}

fn item_words_by_kind_in(path: &Path, item_words_by_kind: &mut BTreeMap<(ItemWordOwner, String), BTreeSet<String>>) {
    let file_text = std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let file_text = file_text.strip_prefix('\u{feff}').unwrap_or(&file_text);
    let mut reader = Reader::from_str(file_text);
    reader.config_mut().trim_text(true);
    let mut depth = 0usize;
    let mut open_element_names: Vec<String> = Vec::new();
    let mut open_typed_elements: Vec<OpenTypedElement> = Vec::new();
    loop {
        match reader.read_event().unwrap_or_else(|error| panic!("{}: {error}", path.display())) {
            Event::Start(start) => {
                depth += 1;
                let element_name = start.name().as_ref().to_string();
                let owner = match element_name.as_str() {
                    "Buff" => Some(ItemWordOwner::Buff),
                    "Effect" => Some(ItemWordOwner::Effect),
                    _ => None,
                };
                if let Some(owner) = owner {
                    open_typed_elements.push(OpenTypedElement { owner, depth, kind: None, item_words: Vec::new() });
                }
                open_element_names.push(element_name);
            }
            Event::Text(text) => {
                let content = text.xml10_content().trim().to_string();
                let Some(innermost) = open_typed_elements.last_mut() else { continue };
                if innermost.depth + 1 != depth || content.is_empty() {
                    continue;
                }
                match open_element_names.last().map(String::as_str) {
                    Some("Type") if innermost.kind.is_none() => innermost.kind = Some(content),
                    Some("Item") => innermost.item_words.push(content),
                    _ => {}
                }
            }
            Event::End(_) => {
                let closed = open_typed_elements.pop_if(|innermost| innermost.depth == depth);
                if let Some(OpenTypedElement { owner, kind: Some(kind), item_words, .. }) = closed {
                    item_words_by_kind.entry((owner, kind)).or_default().extend(item_words);
                }
                open_element_names.pop();
                depth -= 1;
            }
            Event::Eof => break,
            _ => {}
        }
    }
}

fn his_item_words_by_kind() -> BTreeMap<(ItemWordOwner, String), BTreeSet<String>> {
    let mut item_words_by_kind = BTreeMap::new();
    for dir in [fixtures_dir().join("DataFiles"), fixtures_dir().join("seed_stat_sources")] {
        for path in files_under(&dir) {
            if path.extension().is_some_and(|extension| extension == "xml" || extension == "item") {
                item_words_by_kind_in(&path, &mut item_words_by_kind);
            }
        }
    }
    item_words_by_kind
}

fn record_source(sources_by_stat: &mut BTreeMap<String, String>, stat_name: &str, source: String) {
    sources_by_stat.entry(stat_name.to_string()).or_insert(source);
}

fn record_template_targets(
    sources_by_stat: &mut BTreeMap<String, String>,
    map_name: &str,
    templates_by_kind: &BTreeMap<String, String>,
    item_aliases: &BTreeMap<String, String>,
    item_words_by_kind: &BTreeMap<(ItemWordOwner, String), BTreeSet<String>>,
    owner: ItemWordOwner,
) {
    for (kind, template) in templates_by_kind {
        for alias in item_aliases.values() {
            record_source(
                sources_by_stat,
                &template.replace("{item}", alias),
                format!("{map_name} [by_item] {kind} over the [item_aliases] word {alias:?}"),
            );
        }
        let item_words = item_words_by_kind.get(&(owner, kind.clone())).into_iter().flatten();
        for item_word in item_words {
            let canonical_word = item_aliases.get(item_word).unwrap_or(item_word);
            record_source(
                sources_by_stat,
                &template.replace("{item}", canonical_word),
                format!("{map_name} [by_item] {kind} over his item word {item_word:?}"),
            );
        }
    }
}

fn record_map_targets(sources_by_stat: &mut BTreeMap<String, String>) {
    let item_words_by_kind = his_item_words_by_kind();
    let vocabulary = &*EFFECT_MAP;
    for (kind, stat_name) in &vocabulary.family.fixed {
        record_source(sources_by_stat, stat_name, format!("effect_map.toml [family.fixed] {kind}"));
    }
    record_template_targets(
        sources_by_stat,
        "effect_map.toml [family]",
        &vocabulary.family.by_item,
        &vocabulary.item_aliases,
        &item_words_by_kind,
        ItemWordOwner::Buff,
    );
    for (section, stat_names_by_kind) in
        [("[effect.fixed]", &vocabulary.effect.fixed), ("[effect.by_item_default]", &vocabulary.effect.by_item_default)]
    {
        for (kind, stat_name) in stat_names_by_kind {
            record_source(sources_by_stat, stat_name, format!("effect_map.toml {section} {kind}"));
        }
    }
    for (kind, definition) in &vocabulary.effect.targeted {
        for (target, stat_names) in &definition.targets {
            for stat_name in stat_names {
                record_source(sources_by_stat, stat_name, format!("effect_map.toml [effect.targeted.{kind}] {target}"));
            }
        }
    }
    record_template_targets(
        sources_by_stat,
        "effect_map.toml [effect]",
        &vocabulary.effect.by_item,
        &vocabulary.item_aliases,
        &item_words_by_kind,
        ItemWordOwner::Effect,
    );
}

fn record_fixture_build_bonuses(sources_by_stat: &mut BTreeMap<String, String>) {
    let mut db = Connection::open_in_memory().unwrap();
    build_database(
        &fixtures_dir().join("DataFiles"),
        &WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap(),
        &Corrections::default(),
        &mut db,
        &DatasetVersion { upstream_sha: "31ef0201".into(), built_at: "2026-09-20T00:00:00Z".into() },
    )
    .expect("build succeeds on fixtures");
    let mut statement = db
        .prepare(
            "SELECT s.name, 'a feat bonus' FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
             WHERE ob.owner_kind = 'feat'
             UNION SELECT s.name, 'a set tier bonus' FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
             WHERE ob.owner_kind = 'set_bonus_tier'
             UNION SELECT s.name, 'an augment bonus' FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
             JOIN augments a ON a.id = ob.owner_id
             WHERE ob.owner_kind = 'augment' AND a.provenance = 'maetrim'",
        )
        .unwrap();
    let stat_names_with_owner: Vec<(String, String)> =
        statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?))).unwrap().map(Result::unwrap).collect();
    for (stat_name, owner) in stat_names_with_owner {
        record_source(sources_by_stat, &stat_name, format!("{owner} in the fixture build"));
    }
}

fn stat_names_under_key(toml_value: &toml::Value, stat_names: &mut Vec<String>) {
    match toml_value {
        toml::Value::Table(table) => {
            for (key, nested_value) in table {
                match (key.as_str(), nested_value) {
                    ("stat", toml::Value::String(stat_name)) => stat_names.push(stat_name.clone()),
                    _ => stat_names_under_key(nested_value, stat_names),
                }
            }
        }
        toml::Value::Array(values) => {
            for nested_value in values {
                stat_names_under_key(nested_value, stat_names);
            }
        }
        _ => {}
    }
}

fn record_override_bonuses(sources_by_stat: &mut BTreeMap<String, String>) {
    for dir in [data_dir().join("wiki"), data_dir().join("corrections")] {
        for path in files_under(&dir).into_iter().filter(|path| path.extension().is_some_and(|ext| ext == "toml")) {
            let file_text = std::fs::read_to_string(&path).unwrap();
            let toml_value: toml::Value =
                toml::from_str(&file_text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            let mut stat_names = Vec::new();
            stat_names_under_key(&toml_value, &mut stat_names);
            let file_name = path.strip_prefix(data_dir()).unwrap().display().to_string();
            for stat_name in stat_names {
                record_source(sources_by_stat, &stat_name, format!("a bonus in data/{file_name}"));
            }
        }
    }
}

#[test]
fn every_seed_stat_has_a_source_or_is_kept_without_one() {
    let mut sources_by_stat = BTreeMap::new();
    record_map_targets(&mut sources_by_stat);
    record_fixture_build_bonuses(&mut sources_by_stat);
    record_override_bonuses(&mut sources_by_stat);
    let kept_stat_names: BTreeSet<&str> = STATS_KEPT_WITHOUT_A_SOURCE.iter().map(|(name, _)| *name).collect();
    let seed_stat_names: BTreeSet<&str> = STATS.iter().map(|stat| stat.name).collect();

    let mut problems = Vec::new();
    for stat_name in &seed_stat_names {
        if !sources_by_stat.contains_key(*stat_name) && !kept_stat_names.contains(stat_name) {
            problems.push(format!(
                "{stat_name} has no source: no buff or effect map entry targets it and no feat, set tier, augment, \
                 wiki or correction bonus is on it; remove it from the seed, map it, or keep it in STATS_KEPT_WITHOUT_A_SOURCE"
            ));
        }
    }
    for stat_name in &kept_stat_names {
        if let Some(source) = sources_by_stat.get(*stat_name) {
            problems.push(format!(
                "{stat_name} is kept without a source but has one ({source}); drop it from STATS_KEPT_WITHOUT_A_SOURCE"
            ));
        }
        if !seed_stat_names.contains(stat_name) {
            problems.push(format!("{stat_name} is kept without a source but is no seed stat"));
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}
