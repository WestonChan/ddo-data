use crate::dataset::{build_in_memory_database, corrections_from, wiki_overrides_from};
use anyhow::{Context, Result};
use ddo_etl::build::BuildReport;
use ddo_etl::wiki::DescriptionKind;
use ddo_model::enums::RowSource;
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::path::Path;

const WIKI_PAGE_URL_PREFIX: &str = "https://ddowiki.com/page/";
const DIFFICULTY_NAME_SUFFIXES: &[&str] = &[" (Casual)", " (Normal)", " (Hard)", " (Elite)"];
const CRAFTING_SYSTEMS_JSON: &str = include_str!("../data/crafting_systems.json");
const KNOWN_SOCKET_LABEL_MISSPELLINGS: &[(&str, &str)] = &[("zentarim", "zhentarim"), ("upgradable", "upgradeable")];

pub fn wiki_check_report(
    data_files_dir: &Path,
    wiki_dir: Option<&Path>,
    corrections_dir: Option<&Path>,
) -> Result<String> {
    let wiki_overrides = wiki_overrides_from(wiki_dir)?;
    let (db, report) = build_in_memory_database(data_files_dir, &wiki_overrides, &corrections_from(corrections_dir)?)?;
    let mut report_lines = wiki_report_lines(&report);
    report_lines.push("warnings:".to_string());
    report_lines.extend(unused_family_augment_warnings(&db)?);
    report_lines.extend(socket_label_warnings(&db)?);
    report_lines.extend(wiki_quest_warnings(&report));
    report_lines.extend(wiki_item_warnings(&report));
    report_lines.extend(stale_correction_warnings(&report));
    report_lines.extend(looks_variant_warnings(&db)?);
    report_lines.extend(effect_spelling_warnings(&db)?);
    Ok(report_lines.join("\n"))
}

fn unused_family_augment_warnings(db: &Connection) -> Result<Vec<String>> {
    let mut statement = db.prepare(
        "SELECT DISTINCT crafting_systems.name, augments.family, augments.name
         FROM crafting_systems
         JOIN crafting_system_families ON crafting_system_families.system_id = crafting_systems.id
         JOIN augments ON augments.family = crafting_system_families.family
         WHERE NOT EXISTS (
             SELECT 1 FROM crafting_recipe_augments
             JOIN crafting_recipes ON crafting_recipes.id = crafting_recipe_augments.recipe_id
             WHERE crafting_recipes.system_id = crafting_systems.id AND crafting_recipe_augments.augment_id = augments.id
         )
         ORDER BY crafting_systems.name, augments.family, augments.name",
    )?;
    let warnings = statement
        .query_map([], |row| {
            let (system_name, family, augment_name): (String, String, String) = (row.get(0)?, row.get(1)?, row.get(2)?);
            Ok(format!("warning: {system_name}: {family} augment {augment_name:?} has no recipe"))
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(warnings)
}

fn wiki_report_lines(report: &BuildReport) -> Vec<String> {
    [
        ("wiki_quest_loot_entry_count", report.wiki_quest_loot_entry_count),
        ("wiki_rare_drop_count", report.wiki_rare_drop_count),
        ("wiki_added_quest_loot_link_count", report.wiki_added_quest_loot_link_count),
        ("drop_text_rare_link_count", report.drop_text_rare_link_count),
        ("quest_augment_loot_link_count", report.quest_augment_loot_link_count),
        ("drop_text_rare_augment_link_count", report.drop_text_rare_augment_link_count),
        ("wiki_quest_entry_count", report.wiki_quest_entry_count),
        ("wiki_quest_created_count", report.wiki_quest_created_count),
        ("drop_text_wiki_quest_link_count", report.drop_text_wiki_quest_link_count),
        ("wiki_quest_superseded_count", report.wiki_quest_superseded_count),
        ("wiki_quest_probable_duplicate_count", report.wiki_quest_probable_duplicate_count),
        ("wiki_crafting_system_count", report.wiki_crafting_system_count),
        ("wiki_crafting_recipe_count", report.wiki_crafting_recipe_count),
        ("wiki_crafting_ingredient_count", report.wiki_crafting_ingredient_count),
        ("wiki_description_entry_count", report.wiki_description_entry_count),
        ("wiki_description_filled_count", report.wiki_description_filled_count),
        ("wiki_description_skipped_count", report.wiki_description_skipped_count),
        ("wiki_item_written_count", report.wiki_item_written_count),
        ("wiki_item_superseded_count", report.wiki_item_superseded_count),
        ("wiki_item_probable_duplicate_count", report.wiki_item_probable_duplicate_count),
        ("correction_applied_count", report.correction_applied_count),
        ("correction_stale_count", report.correction_stale_count),
    ]
    .iter()
    .map(|(field_name, count)| format!("{field_name}: {count}"))
    .collect()
}

fn wiki_quest_warnings(report: &BuildReport) -> Vec<String> {
    let superseded_warnings = report.superseded_wiki_quests.iter().map(|superseded_quest| {
        format!(
            "warning: wiki quest {:?} is now in Maetrim's files; delete its quest fields from {}",
            superseded_quest.name, superseded_quest.file_name
        )
    });
    let probable_duplicate_warnings = report.probable_duplicate_wiki_quests.iter().map(|duplicate_quest| {
        format!(
            "warning: wiki quest {:?} may duplicate Maetrim's {:?}",
            duplicate_quest.name, duplicate_quest.maetrim_name
        )
    });
    superseded_warnings.chain(probable_duplicate_warnings).collect()
}

fn wiki_item_warnings(report: &BuildReport) -> Vec<String> {
    let superseded_warnings = report.superseded_wiki_items.iter().map(|superseded_item| {
        format!(
            "warning: wiki item {:?} is now in Maetrim's files; delete it from {}",
            superseded_item.name, superseded_item.file_name
        )
    });
    let probable_duplicate_warnings = report.probable_duplicate_wiki_items.iter().map(|duplicate_item| {
        format!(
            "warning: wiki item {:?} may duplicate Maetrim's {:?}",
            duplicate_item.name, duplicate_item.maetrim_name
        )
    });
    superseded_warnings.chain(probable_duplicate_warnings).collect()
}

fn effect_names_from(db: &Connection, sql: &str, item_sources: &[&str]) -> Result<Vec<String>> {
    let mut statement = db.prepare(sql)?;
    let effect_names = statement
        .query_map(rusqlite::params_from_iter(item_sources), |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(effect_names)
}

fn maetrim_effect_names(db: &Connection) -> Result<Vec<String>> {
    effect_names_from(
        db,
        "SELECT DISTINCT effects.name
         FROM effects
         JOIN item_effects ON item_effects.effect_id = effects.id
         JOIN items ON items.id = item_effects.item_id
         WHERE items.source = ?1
         ORDER BY effects.id",
        &[RowSource::Maetrim.as_str()],
    )
}

fn wiki_created_effect_names(db: &Connection) -> Result<Vec<String>> {
    effect_names_from(
        db,
        "SELECT DISTINCT effects.name
         FROM effects
         JOIN item_effects ON item_effects.effect_id = effects.id
         JOIN items ON items.id = item_effects.item_id
         WHERE items.source = ?2
           AND NOT EXISTS (
             SELECT 1 FROM item_effects AS maetrim_item_effects
             JOIN items AS maetrim_items ON maetrim_items.id = maetrim_item_effects.item_id
             WHERE maetrim_item_effects.effect_id = effects.id AND maetrim_items.source = ?1)
         ORDER BY effects.name",
        &[RowSource::Maetrim.as_str(), RowSource::Wiki.as_str()],
    )
}

fn effect_spelling_warnings(db: &Connection) -> Result<Vec<String>> {
    let maetrim_effect_names = maetrim_effect_names(db)?;
    let mut maetrim_effect_names_by_spelling_key: BTreeMap<String, &str> = BTreeMap::new();
    for maetrim_effect_name in &maetrim_effect_names {
        maetrim_effect_names_by_spelling_key
            .entry(lowercase_letters_and_digits(maetrim_effect_name))
            .or_insert(maetrim_effect_name);
    }
    let wiki_effect_names = wiki_created_effect_names(db)?;
    let warnings = wiki_effect_names
        .iter()
        .filter_map(|wiki_effect_name| {
            let maetrim_effect_name =
                maetrim_effect_names_by_spelling_key.get(&lowercase_letters_and_digits(wiki_effect_name))?;
            Some(format!(
                "warning: wiki effect {wiki_effect_name:?} may be Maetrim's {maetrim_effect_name:?} spelled differently"
            ))
        })
        .collect();
    Ok(warnings)
}

fn stale_correction_warnings(report: &BuildReport) -> Vec<String> {
    report
        .stale_corrections
        .iter()
        .map(|stale_correction| {
            format!(
                "warning: correction {} {:?}.{} expects {} but Maetrim now has {}; delete it from {}",
                stale_correction.kind,
                stale_correction.name,
                stale_correction.field,
                stale_correction.expected_value,
                stale_correction.maetrim_value,
                stale_correction.file_name
            )
        })
        .collect()
}

fn looks_variant_warnings(db: &Connection) -> Result<Vec<String>> {
    let mut statement = db.prepare(
        "SELECT wiki_items.name, maetrim_items.name
         FROM items AS wiki_items
         JOIN items AS maetrim_items
           ON maetrim_items.source = ?2
          AND maetrim_items.minimum_level = wiki_items.minimum_level
          AND maetrim_items.drop_location = wiki_items.drop_location
         WHERE wiki_items.source = ?1
           AND substr(wiki_items.name, 1, length(maetrim_items.name) + 2) = maetrim_items.name || ' ('
           AND wiki_items.name LIKE '%)'
         ORDER BY wiki_items.name, maetrim_items.name",
    )?;
    let warnings = statement
        .query_map([RowSource::Wiki.as_str(), RowSource::Maetrim.as_str()], |row| {
            let (wiki_name, maetrim_name): (String, String) = (row.get(0)?, row.get(1)?);
            Ok(format!(
                "warning: wiki item {wiki_name:?} looks like a variant of Maetrim's {maetrim_name:?} (same level and drop location)"
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(warnings)
}

pub fn write_wiki_batch(
    data_files_dir: &Path,
    wiki_dir: Option<&Path>,
    corrections_dir: Option<&Path>,
    out_dir: &Path,
) -> Result<Vec<String>> {
    let wiki_overrides = wiki_overrides_from(wiki_dir)?;
    let (db, _) = build_in_memory_database(data_files_dir, &wiki_overrides, &corrections_from(corrections_dir)?)?;
    std::fs::create_dir_all(out_dir).with_context(|| format!("creating {}", out_dir.display()))?;
    let batch_files = [
        ("item_names.txt", as_lines(names_from(&db, "items", RowSource::Maetrim)?)),
        ("wiki_source_items.txt", as_lines(names_from(&db, "items", RowSource::Wiki)?)),
        ("wiki_source_quests.txt", as_lines(names_from(&db, "quests", RowSource::Wiki)?)),
        ("augment_names.txt", as_lines(augment_name_lines(&db)?)),
        ("quest_pages.json", serde_json::to_string_pretty(&quest_page_urls(&db)?)? + "\n"),
        ("crafting_systems.json", CRAFTING_SYSTEMS_JSON.to_string()),
        ("blank_descriptions.txt", as_lines(blank_description_lines(&db)?)),
    ];
    batch_files
        .into_iter()
        .map(|(file_name, file_text)| {
            let file_path = out_dir.join(file_name);
            std::fs::write(&file_path, file_text).with_context(|| format!("writing {}", file_path.display()))?;
            Ok(file_path.display().to_string())
        })
        .collect()
}

pub fn likely_wiki_page_url(quest_name: &str) -> String {
    let page_name = DIFFICULTY_NAME_SUFFIXES
        .iter()
        .find_map(|difficulty_suffix| quest_name.strip_suffix(difficulty_suffix))
        .unwrap_or(quest_name);
    wiki_page_url(page_name)
}

pub fn blank_description_page_url(kind: DescriptionKind, name: &str) -> String {
    match kind {
        DescriptionKind::Item | DescriptionKind::Augment => wiki_page_url(&format!("Item:{name}")),
        DescriptionKind::Race | DescriptionKind::Feat | DescriptionKind::Enhancement => wiki_page_url(name),
    }
}

fn wiki_page_url(page_name: &str) -> String {
    format!("{WIKI_PAGE_URL_PREFIX}{}", page_name.replace(' ', "_").replace('\'', "%27"))
}

fn as_lines(lines: Vec<String>) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

fn names_from(db: &Connection, table_name: &str, source: RowSource) -> Result<Vec<String>> {
    let mut statement = db.prepare(&format!("SELECT name FROM {table_name} WHERE source = ?1 ORDER BY name"))?;
    let names = statement.query_map([source.as_str()], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?;
    Ok(names)
}

fn augment_name_lines(db: &Connection) -> Result<Vec<String>> {
    let mut statement = db.prepare("SELECT family, name, min_level FROM augments ORDER BY family, name, min_level")?;
    let lines = statement
        .query_map([], |row| {
            let (family, name, min_level): (String, String, Option<i64>) = (row.get(0)?, row.get(1)?, row.get(2)?);
            Ok(format!("{family}\t{name}\t{}", min_level.map(|level| level.to_string()).unwrap_or_default()))
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(lines)
}

fn quest_page_urls(db: &Connection) -> Result<BTreeMap<String, String>> {
    let mut statement = db.prepare("SELECT name FROM quests")?;
    let quest_names: Vec<String> = statement.query_map([], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?;
    Ok(quest_names
        .into_iter()
        .map(|quest_name| {
            let page_url = likely_wiki_page_url(&quest_name);
            (quest_name, page_url)
        })
        .collect())
}

fn blank_description_lines(db: &Connection) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    for kind in DescriptionKind::ALL {
        let mut statement =
            db.prepare(&format!("SELECT DISTINCT name, description FROM {} ORDER BY name", kind.table_name()))?;
        let names_and_descriptions: Vec<(String, Option<String>)> =
            statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        let mut names_awaiting_description: Vec<String> = names_and_descriptions
            .into_iter()
            .filter(|(_, description)| kind.is_awaiting_description(description.as_deref()))
            .map(|(name, _)| name)
            .collect();
        names_awaiting_description.dedup();
        lines.extend(names_awaiting_description.into_iter().map(|name| {
            let page_url = blank_description_page_url(kind, &name);
            format!("{}\t{name}\t{page_url}", kind.as_str())
        }));
    }
    Ok(lines)
}

fn lowercase_letters_and_digits(text: &str) -> String {
    text.chars().filter(|character| character.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

pub fn socket_label_spelling_key(label: &str) -> String {
    KNOWN_SOCKET_LABEL_MISSPELLINGS
        .iter()
        .fold(lowercase_letters_and_digits(label), |key, (misspelling, spelling)| key.replace(misspelling, spelling))
}

pub fn socket_label_spelling_warnings(labels: &[String]) -> Vec<String> {
    let mut labels_by_spelling_key: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for label in labels {
        labels_by_spelling_key.entry(socket_label_spelling_key(label)).or_default().push(label);
    }
    labels_by_spelling_key
        .into_values()
        .filter(|colliding_labels| colliding_labels.len() > 1)
        .map(|mut colliding_labels| {
            colliding_labels.sort_unstable();
            let quoted_labels: Vec<String> = colliding_labels.iter().map(|label| format!("{label:?}")).collect();
            format!("warning: socket labels differ only by spelling: {}", quoted_labels.join(" / "))
        })
        .collect()
}

fn socket_label_warnings(db: &Connection) -> Result<Vec<String>> {
    let mut statement = db.prepare("SELECT label FROM augment_slot_types ORDER BY label")?;
    let labels: Vec<String> = statement.query_map([], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?;
    Ok(socket_label_spelling_warnings(&labels))
}
