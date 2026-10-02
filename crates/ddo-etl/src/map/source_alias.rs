use anyhow::{bail, Context, Result};
use serde::Deserialize;

const EMBEDDED_FILE_NAME: &str = "data/source_aliases.toml";
const EMBEDDED_FILE: &str = include_str!("../../data/source_aliases.toml");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CraftingSystemAlias {
    pub text: String,
    pub system: String,
    pub reason: String,
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
            target_name: &alias.pack,
            reason: &alias.reason,
        });
        let crafting_system_aliases = aliases.crafting_systems.iter().map(|alias| AliasFields {
            table_name: "crafting_system",
            text: &alias.text,
            contains: None,
            target_name: &alias.system,
            reason: &alias.reason,
        });
        let vendor_aliases = aliases.vendors.iter().map(|alias| AliasFields {
            table_name: "vendor",
            text: &alias.text,
            contains: alias.contains.as_deref(),
            target_name: &alias.vendor,
            reason: &alias.reason,
        });
        let mut known_matches: Vec<(String, Option<String>)> = Vec::new();
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
            let known_match = (lowercase_text, lowercase_contains);
            if known_matches.contains(&known_match) {
                bail!("alias text {:?} appears twice with the same contains", alias.text);
            }
            known_matches.push(known_match);
        }
        Ok(aliases)
    }
}

struct AliasFields<'alias> {
    table_name: &'static str,
    text: &'alias str,
    contains: Option<&'alias str>,
    target_name: &'alias str,
    reason: &'alias str,
}
