use super::classes::FeatSlot;
use super::feats::Feat;
use super::{EmptyElement, NumberList};
use anyhow::{bail, Result};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};
use std::path::Path;

pub fn parse(path: &Path) -> Result<Race> {
    let file: RaceFile = super::parse_xml_file(path)?;
    match file.races.into_iter().next() {
        Some(race) => Ok(race),
        None => bail!("{}: no <Race>", path.display()),
    }
}

#[derive(Deserialize)]
struct RaceFile {
    #[serde(rename = "Race", default)]
    races: Vec<Race>,
}

#[derive(Debug, Default)]
pub struct Race {
    pub name: String,
    pub short_name: Option<String>,
    pub description: Option<String>,
    pub starting_world: Option<String>,
    pub build_points: Vec<i64>,
    pub ability_modifiers: Vec<(String, i64)>,
    pub granted_feat_names: Vec<String>,
    pub feats: Vec<Feat>,
    pub iconic_class: Option<String>,
    pub feat_slots: Vec<FeatSlot>,
    pub is_construct: bool,
    pub lacks_past_life: bool,
    pub auto_buy_skills: Vec<String>,
    pub skill_points: Option<i64>,
}

#[derive(Deserialize)]
struct RawRace {
    #[serde(rename = "$value", default)]
    children: Vec<RaceChild>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Deserialize)]
enum RaceChild {
    Name(String),
    ShortName(String),
    Description(String),
    StartingWorld(String),
    BuildPoints(NumberList),
    Strength(String),
    Dexterity(String),
    Constitution(String),
    Intelligence(String),
    Wisdom(String),
    Charisma(String),
    GrantedFeat(String),
    Feat(Feat),
    IconicClass(String),
    FeatSlot(FeatSlot),
    IsConstruct(EmptyElement),
    NoPastLife(EmptyElement),
    AutoBuySkill(String),
    SkillPoints(i64),
}

fn ability_modifier(ability: &str, text: &str) -> Result<(String, i64), String> {
    let modifier =
        text.trim().trim_start_matches('+').parse::<i64>().map_err(|e| format!("{ability} {text:?}: {e}"))?;
    Ok((ability.to_string(), modifier))
}

impl<'de> Deserialize<'de> for Race {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw_race = RawRace::deserialize(deserializer)?;
        let mut race = Race::default();
        let trimmed = |s: String| s.trim().to_string();
        for child in raw_race.children {
            match child {
                RaceChild::Name(v) => race.name = trimmed(v),
                RaceChild::ShortName(v) => race.short_name = Some(trimmed(v)),
                RaceChild::Description(v) => race.description = Some(trimmed(v)),
                RaceChild::StartingWorld(v) => race.starting_world = Some(trimmed(v)),
                RaceChild::BuildPoints(v) => race.build_points = v.integers().map_err(D::Error::custom)?,
                RaceChild::Strength(v) => {
                    race.ability_modifiers.push(ability_modifier("Strength", &v).map_err(D::Error::custom)?)
                }
                RaceChild::Dexterity(v) => {
                    race.ability_modifiers.push(ability_modifier("Dexterity", &v).map_err(D::Error::custom)?)
                }
                RaceChild::Constitution(v) => {
                    race.ability_modifiers.push(ability_modifier("Constitution", &v).map_err(D::Error::custom)?)
                }
                RaceChild::Intelligence(v) => {
                    race.ability_modifiers.push(ability_modifier("Intelligence", &v).map_err(D::Error::custom)?)
                }
                RaceChild::Wisdom(v) => {
                    race.ability_modifiers.push(ability_modifier("Wisdom", &v).map_err(D::Error::custom)?)
                }
                RaceChild::Charisma(v) => {
                    race.ability_modifiers.push(ability_modifier("Charisma", &v).map_err(D::Error::custom)?)
                }
                RaceChild::GrantedFeat(v) => race.granted_feat_names.push(trimmed(v)),
                RaceChild::Feat(v) => race.feats.push(v),
                RaceChild::IconicClass(v) => race.iconic_class = Some(trimmed(v)),
                RaceChild::FeatSlot(v) => race.feat_slots.push(v),
                RaceChild::IsConstruct(_) => race.is_construct = true,
                RaceChild::NoPastLife(_) => race.lacks_past_life = true,
                RaceChild::AutoBuySkill(v) => race.auto_buy_skills.push(trimmed(v)),
                RaceChild::SkillPoints(v) => race.skill_points = Some(v),
            }
        }
        if race.name.is_empty() {
            return Err(D::Error::custom("<Race> without a <Name>"));
        }
        Ok(race)
    }
}
