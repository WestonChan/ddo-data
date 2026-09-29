use super::effect::Effect;
use super::feats::{Attack, Dc, Stance};
use super::requirements::Requirements;
use super::{EmptyElement, NumberList};
use anyhow::{bail, Result};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};
use std::path::Path;

pub fn parse(path: &Path) -> Result<EnhancementTree> {
    let file: EnhancementTreeFile = super::parse_xml_file(path)?;
    match file.trees.into_iter().next() {
        Some(tree) => Ok(tree),
        None => bail!("{}: no <EnhancementTree>", path.display()),
    }
}

#[derive(Deserialize)]
struct EnhancementTreeFile {
    #[serde(rename = "EnhancementTree", default)]
    trees: Vec<EnhancementTree>,
}

#[derive(Debug, Default)]
pub struct EnhancementTree {
    pub name: String,
    pub version: Option<i64>,
    pub icon: Option<String>,
    pub background: Option<String>,
    pub requirements: Option<Requirements>,
    pub is_racial: bool,
    pub is_destiny: bool,
    pub is_universal: bool,
    pub is_reaper: bool,
    pub is_legacy: bool,
    pub enhancements: Vec<Enhancement>,
}

#[derive(Debug, Default)]
pub struct Enhancement {
    pub name: String,
    pub internal_name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub x: Option<i64>,
    pub y: Option<i64>,
    pub cost_per_rank: Vec<f64>,
    pub rank_count: Option<i64>,
    pub minimum_points_spent: Option<i64>,
    pub requirements: Option<Requirements>,
    pub effects: Vec<Effect>,
    pub is_clickie: bool,
    pub is_tier5: bool,
    pub arrows: Vec<String>,
    pub selector: Option<EnhancementSelector>,
    pub stances: Vec<Stance>,
    pub dcs: Vec<Dc>,
    pub attack: Option<Attack>,
}

#[derive(Debug, Default)]
pub struct EnhancementSelector {
    pub excluded_internal_names: Vec<String>,
    pub selections: Vec<EnhancementSelection>,
}

#[derive(Debug, Default)]
pub struct EnhancementSelection {
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub cost_per_rank: Vec<f64>,
    pub rank_count: Option<i64>,
    pub minimum_points_spent: Option<i64>,
    pub requirements: Option<Requirements>,
    pub effects: Vec<Effect>,
    pub is_clickie: bool,
    pub stances: Vec<Stance>,
    pub dcs: Vec<Dc>,
    pub attack: Option<Attack>,
}

#[derive(Deserialize)]
struct RawEnhancementTree {
    #[serde(rename = "$value", default)]
    children: Vec<EnhancementTreeChild>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Deserialize)]
enum EnhancementTreeChild {
    Name(String),
    Version(i64),
    Icon(String),
    Background(String),
    Requirements(Requirements),
    IsRacialTree(EmptyElement),
    IsEpicDestiny(EmptyElement),
    IsUniversalTree(EmptyElement),
    IsReaperTree(EmptyElement),
    Legacy(EmptyElement),
    EnhancementTreeItem(Enhancement),
}

impl<'de> Deserialize<'de> for EnhancementTree {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw_tree = RawEnhancementTree::deserialize(deserializer)?;
        let mut tree = EnhancementTree::default();
        let trimmed = |s: String| s.trim().to_string();
        for child in raw_tree.children {
            match child {
                EnhancementTreeChild::Name(v) => tree.name = trimmed(v),
                EnhancementTreeChild::Version(v) => tree.version = Some(v),
                EnhancementTreeChild::Icon(v) => tree.icon = Some(trimmed(v)),
                EnhancementTreeChild::Background(v) => tree.background = Some(trimmed(v)),
                EnhancementTreeChild::Requirements(v) => tree.requirements = Some(v),
                EnhancementTreeChild::IsRacialTree(_) => tree.is_racial = true,
                EnhancementTreeChild::IsEpicDestiny(_) => tree.is_destiny = true,
                EnhancementTreeChild::IsUniversalTree(_) => tree.is_universal = true,
                EnhancementTreeChild::IsReaperTree(_) => tree.is_reaper = true,
                EnhancementTreeChild::Legacy(_) => tree.is_legacy = true,
                EnhancementTreeChild::EnhancementTreeItem(v) => tree.enhancements.push(v),
            }
        }
        if tree.name.is_empty() {
            return Err(D::Error::custom("<EnhancementTree> without a <Name>"));
        }
        Ok(tree)
    }
}

#[derive(Deserialize)]
struct RawEnhancement {
    #[serde(rename = "$value", default)]
    children: Vec<EnhancementChild>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Deserialize)]
enum EnhancementChild {
    Name(String),
    InternalName(String),
    Description(String),
    Icon(String),
    XPosition(i64),
    YPosition(i64),
    CostPerRank(NumberList),
    Ranks(i64),
    MinSpent(i64),
    Requirements(Requirements),
    Effect(Effect),
    Clickie(EmptyElement),
    Tier5(EmptyElement),
    ArrowUp(EmptyElement),
    ArrowRight(EmptyElement),
    ArrowLeft(EmptyElement),
    LongArrowUp(EmptyElement),
    ExtraLongArrowUp(EmptyElement),
    Selector(EnhancementSelector),
    Stance(Stance),
    #[serde(rename = "DC")]
    Dc(Dc),
    Attack(Attack),
}

impl<'de> Deserialize<'de> for Enhancement {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw_enhancement = RawEnhancement::deserialize(deserializer)?;
        let mut enhancement = Enhancement::default();
        let trimmed = |s: String| s.trim().to_string();
        for child in raw_enhancement.children {
            match child {
                EnhancementChild::Name(v) => enhancement.name = trimmed(v),
                EnhancementChild::InternalName(v) => enhancement.internal_name = trimmed(v),
                EnhancementChild::Description(v) => enhancement.description = Some(trimmed(v)),
                EnhancementChild::Icon(v) => enhancement.icon = Some(trimmed(v)),
                EnhancementChild::XPosition(v) => enhancement.x = Some(v),
                EnhancementChild::YPosition(v) => enhancement.y = Some(v),
                EnhancementChild::CostPerRank(v) => {
                    enhancement.cost_per_rank = v.numbers().map_err(D::Error::custom)?
                }
                EnhancementChild::Ranks(v) => enhancement.rank_count = Some(v),
                EnhancementChild::MinSpent(v) => enhancement.minimum_points_spent = Some(v),
                EnhancementChild::Requirements(v) => enhancement.requirements = Some(v),
                EnhancementChild::Effect(v) => enhancement.effects.push(v),
                EnhancementChild::Clickie(_) => enhancement.is_clickie = true,
                EnhancementChild::Tier5(_) => enhancement.is_tier5 = true,
                EnhancementChild::ArrowUp(_) => enhancement.arrows.push("ArrowUp".into()),
                EnhancementChild::ArrowRight(_) => enhancement.arrows.push("ArrowRight".into()),
                EnhancementChild::ArrowLeft(_) => enhancement.arrows.push("ArrowLeft".into()),
                EnhancementChild::LongArrowUp(_) => enhancement.arrows.push("LongArrowUp".into()),
                EnhancementChild::ExtraLongArrowUp(_) => enhancement.arrows.push("ExtraLongArrowUp".into()),
                EnhancementChild::Selector(v) => enhancement.selector = Some(v),
                EnhancementChild::Stance(v) => enhancement.stances.push(v),
                EnhancementChild::Dc(v) => enhancement.dcs.push(v),
                EnhancementChild::Attack(v) => enhancement.attack = Some(v),
            }
        }
        if enhancement.name.is_empty() || enhancement.internal_name.is_empty() {
            return Err(D::Error::custom("<EnhancementTreeItem> without a <Name> or <InternalName>"));
        }
        Ok(enhancement)
    }
}

#[derive(Deserialize)]
struct RawEnhancementSelector {
    #[serde(rename = "$value", default)]
    children: Vec<EnhancementSelectorChild>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Deserialize)]
enum EnhancementSelectorChild {
    Exclusions(String),
    EnhancementSelection(EnhancementSelection),
}

impl<'de> Deserialize<'de> for EnhancementSelector {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw_selector = RawEnhancementSelector::deserialize(deserializer)?;
        let mut selector = EnhancementSelector::default();
        for child in raw_selector.children {
            match child {
                EnhancementSelectorChild::Exclusions(v) => selector.excluded_internal_names.push(v.trim().to_string()),
                EnhancementSelectorChild::EnhancementSelection(v) => selector.selections.push(v),
            }
        }
        Ok(selector)
    }
}

#[derive(Deserialize)]
struct RawEnhancementSelection {
    #[serde(rename = "$value", default)]
    children: Vec<EnhancementSelectionChild>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Deserialize)]
enum EnhancementSelectionChild {
    Name(String),
    Description(String),
    Icon(String),
    CostPerRank(NumberList),
    Ranks(i64),
    MinSpent(i64),
    Requirements(Requirements),
    Effect(Effect),
    Clickie(EmptyElement),
    Stance(Stance),
    #[serde(rename = "DC")]
    Dc(Dc),
    Attack(Attack),
}

impl<'de> Deserialize<'de> for EnhancementSelection {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw_selection = RawEnhancementSelection::deserialize(deserializer)?;
        let mut selection = EnhancementSelection::default();
        let trimmed = |v: String| v.trim().to_string();
        for child in raw_selection.children {
            match child {
                EnhancementSelectionChild::Name(v) => selection.name = trimmed(v),
                EnhancementSelectionChild::Description(v) => selection.description = Some(trimmed(v)),
                EnhancementSelectionChild::Icon(v) => selection.icon = Some(trimmed(v)),
                EnhancementSelectionChild::CostPerRank(v) => {
                    selection.cost_per_rank = v.numbers().map_err(D::Error::custom)?
                }
                EnhancementSelectionChild::Ranks(v) => selection.rank_count = Some(v),
                EnhancementSelectionChild::MinSpent(v) => selection.minimum_points_spent = Some(v),
                EnhancementSelectionChild::Requirements(v) => selection.requirements = Some(v),
                EnhancementSelectionChild::Effect(v) => selection.effects.push(v),
                EnhancementSelectionChild::Clickie(_) => selection.is_clickie = true,
                EnhancementSelectionChild::Stance(v) => selection.stances.push(v),
                EnhancementSelectionChild::Dc(v) => selection.dcs.push(v),
                EnhancementSelectionChild::Attack(v) => selection.attack = Some(v),
            }
        }
        if selection.name.is_empty() {
            return Err(D::Error::custom("<EnhancementSelection> without a <Name>"));
        }
        Ok(selection)
    }
}
