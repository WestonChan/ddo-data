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

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceAliases {
    #[serde(default, rename = "crafting_system")]
    pub crafting_systems: Vec<CraftingSystemAlias>,
}

impl SourceAliases {
    pub fn embedded() -> Result<Self> {
        Self::from_toml_str(EMBEDDED_FILE).context(EMBEDDED_FILE_NAME)
    }

    pub fn from_toml_str(text: &str) -> Result<Self> {
        let aliases: Self = toml::from_str(text)?;
        let mut known_texts: Vec<String> = Vec::new();
        for alias in &aliases.crafting_systems {
            let lowercase_text = alias.text.trim().to_lowercase();
            if lowercase_text.is_empty() || alias.system.trim().is_empty() || alias.reason.trim().is_empty() {
                bail!("crafting_system alias {:?}: text, system and reason must not be blank", alias.text);
            }
            if known_texts.contains(&lowercase_text) {
                bail!("alias text {:?} appears twice", alias.text);
            }
            known_texts.push(lowercase_text);
        }
        Ok(aliases)
    }
}
