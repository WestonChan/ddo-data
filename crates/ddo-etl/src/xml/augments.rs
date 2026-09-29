use super::effect::Effect;
use super::{EmptyElement, NumberList};
use anyhow::Result;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};
use std::path::Path;

pub fn parse_augments_file(path: &Path) -> Result<(String, Vec<Augment>)> {
    let file: AugmentFile = super::parse_xml_file(path)?;
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let family = file_name.split(".Augments").next().unwrap_or(file_name).to_string();
    Ok((family, file.augments))
}

#[derive(Deserialize)]
struct AugmentFile {
    #[serde(rename = "Augment", default)]
    augments: Vec<Augment>,
}

#[derive(Debug, Default, Clone)]
pub struct Augment {
    pub name: String,
    pub description: Option<String>,
    pub effect_descriptions: Vec<String>,
    pub minimum_level: Option<i64>,
    pub slot_types: Vec<String>,
    pub icon: Option<String>,
    pub effects: Vec<Effect>,
    pub has_selectable_level: bool,
    pub levels: Vec<f64>,
    pub level_values: Vec<f64>,
    pub second_level_values: Vec<f64>,
    pub has_dual_values: bool,
    pub has_enterable_value: bool,
    pub suppresses_set_bonus: bool,
    pub set_bonus_names: Vec<String>,
    pub added_augments: Vec<String>,
    pub granted_augment: Option<String>,
    pub granted_conditional_augment: Option<String>,
    pub weapon_class: Option<String>,
}

#[derive(Deserialize)]
struct RawAugment {
    #[serde(rename = "$value", default)]
    children: Vec<AugmentChild>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Deserialize)]
enum AugmentChild {
    Name(String),
    Description(String),
    EffectDescription(String),
    MinLevel(i64),
    Type(String),
    Icon(String),
    Effect(Effect),
    ChooseLevel(EmptyElement),
    Levels(NumberList),
    LevelValue(NumberList),
    LevelValue2(NumberList),
    DualValues(EmptyElement),
    EnterValue(EmptyElement),
    SuppressSetBonus(EmptyElement),
    SetBonus(String),
    AddAugment(String),
    GrantAugment(String),
    GrantConditionalAugment(String),
    WeaponClass(String),
}

impl<'de> Deserialize<'de> for Augment {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw_augment = RawAugment::deserialize(deserializer)?;
        let mut augment = Augment::default();
        let trimmed = |s: String| s.trim().to_string();
        for child in raw_augment.children {
            match child {
                AugmentChild::Name(v) => augment.name = trimmed(v),
                AugmentChild::Description(v) => augment.description = Some(trimmed(v)),
                AugmentChild::EffectDescription(v) => augment.effect_descriptions.push(trimmed(v)),
                AugmentChild::MinLevel(v) => augment.minimum_level = Some(v),
                AugmentChild::Type(v) => augment.slot_types.push(trimmed(v)),
                AugmentChild::Icon(v) => augment.icon = Some(trimmed(v)),
                AugmentChild::Effect(e) => augment.effects.push(e),
                AugmentChild::ChooseLevel(_) => augment.has_selectable_level = true,
                AugmentChild::Levels(v) => augment.levels = v.numbers().map_err(D::Error::custom)?,
                AugmentChild::LevelValue(v) => augment.level_values = v.numbers().map_err(D::Error::custom)?,
                AugmentChild::LevelValue2(v) => augment.second_level_values = v.numbers().map_err(D::Error::custom)?,
                AugmentChild::DualValues(_) => augment.has_dual_values = true,
                AugmentChild::EnterValue(_) => augment.has_enterable_value = true,
                AugmentChild::SuppressSetBonus(_) => augment.suppresses_set_bonus = true,
                AugmentChild::SetBonus(v) => augment.set_bonus_names.push(trimmed(v)),
                AugmentChild::AddAugment(v) => augment.added_augments.push(trimmed(v)),
                AugmentChild::GrantAugment(v) => augment.granted_augment = Some(trimmed(v)),
                AugmentChild::GrantConditionalAugment(v) => augment.granted_conditional_augment = Some(trimmed(v)),
                AugmentChild::WeaponClass(v) => augment.weapon_class = Some(trimmed(v)),
            }
        }
        if augment.name.is_empty() {
            return Err(D::Error::custom("<Augment> without a <Name>"));
        }
        Ok(augment)
    }
}
