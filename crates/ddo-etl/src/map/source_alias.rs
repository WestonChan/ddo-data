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

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceAliases {
    #[serde(default, rename = "challenge")]
    pub challenges: Vec<ChallengeAlias>,
    #[serde(default, rename = "crafting_system")]
    pub crafting_systems: Vec<CraftingSystemAlias>,
}

impl SourceAliases {
    pub fn embedded() -> Result<Self> {
        Self::from_toml_str(EMBEDDED_FILE).context(EMBEDDED_FILE_NAME)
    }

    pub fn from_toml_str(text: &str) -> Result<Self> {
        let aliases: Self = toml::from_str(text)?;
        let challenge_aliases =
            aliases.challenges.iter().map(|alias| ("challenge", &alias.text, &alias.pack, &alias.reason));
        let crafting_system_aliases =
            aliases.crafting_systems.iter().map(|alias| ("crafting_system", &alias.text, &alias.system, &alias.reason));
        let mut known_texts: Vec<String> = Vec::new();
        for (table_name, text, target_name, reason) in challenge_aliases.chain(crafting_system_aliases) {
            let lowercase_text = text.trim().to_lowercase();
            if lowercase_text.is_empty() || target_name.trim().is_empty() || reason.trim().is_empty() {
                bail!("{table_name} alias {text:?}: no field may be blank");
            }
            if known_texts.contains(&lowercase_text) {
                bail!("alias text {text:?} appears twice");
            }
            known_texts.push(lowercase_text);
        }
        Ok(aliases)
    }
}
