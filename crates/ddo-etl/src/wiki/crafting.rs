#[derive(Debug, Clone, PartialEq)]
pub struct CraftingSystem {
    pub name: String,
    pub wiki_url: String,
    pub pack: Option<String>,
    pub families: Vec<String>,
    pub recipes: Vec<CraftingRecipe>,
    pub stations: Vec<CraftingSystemStation>,
    pub unlocks: Vec<CraftingUnlock>,
    pub steps: Vec<CraftingStep>,
    pub file_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CraftingStep {
    pub name: String,
    pub slot: Option<String>,
    pub is_random: bool,
    pub consumed_outputs: Option<i64>,
    pub effects: Vec<CraftingStepEffect>,
    pub(crate) modifiers: Vec<crate::tree::Modifier>,
    pub costs: Vec<CraftingStepCost>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CraftingStepEffect {
    pub effect: String,
    pub bonus_type: Option<String>,
    pub alias: Option<String>,
    pub value: Option<i64>,
    pub value2: Option<i64>,
    pub equipment_slot: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CraftingStepCost {
    pub tier: String,
    pub cost: IngredientCost,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CraftingStation {
    pub name: String,
    pub variant: Option<String>,
    pub kind: ddo_model::enums::StationKind,
    pub place: Option<String>,
    pub quest: Option<String>,
    pub unlocks: Vec<CraftingUnlock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CraftingSystemStation {
    pub name: String,
    pub variant: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CraftingUnlock {
    pub kind: ddo_model::enums::CraftingUnlockKind,
    pub quest: Option<String>,
    pub patron: Option<String>,
    pub pack: Option<String>,
    pub item: Option<String>,
    pub crafting_ingredient: Option<String>,
    pub feat: Option<String>,
    pub variant: Option<String>,
    pub value: Option<i64>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CraftingIngredient {
    pub name: String,
    pub variant: Option<String>,
    pub tier: String,
    pub is_item_placeholder: bool,
    pub bind: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CraftingRecipe {
    pub tier: String,
    pub step: Option<String>,
    pub yields_ingredient: Option<String>,
    pub yields_variant: Option<String>,
    pub option: String,
    pub augments: Vec<String>,
    pub note: Option<String>,
    pub grants_slot: Option<String>,
    pub cost: Vec<IngredientCost>,
    pub unlocks: Vec<CraftingUnlock>,
    pub set_options: Vec<CraftingRecipeSetOption>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CraftingRecipeSetOption {
    pub set: String,
    pub variant: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngredientCost {
    pub ingredient: Option<String>,
    pub item: Option<String>,
    pub variant: Option<String>,
    pub quantity: i64,
}
