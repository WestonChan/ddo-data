use anyhow::{bail, Context, Result};
use ddo_model::enums::CraftingTier;
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CraftingSystem {
    pub name: String,
    pub page: String,
    pub read: String,
    pub pack: Option<String>,
    pub families: Vec<String>,
    pub npc: Option<String>,
    #[serde(default, rename = "ingredient")]
    pub ingredients: Vec<CraftingIngredient>,
    #[serde(default, rename = "recipe")]
    pub recipes: Vec<CraftingRecipe>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CraftingIngredient {
    pub name: String,
    pub tier: String,
    pub bind: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CraftingRecipe {
    pub tier: String,
    pub slot: Option<String>,
    pub option: String,
    #[serde(default)]
    pub augments: Vec<String>,
    pub note: Option<String>,
    #[serde(default)]
    pub cost: Vec<IngredientCost>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
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

    pub(super) fn validate(&self) -> Result<()> {
        if self.families.is_empty() {
            bail!("families is empty; name the Maetrim augment families this system's options live in");
        }
        let mut declared_ingredient_keys = HashSet::new();
        for ingredient in &self.ingredients {
            validate_tier(&ingredient.tier).with_context(|| format!("ingredient {:?}", ingredient.name))?;
            if !declared_ingredient_keys.insert((ingredient.name.as_str(), ingredient.tier.as_str())) {
                bail!("ingredient {:?} is declared twice at tier {:?}", ingredient.name, ingredient.tier);
            }
        }
        for recipe in &self.recipes {
            self.validate_recipe(recipe).with_context(|| format!("recipe {:?}", recipe.option))?;
        }
        Ok(())
    }

    fn validate_recipe(&self, recipe: &CraftingRecipe) -> Result<()> {
        validate_tier(&recipe.tier)?;
        if recipe.augments.is_empty() && recipe.note.is_none() {
            bail!("augments is empty, so a note must say why the wiki row has no augment counterpart");
        }
        let mut costed_ingredient_keys = HashSet::new();
        for cost in &recipe.cost {
            if cost.quantity <= 0 {
                bail!("cost of {:?} has quantity {}; quantities are positive integers", cost.ingredient, cost.quantity);
            }
            let ingredient = self.ingredient_for(recipe, &cost.ingredient)?;
            if !costed_ingredient_keys.insert((ingredient.name.as_str(), ingredient.tier.as_str())) {
                bail!("ingredient {:?} appears twice in the cost", cost.ingredient);
            }
        }
        Ok(())
    }
}

fn validate_tier(tier: &str) -> Result<()> {
    if CraftingTier::parse(tier).is_none() {
        let allowed_tiers: Vec<&str> = CraftingTier::ALL.iter().map(|t| t.as_str()).collect();
        bail!("tier {tier:?} must be one of {}", allowed_tiers.join(", "));
    }
    Ok(())
}
