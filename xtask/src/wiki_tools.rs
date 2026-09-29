use crate::dataset::{build_in_memory_database, wiki_overrides_from};
use anyhow::{Context, Result};
use ddo_etl::build::BuildReport;
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::path::Path;

const WIKI_PAGE_URL_PREFIX: &str = "https://ddowiki.com/page/";
const DIFFICULTY_NAME_SUFFIXES: &[&str] = &[" (Casual)", " (Normal)", " (Hard)", " (Elite)"];
const CRAFTING_SYSTEMS_JSON: &str = include_str!("../data/crafting_systems.json");

pub fn wiki_check_report(data_files_dir: &Path, wiki_dir: Option<&Path>) -> Result<String> {
    let wiki_overrides = wiki_overrides_from(wiki_dir)?;
    let (_, report) = build_in_memory_database(data_files_dir, &wiki_overrides)?;
    Ok(wiki_report_lines(&report).join("\n"))
}

fn wiki_report_lines(report: &BuildReport) -> Vec<String> {
    [
        ("wiki_quest_loot_entry_count", report.wiki_quest_loot_entry_count),
        ("wiki_rare_drop_count", report.wiki_rare_drop_count),
        ("wiki_added_quest_loot_link_count", report.wiki_added_quest_loot_link_count),
        ("drop_text_rare_link_count", report.drop_text_rare_link_count),
        ("wiki_quest_entry_count", report.wiki_quest_entry_count),
        ("wiki_quest_xp_row_count", report.wiki_quest_xp_row_count),
        ("wiki_crafting_system_count", report.wiki_crafting_system_count),
        ("wiki_crafting_recipe_count", report.wiki_crafting_recipe_count),
        ("wiki_crafting_ingredient_count", report.wiki_crafting_ingredient_count),
    ]
    .iter()
    .map(|(field_name, count)| format!("{field_name}: {count}"))
    .collect()
}

pub fn write_wiki_batch(data_files_dir: &Path, wiki_dir: Option<&Path>, out_dir: &Path) -> Result<Vec<String>> {
    let wiki_overrides = wiki_overrides_from(wiki_dir)?;
    let (db, _) = build_in_memory_database(data_files_dir, &wiki_overrides)?;
    std::fs::create_dir_all(out_dir).with_context(|| format!("creating {}", out_dir.display()))?;
    let batch_files = [
        ("item_names.txt", as_lines(item_names(&db)?)),
        ("augment_names.txt", as_lines(augment_name_lines(&db)?)),
        ("quest_pages.json", serde_json::to_string_pretty(&quest_page_urls(&db)?)? + "\n"),
        ("crafting_systems.json", CRAFTING_SYSTEMS_JSON.to_string()),
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
    format!("{WIKI_PAGE_URL_PREFIX}{}", page_name.replace(' ', "_").replace('\'', "%27"))
}

fn as_lines(lines: Vec<String>) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

fn item_names(db: &Connection) -> Result<Vec<String>> {
    let mut statement = db.prepare("SELECT name FROM items ORDER BY name")?;
    let names = statement.query_map([], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?;
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
