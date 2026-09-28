use anyhow::{bail, Result};
use ddo_model::enums::{QuestDuration, XpTier};
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestFacts {
    pub name: String,
    pub page: String,
    pub read: String,
    pub duration: Option<String>,
    pub free_to_play: bool,
    pub legendary_level: Option<i64>,
    pub zone: Option<String>,
    pub bestowed_by: Option<String>,
    pub flagging: Option<String>,
    #[serde(default)]
    pub xp: QuestXp,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestXp {
    pub heroic: Option<TierXp>,
    pub epic: Option<TierXp>,
    pub legendary: Option<TierXp>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TierXp {
    pub casual: Option<i64>,
    pub normal: Option<i64>,
    pub hard: Option<i64>,
    pub elite: Option<i64>,
}

impl QuestFacts {
    pub fn duration(&self) -> Option<QuestDuration> {
        self.duration.as_deref().and_then(QuestDuration::from_wiki)
    }

    pub(super) fn check_values(&self) -> Result<()> {
        if let Some(text) = &self.duration {
            if QuestDuration::from_wiki(text).is_none() {
                let allowed: Vec<&str> = QuestDuration::ALL.iter().map(|d| d.as_str()).collect();
                bail!("duration {text:?} must be one of {}", allowed.join(", "));
            }
        }
        for (tier, xp) in self.xp.tiers() {
            for (difficulty, value) in xp.by_difficulty() {
                if let Some(value) = value.filter(|v| *v < 0) {
                    bail!("xp.{}.{difficulty} is {value}; XP is never negative", tier.as_str());
                }
            }
        }
        Ok(())
    }
}

impl QuestXp {
    pub fn tiers(&self) -> impl Iterator<Item = (XpTier, &TierXp)> {
        [(XpTier::Heroic, &self.heroic), (XpTier::Epic, &self.epic), (XpTier::Legendary, &self.legendary)]
            .into_iter()
            .filter_map(|(tier, xp)| xp.as_ref().map(|xp| (tier, xp)))
    }
}

impl TierXp {
    fn by_difficulty(&self) -> [(&'static str, Option<i64>); 4] {
        [("casual", self.casual), ("normal", self.normal), ("hard", self.hard), ("elite", self.elite)]
    }
}
