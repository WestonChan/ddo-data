//! `<Requirements>`: the condition grammar every family shares. Children are `<Requirement>`
//! (all must hold), `<RequiresOneOf>` and `<RequiresNoneOf>` blocks, in any order and any number.
//! Consecutive top-level `<Requirement>` elements fold into one `All` group.

use anyhow::Result;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};

pub fn parse_requirements(xml: &str) -> Result<Requirements> {
    Ok(quick_xml::de::from_str(xml)?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequirementGroupKind {
    All,
    OneOf,
    NoneOf,
}

impl RequirementGroupKind {
    pub fn to_model(self) -> ddo_model::enums::RequirementGroup {
        match self {
            Self::All => ddo_model::enums::RequirementGroup::All,
            Self::OneOf => ddo_model::enums::RequirementGroup::OneOf,
            Self::NoneOf => ddo_model::enums::RequirementGroup::NoneOf,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Requirements {
    pub groups: Vec<RequirementGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequirementGroup {
    pub kind: RequirementGroupKind,
    pub display_description: Option<String>,
    pub requirements: Vec<Requirement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Requirement {
    #[serde(rename = "Type")]
    pub kind: String,
    #[serde(rename = "Item", default)]
    pub items: Vec<String>,
    #[serde(rename = "Value")]
    pub value: Option<String>,
}

#[derive(Deserialize)]
struct RawRequirements {
    #[serde(rename = "$value", default)]
    children: Vec<RawChild>,
}

#[derive(Deserialize)]
enum RawChild {
    Requirement(Requirement),
    RequiresOneOf(RawGroup),
    RequiresNoneOf(RawGroup),
}

/// A `<RequiresOneOf>` / `<RequiresNoneOf>` block as written.
#[derive(Debug, Clone, Deserialize)]
pub struct RawGroup {
    #[serde(rename = "DisplayDescription")]
    pub display_description: Option<String>,
    #[serde(rename = "Requirement", default)]
    pub requirements: Vec<Requirement>,
}

impl RequirementGroup {
    /// Build a group from a raw block, trimming requirement types.
    pub fn from_raw(kind: RequirementGroupKind, raw: RawGroup) -> Self {
        group(kind, raw)
    }

    /// An `All` group of loose `<Requirement>` elements.
    pub fn all(requirements: Vec<Requirement>) -> Self {
        RequirementGroup { kind: RequirementGroupKind::All, display_description: None, requirements }
    }
}

impl<'de> Deserialize<'de> for Requirements {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = RawRequirements::deserialize(d)?;
        let mut groups: Vec<RequirementGroup> = Vec::new();
        for child in raw.children {
            match child {
                RawChild::Requirement(mut r) => {
                    r.kind = r.kind.trim().to_string();
                    match groups.last_mut() {
                        Some(g) if g.kind == RequirementGroupKind::All => g.requirements.push(r),
                        _ => groups.push(RequirementGroup {
                            kind: RequirementGroupKind::All,
                            display_description: None,
                            requirements: vec![r],
                        }),
                    }
                }
                RawChild::RequiresOneOf(g) => groups.push(group(RequirementGroupKind::OneOf, g)),
                RawChild::RequiresNoneOf(g) => groups.push(group(RequirementGroupKind::NoneOf, g)),
            }
        }
        if groups.iter().any(|g| g.requirements.iter().any(|r| r.kind.is_empty())) {
            return Err(D::Error::custom("<Requirement> without a <Type>"));
        }
        Ok(Requirements { groups })
    }
}

fn group(kind: RequirementGroupKind, raw: RawGroup) -> RequirementGroup {
    RequirementGroup {
        kind,
        display_description: raw.display_description.map(|s| s.trim().to_string()),
        requirements: raw
            .requirements
            .into_iter()
            .map(|mut r| {
                r.kind = r.kind.trim().to_string();
                r
            })
            .collect(),
    }
}

impl Requirements {
    /// Every positive requirement (`All` and `OneOf` groups), flattened.
    pub fn positive(&self) -> impl Iterator<Item = &Requirement> {
        self.groups.iter().filter(|g| g.kind != RequirementGroupKind::NoneOf).flat_map(|g| g.requirements.iter())
    }
}
