//! `Classes/*.class.xml`: one `<Class>` per file with its class feats, spell list, spell slots
//! per level, feat slots and automatic feats nested.

use super::feats::Feat;
use super::{Empty, Vector};
use anyhow::{bail, Result};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};
use std::collections::BTreeMap;
use std::path::Path;

pub fn parse(path: &Path) -> Result<Class> {
    let file: ClassFile = super::read_xml(path)?;
    match file.classes.into_iter().next() {
        Some(c) => Ok(c),
        None => bail!("{}: no <Class>", path.display()),
    }
}

#[derive(Deserialize)]
struct ClassFile {
    #[serde(rename = "Class", default)]
    classes: Vec<Class>,
}

#[derive(Debug, Default)]
pub struct Class {
    pub name: String,
    pub base_class: Option<String>,
    pub not_heroic: bool,
    pub description: Option<String>,
    pub small_icon: Option<String>,
    pub large_icon: Option<String>,
    pub skill_points: Option<i64>,
    pub hit_points: Option<i64>,
    pub alignments: Vec<String>,
    pub fortitude: Option<String>,
    pub reflex: Option<String>,
    pub will: Option<String>,
    pub spell_points_per_level: Vec<f64>,
    pub bab: Vec<f64>,
    pub casting_stats: Vec<String>,
    pub class_specific_feat_types: Vec<String>,
    pub class_skills: Vec<String>,
    pub auto_buy_skills: Vec<String>,
    /// Class level → slots per spell level (index 0 = spell level 1).
    pub spell_slots: BTreeMap<u8, Vec<i64>>,
    pub class_spells: Vec<ClassSpell>,
    pub feat_slots: Vec<FeatSlot>,
    pub automatic_feats: Vec<AutomaticFeats>,
    pub feats: Vec<Feat>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClassSpell {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Level")]
    pub level: i64,
    #[serde(rename = "Cost")]
    pub cost: Option<i64>,
    #[serde(rename = "MaxCasterLevel")]
    pub max_caster_level: Option<i64>,
}

/// A feat slot a class or race grants at a level.
#[derive(Debug, Clone, Deserialize)]
pub struct FeatSlot {
    #[serde(rename = "Level")]
    pub level: i64,
    #[serde(rename = "FeatType")]
    pub feat_type: String,
    #[serde(rename = "AutoPopulate")]
    pub auto_populate: Option<Empty>,
    #[serde(rename = "Singular")]
    pub singular: Option<Empty>,
    #[serde(rename = "FeatUpdateList", default)]
    pub update_list: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AutomaticFeats {
    #[serde(rename = "Level")]
    pub level: i64,
    #[serde(rename = "Feats", default)]
    pub feats: Vec<String>,
}

#[derive(Deserialize)]
struct RawClass {
    #[serde(rename = "$value", default)]
    children: Vec<ClassChild>,
}

// A transient parse buffer; the size skew between variants does not matter here.
#[allow(clippy::large_enum_variant)]
#[derive(Deserialize)]
enum ClassChild {
    Name(String),
    BaseClass(String),
    NotHeroic(Empty),
    Description(String),
    SmallIcon(String),
    LargeIcon(String),
    SkillPoints(i64),
    HitPoints(i64),
    Alignment(String),
    Fortitude(String),
    Reflex(String),
    Will(String),
    SpellPointsPerLevel(Vector),
    #[serde(rename = "BAB")]
    Bab(Vector),
    CastingStat(String),
    ClassSpecificFeatType(String),
    ClassSkill(String),
    AutoBuySkill(String),
    Level1(Vector),
    Level2(Vector),
    Level3(Vector),
    Level4(Vector),
    Level5(Vector),
    Level6(Vector),
    Level7(Vector),
    Level8(Vector),
    Level9(Vector),
    Level10(Vector),
    Level11(Vector),
    Level12(Vector),
    Level13(Vector),
    Level14(Vector),
    Level15(Vector),
    Level16(Vector),
    Level17(Vector),
    Level18(Vector),
    Level19(Vector),
    Level20(Vector),
    ClassSpell(ClassSpell),
    FeatSlot(FeatSlot),
    AutomaticFeats(AutomaticFeats),
    Feat(Feat),
}

impl<'de> Deserialize<'de> for Class {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = RawClass::deserialize(d)?;
        let mut c = Class::default();
        let t = |s: String| s.trim().to_string();
        let slots = |c: &mut Class, level: u8, v: Vector| -> Result<(), D::Error> {
            c.spell_slots.insert(level, v.integers().map_err(D::Error::custom)?);
            Ok(())
        };
        for child in raw.children {
            match child {
                ClassChild::Name(v) => c.name = t(v),
                ClassChild::BaseClass(v) => c.base_class = Some(t(v)),
                ClassChild::NotHeroic(_) => c.not_heroic = true,
                ClassChild::Description(v) => c.description = Some(t(v)),
                ClassChild::SmallIcon(v) => c.small_icon = Some(t(v)),
                ClassChild::LargeIcon(v) => c.large_icon = Some(t(v)),
                ClassChild::SkillPoints(v) => c.skill_points = Some(v),
                ClassChild::HitPoints(v) => c.hit_points = Some(v),
                ClassChild::Alignment(v) => c.alignments.push(t(v)),
                ClassChild::Fortitude(v) => c.fortitude = Some(t(v)),
                ClassChild::Reflex(v) => c.reflex = Some(t(v)),
                ClassChild::Will(v) => c.will = Some(t(v)),
                ClassChild::SpellPointsPerLevel(v) => {
                    c.spell_points_per_level = v.numbers().map_err(D::Error::custom)?
                }
                ClassChild::Bab(v) => c.bab = v.numbers().map_err(D::Error::custom)?,
                ClassChild::CastingStat(v) => c.casting_stats.push(t(v)),
                ClassChild::ClassSpecificFeatType(v) => c.class_specific_feat_types.push(t(v)),
                ClassChild::ClassSkill(v) => c.class_skills.push(t(v)),
                ClassChild::AutoBuySkill(v) => c.auto_buy_skills.push(t(v)),
                ClassChild::Level1(v) => slots(&mut c, 1, v)?,
                ClassChild::Level2(v) => slots(&mut c, 2, v)?,
                ClassChild::Level3(v) => slots(&mut c, 3, v)?,
                ClassChild::Level4(v) => slots(&mut c, 4, v)?,
                ClassChild::Level5(v) => slots(&mut c, 5, v)?,
                ClassChild::Level6(v) => slots(&mut c, 6, v)?,
                ClassChild::Level7(v) => slots(&mut c, 7, v)?,
                ClassChild::Level8(v) => slots(&mut c, 8, v)?,
                ClassChild::Level9(v) => slots(&mut c, 9, v)?,
                ClassChild::Level10(v) => slots(&mut c, 10, v)?,
                ClassChild::Level11(v) => slots(&mut c, 11, v)?,
                ClassChild::Level12(v) => slots(&mut c, 12, v)?,
                ClassChild::Level13(v) => slots(&mut c, 13, v)?,
                ClassChild::Level14(v) => slots(&mut c, 14, v)?,
                ClassChild::Level15(v) => slots(&mut c, 15, v)?,
                ClassChild::Level16(v) => slots(&mut c, 16, v)?,
                ClassChild::Level17(v) => slots(&mut c, 17, v)?,
                ClassChild::Level18(v) => slots(&mut c, 18, v)?,
                ClassChild::Level19(v) => slots(&mut c, 19, v)?,
                ClassChild::Level20(v) => slots(&mut c, 20, v)?,
                ClassChild::ClassSpell(v) => c.class_spells.push(v),
                ClassChild::FeatSlot(v) => c.feat_slots.push(v),
                ClassChild::AutomaticFeats(v) => c.automatic_feats.push(v),
                ClassChild::Feat(v) => c.feats.push(v),
            }
        }
        if c.name.is_empty() {
            return Err(D::Error::custom("<Class> without a <Name>"));
        }
        Ok(c)
    }
}
