//! Compare a freshly built database with a previous one by item name. The legacy DDO Tools
//! database is the correctness fixture for the port: the roadmap's V2 bar is ≥ 95% of its item
//! names present here after normalisation, not counting items excluded on purpose.

use anyhow::Result;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DiffReport {
    pub matched: usize,
    pub only_new: Vec<String>,
    /// Legacy names with no counterpart, after removing the ones the new build excluded on purpose.
    pub only_legacy: Vec<String>,
    /// Legacy names the new build lists in `excluded_items`.
    pub excluded_by_design: Vec<String>,
}

impl DiffReport {
    /// Share of legacy names found in the new database, ignoring deliberate exclusions.
    pub fn coverage(&self) -> f64 {
        let denominator = self.matched + self.only_legacy.len();
        if denominator == 0 {
            1.0
        } else {
            self.matched as f64 / denominator as f64
        }
    }
}

/// Lower-case alphanumerics only, with a trailing "(Level 12)" removed: upstream files one item
/// per level for randomly-levelled loot, where the wiki (and so the legacy database) had one page.
pub fn normalize_name(name: &str) -> String {
    let base = strip_level_suffix(name.trim());
    base.chars().filter(|c| c.is_ascii_alphanumeric()).flat_map(char::to_lowercase).collect()
}

fn strip_level_suffix(name: &str) -> &str {
    let Some(open) = name.rfind('(') else {
        return name;
    };
    let inner = name[open + 1..].trim_end_matches(')').trim();
    let is_level = inner
        .strip_prefix("level ")
        .or_else(|| inner.strip_prefix("Level "))
        .or_else(|| inner.strip_prefix("ML "))
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    if name.ends_with(')') && is_level {
        name[..open].trim_end()
    } else {
        name
    }
}

fn names(conn: &Connection, sql: &str) -> Result<HashMap<String, String>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    let mut out = HashMap::new();
    for name in rows {
        let name = name?;
        out.insert(normalize_name(&name), name);
    }
    Ok(out)
}

pub fn compare(new: &Connection, legacy: &Connection) -> Result<DiffReport> {
    let new_names = names(new, "SELECT name FROM items")?;
    let legacy_names = names(legacy, "SELECT name FROM items")?;
    let excluded: HashSet<String> = match names(new, "SELECT name FROM excluded_items") {
        Ok(map) => map.into_keys().collect(),
        Err(_) => HashSet::new(), // an older database without the table
    };
    let mut report = DiffReport::default();
    for (key, name) in &legacy_names {
        if new_names.contains_key(key) {
            report.matched += 1;
        } else if excluded.contains(key) {
            report.excluded_by_design.push(name.clone());
        } else {
            report.only_legacy.push(name.clone());
        }
    }
    for (key, name) in &new_names {
        if !legacy_names.contains_key(key) {
            report.only_new.push(name.clone());
        }
    }
    report.only_legacy.sort();
    report.only_new.sort();
    report.excluded_by_design.sort();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::normalize_name;

    #[test]
    fn level_suffix_variants_share_a_key() {
        assert_eq!(normalize_name("Allegiance (Level 12)"), "allegiance");
        assert_eq!(normalize_name("Adamantine Knuckles (level 23)"), "adamantineknuckles");
        assert_eq!(normalize_name("Allegiance"), "allegiance");
        assert_eq!(
            normalize_name("Cloak of the Reaper (Blue)"),
            "cloakofthereaperblue",
            "only level suffixes are stripped"
        );
        assert_eq!(normalize_name("Sireth, Spear of the Sky"), "sirethspearofthesky");
    }
}
