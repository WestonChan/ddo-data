use anyhow::Result;
use ddo_model::enums::RequirementGroupKind;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};

pub fn parse_requirements(xml: &str) -> Result<Requirements> {
    Ok(quick_xml::de::from_str(xml)?)
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
    children: Vec<RequirementsChild>,
}

#[derive(Deserialize)]
enum RequirementsChild {
    Requirement(Requirement),
    RequiresOneOf(RawRequirementGroup),
    RequiresNoneOf(RawRequirementGroup),
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawRequirementGroup {
    #[serde(rename = "DisplayDescription")]
    pub display_description: Option<String>,
    #[serde(rename = "Requirement", default)]
    pub requirements: Vec<Requirement>,
}

impl RequirementGroup {
    pub fn from_raw(kind: RequirementGroupKind, raw_group: RawRequirementGroup) -> Self {
        RequirementGroup {
            kind,
            display_description: raw_group.display_description.map(|s| s.trim().to_string()),
            requirements: raw_group.requirements.into_iter().map(Requirement::with_trimmed_kind).collect(),
        }
    }

    pub fn all_of(requirements: Vec<Requirement>) -> Self {
        RequirementGroup { kind: RequirementGroupKind::All, display_description: None, requirements }
    }
}

impl Requirement {
    fn with_trimmed_kind(mut self) -> Self {
        self.kind = self.kind.trim().to_string();
        self
    }
}

impl<'de> Deserialize<'de> for Requirements {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw_requirements = RawRequirements::deserialize(deserializer)?;
        let mut groups: Vec<RequirementGroup> = Vec::new();
        for child in raw_requirements.children {
            match child {
                RequirementsChild::Requirement(requirement) => {
                    let requirement = requirement.with_trimmed_kind();
                    match groups.last_mut() {
                        Some(last_group) if last_group.kind == RequirementGroupKind::All => {
                            last_group.requirements.push(requirement)
                        }
                        _ => groups.push(RequirementGroup::all_of(vec![requirement])),
                    }
                }
                RequirementsChild::RequiresOneOf(g) => {
                    groups.push(RequirementGroup::from_raw(RequirementGroupKind::OneOf, g))
                }
                RequirementsChild::RequiresNoneOf(g) => {
                    groups.push(RequirementGroup::from_raw(RequirementGroupKind::NoneOf, g))
                }
            }
        }
        if groups.iter().any(|g| g.requirements.iter().any(|r| r.kind.is_empty())) {
            return Err(D::Error::custom("<Requirement> without a <Type>"));
        }
        Ok(Requirements { groups })
    }
}

impl Requirements {
    pub fn requirements_to_meet(&self) -> impl Iterator<Item = &Requirement> {
        self.groups.iter().filter(|g| g.kind != RequirementGroupKind::NoneOf).flat_map(|g| g.requirements.iter())
    }
}
