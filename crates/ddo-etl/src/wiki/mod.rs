mod crafting;
mod descriptions;
mod quest_loot;
mod quests;

pub use crafting::{CraftingIngredient, CraftingRecipe, CraftingSystem, IngredientCost};
pub use descriptions::{DescriptionKind, WikiDescription};
pub use quest_loot::QuestLoot;
pub use quests::{QuestFacts, QuestXp, TierXp};

use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::path::Path;

const EMBEDDED_WIKI_FILES: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/wiki_files.rs"));
const WIKI_PAGE_URL_PREFIX: &str = "https://ddowiki.com/page/";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WikiOverrides {
    pub quest_loot: Vec<QuestLoot>,
    pub quest_facts: Vec<QuestFacts>,
    pub crafting_systems: Vec<CraftingSystem>,
    pub descriptions: Vec<WikiDescription>,
}

trait WikiEntry: DeserializeOwned {
    const TOML_TABLE_NAME: &'static str;
    fn name(&self) -> &str;
    fn citation(&self) -> (&str, &str);
    fn unique_key(&self) -> String {
        self.name().to_owned()
    }
    fn validate(&self) -> Result<()> {
        Ok(())
    }
}

impl WikiEntry for QuestLoot {
    const TOML_TABLE_NAME: &'static str = "quest";
    fn name(&self) -> &str {
        &self.name
    }
    fn citation(&self) -> (&str, &str) {
        (&self.page, &self.read)
    }
}

impl WikiEntry for QuestFacts {
    const TOML_TABLE_NAME: &'static str = "quest";
    fn name(&self) -> &str {
        &self.name
    }
    fn citation(&self) -> (&str, &str) {
        (&self.page, &self.read)
    }
    fn validate(&self) -> Result<()> {
        QuestFacts::validate(self)
    }
}

impl WikiEntry for CraftingSystem {
    const TOML_TABLE_NAME: &'static str = "system";
    fn name(&self) -> &str {
        &self.name
    }
    fn citation(&self) -> (&str, &str) {
        (&self.page, &self.read)
    }
    fn validate(&self) -> Result<()> {
        CraftingSystem::validate(self)
    }
}

impl WikiEntry for WikiDescription {
    const TOML_TABLE_NAME: &'static str = "entry";
    fn name(&self) -> &str {
        &self.name
    }
    fn citation(&self) -> (&str, &str) {
        (&self.page, &self.read)
    }
    fn unique_key(&self) -> String {
        format!("{} {}", self.kind.as_str(), self.name)
    }
    fn validate(&self) -> Result<()> {
        WikiDescription::validate(self)
    }
}

enum WikiFileKind {
    QuestLoot,
    QuestFacts,
    CraftingSystems,
    Descriptions,
}

impl WikiFileKind {
    fn from_file_name(file_name: &str) -> Result<Self> {
        let stem = file_name.strip_suffix(".toml").unwrap_or(file_name);
        if stem.starts_with("quest_loot") {
            Ok(Self::QuestLoot)
        } else if stem.starts_with("quests") {
            Ok(Self::QuestFacts)
        } else if stem.starts_with("crafting") {
            Ok(Self::CraftingSystems)
        } else if stem.starts_with("descriptions") {
            Ok(Self::Descriptions)
        } else {
            bail!(
                "wiki file {file_name}: the name must start with quest_loot, quests, crafting or descriptions, which says what it holds"
            )
        }
    }
}

fn parse_wiki_entries<'a, T: WikiEntry>(
    file_name: &'a str,
    toml_text: &str,
    first_file_by_entry_key: &mut HashMap<String, &'a str>,
) -> Result<Vec<T>> {
    let table_name = T::TOML_TABLE_NAME;
    let mut document: toml::Table = toml::from_str(toml_text).with_context(|| format!("wiki file {file_name}"))?;
    let tables = match document.remove(table_name) {
        None => Vec::new(),
        Some(toml::Value::Array(tables)) => tables,
        Some(_) => bail!("wiki file {file_name}: {table_name} must be an array of tables, written [[{table_name}]]"),
    };
    if let Some(unknown_key) = document.keys().next() {
        bail!(
            "wiki file {file_name}: unknown top-level key {unknown_key:?}; this file type holds only [[{table_name}]] tables"
        );
    }
    let mut entries = Vec::with_capacity(tables.len());
    for (index, table) in tables.into_iter().enumerate() {
        let entry_label = match table.get("name").and_then(toml::Value::as_str) {
            Some(name) => format!("{table_name} {name:?}"),
            None => format!("{table_name} #{}", index + 1),
        };
        let entry: T = table.try_into().with_context(|| format!("wiki file {file_name}: {entry_label}"))?;
        let (page, read) = entry.citation();
        validate_citation(page, read)
            .and_then(|()| entry.validate())
            .with_context(|| format!("wiki file {file_name}: {entry_label}"))?;
        if let Some(first_file) = first_file_by_entry_key.insert(entry.unique_key(), file_name) {
            bail!("wiki file {file_name}: {entry_label} is already listed in {first_file}");
        }
        entries.push(entry);
    }
    Ok(entries)
}

impl WikiOverrides {
    pub fn embedded() -> Result<Self> {
        Self::from_toml_files(EMBEDDED_WIKI_FILES)
    }

    pub fn from_dir(dir: &Path) -> Result<Self> {
        let mut paths: Vec<_> = std::fs::read_dir(dir)
            .with_context(|| format!("listing {}", dir.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "toml"))
            .collect();
        paths.sort();
        let toml_files = paths
            .iter()
            .map(|path| {
                let toml_text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
                Ok((path.file_name().unwrap_or_default().to_string_lossy().into_owned(), toml_text))
            })
            .collect::<Result<Vec<_>>>()?;
        Self::from_toml_files(
            &toml_files
                .iter()
                .map(|(file_name, toml_text)| (file_name.as_str(), toml_text.as_str()))
                .collect::<Vec<_>>(),
        )
    }

    pub fn from_toml_files(toml_files: &[(&str, &str)]) -> Result<Self> {
        let mut overrides = Self::default();
        let (mut quest_loot_file_by_quest, mut quest_facts_file_by_quest, mut crafting_file_by_system) =
            (HashMap::new(), HashMap::new(), HashMap::new());
        let mut description_file_by_kind_and_name = HashMap::new();
        for (file_name, toml_text) in toml_files {
            match WikiFileKind::from_file_name(file_name)? {
                WikiFileKind::QuestLoot => overrides.quest_loot.extend(parse_wiki_entries(
                    file_name,
                    toml_text,
                    &mut quest_loot_file_by_quest,
                )?),
                WikiFileKind::QuestFacts => overrides.quest_facts.extend(parse_wiki_entries(
                    file_name,
                    toml_text,
                    &mut quest_facts_file_by_quest,
                )?),
                WikiFileKind::CraftingSystems => overrides.crafting_systems.extend(parse_wiki_entries(
                    file_name,
                    toml_text,
                    &mut crafting_file_by_system,
                )?),
                WikiFileKind::Descriptions => overrides.descriptions.extend(parse_wiki_entries(
                    file_name,
                    toml_text,
                    &mut description_file_by_kind_and_name,
                )?),
            }
        }
        Ok(overrides)
    }
}

fn validate_citation(page: &str, read: &str) -> Result<()> {
    if !page.starts_with(WIKI_PAGE_URL_PREFIX) || page.len() == WIKI_PAGE_URL_PREFIX.len() {
        bail!("page {page:?} must start with {WIKI_PAGE_URL_PREFIX} and name the page read");
    }
    if !is_iso_date(read) {
        bail!("read {read:?} must be the ISO date the page was read, YYYY-MM-DD");
    }
    Ok(())
}

fn is_iso_date(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    let [year, month, day] = parts.as_slice() else {
        return false;
    };
    let is_digit_run = |s: &str, length: usize| s.len() == length && s.bytes().all(|b| b.is_ascii_digit());
    if !(is_digit_run(year, 4) && is_digit_run(month, 2) && is_digit_run(day, 2)) {
        return false;
    }
    let (year, month, day): (u32, u32, u32) = (year.parse().unwrap(), month.parse().unwrap(), day.parse().unwrap());
    let is_leap_year = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days_in_month).contains(&day)
}
