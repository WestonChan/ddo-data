pub mod augments;
pub mod challenges;
pub mod classes;
pub mod clickies;
pub mod effect;
pub mod feats;
pub mod guild_buffs;
pub mod item_buffs;
pub mod items;
pub mod optional_buffs;
pub mod patrons;
pub mod quests;
pub mod races;
pub mod requirements;
pub mod sentient_gems;
pub mod set_bonuses;
pub mod spells;
pub mod stances;
pub mod trees;

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use std::path::Path;

pub(crate) fn parse_xml_file<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let file_text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let file_text = file_text.strip_prefix('\u{feff}').unwrap_or(&file_text);
    quick_xml::de::from_str(file_text).with_context(|| format!("parsing {}", path.display()))
}

#[derive(Debug, Default, Clone, serde::Deserialize)]
pub struct NumberList {
    #[serde(rename = "$text", default)]
    pub text: String,
}

impl NumberList {
    pub fn numbers(&self) -> Result<Vec<f64>, String> {
        self.text.split_whitespace().map(|n| n.parse::<f64>().map_err(|e| format!("{n:?}: {e}"))).collect()
    }

    pub fn integers(&self) -> Result<Vec<i64>, String> {
        self.numbers()?
            .into_iter()
            .map(|v| if v.fract() == 0.0 { Ok(v as i64) } else { Err(format!("{v} is not an integer")) })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
pub struct EmptyElement {}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Dice {
    #[serde(rename = "Number")]
    pub count: Option<i64>,
    #[serde(rename = "Sides")]
    pub sides: Option<i64>,
    #[serde(rename = "Bonus")]
    pub bonus: Option<i64>,
}
