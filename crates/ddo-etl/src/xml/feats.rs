use super::effect::Effect;
use super::requirements::{RawGroup, Requirement, RequirementGroup, RequirementGroupKind, Requirements};
use super::{Empty, Vector};
use anyhow::Result;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};
use std::path::Path;

pub fn parse(path: &Path) -> Result<Vec<Feat>> {
    let file: FeatFile = super::read_xml(path)?;
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
    pub max_times_acquire: Option<i64>,
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
    pub auto_controlled: Option<Empty>,
    #[serde(rename = "Requirements")]
    pub requirements: Option<Requirements>,
    #[serde(rename = "IncompatibleStance", default)]
    pub incompatible: Vec<String>,
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
    pub mod_ability: Vec<String>,
    #[serde(rename = "Amount")]
    pub amount: Option<Vector>,
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
    #[serde(rename = "FollowOn")]
    pub follow_on: Option<FollowOn>,
}

#[derive(Debug, Clone, Default)]
pub struct FollowOn {
    pub duration_seconds: Option<i64>,
    pub effects: Vec<Effect>,
}

fn first_integer<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    let Some(vector) = Option::<Vector>::deserialize(d)? else {
        return Ok(None);
    };
    Ok(vector.integers().map_err(D::Error::custom)?.first().copied())
}

fn attack_bonus_effect(name: String, amounts: Vec<f64>) -> Effect {
    let amount_type = match amounts.len() {
        0 => "NotNeeded",
        1 => "Simple",
        _ => "Stacks",
    };
    Effect { types: vec![name], amount_type: Some(amount_type.into()), amounts, ..Effect::default() }
}

impl<'de> Deserialize<'de> for FollowOn {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct FollowOnVisitor;
        impl<'de> serde::de::Visitor<'de> for FollowOnVisitor {
            type Value = FollowOn;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a <FollowOn> of attack bonus vectors")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<FollowOn, A::Error> {
                let mut follow_on = FollowOn::default();
                while let Some(name) = map.next_key::<String>()? {
                    let vector: Vector = map.next_value()?;
                    if name == "Duration" {
                        follow_on.duration_seconds = vector.integers().map_err(A::Error::custom)?.first().copied();
                    } else {
                        let amounts = vector.numbers().map_err(A::Error::custom)?;
                        follow_on.effects.push(attack_bonus_effect(name, amounts));
                    }
                }
                Ok(follow_on)
            }
        }
        d.deserialize_map(FollowOnVisitor)
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
    pub ignore_requirements: bool,
}

#[derive(Deserialize)]
struct RawAuto {
    #[serde(rename = "$value", default)]
    children: Vec<AutoChild>,
}

#[derive(Deserialize)]
enum AutoChild {
    Requirement(Requirement),
    RequiresOneOf(RawGroup),
    RequiresNoneOf(RawGroup),
    IgnoreRequirements(Empty),
}

impl<'de> Deserialize<'de> for AutomaticAcquisition {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = RawAuto::deserialize(d)?;
        let mut out = AutomaticAcquisition::default();
        let mut loose = Vec::new();
        for child in raw.children {
            match child {
                AutoChild::Requirement(r) => loose.push(r),
                AutoChild::RequiresOneOf(g) => {
                    out.requirements.groups.push(RequirementGroup::from_raw(RequirementGroupKind::OneOf, g))
                }
                AutoChild::RequiresNoneOf(g) => {
                    out.requirements.groups.push(RequirementGroup::from_raw(RequirementGroupKind::NoneOf, g))
                }
                AutoChild::IgnoreRequirements(_) => out.ignore_requirements = true,
            }
        }
        if !loose.is_empty() {
            out.requirements.groups.insert(0, RequirementGroup::all(loose));
        }
        Ok(out)
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
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = RawFeat::deserialize(d)?;
        let mut f = Feat::default();
        let t = |s: String| s.trim().to_string();
        for child in raw.children {
            match child {
                FeatChild::Name(v) => f.name = t(v),
                FeatChild::Description(v) => f.description = Some(t(v)),
                FeatChild::Acquire(v) => f.acquire = Some(t(v)),
                FeatChild::Icon(v) => f.icon = Some(t(v)),
                FeatChild::MaxTimesAcquire(v) => f.max_times_acquire = Some(v),
                FeatChild::Sphere(v) => f.sphere = Some(t(v)),
                FeatChild::Group(v) => f.groups.push(t(v)),
                FeatChild::Requirements(v) => f.requirements = Some(v),
                FeatChild::Effect(v) => f.effects.push(v),
                FeatChild::Stance(v) => f.stances.push(v),
                FeatChild::Dc(v) => f.dcs.push(v),
                FeatChild::ConditionalGroup(v) => f.conditional_groups.push(v),
                FeatChild::AutomaticAcquisition(v) => f.automatic_acquisition = Some(v),
                FeatChild::Attack(v) => f.attack = Some(v),
                FeatChild::SubItem(v) => f.sub_items.push(v),
            }
        }
        if f.name.is_empty() {
            return Err(D::Error::custom("<Feat> without a <Name>"));
        }
        Ok(f)
    }
}
