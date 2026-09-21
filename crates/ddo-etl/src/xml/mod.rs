//! Deserialisation of DDOBuilderV2 files. Struct and field names mirror the upstream element
//! names so a diff against a new upstream commit reads naturally; only Rust-keyword clashes
//! (`Type` → `kind`) are renamed.

pub mod augments;
pub mod classes;
pub mod clickies;
pub mod effect;
pub mod feats;
pub mod item_buffs;
pub mod items;
pub mod patrons;
pub mod quests;
pub mod races;
pub mod requirements;
pub mod set_bonuses;
pub mod spells;
pub mod trees;

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use std::path::Path;

/// Read a file and deserialise its root element. Upstream files may start with a UTF-8 BOM and
/// an XML declaration; both are handled.
pub(crate) fn read_xml<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    quick_xml::de::from_str(text).with_context(|| format!("parsing {}", path.display()))
}

/// `<Tag size="n">a b c</Tag>`: a whitespace-separated numeric vector. The `size` attribute is
/// ignored; the values are what count.
#[derive(Debug, Default, Clone, serde::Deserialize)]
pub struct Vector {
    #[serde(rename = "$text", default)]
    pub text: String,
}

impl Vector {
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

/// An element that carries no content, used as a presence flag (`<IsRaid/>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
pub struct Empty {}
