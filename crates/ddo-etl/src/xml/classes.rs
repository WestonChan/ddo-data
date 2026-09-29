use super::feats::Feat;
use super::{EmptyElement, NumberList};
use anyhow::{bail, Result};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};
use std::collections::BTreeMap;
use std::path::Path;

pub fn parse(path: &Path) -> Result<Class> {
    let file: ClassFile = super::parse_xml_file(path)?;
    match file.classes.into_iter().next() {
        Some(class) => Ok(class),
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
    pub is_non_heroic: bool,
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
    pub spell_slots_by_class_level: BTreeMap<u8, Vec<i64>>,
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
    pub maximum_caster_level: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeatSlot {
    #[serde(rename = "Level")]
    pub level: i64,
    #[serde(rename = "FeatType")]
    pub feat_type: String,
    #[serde(rename = "AutoPopulate")]
    pub auto_populate: Option<EmptyElement>,
    #[serde(rename = "Singular")]
    pub singular: Option<EmptyElement>,
    #[serde(rename = "FeatUpdateList", default)]
    pub update_list: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AutomaticFeats {
    #[serde(rename = "Level")]
    pub level: i64,
    #[serde(rename = "Feats", default)]
    pub feat_names: Vec<String>,
}

#[derive(Deserialize)]
struct RawClass {
    #[serde(rename = "$value", default)]
    children: Vec<ClassChild>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Deserialize)]
enum ClassChild {
    Name(String),
    BaseClass(String),
    NotHeroic(EmptyElement),
    Description(String),
    SmallIcon(String),
    LargeIcon(String),
    SkillPoints(i64),
    HitPoints(i64),
    Alignment(String),
    Fortitude(String),
    Reflex(String),
    Will(String),
    SpellPointsPerLevel(NumberList),
    #[serde(rename = "BAB")]
    Bab(NumberList),
    CastingStat(String),
    ClassSpecificFeatType(String),
    ClassSkill(String),
    AutoBuySkill(String),
    Level1(NumberList),
    Level2(NumberList),
    Level3(NumberList),
    Level4(NumberList),
    Level5(NumberList),
    Level6(NumberList),
    Level7(NumberList),
    Level8(NumberList),
    Level9(NumberList),
    Level10(NumberList),
    Level11(NumberList),
    Level12(NumberList),
    Level13(NumberList),
    Level14(NumberList),
    Level15(NumberList),
    Level16(NumberList),
    Level17(NumberList),
    Level18(NumberList),
    Level19(NumberList),
    Level20(NumberList),
    ClassSpell(ClassSpell),
    FeatSlot(FeatSlot),
    AutomaticFeats(AutomaticFeats),
    Feat(Feat),
}

impl<'de> Deserialize<'de> for Class {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw_class = RawClass::deserialize(deserializer)?;
        let mut class = Class::default();
        let trimmed = |s: String| s.trim().to_string();
        let insert_spell_slots = |class: &mut Class, class_level: u8, slots: NumberList| -> Result<(), D::Error> {
            class.spell_slots_by_class_level.insert(class_level, slots.integers().map_err(D::Error::custom)?);
            Ok(())
        };
        for child in raw_class.children {
            match child {
                ClassChild::Name(v) => class.name = trimmed(v),
                ClassChild::BaseClass(v) => class.base_class = Some(trimmed(v)),
                ClassChild::NotHeroic(_) => class.is_non_heroic = true,
                ClassChild::Description(v) => class.description = Some(trimmed(v)),
                ClassChild::SmallIcon(v) => class.small_icon = Some(trimmed(v)),
                ClassChild::LargeIcon(v) => class.large_icon = Some(trimmed(v)),
                ClassChild::SkillPoints(v) => class.skill_points = Some(v),
                ClassChild::HitPoints(v) => class.hit_points = Some(v),
                ClassChild::Alignment(v) => class.alignments.push(trimmed(v)),
                ClassChild::Fortitude(v) => class.fortitude = Some(trimmed(v)),
                ClassChild::Reflex(v) => class.reflex = Some(trimmed(v)),
                ClassChild::Will(v) => class.will = Some(trimmed(v)),
                ClassChild::SpellPointsPerLevel(v) => {
                    class.spell_points_per_level = v.numbers().map_err(D::Error::custom)?
                }
                ClassChild::Bab(v) => class.bab = v.numbers().map_err(D::Error::custom)?,
                ClassChild::CastingStat(v) => class.casting_stats.push(trimmed(v)),
                ClassChild::ClassSpecificFeatType(v) => class.class_specific_feat_types.push(trimmed(v)),
                ClassChild::ClassSkill(v) => class.class_skills.push(trimmed(v)),
                ClassChild::AutoBuySkill(v) => class.auto_buy_skills.push(trimmed(v)),
                ClassChild::Level1(v) => insert_spell_slots(&mut class, 1, v)?,
                ClassChild::Level2(v) => insert_spell_slots(&mut class, 2, v)?,
                ClassChild::Level3(v) => insert_spell_slots(&mut class, 3, v)?,
                ClassChild::Level4(v) => insert_spell_slots(&mut class, 4, v)?,
                ClassChild::Level5(v) => insert_spell_slots(&mut class, 5, v)?,
                ClassChild::Level6(v) => insert_spell_slots(&mut class, 6, v)?,
                ClassChild::Level7(v) => insert_spell_slots(&mut class, 7, v)?,
                ClassChild::Level8(v) => insert_spell_slots(&mut class, 8, v)?,
                ClassChild::Level9(v) => insert_spell_slots(&mut class, 9, v)?,
                ClassChild::Level10(v) => insert_spell_slots(&mut class, 10, v)?,
                ClassChild::Level11(v) => insert_spell_slots(&mut class, 11, v)?,
                ClassChild::Level12(v) => insert_spell_slots(&mut class, 12, v)?,
                ClassChild::Level13(v) => insert_spell_slots(&mut class, 13, v)?,
                ClassChild::Level14(v) => insert_spell_slots(&mut class, 14, v)?,
                ClassChild::Level15(v) => insert_spell_slots(&mut class, 15, v)?,
                ClassChild::Level16(v) => insert_spell_slots(&mut class, 16, v)?,
                ClassChild::Level17(v) => insert_spell_slots(&mut class, 17, v)?,
                ClassChild::Level18(v) => insert_spell_slots(&mut class, 18, v)?,
                ClassChild::Level19(v) => insert_spell_slots(&mut class, 19, v)?,
                ClassChild::Level20(v) => insert_spell_slots(&mut class, 20, v)?,
                ClassChild::ClassSpell(v) => class.class_spells.push(v),
                ClassChild::FeatSlot(v) => class.feat_slots.push(v),
                ClassChild::AutomaticFeats(v) => class.automatic_feats.push(v),
                ClassChild::Feat(v) => class.feats.push(v),
            }
        }
        if class.name.is_empty() {
            return Err(D::Error::custom("<Class> without a <Name>"));
        }
        Ok(class)
    }
}
