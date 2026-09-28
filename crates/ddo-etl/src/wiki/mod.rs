mod crafting;
mod quest_loot;
mod quests;

pub use crafting::{Cost, CraftingSystem, Ingredient, Recipe};
pub use quest_loot::QuestLoot;
pub use quests::{QuestFacts, QuestXp, TierXp};

use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::path::Path;

const EMBEDDED: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/wiki_files.rs"));
const PAGE_PREFIX: &str = "https://ddowiki.com/page/";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WikiOverrides {
    pub quest_loot: Vec<QuestLoot>,
    pub quests: Vec<QuestFacts>,
    pub crafting: Vec<CraftingSystem>,
}

trait WikiEntry: DeserializeOwned {
    const TABLE: &'static str;
    fn name(&self) -> &str;
    fn citation(&self) -> (&str, &str);
    fn check(&self) -> Result<()> {
        Ok(())
    }
}

impl WikiEntry for QuestLoot {
    const TABLE: &'static str = "quest";
    fn name(&self) -> &str {
        &self.name
    }
    fn citation(&self) -> (&str, &str) {
        (&self.page, &self.read)
    }
}

impl WikiEntry for QuestFacts {
    const TABLE: &'static str = "quest";
    fn name(&self) -> &str {
        &self.name
    }
    fn citation(&self) -> (&str, &str) {
        (&self.page, &self.read)
    }
    fn check(&self) -> Result<()> {
        self.check_values()
    }
}

impl WikiEntry for CraftingSystem {
    const TABLE: &'static str = "system";
    fn name(&self) -> &str {
        &self.name
    }
    fn citation(&self) -> (&str, &str) {
        (&self.page, &self.read)
    }
    fn check(&self) -> Result<()> {
        self.check_values()
    }
}

enum FileKind {
    QuestLoot,
    Quests,
    Crafting,
}

impl FileKind {
    fn of(file_name: &str) -> Result<Self> {
        let stem = file_name.strip_suffix(".toml").unwrap_or(file_name);
        if stem.starts_with("quest_loot") {
            Ok(Self::QuestLoot)
        } else if stem.starts_with("quests") {
            Ok(Self::Quests)
        } else if stem.starts_with("crafting") {
            Ok(Self::Crafting)
        } else {
            bail!(
                "wiki file {file_name}: the name must start with quest_loot, quests or crafting, which says what it holds"
            )
        }
    }
}

fn read_entries<'a, T: WikiEntry>(
    file_name: &'a str,
    source: &str,
    seen: &mut HashMap<String, &'a str>,
) -> Result<Vec<T>> {
    let table = T::TABLE;
    let mut document: toml::Table = toml::from_str(source).with_context(|| format!("wiki file {file_name}"))?;
    let rows = match document.remove(table) {
        None => Vec::new(),
        Some(toml::Value::Array(rows)) => rows,
        Some(_) => bail!("wiki file {file_name}: {table} must be an array of tables, written [[{table}]]"),
    };
    if let Some(other) = document.keys().next() {
        bail!("wiki file {file_name}: unknown top-level key {other:?}; this file type holds only [[{table}]] tables");
    }
    let mut entries = Vec::with_capacity(rows.len());
    for (index, row) in rows.into_iter().enumerate() {
        let label = match row.get("name").and_then(toml::Value::as_str) {
            Some(name) => format!("{table} {name:?}"),
            None => format!("{table} #{}", index + 1),
        };
        let entry: T = row.try_into().with_context(|| format!("wiki file {file_name}: {label}"))?;
        let (page, read) = entry.citation();
        check_citation(page, read)
            .and_then(|()| entry.check())
            .with_context(|| format!("wiki file {file_name}: {label}"))?;
        if let Some(first) = seen.insert(entry.name().to_owned(), file_name) {
            bail!("wiki file {file_name}: {label} is already listed in {first}");
        }
        entries.push(entry);
    }
    Ok(entries)
}

impl WikiOverrides {
    pub fn embedded() -> Result<Self> {
        Self::from_sources(EMBEDDED)
    }

    pub fn from_dir(dir: &Path) -> Result<Self> {
        let mut paths: Vec<_> = std::fs::read_dir(dir)
            .with_context(|| format!("listing {}", dir.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "toml"))
            .collect();
        paths.sort();
        let files = paths
            .iter()
            .map(|p| {
                let source = std::fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?;
                Ok((p.file_name().unwrap_or_default().to_string_lossy().into_owned(), source))
            })
            .collect::<Result<Vec<_>>>()?;
        Self::from_sources(&files.iter().map(|(n, s)| (n.as_str(), s.as_str())).collect::<Vec<_>>())
    }

    pub fn from_sources(files: &[(&str, &str)]) -> Result<Self> {
        let mut overrides = Self::default();
        let (mut quest_loot_files, mut quests_files, mut crafting_files) =
            (HashMap::new(), HashMap::new(), HashMap::new());
        for (file_name, source) in files {
            match FileKind::of(file_name)? {
                FileKind::QuestLoot => {
                    overrides.quest_loot.extend(read_entries(file_name, source, &mut quest_loot_files)?);
                }
                FileKind::Quests => overrides.quests.extend(read_entries(file_name, source, &mut quests_files)?),
                FileKind::Crafting => {
                    overrides.crafting.extend(read_entries(file_name, source, &mut crafting_files)?);
                }
            }
        }
        Ok(overrides)
    }
}

fn check_citation(page: &str, read: &str) -> Result<()> {
    if !page.starts_with(PAGE_PREFIX) || page.len() == PAGE_PREFIX.len() {
        bail!("page {page:?} must start with {PAGE_PREFIX} and name the page read");
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
    let digits = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_digit());
    if !(digits(year, 4) && digits(month, 2) && digits(day, 2)) {
        return false;
    }
    let (year, month, day): (u32, u32, u32) = (year.parse().unwrap(), month.parse().unwrap(), day.parse().unwrap());
    let is_leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days_in_month).contains(&day)
}
