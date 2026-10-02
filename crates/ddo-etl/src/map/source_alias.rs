use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::ops::RangeInclusive;

const EMBEDDED_FILE_NAME: &str = "data/source_aliases.toml";
const EMBEDDED_FILE: &str = include_str!("../../data/source_aliases.toml");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CraftingSystemAlias {
    pub text: String,
    pub min_minimum_level: Option<i64>,
    pub max_minimum_level: Option<i64>,
    pub system: String,
    pub reason: String,
}

impl CraftingSystemAlias {
    pub fn minimum_levels(&self) -> Option<RangeInclusive<i64>> {
        if self.min_minimum_level.is_none() && self.max_minimum_level.is_none() {
            return None;
        }
        Some(self.min_minimum_level.unwrap_or(i64::MIN)..=self.max_minimum_level.unwrap_or(i64::MAX))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChallengeAlias {
    pub text: String,
    pub pack: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VendorAlias {
    pub text: String,
    pub contains: Option<String>,
    pub vendor: String,
    pub reason: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceAliases {
    #[serde(default, rename = "challenge")]
    pub challenges: Vec<ChallengeAlias>,
    #[serde(default, rename = "crafting_system")]
    pub crafting_systems: Vec<CraftingSystemAlias>,
    #[serde(default, rename = "vendor")]
    pub vendors: Vec<VendorAlias>,
}

impl SourceAliases {
    pub fn embedded() -> Result<Self> {
        Self::from_toml_str(EMBEDDED_FILE).context(EMBEDDED_FILE_NAME)
    }

    pub fn from_toml_str(text: &str) -> Result<Self> {
        let aliases: Self = toml::from_str(text)?;
        let challenge_aliases = aliases.challenges.iter().map(|alias| AliasFields {
            table_name: "challenge",
            text: &alias.text,
            contains: None,
            minimum_levels: None,
            target_name: &alias.pack,
            reason: &alias.reason,
        });
        let crafting_system_aliases = aliases.crafting_systems.iter().map(|alias| AliasFields {
            table_name: "crafting_system",
            text: &alias.text,
            contains: None,
            minimum_levels: alias.minimum_levels(),
            target_name: &alias.system,
            reason: &alias.reason,
        });
        let vendor_aliases = aliases.vendors.iter().map(|alias| AliasFields {
            table_name: "vendor",
            text: &alias.text,
            contains: alias.contains.as_deref(),
            minimum_levels: None,
            target_name: &alias.vendor,
            reason: &alias.reason,
        });
        let mut known_matches: Vec<(String, Option<String>, RangeInclusive<i64>)> = Vec::new();
        for alias in challenge_aliases.chain(crafting_system_aliases).chain(vendor_aliases) {
            let lowercase_text = alias.text.trim().to_lowercase();
            let lowercase_contains = alias.contains.map(|contains| contains.trim().to_lowercase());
            let has_blank_field = lowercase_text.is_empty()
                || lowercase_contains.as_ref().is_some_and(String::is_empty)
                || alias.target_name.trim().is_empty()
                || alias.reason.trim().is_empty();
            if has_blank_field {
                bail!("{} alias {:?}: no field may be blank", alias.table_name, alias.text);
            }
            let minimum_levels = alias.minimum_levels.unwrap_or(i64::MIN..=i64::MAX);
            if minimum_levels.is_empty() {
                bail!("{} alias {:?}: min_minimum_level is above max_minimum_level", alias.table_name, alias.text);
            }
            let is_matched_by_another_alias = known_matches.iter().any(|(known_text, known_contains, known_levels)| {
                *known_text == lowercase_text
                    && *known_contains == lowercase_contains
                    && known_levels.start() <= minimum_levels.end()
                    && minimum_levels.start() <= known_levels.end()
            });
            if is_matched_by_another_alias {
                bail!(
                    "alias text {:?} appears twice with the same contains and overlapping minimum levels",
                    alias.text
                );
            }
            known_matches.push((lowercase_text, lowercase_contains, minimum_levels));
        }
        Ok(aliases)
    }
}

struct AliasFields<'alias> {
    table_name: &'static str,
    text: &'alias str,
    contains: Option<&'alias str>,
    minimum_levels: Option<RangeInclusive<i64>>,
    target_name: &'alias str,
    reason: &'alias str,
}
