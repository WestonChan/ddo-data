use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct CraftingSystemRecipeAugment in "crafting_recipe_augments" {
        context { augment_id: i64 }
        keys { recipe_id: i64, sort_order: i64 }
        fields {
            name: String => augment_id [resolved_reference "augments" augment_id],
        }
        projected {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            family: Option<String> => "(SELECT family FROM augments WHERE id = crafting_recipe_augments.augment_id)",
            level: i64 => "(SELECT min_level FROM augments WHERE id = crafting_recipe_augments.augment_id)",
            #[serde(default, skip_serializing_if = "Option::is_none")]
            description: Option<String> => "(SELECT description FROM augments WHERE id = crafting_recipe_augments.augment_id AND (SELECT COUNT(*) FROM augments matching WHERE matching.name = augments.name AND matching.family = augments.family AND matching.min_level = augments.min_level) > 1)",
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct CraftingSystemRecipeIngredient in "crafting_recipe_ingredients" {
        context { ingredient_id: i64 }
        keys { recipe_id: i64, sort_order: i64 }
        fields {
            ingredient: String => ingredient_id [resolved_reference "crafting_ingredients" ingredient_id],
            quantity: i64 => quantity,
        }
        projected {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            tier: Option<String> => "(SELECT tier FROM crafting_ingredients WHERE id = crafting_recipe_ingredients.ingredient_id AND (SELECT COUNT(*) FROM crafting_ingredients matching WHERE matching.system_id = crafting_ingredients.system_id AND matching.name = crafting_ingredients.name) > 1)",
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct CraftingSystemFamily in "crafting_system_families" {
        keys { system_id: i64 }
        fields {
            family: String => family,
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct CraftingSystemIngredient in "crafting_ingredients" {
        keys { id: i64, slug: &str, system_id: i64, sort_order: i64 }
        fields {
            name: String => name,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            bind: Option<String> => bind,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            drop_location: Option<String> => source,
            tier: String => tier,
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            citation: Vec<crate::tree::Citation>,
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct CraftingSystemRecipe in "crafting_recipes" {
        keys { id: Option<i64>, system_id: i64, sort_order: i64 }
        fields {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            focus_path: Option<String> => focus_path,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            grants_slot: Option<String> => grants_slot_id [optional_reference "augment_slot_types" "label"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            note: Option<String> => note,
            option: String => option,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            slot: Option<String> => slot_id [optional_reference "augment_slot_types" "label"],
            tier: String => tier,
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            augment: Vec<CraftingSystemRecipeAugment>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            ingredient: Vec<CraftingSystemRecipeIngredient>,
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct CraftingSystemFile in "crafting_systems" {
        keys { id: i64, slug: &str }
        fields {
            name: String => name,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            npc: Option<String> => npc,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pack: Option<String> => pack_id [optional_reference "adventure_packs"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            page: Option<String> => page [citation_page],
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            family: Vec<CraftingSystemFamily>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            ingredient: Vec<CraftingSystemIngredient>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            removed: Vec<super::super::RemovedEntry>,
            citation: Vec<super::super::Citation>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            recipe: Vec<CraftingSystemRecipe>,
        }
    }
}

impl CraftingSystemFile {
    pub(in crate::tree) fn all_from_rows(
        db: &rusqlite::Connection,
        annotations: &super::super::annotations::EntityAnnotations,
    ) -> anyhow::Result<std::collections::BTreeMap<i64, Self>> {
        use anyhow::Context;
        let mut files = std::collections::BTreeMap::new();
        let mut select = db.prepare(&format!("SELECT {} FROM crafting_systems ORDER BY id", Self::select_columns()))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("id")?, Self::from_row(row)?)))? {
            let (id, mut file) = row?;
            let annotation = annotations.entity("crafting_systems", id).context("crafting system has no citation")?;
            let primary = annotation
                .citation
                .iter()
                .find(|citation| citation.field.is_empty())
                .context("crafting system has no primary citation")?;
            file.page = file.page.filter(|page| Some(page.as_str()) != primary.source.page_url());
            file.attach_citations(annotations, id);
            file.removed = annotation.removed.clone();
            files.insert(id, file);
        }
        let mut select = db.prepare(&format!(
            "SELECT {} FROM crafting_system_families ORDER BY rowid",
            CraftingSystemFamily::select_columns()
        ))?;
        for row in
            select.query_map([], |row| Ok((row.get::<_, i64>("system_id")?, CraftingSystemFamily::from_row(row)?)))?
        {
            let (id, record) = row?;
            files.get_mut(&id).context("crafting family has no system")?.family.push(record);
        }
        let mut select = db.prepare(&format!(
            "SELECT {} FROM crafting_ingredients ORDER BY sort_order, rowid",
            CraftingSystemIngredient::select_columns()
        ))?;
        for row in select.query_map([], |row| {
            Ok((row.get::<_, i64>("system_id")?, {
                let mut ingredient = CraftingSystemIngredient::from_row(row)?;
                ingredient.attach_citations(annotations, row.get("id")?);
                ingredient
            }))
        })? {
            let (id, record) = row?;
            files.get_mut(&id).context("crafting ingredient has no system")?.ingredient.push(record);
        }
        let mut recipes = db.prepare(&format!(
            "SELECT {} FROM crafting_recipes ORDER BY sort_order, rowid",
            CraftingSystemRecipe::select_columns()
        ))?;
        let mut recipe_locations = std::collections::BTreeMap::new();
        for row in recipes.query_map([], |row| {
            Ok((row.get::<_, i64>("system_id")?, row.get::<_, i64>("id")?, CraftingSystemRecipe::from_row(row)?))
        })? {
            let (system_id, id, recipe) = row?;
            let system = files.get_mut(&system_id).context("crafting recipe has no system")?;
            recipe_locations.insert(id, (system_id, system.recipe.len()));
            system.recipe.push(recipe);
        }
        let mut outputs = db.prepare(&format!(
            "SELECT {} FROM crafting_recipe_augments ORDER BY sort_order, rowid",
            CraftingSystemRecipeAugment::select_columns()
        ))?;
        for row in outputs
            .query_map([], |row| Ok((row.get::<_, i64>("recipe_id")?, CraftingSystemRecipeAugment::from_row(row)?)))?
        {
            let (id, mut output) = row?;
            let &(system_id, position) = recipe_locations.get(&id).context("crafting output has no recipe")?;
            let system = files.get_mut(&system_id).unwrap();
            if system.family.len() == 1 {
                anyhow::ensure!(
                    output.family.as_deref() == Some(system.family[0].family.as_str()),
                    "recipe augment family disagrees with {:?}",
                    system.name
                );
                output.family = None;
            }
            system.recipe[position].augment.push(output);
        }
        let mut costs = db.prepare(&format!("SELECT {}, (SELECT system_id FROM crafting_ingredients WHERE id = crafting_recipe_ingredients.ingredient_id) AS ingredient_system FROM crafting_recipe_ingredients ORDER BY sort_order, rowid", CraftingSystemRecipeIngredient::select_columns()))?;
        for row in costs.query_map([], |row| {
            Ok((
                row.get::<_, i64>("recipe_id")?,
                row.get::<_, i64>("ingredient_system")?,
                CraftingSystemRecipeIngredient::from_row(row)?,
            ))
        })? {
            let (id, ingredient_system, cost) = row?;
            let &(system_id, position) = recipe_locations.get(&id).context("crafting cost has no recipe")?;
            anyhow::ensure!(system_id == ingredient_system, "recipe ingredient belongs to another system");
            files.get_mut(&system_id).unwrap().recipe[position].ingredient.push(cost);
        }
        Ok(files)
    }

    pub(in crate::tree) fn file_text(&self) -> anyhow::Result<String> {
        super::super::typed_file_text(self)
    }

    pub(in crate::tree) fn write_crafting_system(
        &self,
        db: &rusqlite::Connection,
        path: &std::path::Path,
        ids: &mut super::super::ids::StableIds,
    ) -> anyhow::Result<i64> {
        let identity = super::super::slug(&self.name);
        let id = ids.issue("crafting_systems", &identity, path)?;
        self.insert(db, id, &identity)?;
        for family in &self.family {
            family.insert(db, id)?;
        }
        for (position, ingredient) in self.ingredient.iter().enumerate() {
            let ingredient_slug = format!(
                "{}-{}-{}",
                identity,
                super::super::slug(&ingredient.name),
                super::super::slug(&ingredient.tier)
            );
            let ingredient_id = ids.issue("crafting_ingredients", &ingredient_slug, path)?;
            ingredient.insert(db, ingredient_id, &ingredient_slug, id, position as i64)?;
        }
        Ok(id)
    }

    pub(in crate::tree) fn insert_recipes(
        &self,
        db: &rusqlite::Connection,
        path: &std::path::Path,
        id: i64,
    ) -> anyhow::Result<usize> {
        use anyhow::Context;
        let mut count = 0;
        for (position, recipe) in self.recipe.iter().enumerate() {
            recipe.insert(db, None, id, position as i64)?;
            let recipe_id = db.last_insert_rowid();
            count += 1;
            for (position, output) in recipe.augment.iter().enumerate() {
                let family = output
                    .family
                    .as_deref()
                    .or_else(|| (self.family.len() == 1).then(|| self.family[0].family.as_str()))
                    .with_context(|| format!("{}: recipe augment {:?} needs a family", path.display(), output.name))?;
                let mut select = db.prepare_cached(
                    "SELECT id FROM augments WHERE name = ?1 AND family = ?2 AND min_level = ?3 AND (?4 IS NULL OR description = ?4)")?;
                let matches = select
                    .query_map(rusqlite::params![output.name, family, output.level, output.description], |row| {
                        row.get::<_, i64>(0)
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                anyhow::ensure!(
                    matches.len() == 1,
                    "{}: recipe augment {:?} in {family:?} at level {} matched {} rows",
                    path.display(),
                    output.name,
                    output.level,
                    matches.len()
                );
                output.insert(db, matches[0], recipe_id, position as i64)?;
                count += 1;
            }
            for (position, cost) in recipe.ingredient.iter().enumerate() {
                let mut select = db.prepare_cached("SELECT id FROM crafting_ingredients WHERE system_id = ?1 AND name = ?2 AND (?3 IS NULL OR tier = ?3)")?;
                let matches = select
                    .query_map(rusqlite::params![id, cost.ingredient, cost.tier], |row| row.get::<_, i64>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                anyhow::ensure!(
                    matches.len() == 1,
                    "{}: recipe ingredient {:?} matched {} rows; specify its tier when ambiguous",
                    path.display(),
                    cost.ingredient,
                    matches.len()
                );
                cost.insert(db, matches[0], recipe_id, position as i64)?;
                count += 1;
            }
        }
        Ok(count)
    }
}
