mod augments;
mod bonuses;
mod crafting;
mod database;
mod events;
mod items;
mod quest_loot;
mod quest_series;
mod quests;
mod vendors;

pub use augments::WikiAugment;
pub use bonuses::WikiBonus;
pub use crafting::{
    CraftingIngredient, CraftingRecipe, CraftingStation, CraftingSystem, CraftingSystemStation, CraftingUnlock,
    IngredientCost,
};
pub use crafting::{CraftingRecipeSetOption, CraftingStep, CraftingStepCost, CraftingStepEffect};
pub use events::WikiEvent;
pub use items::{WikiArmorStats, WikiItem, WikiItemEffect, WikiWeaponStats};
pub use quest_loot::{DescribedListedDrop, ListedDrop, QuestLoot, RareDrop, RareDropInChest};
pub use quest_series::{
    DescribedQuestChainReward, DescribedSagaReward, QuestChainReward, QuestSeriesReward, SagaReward, WikiQuestChain,
    WikiQuestSeries, WikiSaga,
};
pub use quests::WikiQuest;
pub use vendors::{PricedVendorItem, VendorItem, WikiVendor};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct WikiOverrides {
    pub socket_option_sets: crate::build::ExistingSocketOptionSets,
    pub per_item_slot_types: Vec<PerItemSlotType>,
    pub retired_socket_labels: std::collections::BTreeSet<String>,
    pub quest_loot: Vec<QuestLoot>,
    pub quests: Vec<WikiQuest>,
    pub crafting_systems: Vec<CraftingSystem>,
    pub crafting_ingredients: Vec<CraftingIngredient>,
    pub crafting_stations: Vec<CraftingStation>,
    pub items: Vec<WikiItem>,
    pub augments: Vec<WikiAugment>,
    pub quest_chains: Vec<WikiQuestChain>,
    pub sagas: Vec<WikiSaga>,
    pub vendors: Vec<WikiVendor>,
    pub events: Vec<WikiEvent>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PerItemSlotType {
    pub slug: String,
    pub slot_type: crate::map::augment_slot::AugmentSlotType,
}
