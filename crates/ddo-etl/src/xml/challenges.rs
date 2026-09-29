use super::NumberList;
use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::path::Path;

pub fn parse(path: &Path) -> Result<Vec<Challenge>> {
    let file: ChallengeFile = super::parse_xml_file(path)?;
    file.challenges.into_iter().map(Challenge::try_from).collect()
}

#[derive(Deserialize)]
struct ChallengeFile {
    #[serde(rename = "Challenge", default)]
    challenges: Vec<RawChallenge>,
}

#[derive(Deserialize)]
struct RawChallenge {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Patron")]
    patron: Option<String>,
    #[serde(rename = "AdventurePack")]
    adventure_pack: Option<String>,
    #[serde(rename = "LevelRange")]
    level_range: Option<NumberList>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Challenge {
    pub name: String,
    pub patron: Option<String>,
    pub adventure_pack: Option<String>,
    pub level_range: Vec<i64>,
}

impl TryFrom<RawChallenge> for Challenge {
    type Error = anyhow::Error;

    fn try_from(raw_challenge: RawChallenge) -> Result<Self> {
        let name = raw_challenge.name.trim().to_string();
        let level_range = raw_challenge
            .level_range
            .map(|v| v.integers())
            .transpose()
            .map_err(|e| anyhow!("challenge {name}: level range {e}"))?
            .unwrap_or_default();
        Ok(Challenge {
            name,
            patron: raw_challenge.patron.map(|p| p.trim().to_string()).filter(|p| !p.is_empty() && p != "None"),
            adventure_pack: raw_challenge.adventure_pack.map(|p| p.trim().to_string()).filter(|p| !p.is_empty()),
            level_range,
        })
    }
}
