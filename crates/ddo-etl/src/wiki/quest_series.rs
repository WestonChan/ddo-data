use ddo_model::enums::SagaTier;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiQuestSeries<Reward> {
    pub name: String,
    pub wiki_url: String,
    pub pack: Option<String>,
    pub quests: Vec<String>,
    pub rewards: Vec<Reward>,
    pub file_name: String,
}

pub type WikiQuestChain = WikiQuestSeries<QuestChainReward>;
pub type WikiSaga = WikiQuestSeries<SagaReward>;

pub trait QuestSeriesReward {
    fn name(&self) -> &str;
    fn is_rare(&self) -> bool;
    fn tier_name(&self) -> Option<&str>;
    fn tier(&self) -> Option<SagaTier> {
        self.tier_name().map(|name| SagaTier::ALL.iter().copied().find(|t| t.as_str() == name).expect("validated tier"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestChainReward {
    Named(String),
    Described(DescribedQuestChainReward),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescribedQuestChainReward {
    pub name: String,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SagaReward {
    Named(String),
    Described(DescribedSagaReward),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescribedSagaReward {
    pub name: String,
    pub tier: Option<String>,
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
