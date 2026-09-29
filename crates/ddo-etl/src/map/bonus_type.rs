use super::BUFF_VOCABULARY;
use anyhow::{bail, Result};
use ddo_model::enums::BonusType;

pub fn parse_buff_bonus_type(upstream_name: &str) -> Result<Option<BonusType>> {
    let upstream_name = upstream_name.trim();
    if upstream_name.is_empty() {
        return Ok(None);
    }
    let canonical_name =
        BUFF_VOCABULARY.bonus_type_aliases.get(upstream_name).map(String::as_str).unwrap_or(upstream_name);
    if canonical_name.is_empty() {
        return Ok(None);
    }
    match BonusType::parse(canonical_name) {
        Some(bonus_type) => Ok(Some(bonus_type)),
        None => bail!(
            "unknown bonus type {upstream_name:?}; add it to data/buff_map.toml [bonus_type_aliases] or to ddo-model's BonusType"
        ),
    }
}
