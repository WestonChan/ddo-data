//! `Races/*.race.xml`: one `<Race>` per file, with its racial feats nested.

use super::classes::FeatSlot;
use super::feats::Feat;
use super::{Empty, Vector};
use anyhow::{bail, Result};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};
use std::path::Path;

pub fn parse(path: &Path) -> Result<Race> {
    let file: RaceFile = super::read_xml(path)?;
    match file.races.into_iter().next() {
        Some(r) => Ok(r),
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
    /// `(ability, modifier)` in document order, from `<Strength>+2</Strength>` and friends.
    pub ability_modifiers: Vec<(String, i64)>,
    pub granted_feats: Vec<String>,
    pub feats: Vec<Feat>,
    pub iconic_class: Option<String>,
    pub feat_slots: Vec<FeatSlot>,
    pub is_construct: bool,
    pub no_past_life: bool,
    pub auto_buy_skills: Vec<String>,
    pub skill_points: Option<i64>,
}

#[derive(Deserialize)]
struct RawRace {
    #[serde(rename = "$value", default)]
    children: Vec<RaceChild>,
}

// A transient parse buffer; the size skew between variants does not matter here.
#[allow(clippy::large_enum_variant)]
#[derive(Deserialize)]
enum RaceChild {
    Name(String),
    ShortName(String),
    Description(String),
    StartingWorld(String),
    BuildPoints(Vector),
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
    IsConstruct(Empty),
    NoPastLife(Empty),
    AutoBuySkill(String),
    SkillPoints(i64),
}

fn signed(ability: &str, v: &str) -> Result<i64, String> {
    v.trim().trim_start_matches('+').parse::<i64>().map_err(|e| format!("{ability} {v:?}: {e}"))
}

impl<'de> Deserialize<'de> for Race {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = RawRace::deserialize(d)?;
        let mut r = Race::default();
        let t = |s: String| s.trim().to_string();
        for child in raw.children {
            match child {
                RaceChild::Name(v) => r.name = t(v),
                RaceChild::ShortName(v) => r.short_name = Some(t(v)),
                RaceChild::Description(v) => r.description = Some(t(v)),
                RaceChild::StartingWorld(v) => r.starting_world = Some(t(v)),
                RaceChild::BuildPoints(v) => r.build_points = v.integers().map_err(D::Error::custom)?,
                RaceChild::Strength(v) => {
                    r.ability_modifiers.push(("Strength".into(), signed("Strength", &v).map_err(D::Error::custom)?))
                }
                RaceChild::Dexterity(v) => {
                    r.ability_modifiers.push(("Dexterity".into(), signed("Dexterity", &v).map_err(D::Error::custom)?))
                }
                RaceChild::Constitution(v) => r
                    .ability_modifiers
                    .push(("Constitution".into(), signed("Constitution", &v).map_err(D::Error::custom)?)),
                RaceChild::Intelligence(v) => r
                    .ability_modifiers
                    .push(("Intelligence".into(), signed("Intelligence", &v).map_err(D::Error::custom)?)),
                RaceChild::Wisdom(v) => {
                    r.ability_modifiers.push(("Wisdom".into(), signed("Wisdom", &v).map_err(D::Error::custom)?))
                }
                RaceChild::Charisma(v) => {
                    r.ability_modifiers.push(("Charisma".into(), signed("Charisma", &v).map_err(D::Error::custom)?))
                }
                RaceChild::GrantedFeat(v) => r.granted_feats.push(t(v)),
                RaceChild::Feat(v) => r.feats.push(v),
                RaceChild::IconicClass(v) => r.iconic_class = Some(t(v)),
                RaceChild::FeatSlot(v) => r.feat_slots.push(v),
                RaceChild::IsConstruct(_) => r.is_construct = true,
                RaceChild::NoPastLife(_) => r.no_past_life = true,
                RaceChild::AutoBuySkill(v) => r.auto_buy_skills.push(t(v)),
                RaceChild::SkillPoints(v) => r.skill_points = Some(v),
            }
        }
        if r.name.is_empty() {
            return Err(D::Error::custom("<Race> without a <Name>"));
        }
        Ok(r)
    }
}
