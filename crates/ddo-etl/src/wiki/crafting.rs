use anyhow::{bail, Context, Result};
use ddo_model::enums::CraftingTier;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CraftingSystem {
    pub name: String,
    pub wiki_url: String,
    pub pack: Option<String>,
    pub families: Vec<String>,
    pub npc: Option<String>,
    pub ingredients: Vec<CraftingIngredient>,
    pub recipes: Vec<CraftingRecipe>,
    pub file_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CraftingIngredient {
    pub name: String,
    pub tier: String,
    pub bind: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CraftingRecipe {
    pub tier: String,
    pub slot: Option<String>,
    pub option: String,
    pub augments: Vec<String>,
    pub note: Option<String>,
    pub grants_slot: Option<String>,
    pub cost: Vec<IngredientCost>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngredientCost {
    pub ingredient: String,
    pub quantity: i64,
}

impl CraftingSystem {
    pub fn ingredient_for(&self, recipe: &CraftingRecipe, ingredient_name: &str) -> Result<&CraftingIngredient> {
        let ingredients_with_name: Vec<&CraftingIngredient> =
            self.ingredients.iter().filter(|i| i.name == ingredient_name).collect();
        match ingredients_with_name.as_slice() {
            [] => bail!("ingredient {ingredient_name:?} is not declared in this system's [[system.ingredient]] list"),
            [only_ingredient] => Ok(only_ingredient),
            _ => ingredients_with_name
                .iter()
                .find(|i| i.tier == recipe.tier)
                .or_else(|| ingredients_with_name.iter().find(|i| i.tier == CraftingTier::Any.as_str()))
                .copied()
                .with_context(|| {
                    let tiers: Vec<&str> = ingredients_with_name.iter().map(|i| i.tier.as_str()).collect();
                    format!(
                        "ingredient {ingredient_name:?} is declared at tiers {}, and the recipe's tier {:?} picks none of them",
                        tiers.join(", "),
                        recipe.tier
                    )
                }),
        }
    }
}
