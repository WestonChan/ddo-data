//! `EnhancementTrees/*.xml`: one `<EnhancementTree>` per file with its items, each item optionally
//! a selector over several `<EnhancementSelection>` choices.

use super::effect::Effect;
use super::feats::{Attack, Dc, Stance};
use super::requirements::Requirements;
use super::{Empty, Vector};
use anyhow::{bail, Result};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};
use std::path::Path;

pub fn parse(path: &Path) -> Result<Tree> {
    let file: TreeFile = super::read_xml(path)?;
    match file.trees.into_iter().next() {
        Some(t) => Ok(t),
        None => bail!("{}: no <EnhancementTree>", path.display()),
    }
}

#[derive(Deserialize)]
struct TreeFile {
    #[serde(rename = "EnhancementTree", default)]
    trees: Vec<Tree>,
}

#[derive(Debug, Default)]
pub struct Tree {
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
    pub items: Vec<TreeItem>,
}

#[derive(Debug, Default)]
pub struct TreeItem {
    pub name: String,
    pub internal_name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub x: Option<i64>,
    pub y: Option<i64>,
    pub cost_per_rank: Vec<f64>,
    pub ranks: Option<i64>,
    pub min_spent: Option<i64>,
    pub requirements: Option<Requirements>,
    pub effects: Vec<Effect>,
    pub is_clickie: bool,
    pub is_tier5: bool,
    /// `ArrowUp`, `ArrowRight`, `ArrowLeft`, `LongArrowUp`, `ExtraLongArrowUp`, in document order.
    pub arrows: Vec<String>,
    pub selector: Option<Selector>,
    pub stances: Vec<Stance>,
    pub dcs: Vec<Dc>,
    pub attack: Option<Attack>,
}

#[derive(Debug, Default)]
pub struct Selector {
    pub exclusions: Vec<String>,
    pub selections: Vec<Selection>,
}

#[derive(Debug, Default)]
pub struct Selection {
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub cost_per_rank: Vec<f64>,
    pub ranks: Option<i64>,
    pub min_spent: Option<i64>,
    pub requirements: Option<Requirements>,
    pub effects: Vec<Effect>,
    pub is_clickie: bool,
    pub stances: Vec<Stance>,
    pub dcs: Vec<Dc>,
    pub attack: Option<Attack>,
}

#[derive(Deserialize)]
struct RawTree {
    #[serde(rename = "$value", default)]
    children: Vec<TreeChild>,
}

// A transient parse buffer; the size skew between variants does not matter here.
#[allow(clippy::large_enum_variant)]
#[derive(Deserialize)]
enum TreeChild {
    Name(String),
    Version(i64),
    Icon(String),
    Background(String),
    Requirements(Requirements),
    IsRacialTree(Empty),
    IsEpicDestiny(Empty),
    IsUniversalTree(Empty),
    IsReaperTree(Empty),
    Legacy(Empty),
    EnhancementTreeItem(TreeItem),
}

impl<'de> Deserialize<'de> for Tree {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = RawTree::deserialize(d)?;
        let mut t = Tree::default();
        let trim = |s: String| s.trim().to_string();
        for child in raw.children {
            match child {
                TreeChild::Name(v) => t.name = trim(v),
                TreeChild::Version(v) => t.version = Some(v),
                TreeChild::Icon(v) => t.icon = Some(trim(v)),
                TreeChild::Background(v) => t.background = Some(trim(v)),
                TreeChild::Requirements(v) => t.requirements = Some(v),
                TreeChild::IsRacialTree(_) => t.is_racial = true,
                TreeChild::IsEpicDestiny(_) => t.is_destiny = true,
                TreeChild::IsUniversalTree(_) => t.is_universal = true,
                TreeChild::IsReaperTree(_) => t.is_reaper = true,
                TreeChild::Legacy(_) => t.is_legacy = true,
                TreeChild::EnhancementTreeItem(v) => t.items.push(v),
            }
        }
        if t.name.is_empty() {
            return Err(D::Error::custom("<EnhancementTree> without a <Name>"));
        }
        Ok(t)
    }
}

#[derive(Deserialize)]
struct RawItem {
    #[serde(rename = "$value", default)]
    children: Vec<ItemChild>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Deserialize)]
enum ItemChild {
    Name(String),
    InternalName(String),
    Description(String),
    Icon(String),
    XPosition(i64),
    YPosition(i64),
    CostPerRank(Vector),
    Ranks(i64),
    MinSpent(i64),
    Requirements(Requirements),
    Effect(Effect),
    Clickie(Empty),
    Tier5(Empty),
    ArrowUp(Empty),
    ArrowRight(Empty),
    ArrowLeft(Empty),
    LongArrowUp(Empty),
    ExtraLongArrowUp(Empty),
    Selector(Selector),
    Stance(Stance),
    #[serde(rename = "DC")]
    Dc(Dc),
    Attack(Attack),
}

impl<'de> Deserialize<'de> for TreeItem {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = RawItem::deserialize(d)?;
        let mut i = TreeItem::default();
        let trim = |s: String| s.trim().to_string();
        for child in raw.children {
            match child {
                ItemChild::Name(v) => i.name = trim(v),
                ItemChild::InternalName(v) => i.internal_name = trim(v),
                ItemChild::Description(v) => i.description = Some(trim(v)),
                ItemChild::Icon(v) => i.icon = Some(trim(v)),
                ItemChild::XPosition(v) => i.x = Some(v),
                ItemChild::YPosition(v) => i.y = Some(v),
                ItemChild::CostPerRank(v) => i.cost_per_rank = v.numbers().map_err(D::Error::custom)?,
                ItemChild::Ranks(v) => i.ranks = Some(v),
                ItemChild::MinSpent(v) => i.min_spent = Some(v),
                ItemChild::Requirements(v) => i.requirements = Some(v),
                ItemChild::Effect(v) => i.effects.push(v),
                ItemChild::Clickie(_) => i.is_clickie = true,
                ItemChild::Tier5(_) => i.is_tier5 = true,
                ItemChild::ArrowUp(_) => i.arrows.push("ArrowUp".into()),
                ItemChild::ArrowRight(_) => i.arrows.push("ArrowRight".into()),
                ItemChild::ArrowLeft(_) => i.arrows.push("ArrowLeft".into()),
                ItemChild::LongArrowUp(_) => i.arrows.push("LongArrowUp".into()),
                ItemChild::ExtraLongArrowUp(_) => i.arrows.push("ExtraLongArrowUp".into()),
                ItemChild::Selector(v) => i.selector = Some(v),
                ItemChild::Stance(v) => i.stances.push(v),
                ItemChild::Dc(v) => i.dcs.push(v),
                ItemChild::Attack(v) => i.attack = Some(v),
            }
        }
        if i.name.is_empty() || i.internal_name.is_empty() {
            return Err(D::Error::custom("<EnhancementTreeItem> without a <Name> or <InternalName>"));
        }
        Ok(i)
    }
}

#[derive(Deserialize)]
struct RawSelector {
    #[serde(rename = "$value", default)]
    children: Vec<SelectorChild>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Deserialize)]
enum SelectorChild {
    Exclusions(String),
    EnhancementSelection(Selection),
}

impl<'de> Deserialize<'de> for Selector {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = RawSelector::deserialize(d)?;
        let mut s = Selector::default();
        for child in raw.children {
            match child {
                SelectorChild::Exclusions(v) => s.exclusions.push(v.trim().to_string()),
                SelectorChild::EnhancementSelection(v) => s.selections.push(v),
            }
        }
        Ok(s)
    }
}

#[derive(Deserialize)]
struct RawSelection {
    #[serde(rename = "$value", default)]
    children: Vec<SelectionChild>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Deserialize)]
enum SelectionChild {
    Name(String),
    Description(String),
    Icon(String),
    CostPerRank(Vector),
    Ranks(i64),
    MinSpent(i64),
    Requirements(Requirements),
    Effect(Effect),
    Clickie(Empty),
    Stance(Stance),
    #[serde(rename = "DC")]
    Dc(Dc),
    Attack(Attack),
}

impl<'de> Deserialize<'de> for Selection {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = RawSelection::deserialize(d)?;
        let mut s = Selection::default();
        let trim = |v: String| v.trim().to_string();
        for child in raw.children {
            match child {
                SelectionChild::Name(v) => s.name = trim(v),
                SelectionChild::Description(v) => s.description = Some(trim(v)),
                SelectionChild::Icon(v) => s.icon = Some(trim(v)),
                SelectionChild::CostPerRank(v) => s.cost_per_rank = v.numbers().map_err(D::Error::custom)?,
                SelectionChild::Ranks(v) => s.ranks = Some(v),
                SelectionChild::MinSpent(v) => s.min_spent = Some(v),
                SelectionChild::Requirements(v) => s.requirements = Some(v),
                SelectionChild::Effect(v) => s.effects.push(v),
                SelectionChild::Clickie(_) => s.is_clickie = true,
                SelectionChild::Stance(v) => s.stances.push(v),
                SelectionChild::Dc(v) => s.dcs.push(v),
                SelectionChild::Attack(v) => s.attack = Some(v),
            }
        }
        if s.name.is_empty() {
            return Err(D::Error::custom("<EnhancementSelection> without a <Name>"));
        }
        Ok(s)
    }
}
