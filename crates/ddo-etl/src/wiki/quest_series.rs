use anyhow::{bail, Result};
use ddo_model::enums::SagaTier;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WikiQuestSeries<Reward> {
    pub name: String,
    pub page: String,
    pub read: String,
    pub pack: Option<String>,
    #[serde(default)]
    pub quests: Vec<String>,
    #[serde(default = "Vec::new")]
    pub rewards: Vec<Reward>,
    #[serde(skip)]
    pub file_name: String,
}

pub type WikiQuestChain = WikiQuestSeries<QuestChainReward>;
pub type WikiSaga = WikiQuestSeries<SagaReward>;

pub trait QuestSeriesReward: DeserializeOwned {
    fn name(&self) -> &str;
    fn is_rare(&self) -> bool;
    fn tier_name(&self) -> Option<&str>;
    fn tier(&self) -> Option<SagaTier> {
        self.tier_name().map(|name| SagaTier::ALL.iter().copied().find(|t| t.as_str() == name).expect("validated tier"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum QuestChainReward {
    Named(String),
    Described(DescribedQuestChainReward),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DescribedQuestChainReward {
    pub name: String,
    #[serde(default)]
    pub rare: bool,
}

impl QuestSeriesReward for QuestChainReward {
    fn name(&self) -> &str {
        match self {
            Self::Named(name) => name,
            Self::Described(reward) => &reward.name,
        }
    }

    fn is_rare(&self) -> bool {
        matches!(self, Self::Described(reward) if reward.rare)
    }

    fn tier_name(&self) -> Option<&str> {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum SagaReward {
    Named(String),
    Described(DescribedSagaReward),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DescribedSagaReward {
    pub name: String,
    pub tier: Option<String>,
    #[serde(default)]
    pub rare: bool,
}

impl QuestSeriesReward for SagaReward {
    fn name(&self) -> &str {
        match self {
            Self::Named(name) => name,
            Self::Described(reward) => &reward.name,
        }
    }

    fn is_rare(&self) -> bool {
        matches!(self, Self::Described(reward) if reward.rare)
    }

    fn tier_name(&self) -> Option<&str> {
        match self {
            Self::Named(_) => None,
            Self::Described(reward) => reward.tier.as_deref(),
        }
    }
}

impl<Reward: QuestSeriesReward> WikiQuestSeries<Reward> {
    pub(super) fn validate(&self) -> Result<()> {
        let mut listed_quest_names = HashSet::new();
        for quest_name in &self.quests {
            if !listed_quest_names.insert(quest_name.as_str()) {
                bail!("quest {quest_name:?} is listed twice; list each quest once, in order");
            }
        }
        let mut listed_rewards = HashSet::new();
        for reward in &self.rewards {
            if let Some(tier_name) = reward.tier_name() {
                if SagaTier::ALL.iter().all(|tier| tier.as_str() != tier_name) {
                    let tier_names: Vec<&str> = SagaTier::ALL.iter().map(|tier| tier.as_str()).collect();
                    bail!("reward {:?}: tier {tier_name:?} must be one of {}", reward.name(), tier_names.join(", "));
                }
            }
            if !listed_rewards.insert((reward.name(), reward.tier_name())) {
                bail!("reward {:?} is listed twice at one tier", reward.name());
            }
        }
        Ok(())
    }
}
