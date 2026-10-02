pub mod augment_slot;
pub mod bonus_type;
pub mod buff;
pub mod drop_location;
pub mod effect;
pub mod item_version;
pub mod legacy_drop_source;
pub mod material;
pub mod placement;
pub mod source_alias;

use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::LazyLock;

#[derive(Debug, Deserialize)]
pub struct BuffVocabulary {
    pub enhancement: Vec<String>,
    pub fixed: BTreeMap<String, String>,
    pub by_item: BTreeMap<String, String>,
    pub item_aliases: BTreeMap<String, String>,
    pub bonus_type_aliases: BTreeMap<String, String>,
    pub weapon_aliases: BTreeMap<String, String>,
}

pub static BUFF_VOCABULARY: LazyLock<BuffVocabulary> =
    LazyLock::new(|| toml::from_str(include_str!("../../data/buff_map.toml")).expect("data/buff_map.toml is valid"));
