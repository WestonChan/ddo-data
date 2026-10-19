mod augments;
mod bonuses;
mod crafting;
mod database;
mod descriptions;
mod events;
mod items;
mod quest_loot;
mod quest_series;
mod quests;
mod vendors;

pub use augments::WikiAugment;
pub use bonuses::WikiBonus;
pub use crafting::{CraftingIngredient, CraftingRecipe, CraftingSystem, IngredientCost};
pub use descriptions::{DescriptionKind, WikiDescription};
pub use events::WikiEvent;
pub use items::{WikiArmorStats, WikiItem, WikiItemEffect, WikiItemQuest, WikiWeaponStats};
pub use quest_loot::{DescribedListedDrop, ListedDrop, QuestLoot, RareDrop, RareDropInChest};
pub use quest_series::{
    DescribedQuestChainReward, DescribedSagaReward, QuestChainReward, QuestSeriesReward, SagaReward, WikiQuestChain,
    WikiQuestSeries, WikiSaga,
};
pub use quests::WikiQuest;
pub use vendors::{PricedVendorItem, VendorItem, WikiVendor};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct WikiOverrides {
    pub quest_loot: Vec<QuestLoot>,
    pub quests: Vec<WikiQuest>,
    pub crafting_systems: Vec<CraftingSystem>,
    pub descriptions: Vec<WikiDescription>,
    pub items: Vec<WikiItem>,
    pub augments: Vec<WikiAugment>,
    pub quest_chains: Vec<WikiQuestChain>,
    pub sagas: Vec<WikiSaga>,
    pub vendors: Vec<WikiVendor>,
    pub events: Vec<WikiEvent>,
}
