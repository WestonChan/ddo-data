use super::requirements::Requirements;
use super::{EmptyElement, NumberList};
use anyhow::Result;
use serde::de::{Error as _, IgnoredAny};
use serde::{Deserialize, Deserializer};

pub fn parse_effect(xml: &str) -> Result<Effect> {
    Ok(quick_xml::de::from_str(xml)?)
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Effect {
    pub types: Vec<String>,
    pub bonus: Option<String>,
    pub amount_type: Option<String>,
    pub amounts: Vec<f64>,
    pub targets: Vec<String>,
    pub value: Option<String>,
    pub dice: Option<EffectDice>,
    pub damage: Option<String>,
    pub is_percent: bool,
    pub rank: Option<i64>,
    pub cap: Option<String>,
    pub stack_source: Option<String>,
    pub display_name: Option<String>,
    pub applies_as_item_effect: bool,
    pub is_item_specific: bool,
    pub is_rare: bool,
    pub updates_automatic_effects: bool,
    pub requirements: Option<Requirements>,
    pub has_single_requirement: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EffectDice {
    pub counts: Vec<f64>,
    pub sides: Vec<f64>,
    pub bonuses: Vec<f64>,
    pub damage: Option<String>,
}

#[derive(Deserialize)]
struct RawEffectDice {
    #[serde(rename = "Number")]
    counts: Option<NumberList>,
    #[serde(rename = "Sides")]
    sides: Option<NumberList>,
    #[serde(rename = "Bonus")]
    bonuses: Option<NumberList>,
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
    Amount(NumberList),
    Item(String),
    Value(String),
    Dice(RawEffectDice),
    Damage(String),
    Percent(EmptyElement),
    Rank(i64),
    Cap(String),
    StackSource(String),
    DisplayName(String),
    ApplyAsItemEffect(EmptyElement),
    IsItemSpecific(EmptyElement),
    Rare(EmptyElement),
    UpdateAutomaticEffects(EmptyElement),
    Requirements(Requirements),
    Requirement(IgnoredAny),
}

fn numbers_or_empty(number_list: Option<NumberList>) -> Result<Vec<f64>, String> {
    Ok(number_list.map(|l| l.numbers()).transpose()?.unwrap_or_default())
}

impl<'de> Deserialize<'de> for Effect {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw_effect = RawEffect::deserialize(deserializer)?;
        let mut effect = Effect::default();
        let trimmed = |s: String| s.trim().to_string();
        for child in raw_effect.children {
            match child {
                EffectChild::Type(v) => effect.types.push(trimmed(v)),
                EffectChild::Bonus(v) => effect.bonus = Some(trimmed(v)),
                EffectChild::AType(v) => effect.amount_type = Some(trimmed(v)),
                EffectChild::Amount(v) => effect.amounts = v.numbers().map_err(D::Error::custom)?,
                EffectChild::Item(v) => effect.targets.push(trimmed(v)),
                EffectChild::Value(v) => effect.value = Some(trimmed(v)),
                EffectChild::Dice(raw_dice) => {
                    effect.dice = Some(EffectDice {
                        counts: numbers_or_empty(raw_dice.counts).map_err(D::Error::custom)?,
                        sides: numbers_or_empty(raw_dice.sides).map_err(D::Error::custom)?,
                        bonuses: numbers_or_empty(raw_dice.bonuses).map_err(D::Error::custom)?,
                        damage: raw_dice.damage.map(trimmed),
                    })
                }
                EffectChild::Damage(v) => effect.damage = Some(trimmed(v)),
                EffectChild::Percent(_) => effect.is_percent = true,
                EffectChild::Rank(v) => effect.rank = Some(v),
                EffectChild::Cap(v) => effect.cap = Some(trimmed(v)),
                EffectChild::StackSource(v) => effect.stack_source = Some(trimmed(v)),
                EffectChild::DisplayName(v) => effect.display_name = Some(trimmed(v)),
                EffectChild::ApplyAsItemEffect(_) => effect.applies_as_item_effect = true,
                EffectChild::IsItemSpecific(_) => effect.is_item_specific = true,
                EffectChild::Rare(_) => effect.is_rare = true,
                EffectChild::UpdateAutomaticEffects(_) => effect.updates_automatic_effects = true,
                EffectChild::Requirements(v) => effect.requirements = Some(v),
                EffectChild::Requirement(_) => effect.has_single_requirement = true,
            }
        }
        if effect.types.is_empty() {
            return Err(D::Error::custom("<Effect> without a <Type>"));
        }
        Ok(effect)
    }
}

impl Effect {
    pub fn simple_integer_amount(&self) -> Option<i64> {
        if self.amount_type.as_deref() != Some("Simple") || self.types.len() != 1 || self.amounts.len() != 1 {
            return None;
        }
        let amount = self.amounts[0];
        (amount.fract() == 0.0).then_some(amount as i64)
    }

    pub fn plain_integer_amount(&self) -> Option<i64> {
        if self.value.is_some()
            || self.dice.is_some()
            || self.damage.is_some()
            || self.is_percent
            || self.rank.is_some()
            || self.cap.is_some()
            || self.stack_source.is_some()
            || self.display_name.is_some()
            || self.applies_as_item_effect
            || self.is_item_specific
            || self.is_rare
            || self.updates_automatic_effects
            || self.requirements.is_some()
            || self.has_single_requirement
        {
            return None;
        }
        self.simple_integer_amount()
    }
}
