//! Upstream vocabulary → ours. The tables live in `data/buff_map.toml` and are loaded once.

pub mod augment_slot;
pub mod bonus_type;
pub mod buff;
pub mod effect;
pub mod material;
pub mod placement;

use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::LazyLock;

/// The whole mapping file. Kept as one document so a reader sees every rule in one place.
#[derive(Debug, Deserialize)]
pub struct MappingData {
    /// Buff types whose `Value1` is the item's `+N` enhancement bonus, not a stat bonus.
    pub enhancement: Vec<String>,
    /// Buff types whose stat is a fixed name regardless of `<Item>`.
    pub fixed: BTreeMap<String, String>,
    /// Buff types whose stat name is a template over `<Item>` (`"{item} Resistance"`).
    pub by_item: BTreeMap<String, String>,
    /// `<Item>` sub-target spellings that differ from the word our stat names use ("Lawful" → "Law").
    pub item_aliases: BTreeMap<String, String>,
    /// Upstream `BonusType` spellings that are not our canonical name. An empty value means NULL.
    pub bonus_type_aliases: BTreeMap<String, String>,
    /// `<Weapon>` spellings that differ from `weapon_types.name`.
    pub weapon_aliases: BTreeMap<String, String>,
}

pub static MAPPING: LazyLock<MappingData> =
    LazyLock::new(|| toml::from_str(include_str!("../../data/buff_map.toml")).expect("data/buff_map.toml is valid"));
