//! `Augments/*.xml`: one `<Augments>` root per crafting family, `<Augment>` children read as an
//! order-independent sequence (`<EffectDescription>` interleaves with `<Effect>`).

use super::effect::Effect;
use super::Empty;
use anyhow::Result;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};
use std::path::Path;

/// Parse one family file. `family` is the file stem before `.Augments`.
pub fn parse_augments_file(path: &Path) -> Result<(String, Vec<Augment>)> {
    let file: AugmentFile = super::read_xml(path)?;
    let stem = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let family = stem.split(".Augments").next().unwrap_or(stem).to_string();
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
    /// `<EffectDescription>` elements, in order.
    pub effect_descriptions: Vec<String>,
    pub min_level: Option<i64>,
    /// `<Type>`: every slot type this augment fits.
    pub slot_types: Vec<String>,
    pub icon: Option<String>,
    pub effects: Vec<Effect>,
    pub choose_level: bool,
    pub levels: Vec<f64>,
    pub level_values: Vec<f64>,
    pub level_values2: Vec<f64>,
    pub dual_values: bool,
    pub enter_value: bool,
    pub suppress_set_bonus: bool,
    pub set_bonus: Vec<String>,
    pub add_augment: Vec<String>,
    pub grant_augment: Option<String>,
    pub grant_conditional_augment: Option<String>,
    pub weapon_class: Option<String>,
}

/// `<Levels size="3">12 16 20</Levels>`; `LevelValue` can be fractional ("1.5").
#[derive(Deserialize)]
struct Vector {
    #[serde(rename = "$text", default)]
    text: String,
}

impl Vector {
    fn numbers(&self) -> Result<Vec<f64>, String> {
        self.text.split_whitespace().map(|n| n.parse::<f64>().map_err(|e| format!("{n:?}: {e}"))).collect()
    }
}

#[derive(Deserialize)]
struct RawAugment {
    #[serde(rename = "$value", default)]
    children: Vec<AugmentChild>,
}

// A transient parse buffer; the size skew between variants does not matter here.
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
    ChooseLevel(Empty),
    Levels(Vector),
    LevelValue(Vector),
    LevelValue2(Vector),
    DualValues(Empty),
    EnterValue(Empty),
    SuppressSetBonus(Empty),
    SetBonus(String),
    AddAugment(String),
    GrantAugment(String),
    GrantConditionalAugment(String),
    WeaponClass(String),
}

impl<'de> Deserialize<'de> for Augment {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = RawAugment::deserialize(d)?;
        let mut a = Augment::default();
        let t = |s: String| s.trim().to_string();
        for child in raw.children {
            match child {
                AugmentChild::Name(v) => a.name = t(v),
                AugmentChild::Description(v) => a.description = Some(t(v)),
                AugmentChild::EffectDescription(v) => a.effect_descriptions.push(t(v)),
                AugmentChild::MinLevel(v) => a.min_level = Some(v),
                AugmentChild::Type(v) => a.slot_types.push(t(v)),
                AugmentChild::Icon(v) => a.icon = Some(t(v)),
                AugmentChild::Effect(e) => a.effects.push(e),
                AugmentChild::ChooseLevel(_) => a.choose_level = true,
                AugmentChild::Levels(v) => a.levels = v.numbers().map_err(D::Error::custom)?,
                AugmentChild::LevelValue(v) => a.level_values = v.numbers().map_err(D::Error::custom)?,
                AugmentChild::LevelValue2(v) => a.level_values2 = v.numbers().map_err(D::Error::custom)?,
                AugmentChild::DualValues(_) => a.dual_values = true,
                AugmentChild::EnterValue(_) => a.enter_value = true,
                AugmentChild::SuppressSetBonus(_) => a.suppress_set_bonus = true,
                AugmentChild::SetBonus(v) => a.set_bonus.push(t(v)),
                AugmentChild::AddAugment(v) => a.add_augment.push(t(v)),
                AugmentChild::GrantAugment(v) => a.grant_augment = Some(t(v)),
                AugmentChild::GrantConditionalAugment(v) => a.grant_conditional_augment = Some(t(v)),
                AugmentChild::WeaponClass(v) => a.weapon_class = Some(t(v)),
            }
        }
        if a.name.is_empty() {
            return Err(D::Error::custom("<Augment> without a <Name>"));
        }
        Ok(a)
    }
}
