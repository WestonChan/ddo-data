use anyhow::Result;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ItemCoverageReport {
    pub matched_count: usize,
    pub names_only_in_built: Vec<String>,
    pub names_only_in_legacy: Vec<String>,
    pub names_excluded_by_design: Vec<String>,
}

impl ItemCoverageReport {
    pub fn coverage_ratio(&self) -> f64 {
        let comparable_legacy_count = self.matched_count + self.names_only_in_legacy.len();
        if comparable_legacy_count == 0 {
            1.0
        } else {
            self.matched_count as f64 / comparable_legacy_count as f64
        }
    }
}

pub fn name_comparison_key(name: &str) -> String {
    let base_name = without_level_suffix(name.trim());
    base_name.chars().filter(|c| c.is_ascii_alphanumeric()).flat_map(char::to_lowercase).collect()
}

fn without_level_suffix(name: &str) -> &str {
    let Some(open_paren_index) = name.rfind('(') else {
        return name;
    };
    let parenthesized = name[open_paren_index + 1..].trim_end_matches(')').trim();
    let is_level_suffix = parenthesized
        .strip_prefix("level ")
        .or_else(|| parenthesized.strip_prefix("Level "))
        .or_else(|| parenthesized.strip_prefix("ML "))
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    if name.ends_with(')') && is_level_suffix {
        name[..open_paren_index].trim_end()
    } else {
        name
    }
}

fn names_by_comparison_key(db: &Connection, sql: &str) -> Result<HashMap<String, String>> {
    let mut statement = db.prepare(sql)?;
    let names = statement.query_map([], |r| r.get::<_, String>(0))?;
    let mut names_by_key = HashMap::new();
    for name in names {
        let name = name?;
        names_by_key.insert(name_comparison_key(&name), name);
    }
    Ok(names_by_key)
}

pub fn item_coverage(built_db: &Connection, legacy_db: &Connection) -> Result<ItemCoverageReport> {
    let built_names_by_key = names_by_comparison_key(built_db, "SELECT name FROM items")?;
    let legacy_names_by_key = names_by_comparison_key(legacy_db, "SELECT name FROM items")?;
    let excluded_keys: HashSet<String> = match names_by_comparison_key(built_db, "SELECT name FROM excluded_items") {
        Ok(excluded_names_by_key) => excluded_names_by_key.into_keys().collect(),
        Err(_) => HashSet::new(),
    };
    let mut report = ItemCoverageReport::default();
    for (key, name) in &legacy_names_by_key {
        if built_names_by_key.contains_key(key) {
            report.matched_count += 1;
        } else if excluded_keys.contains(key) {
            report.names_excluded_by_design.push(name.clone());
        } else {
            report.names_only_in_legacy.push(name.clone());
        }
    }
    for (key, name) in &built_names_by_key {
        if !legacy_names_by_key.contains_key(key) {
            report.names_only_in_built.push(name.clone());
        }
    }
    report.names_only_in_legacy.sort();
    report.names_only_in_built.sort();
    report.names_excluded_by_design.sort();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::name_comparison_key;

    #[test]
    fn level_suffix_variants_share_a_key() {
        assert_eq!(name_comparison_key("Allegiance (Level 12)"), "allegiance");
        assert_eq!(name_comparison_key("Adamantine Knuckles (level 23)"), "adamantineknuckles");
        assert_eq!(name_comparison_key("Allegiance"), "allegiance");
        assert_eq!(
            name_comparison_key("Cloak of the Reaper (Blue)"),
            "cloakofthereaperblue",
            "only level suffixes are stripped"
        );
        assert_eq!(name_comparison_key("Sireth, Spear of the Sky"), "sirethspearofthesky");
    }
}
