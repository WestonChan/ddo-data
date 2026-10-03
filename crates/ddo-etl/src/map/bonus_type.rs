use super::enchantment::ENCHANTMENT_MAP;
use anyhow::Result;
use ddo_model::enums::BonusType;

pub fn parse_buff_bonus_type(upstream_name: &str) -> Result<Option<BonusType>> {
    ENCHANTMENT_MAP.family_bonus_type(upstream_name)
}
