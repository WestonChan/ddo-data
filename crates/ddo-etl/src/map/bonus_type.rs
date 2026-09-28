use super::MAPPING;
use anyhow::{bail, Result};
use ddo_model::enums::BonusType;

pub fn normalize(raw: &str) -> Result<Option<BonusType>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    let canonical = MAPPING.bonus_type_aliases.get(raw).map(String::as_str).unwrap_or(raw);
    if canonical.is_empty() {
        return Ok(None);
    }
    match BonusType::parse(canonical) {
        Some(b) => Ok(Some(b)),
        None => bail!(
            "unknown bonus type {raw:?}; add it to data/buff_map.toml [bonus_type_aliases] or to ddo-model's BonusType"
        ),
    }
}
