use super::effect::Effect;
use super::feats::Stance;
use super::{Dice, EmptyElement, NumberList};
use anyhow::Result;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};
use std::path::Path;

pub fn parse(path: &Path) -> Result<Vec<Spell>> {
    let file: SpellFile = super::parse_xml_file(path)?;
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
    pub maximum_caster_level: Option<i64>,
    pub cost: Option<i64>,
    pub metamagics: Vec<String>,
    pub effects: Vec<Effect>,
    pub stances: Vec<Stance>,
    pub damage_components: Vec<SpellDamage>,
    pub dcs: Vec<SpellDc>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SpellDice {
    #[serde(rename = "BaseDice")]
    pub base_dice: Option<Dice>,
    #[serde(rename = "PerCasterLevels")]
    pub per_caster_levels: Option<i64>,
    #[serde(rename = "BonusDice")]
    pub bonus_dice: Option<Dice>,
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
    pub fn base_dice(&self) -> Option<&Dice> {
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
    pub casting_stat_modifier_flag: Option<EmptyElement>,
    #[serde(rename = "Amount")]
    pub amount: Option<NumberList>,
    #[serde(rename = "ModAbility", default)]
    pub modifier_abilities: Vec<String>,
}

impl SpellDc {
    pub fn adds_casting_stat_modifier(&self) -> bool {
        self.casting_stat_modifier_flag.is_some()
    }
}

#[derive(Deserialize)]
struct RawSpell {
    #[serde(rename = "$value", default)]
    children: Vec<SpellChild>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Deserialize)]
enum SpellChild {
    Name(String),
    Description(String),
    Icon(String),
    School(String),
    MaxCasterLevel(i64),
    Cost(i64),
    Quicken(EmptyElement),
    Enlarge(EmptyElement),
    Maximize(EmptyElement),
    Empower(EmptyElement),
    Embolden(EmptyElement),
    Intensify(EmptyElement),
    Heighten(EmptyElement),
    Extend(EmptyElement),
    Accelerate(EmptyElement),
    EmpowerHealing(EmptyElement),
    Primer(EmptyElement),
    Effect(Effect),
    Stance(Stance),
    SpellDamage(SpellDamage),
    #[serde(rename = "SpellDC")]
    SpellDc(SpellDc),
}

impl<'de> Deserialize<'de> for Spell {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw_spell = RawSpell::deserialize(deserializer)?;
        let mut spell = Spell::default();
        let trimmed = |v: String| v.trim().to_string();
        let add_metamagic = |spell: &mut Spell, metamagic: &str| spell.metamagics.push(metamagic.to_string());
        for child in raw_spell.children {
            match child {
                SpellChild::Name(v) => spell.name = trimmed(v),
                SpellChild::Description(v) => spell.description = Some(trimmed(v)),
                SpellChild::Icon(v) => spell.icon = Some(trimmed(v)),
                SpellChild::School(v) => spell.schools.push(trimmed(v)),
                SpellChild::MaxCasterLevel(v) => spell.maximum_caster_level = Some(v),
                SpellChild::Cost(v) => spell.cost = Some(v),
                SpellChild::Quicken(_) => add_metamagic(&mut spell, "Quicken"),
                SpellChild::Enlarge(_) => add_metamagic(&mut spell, "Enlarge"),
                SpellChild::Maximize(_) => add_metamagic(&mut spell, "Maximize"),
                SpellChild::Empower(_) => add_metamagic(&mut spell, "Empower"),
                SpellChild::Embolden(_) => add_metamagic(&mut spell, "Embolden"),
                SpellChild::Intensify(_) => add_metamagic(&mut spell, "Intensify"),
                SpellChild::Heighten(_) => add_metamagic(&mut spell, "Heighten"),
                SpellChild::Extend(_) => add_metamagic(&mut spell, "Extend"),
                SpellChild::Accelerate(_) => add_metamagic(&mut spell, "Accelerate"),
                SpellChild::EmpowerHealing(_) => add_metamagic(&mut spell, "EmpowerHealing"),
                SpellChild::Primer(_) => add_metamagic(&mut spell, "Primer"),
                SpellChild::Effect(v) => spell.effects.push(v),
                SpellChild::Stance(v) => spell.stances.push(v),
                SpellChild::SpellDamage(v) => spell.damage_components.push(v),
                SpellChild::SpellDc(v) => spell.dcs.push(v),
            }
        }
        if spell.name.is_empty() {
            return Err(D::Error::custom("<Spell> without a <Name>"));
        }
        Ok(spell)
    }
}
