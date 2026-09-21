//! `<Effect>`: the modifier grammar every family shares. Read as an order-independent child
//! sequence because `<Type>` and `<Item>` repeat and upstream interleaves freely. Every child
//! element seen in the 2026-09-20 survey has a variant; a new one is a hard error.

use super::requirements::Requirements;
use super::Empty;
use anyhow::Result;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};

pub fn parse_effect(xml: &str) -> Result<Effect> {
    Ok(quick_xml::de::from_str(xml)?)
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Effect {
    /// One or more `<Type>`; the first is the primary.
    pub types: Vec<String>,
    pub bonus: Option<String>,
    pub amount_type: Option<String>,
    /// `<Amount size="n">a b c</Amount>` as numbers. Empty when absent.
    pub amounts: Vec<f64>,
    pub items: Vec<String>,
    pub value: Option<String>,
    pub dice: Option<Dice>,
    pub damage: Option<String>,
    pub percent: bool,
    pub rank: Option<i64>,
    pub cap: Option<String>,
    pub stack_source: Option<String>,
    pub display_name: Option<String>,
    pub apply_as_item_effect: bool,
    pub is_item_specific: bool,
    pub rare: bool,
    pub update_automatic_effects: bool,
    pub requirements: Option<Requirements>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Dice {
    pub number: Vec<f64>,
    pub sides: Vec<f64>,
    pub bonus: Vec<f64>,
    pub damage: Option<String>,
}

/// `<Amount size="4">3.5 4.0 4.5 4.5</Amount>`: whitespace-separated numbers.
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
struct RawDice {
    #[serde(rename = "Number")]
    number: Option<Vector>,
    #[serde(rename = "Sides")]
    sides: Option<Vector>,
    #[serde(rename = "Bonus")]
    bonus: Option<Vector>,
    #[serde(rename = "Damage")]
    damage: Option<String>,
}

#[derive(Deserialize)]
struct RawEffect {
    #[serde(rename = "$value", default)]
    children: Vec<EffectChild>,
}

#[derive(Deserialize)]
enum EffectChild {
    Type(String),
    Bonus(String),
    AType(String),
    Amount(Vector),
    Item(String),
    Value(String),
    Dice(RawDice),
    Damage(String),
    Percent(Empty),
    Rank(i64),
    Cap(String),
    StackSource(String),
    DisplayName(String),
    ApplyAsItemEffect(Empty),
    IsItemSpecific(Empty),
    Rare(Empty),
    UpdateAutomaticEffects(Empty),
    Requirements(Requirements),
}

impl<'de> Deserialize<'de> for Effect {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = RawEffect::deserialize(d)?;
        let mut e = Effect::default();
        let trimmed = |s: String| s.trim().to_string();
        for child in raw.children {
            match child {
                EffectChild::Type(v) => e.types.push(trimmed(v)),
                EffectChild::Bonus(v) => e.bonus = Some(trimmed(v)),
                EffectChild::AType(v) => e.amount_type = Some(trimmed(v)),
                EffectChild::Amount(v) => e.amounts = v.numbers().map_err(D::Error::custom)?,
                EffectChild::Item(v) => e.items.push(trimmed(v)),
                EffectChild::Value(v) => e.value = Some(trimmed(v)),
                EffectChild::Dice(d) => {
                    e.dice = Some(Dice {
                        number: d
                            .number
                            .map(|v| v.numbers())
                            .transpose()
                            .map_err(D::Error::custom)?
                            .unwrap_or_default(),
                        sides: d.sides.map(|v| v.numbers()).transpose().map_err(D::Error::custom)?.unwrap_or_default(),
                        bonus: d.bonus.map(|v| v.numbers()).transpose().map_err(D::Error::custom)?.unwrap_or_default(),
                        damage: d.damage.map(trimmed),
                    })
                }
                EffectChild::Damage(v) => e.damage = Some(trimmed(v)),
                EffectChild::Percent(_) => e.percent = true,
                EffectChild::Rank(v) => e.rank = Some(v),
                EffectChild::Cap(v) => e.cap = Some(trimmed(v)),
                EffectChild::StackSource(v) => e.stack_source = Some(trimmed(v)),
                EffectChild::DisplayName(v) => e.display_name = Some(trimmed(v)),
                EffectChild::ApplyAsItemEffect(_) => e.apply_as_item_effect = true,
                EffectChild::IsItemSpecific(_) => e.is_item_specific = true,
                EffectChild::Rare(_) => e.rare = true,
                EffectChild::UpdateAutomaticEffects(_) => e.update_automatic_effects = true,
                EffectChild::Requirements(r) => e.requirements = Some(r),
            }
        }
        if e.types.is_empty() {
            return Err(D::Error::custom("<Effect> without a <Type>"));
        }
        Ok(e)
    }
}

impl Effect {
    /// A plain number: `AType` Simple, one `Type`, exactly one integral amount.
    pub fn simple_integer(&self) -> Option<i64> {
        if self.amount_type.as_deref() != Some("Simple") || self.types.len() != 1 || self.amounts.len() != 1 {
            return None;
        }
        let v = self.amounts[0];
        (v.fract() == 0.0).then_some(v as i64)
    }
}
