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
    pub ingredients: Vec<Ingredient>,
    #[serde(default, rename = "recipe")]
    pub recipes: Vec<Recipe>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ingredient {
    pub name: String,
    pub tier: String,
    pub bind: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub tier: String,
    pub slot: Option<String>,
    pub option: String,
    #[serde(default)]
    pub augments: Vec<String>,
    pub note: Option<String>,
    #[serde(default)]
    pub cost: Vec<Cost>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cost {
    pub ingredient: String,
    pub quantity: i64,
}

impl CraftingSystem {
    pub fn ingredient_for(&self, recipe: &Recipe, name: &str) -> Result<&Ingredient> {
        let named: Vec<&Ingredient> = self.ingredients.iter().filter(|i| i.name == name).collect();
        match named.as_slice() {
            [] => bail!("ingredient {name:?} is not declared in this system's [[system.ingredient]] list"),
            [only] => Ok(only),
            _ => named
                .iter()
                .find(|i| i.tier == recipe.tier)
                .or_else(|| named.iter().find(|i| i.tier == CraftingTier::Any.as_str()))
                .copied()
                .with_context(|| {
                    let tiers: Vec<&str> = named.iter().map(|i| i.tier.as_str()).collect();
                    format!(
                        "ingredient {name:?} is declared at tiers {}, and the recipe's tier {:?} picks none of them",
                        tiers.join(", "),
                        recipe.tier
                    )
                }),
        }
    }

    pub(super) fn check_values(&self) -> Result<()> {
        if self.families.is_empty() {
            bail!("families is empty; name the Maetrim augment families this system's options live in");
        }
        let mut declared = HashSet::new();
        for ingredient in &self.ingredients {
            check_tier(&ingredient.tier).with_context(|| format!("ingredient {:?}", ingredient.name))?;
            if !declared.insert((ingredient.name.as_str(), ingredient.tier.as_str())) {
                bail!("ingredient {:?} is declared twice at tier {:?}", ingredient.name, ingredient.tier);
            }
        }
        for recipe in &self.recipes {
            self.check_recipe(recipe).with_context(|| format!("recipe {:?}", recipe.option))?;
        }
        Ok(())
    }

    fn check_recipe(&self, recipe: &Recipe) -> Result<()> {
        check_tier(&recipe.tier)?;
        if recipe.augments.is_empty() && recipe.note.is_none() {
            bail!("augments is empty, so a note must say why the wiki row has no augment counterpart");
        }
        let mut costed = HashSet::new();
        for cost in &recipe.cost {
            if cost.quantity <= 0 {
                bail!("cost of {:?} has quantity {}; quantities are positive integers", cost.ingredient, cost.quantity);
            }
            let ingredient = self.ingredient_for(recipe, &cost.ingredient)?;
            if !costed.insert((ingredient.name.as_str(), ingredient.tier.as_str())) {
                bail!("ingredient {:?} appears twice in the cost", cost.ingredient);
            }
        }
        Ok(())
    }
}

fn check_tier(tier: &str) -> Result<()> {
    if CraftingTier::parse(tier).is_none() {
        let allowed: Vec<&str> = CraftingTier::ALL.iter().map(|t| t.as_str()).collect();
        bail!("tier {tier:?} must be one of {}", allowed.join(", "));
    }
    Ok(())
}
