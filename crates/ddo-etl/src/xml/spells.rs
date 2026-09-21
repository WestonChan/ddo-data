//! `Spells.xml`: every castable spell with its schools, metamagic flags, damage dice and DCs.

use super::effect::Effect;
use super::feats::Stance;
use super::{Empty, Vector};
use anyhow::Result;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};
use std::path::Path;

pub fn parse(path: &Path) -> Result<Vec<Spell>> {
    let file: SpellFile = super::read_xml(path)?;
    Ok(file.spells)
}

#[derive(Deserialize)]
struct SpellFile {
    #[serde(rename = "Spell", default)]
    spells: Vec<Spell>,
}

#[derive(Debug, Default)]
pub struct Spell {
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub schools: Vec<String>,
    pub max_caster_level: Option<i64>,
    pub cost: Option<i64>,
    /// Metamagic flag elements present: Quicken, Enlarge, Maximize, Empower, Embolden, Intensify,
    /// Heighten, Extend, Accelerate, EmpowerHealing, Primer.
    pub metamagics: Vec<String>,
    pub effects: Vec<Effect>,
    pub stances: Vec<Stance>,
    pub damage: Vec<SpellDamage>,
    pub dcs: Vec<SpellDc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiceSpec {
    #[serde(rename = "Number")]
    pub number: Option<i64>,
    #[serde(rename = "Sides")]
    pub sides: Option<i64>,
    #[serde(rename = "Bonus")]
    pub bonus: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SpellDice {
    #[serde(rename = "BaseDice")]
    pub base_dice: Option<DiceSpec>,
    #[serde(rename = "PerCasterLevels")]
    pub per_caster_levels: Option<i64>,
    #[serde(rename = "BonusDice")]
    pub bonus_dice: Option<DiceSpec>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SpellDamage {
    #[serde(rename = "SpellDice", default)]
    pub dice: SpellDice,
    #[serde(rename = "Damage")]
    pub damage: Option<String>,
    #[serde(rename = "SpellPower")]
    pub spell_power: Option<String>,
}

impl SpellDamage {
    pub fn base_dice(&self) -> Option<&DiceSpec> {
        self.dice.base_dice.as_ref()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpellDc {
    #[serde(rename = "DCType")]
    pub dc_type: Option<String>,
    #[serde(rename = "DCVersus")]
    pub dc_versus: Option<String>,
    #[serde(rename = "School", default)]
    pub schools: Vec<String>,
    #[serde(rename = "CastingStatMod")]
    pub casting_stat_mod_flag: Option<Empty>,
    #[serde(rename = "Amount")]
    pub amount: Option<Vector>,
    #[serde(rename = "ModAbility", default)]
    pub mod_abilities: Vec<String>,
}

impl SpellDc {
    pub fn casting_stat_mod(&self) -> bool {
        self.casting_stat_mod_flag.is_some()
    }
}

#[derive(Deserialize)]
struct RawSpell {
    #[serde(rename = "$value", default)]
    children: Vec<SpellChild>,
}

// A transient parse buffer; the size skew between variants does not matter here.
#[allow(clippy::large_enum_variant)]
#[derive(Deserialize)]
enum SpellChild {
    Name(String),
    Description(String),
    Icon(String),
    School(String),
    MaxCasterLevel(i64),
    Cost(i64),
    Quicken(Empty),
    Enlarge(Empty),
    Maximize(Empty),
    Empower(Empty),
    Embolden(Empty),
    Intensify(Empty),
    Heighten(Empty),
    Extend(Empty),
    Accelerate(Empty),
    EmpowerHealing(Empty),
    Primer(Empty),
    Effect(Effect),
    Stance(Stance),
    SpellDamage(SpellDamage),
    #[serde(rename = "SpellDC")]
    SpellDc(SpellDc),
}

impl<'de> Deserialize<'de> for Spell {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = RawSpell::deserialize(d)?;
        let mut s = Spell::default();
        let trim = |v: String| v.trim().to_string();
        let meta = |s: &mut Spell, name: &str| s.metamagics.push(name.to_string());
        for child in raw.children {
            match child {
                SpellChild::Name(v) => s.name = trim(v),
                SpellChild::Description(v) => s.description = Some(trim(v)),
                SpellChild::Icon(v) => s.icon = Some(trim(v)),
                SpellChild::School(v) => s.schools.push(trim(v)),
                SpellChild::MaxCasterLevel(v) => s.max_caster_level = Some(v),
                SpellChild::Cost(v) => s.cost = Some(v),
                SpellChild::Quicken(_) => meta(&mut s, "Quicken"),
                SpellChild::Enlarge(_) => meta(&mut s, "Enlarge"),
                SpellChild::Maximize(_) => meta(&mut s, "Maximize"),
                SpellChild::Empower(_) => meta(&mut s, "Empower"),
                SpellChild::Embolden(_) => meta(&mut s, "Embolden"),
                SpellChild::Intensify(_) => meta(&mut s, "Intensify"),
                SpellChild::Heighten(_) => meta(&mut s, "Heighten"),
                SpellChild::Extend(_) => meta(&mut s, "Extend"),
                SpellChild::Accelerate(_) => meta(&mut s, "Accelerate"),
                SpellChild::EmpowerHealing(_) => meta(&mut s, "EmpowerHealing"),
                SpellChild::Primer(_) => meta(&mut s, "Primer"),
                SpellChild::Effect(v) => s.effects.push(v),
                SpellChild::Stance(v) => s.stances.push(v),
                SpellChild::SpellDamage(v) => s.damage.push(v),
                SpellChild::SpellDc(v) => s.dcs.push(v),
            }
        }
        if s.name.is_empty() {
            return Err(D::Error::custom("<Spell> without a <Name>"));
        }
        Ok(s)
    }
}
