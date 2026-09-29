use super::effect::Effect;
use super::requirements::{RawRequirementGroup, Requirement, RequirementGroup, Requirements};
use super::{EmptyElement, NumberList};
use anyhow::Result;
use ddo_model::enums::RequirementGroupKind;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};
use std::path::Path;

pub fn parse(path: &Path) -> Result<Vec<Feat>> {
    let file: FeatFile = super::parse_xml_file(path)?;
    Ok(file.feats)
}

#[derive(Deserialize)]
struct FeatFile {
    #[serde(rename = "Feat", default)]
    feats: Vec<Feat>,
}

#[derive(Debug, Default, Clone)]
pub struct Feat {
    pub name: String,
    pub description: Option<String>,
    pub acquire: Option<String>,
    pub icon: Option<String>,
    pub maximum_times_acquired: Option<i64>,
    pub sphere: Option<String>,
    pub groups: Vec<String>,
    pub requirements: Option<Requirements>,
    pub effects: Vec<Effect>,
    pub stances: Vec<Stance>,
    pub dcs: Vec<Dc>,
    pub conditional_groups: Vec<ConditionalGroup>,
    pub automatic_acquisition: Option<AutomaticAcquisition>,
    pub attack: Option<Attack>,
    pub sub_items: Vec<SubItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Stance {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Icon")]
    pub icon: Option<String>,
    #[serde(rename = "Description")]
    pub description: Option<String>,
    #[serde(rename = "Group")]
    pub group: Option<String>,
    #[serde(rename = "AutoControlled")]
    pub auto_controlled: Option<EmptyElement>,
    #[serde(rename = "Requirements")]
    pub requirements: Option<Requirements>,
    #[serde(rename = "IncompatibleStance", default)]
    pub incompatible_stances: Vec<String>,
    #[serde(rename = "Effect", default)]
    pub effects: Vec<Effect>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Dc {
    #[serde(rename = "Name")]
    pub name: Option<String>,
    #[serde(rename = "Description")]
    pub description: Option<String>,
    #[serde(rename = "Icon")]
    pub icon: Option<String>,
    #[serde(rename = "DCType")]
    pub dc_type: Option<String>,
    #[serde(rename = "DCVersus")]
    pub dc_versus: Option<String>,
    #[serde(rename = "ModAbility", default)]
    pub modifier_abilities: Vec<String>,
    #[serde(rename = "Amount")]
    pub amount: Option<NumberList>,
    #[serde(rename = "Tactical")]
    pub tactical: Option<String>,
    #[serde(rename = "Other")]
    pub other: Option<String>,
    #[serde(rename = "Skill")]
    pub skill: Option<String>,
    #[serde(rename = "ClassLevel")]
    pub class_level: Option<String>,
    #[serde(rename = "BaseClassLevel")]
    pub base_class_level: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Attack {
    #[serde(rename = "Name")]
    pub name: Option<String>,
    #[serde(rename = "Description")]
    pub description: Option<String>,
    #[serde(rename = "Icon")]
    pub icon: Option<String>,
    #[serde(rename = "Cooldown", default, deserialize_with = "first_integer")]
    pub cooldown_seconds: Option<i64>,
    #[serde(rename = "ThisAttack")]
    pub this_attack: Option<AttackBonuses>,
    #[serde(rename = "FollowOn")]
    pub follow_on: Option<AttackBonuses>,
}

impl Attack {
    pub fn duration_seconds(&self) -> Option<i64> {
        self.follow_on.as_ref().and_then(|f| f.duration_seconds)
    }

    pub fn follow_on_effects(&self) -> &[Effect] {
        self.follow_on.as_ref().map_or(&[], |f| &f.effects)
    }

    pub fn this_attack_effects(&self) -> &[Effect] {
        self.this_attack.as_ref().map_or(&[], |t| &t.effects)
    }
}

#[derive(Debug, Clone, Default)]
pub struct AttackBonuses {
    pub duration_seconds: Option<i64>,
    pub effects: Vec<Effect>,
}

fn first_integer<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<i64>, D::Error> {
    let Some(numbers) = Option::<NumberList>::deserialize(deserializer)? else {
        return Ok(None);
    };
    Ok(numbers.integers().map_err(D::Error::custom)?.first().copied())
}

fn attack_bonus_effect(bonus_name: String, amounts: Vec<f64>) -> Effect {
    let amount_type = match amounts.len() {
        0 => "NotNeeded",
        1 => "Simple",
        _ => "Stacks",
    };
    Effect { types: vec![bonus_name], amount_type: Some(amount_type.into()), amounts, ..Effect::default() }
}

impl<'de> Deserialize<'de> for AttackBonuses {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct AttackBonusesVisitor;
        impl<'de> serde::de::Visitor<'de> for AttackBonusesVisitor {
            type Value = AttackBonuses;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a <ThisAttack> or <FollowOn> of attack bonus vectors")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut elements: A) -> Result<AttackBonuses, A::Error> {
                let mut bonuses = AttackBonuses::default();
                while let Some(element_name) = elements.next_key::<String>()? {
                    let numbers: NumberList = elements.next_value()?;
                    if element_name == "Duration" {
                        bonuses.duration_seconds = numbers.integers().map_err(A::Error::custom)?.first().copied();
                    } else {
                        let amounts = numbers.numbers().map_err(A::Error::custom)?;
                        bonuses.effects.push(attack_bonus_effect(element_name, amounts));
                    }
                }
                Ok(bonuses)
            }
        }
        deserializer.deserialize_map(AttackBonusesVisitor)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubItem {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Icon")]
    pub icon: Option<String>,
    #[serde(rename = "Description")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConditionalGroup {
    #[serde(rename = "Group", default)]
    pub groups: Vec<String>,
    #[serde(rename = "Requirements")]
    pub requirements: Option<Requirements>,
}

#[derive(Debug, Clone, Default)]
pub struct AutomaticAcquisition {
    pub requirements: Requirements,
    pub ignores_requirements: bool,
}

#[derive(Deserialize)]
struct RawAutomaticAcquisition {
    #[serde(rename = "$value", default)]
    children: Vec<AutomaticAcquisitionChild>,
}

#[derive(Deserialize)]
enum AutomaticAcquisitionChild {
    Requirement(Requirement),
    RequiresOneOf(RawRequirementGroup),
    RequiresNoneOf(RawRequirementGroup),
    IgnoreRequirements(EmptyElement),
}

impl<'de> Deserialize<'de> for AutomaticAcquisition {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw_acquisition = RawAutomaticAcquisition::deserialize(deserializer)?;
        let mut acquisition = AutomaticAcquisition::default();
        let mut ungrouped_requirements = Vec::new();
        for child in raw_acquisition.children {
            match child {
                AutomaticAcquisitionChild::Requirement(r) => ungrouped_requirements.push(r),
                AutomaticAcquisitionChild::RequiresOneOf(g) => {
                    acquisition.requirements.groups.push(RequirementGroup::from_raw(RequirementGroupKind::OneOf, g))
                }
                AutomaticAcquisitionChild::RequiresNoneOf(g) => {
                    acquisition.requirements.groups.push(RequirementGroup::from_raw(RequirementGroupKind::NoneOf, g))
                }
                AutomaticAcquisitionChild::IgnoreRequirements(_) => acquisition.ignores_requirements = true,
            }
        }
        if !ungrouped_requirements.is_empty() {
            acquisition.requirements.groups.insert(0, RequirementGroup::all_of(ungrouped_requirements));
        }
        Ok(acquisition)
    }
}

#[derive(Deserialize)]
struct RawFeat {
    #[serde(rename = "$value", default)]
    children: Vec<FeatChild>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Deserialize)]
enum FeatChild {
    Name(String),
    Description(String),
    Acquire(String),
    Icon(String),
    MaxTimesAcquire(i64),
    Sphere(String),
    Group(String),
    Requirements(Requirements),
    Effect(Effect),
    Stance(Stance),
    #[serde(rename = "DC")]
    Dc(Dc),
    ConditionalGroup(ConditionalGroup),
    AutomaticAcquisition(AutomaticAcquisition),
    Attack(Attack),
    SubItem(SubItem),
}

impl<'de> Deserialize<'de> for Feat {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw_feat = RawFeat::deserialize(deserializer)?;
        let mut feat = Feat::default();
        let trimmed = |s: String| s.trim().to_string();
        for child in raw_feat.children {
            match child {
                FeatChild::Name(v) => feat.name = trimmed(v),
                FeatChild::Description(v) => feat.description = Some(trimmed(v)),
                FeatChild::Acquire(v) => feat.acquire = Some(trimmed(v)),
                FeatChild::Icon(v) => feat.icon = Some(trimmed(v)),
                FeatChild::MaxTimesAcquire(v) => feat.maximum_times_acquired = Some(v),
                FeatChild::Sphere(v) => feat.sphere = Some(trimmed(v)),
                FeatChild::Group(v) => feat.groups.push(trimmed(v)),
                FeatChild::Requirements(v) => feat.requirements = Some(v),
                FeatChild::Effect(v) => feat.effects.push(v),
                FeatChild::Stance(v) => feat.stances.push(v),
                FeatChild::Dc(v) => feat.dcs.push(v),
                FeatChild::ConditionalGroup(v) => feat.conditional_groups.push(v),
                FeatChild::AutomaticAcquisition(v) => feat.automatic_acquisition = Some(v),
                FeatChild::Attack(v) => feat.attack = Some(v),
                FeatChild::SubItem(v) => feat.sub_items.push(v),
            }
        }
        if feat.name.is_empty() {
            return Err(D::Error::custom("<Feat> without a <Name>"));
        }
        Ok(feat)
    }
}
