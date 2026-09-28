mod quest_loot;

pub use quest_loot::QuestLoot;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

const EMBEDDED: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/wiki_files.rs"));
const PAGE_PREFIX: &str = "https://ddowiki.com/page/";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WikiOverrides {
    pub quest_loot: Vec<QuestLoot>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WikiFile {
    #[serde(default)]
    quest: Vec<QuestLoot>,
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
        let mut quest_files: HashMap<String, &str> = HashMap::new();
        for (file_name, source) in files {
            let file: WikiFile = toml::from_str(source).with_context(|| format!("wiki file {file_name}"))?;
            for entry in file.quest {
                check_citation(&entry.page, &entry.read)
                    .with_context(|| format!("wiki file {file_name}: quest {:?}", entry.name))?;
                if let Some(first) = quest_files.insert(entry.name.clone(), file_name) {
                    bail!("wiki file {file_name}: quest {:?} is already listed in {first}", entry.name);
                }
                overrides.quest_loot.push(entry);
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
