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
        self.duration.as_deref().and_then(QuestDuration::parse)
    }

    pub(super) fn validate(&self) -> Result<()> {
        if let Some(duration_text) = &self.duration {
            if QuestDuration::parse(duration_text).is_none() {
                let allowed_durations: Vec<&str> = QuestDuration::ALL.iter().map(|d| d.as_str()).collect();
                bail!("duration {duration_text:?} must be one of {}", allowed_durations.join(", "));
            }
        }
        for (tier, tier_xp) in self.xp.by_tier() {
            for (difficulty, xp) in tier_xp.by_difficulty() {
                if let Some(negative_xp) = xp.filter(|v| *v < 0) {
                    bail!("xp.{}.{difficulty} is {negative_xp}; XP is never negative", tier.as_str());
                }
            }
        }
        Ok(())
    }
}

impl QuestXp {
    pub fn by_tier(&self) -> impl Iterator<Item = (XpTier, &TierXp)> {
        [(XpTier::Heroic, &self.heroic), (XpTier::Epic, &self.epic), (XpTier::Legendary, &self.legendary)]
            .into_iter()
            .filter_map(|(tier, tier_xp)| tier_xp.as_ref().map(|tier_xp| (tier, tier_xp)))
    }
}

impl TierXp {
    fn by_difficulty(&self) -> [(&'static str, Option<i64>); 4] {
        [("casual", self.casual), ("normal", self.normal), ("hard", self.hard), ("elite", self.elite)]
    }
}
