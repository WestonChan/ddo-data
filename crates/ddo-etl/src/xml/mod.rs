//! Deserialisation of DDOBuilderV2 files. Struct and field names mirror the upstream element
//! names so a diff against a new upstream commit reads naturally; only Rust-keyword clashes
//! (`Type` → `kind`) are renamed.

pub mod item_buffs;
pub mod items;
pub mod patrons;
pub mod quests;

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

/// An element that carries no content, used as a presence flag (`<IsRaid/>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
pub struct Empty {}
