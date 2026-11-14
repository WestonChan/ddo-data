use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(crate) enum CraftingUnlockOwner {
    System(i64),
    Station(i64),
    Recipe(i64),
}

impl CraftingUnlockOwner {
    fn system_id(&self) -> Option<i64> {
        match self {
            Self::System(id) => Some(*id),
            _ => None,
        }
    }

    fn station_id(&self) -> Option<i64> {
        match self {
            Self::Station(id) => Some(*id),
            _ => None,
        }
    }

    fn recipe_id(&self) -> Option<i64> {
        match self {
            Self::Recipe(id) => Some(*id),
            _ => None,
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct CraftingUnlock in "crafting_unlocks" {
        context { item_id: Option<i64>, crafting_ingredient_id: Option<i64>, owner: CraftingUnlockOwner }
        keys { id: Option<i64>, sort_order: i64 }
        values { system_id => owner.system_id(), station_id => owner.station_id(), recipe_id => owner.recipe_id() }
        fields {
            kind: ddo_model::enums::CraftingUnlockKind => kind [enum_text],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            quest: Option<String> => quest_id [optional_reference "quests"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            patron: Option<String> => patron_id [optional_reference "patrons"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pack: Option<String> => pack_id [optional_reference "adventure_packs"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            feat: Option<String> => feat_id [optional_reference "feats"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            item: Option<String> => item_id [resolved_reference "items" item_id],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            crafting_ingredient: Option<String> => crafting_ingredient_id [resolved_reference "crafting_ingredients" crafting_ingredient_id],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            value: Option<i64> => value,
            text: String => text,
        }
        projected {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            variant: Option<String> => "COALESCE((SELECT variant FROM items WHERE id=crafting_unlocks.item_id),(SELECT variant FROM crafting_ingredients WHERE id=crafting_unlocks.crafting_ingredient_id))",
        }
    }
}

impl CraftingUnlock {
    pub(crate) fn write_unlock(
        &self,
        db: &rusqlite::Connection,
        owner: CraftingUnlockOwner,
        position: i64,
    ) -> anyhow::Result<()> {
        use anyhow::Context;
        let item = self
            .item
            .as_deref()
            .map(|name| crate::tree::crafting::qualified_reference_id(db, "items", name, self.variant.as_deref()))
            .transpose()?;
        let ingredient = self
            .crafting_ingredient
            .as_deref()
            .map(|name| crate::tree::crafting::ingredient_id(db, name, self.variant.as_deref()))
            .transpose()?;
        self.insert(db, item, ingredient, owner, None, position)
            .with_context(|| format!("unlock {:?}: {:?}", self.kind, self.text))?;
        Ok(())
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct CraftingSystemStation in "crafting_system_stations" {
        context { station_id: i64 }
        keys { system_id: i64, sort_order: i64 }
        fields {
            name: String => station_id [resolved_reference "crafting_stations" station_id],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            note: Option<String> => note,
        }
        projected {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            variant: Option<String> => "(SELECT variant FROM crafting_stations WHERE id=crafting_system_stations.station_id)",
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct CraftingStepEffect in "crafting_step_effects" {
        keys { step_id: i64, sort_order: i64 }
        fields {
            effect: String => effect_id [reference "effects"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            bonus_type: Option<String> => bonus_type_id [optional_reference "bonus_types"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            alias: Option<String> => alias,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            value: Option<i64> => value,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            value2: Option<i64> => value2,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            equipment_slot: Option<String> => equipment_slot_id [optional_reference "equipment_slots"],
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct CraftingStep in "crafting_steps" {
        keys { id: Option<i64>, system_id: i64, sort_order: i64 }
        fields {
            name: String => name,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            slot: Option<String> => slot_id [optional_reference "augment_slot_types" "label"],
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            is_random: bool => is_random,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            consumed_outputs: Option<i64> => consumed_outputs,
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            effect: Vec<CraftingStepEffect>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            modifier: Vec<crate::tree::Modifier>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            cost: Vec<CraftingStepCost>,
        }
    }
}

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
        context { ingredient_id: Option<i64>, item_id: Option<i64> }
        keys { recipe_id: i64, sort_order: i64 }
        fields {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            ingredient: Option<String> => ingredient_id [resolved_reference "crafting_ingredients" ingredient_id],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            item: Option<String> => item_id [resolved_reference "items" item_id],
            quantity: i64 => quantity,
        }
        projected {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            variant: Option<String> => "COALESCE((SELECT variant FROM crafting_ingredients WHERE id=crafting_recipe_ingredients.ingredient_id),(SELECT variant FROM items WHERE id=crafting_recipe_ingredients.item_id))",
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct CraftingStepCost in "crafting_step_costs" {
        context { ingredient_id: Option<i64>, item_id: Option<i64> }
        keys { step_id: i64, sort_order: i64 }
        fields {
            tier: String => tier,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            ingredient: Option<String> => ingredient_id [resolved_reference "crafting_ingredients" ingredient_id],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            item: Option<String> => item_id [resolved_reference "items" item_id],
            quantity: i64 => quantity,
        }
        projected {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            variant: Option<String> => "COALESCE((SELECT variant FROM crafting_ingredients WHERE id=crafting_step_costs.ingredient_id),(SELECT variant FROM items WHERE id=crafting_step_costs.item_id))",
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
    pub(crate) struct CraftingSystemRecipeSetOption in "crafting_recipe_set_options" {
        keys { recipe_id: i64, option_id: i64 }
        fields {}
        projected {
            #[serde(rename = "set")]
            option_set: String => "(SELECT s.name FROM socket_option_sets s JOIN socket_options o ON o.set_id=s.id WHERE o.id=crafting_recipe_set_options.option_id)",
            name: String => "(SELECT name FROM socket_options WHERE id=crafting_recipe_set_options.option_id)",
            #[serde(default, skip_serializing_if = "Option::is_none")]
            set_variant: Option<String> => "(SELECT s.variant FROM socket_option_sets s JOIN socket_options o ON o.set_id=s.id WHERE o.id=crafting_recipe_set_options.option_id)",
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct CraftingSystemRecipe in "crafting_recipes" {
        context { yields_ingredient_id: Option<i64> }
        keys { id: Option<i64>, system_id: i64, sort_order: i64, step_id: Option<i64> }
        fields {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            yields_ingredient: Option<String> => yields_ingredient_id [resolved_reference "crafting_ingredients" yields_ingredient_id],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            focus_path: Option<String> => focus_path,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            grants_slot: Option<String> => grants_slot_id [optional_reference "augment_slot_types" "label"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            note: Option<String> => note,
            option: String => option,
            tier: String => tier,
        }
        projected {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            yields_variant: Option<String> => "(SELECT variant FROM crafting_ingredients WHERE id=crafting_recipes.yields_ingredient_id)",
            #[serde(default, skip_serializing_if = "Option::is_none")]
            step: Option<String> => "(SELECT name FROM crafting_steps WHERE id = crafting_recipes.step_id)",
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            set_option: Vec<CraftingSystemRecipeSetOption>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            unlock: Vec<CraftingUnlock>,
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
            pack: Option<String> => pack_id [optional_reference "adventure_packs"],
            page: String => page,
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            station: Vec<CraftingSystemStation>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            unlock: Vec<CraftingUnlock>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            step: Vec<CraftingStep>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            family: Vec<CraftingSystemFamily>,
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
        modifiers: &mut crate::tree::tables::ModifierRowsByOwner,
    ) -> anyhow::Result<std::collections::BTreeMap<i64, Self>> {
        use anyhow::Context;
        let mut files = std::collections::BTreeMap::new();
        let mut select = db.prepare(&format!("SELECT {} FROM crafting_systems ORDER BY id", Self::select_columns()))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("id")?, Self::from_row(row)?)))? {
            let (id, mut file) = row?;
            file.attach_annotations(annotations, id);
            if let Some(annotation) = annotations.entity("crafting_systems", id) {
                file.removed = annotation.removed.clone();
            }
            files.insert(id, file);
        }
        let mut stations = db.prepare(&format!(
            "SELECT {} FROM crafting_system_stations ORDER BY system_id,sort_order",
            CraftingSystemStation::select_columns()
        ))?;
        for row in stations
            .query_map([], |row| Ok((row.get::<_, i64>("system_id")?, CraftingSystemStation::from_row(row)?)))?
        {
            let (system_id, station) = row?;
            let system = files.get_mut(&system_id).context("station link has no system")?;
            system.station.push(station);
        }
        let mut steps = db.prepare(&format!(
            "SELECT {} FROM crafting_steps ORDER BY system_id,sort_order",
            CraftingStep::select_columns()
        ))?;
        let mut step_locations = std::collections::BTreeMap::new();
        for row in steps.query_map([], |row| {
            Ok((row.get::<_, i64>("system_id")?, row.get::<_, i64>("id")?, CraftingStep::from_row(row)?))
        })? {
            let (system_id, id, step) = row?;
            let system = files.get_mut(&system_id).context("crafting step has no system")?;
            step_locations.insert(id, (system_id, system.step.len()));
            system.step.push(step);
        }
        let mut lines = db.prepare(&format!(
            "SELECT {} FROM crafting_step_effects ORDER BY step_id,sort_order",
            CraftingStepEffect::select_columns()
        ))?;
        for row in lines.query_map([], |row| Ok((row.get::<_, i64>("step_id")?, CraftingStepEffect::from_row(row)?)))? {
            let (id, line) = row?;
            let (system_id, position) = step_locations[&id];
            files.get_mut(&system_id).unwrap().step[position].effect.push(line);
        }
        let mut costs = db.prepare(&format!(
            "SELECT {} FROM crafting_step_costs ORDER BY step_id,sort_order",
            CraftingStepCost::select_columns()
        ))?;
        for row in costs.query_map([], |row| Ok((row.get::<_, i64>("step_id")?, CraftingStepCost::from_row(row)?)))? {
            let (id, cost) = row?;
            let (system_id, position) = step_locations[&id];
            files.get_mut(&system_id).unwrap().step[position].cost.push(cost);
        }
        for (id, (system_id, position)) in &step_locations {
            files.get_mut(system_id).unwrap().step[*position].modifier =
                modifiers.remove(&("crafting_step".into(), *id)).unwrap_or_default();
            files.get_mut(system_id).unwrap().step[*position]
                .cost
                .sort_by_key(|cost| ddo_model::enums::CraftingTier::sort_key(&cost.tier));
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
        let mut set_options = db.prepare(&format!(
            "SELECT {} FROM crafting_recipe_set_options ORDER BY recipe_id,(SELECT s.slug FROM socket_options o JOIN socket_option_sets s ON s.id=o.set_id WHERE o.id=option_id),(SELECT option_order FROM socket_options WHERE id=option_id)",
            CraftingSystemRecipeSetOption::select_columns()
        ))?;
        for row in set_options
            .query_map([], |row| Ok((row.get::<_, i64>("recipe_id")?, CraftingSystemRecipeSetOption::from_row(row)?)))?
        {
            let (recipe, option) = row?;
            let (system, position) = recipe_locations[&recipe];
            files.get_mut(&system).unwrap().recipe[position].set_option.push(option);
        }
        let mut unlocks = db.prepare(&format!(
            "SELECT {} FROM crafting_unlocks WHERE station_id IS NULL ORDER BY sort_order,id",
            CraftingUnlock::select_columns()
        ))?;
        for row in unlocks.query_map([], |row| {
            Ok((
                row.get::<_, Option<i64>>("system_id")?,
                row.get::<_, Option<i64>>("recipe_id")?,
                CraftingUnlock::from_row(row)?,
            ))
        })? {
            let (system, recipe, unlock) = row?;
            if let Some(system) = system {
                files.get_mut(&system).context("unlock has no system")?.unlock.push(unlock);
            } else if let Some(recipe) = recipe {
                let (system, position) = recipe_locations[&recipe];
                files.get_mut(&system).unwrap().recipe[position].unlock.push(unlock);
            }
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
        let mut costs = db.prepare(&format!(
            "SELECT {} FROM crafting_recipe_ingredients ORDER BY sort_order, rowid",
            CraftingSystemRecipeIngredient::select_columns()
        ))?;
        for row in costs.query_map([], |row| {
            Ok((row.get::<_, i64>("recipe_id")?, CraftingSystemRecipeIngredient::from_row(row)?))
        })? {
            let (id, cost) = row?;
            let &(system_id, position) = recipe_locations.get(&id).context("crafting cost has no recipe")?;
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
        for (position, step) in self.step.iter().enumerate() {
            step.insert(db, None, id, position as i64)
                .map_err(|error| anyhow::anyhow!("{}: step {:?}: {error:#}", path.display(), step.name))?;
        }
        for family in &self.family {
            family.insert(db, id)?;
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
        for (position, station) in self.station.iter().enumerate() {
            let station_id = crate::tree::crafting::qualified_reference_id(
                db,
                "crafting_stations",
                &station.name,
                station.variant.as_deref(),
            )?;
            station.insert(db, station_id, id, position as i64)?;
            count += 1;
        }
        for (position, unlock) in self.unlock.iter().enumerate() {
            unlock.write_unlock(db, CraftingUnlockOwner::System(id), position as i64)?;
            count += 1;
        }
        for step in &self.step {
            let step_id: i64 = db.query_row(
                "SELECT id FROM crafting_steps WHERE system_id=?1 AND name=?2",
                rusqlite::params![id, step.name],
                |row| row.get(0),
            )?;
            for (position, line) in step.effect.iter().enumerate() {
                line.insert(db, step_id, position as i64)?;
                count += 1;
            }
            for (position, modifier) in step.modifier.iter().enumerate() {
                count += modifier.write_modifier(db, "crafting_step", step_id, position as i64)?;
            }
            let mut cost_positions = std::collections::BTreeMap::new();
            for cost in &step.cost {
                let (ingredient_id, item_id) = crate::tree::crafting::cost_references(
                    db,
                    cost.ingredient.as_deref(),
                    cost.item.as_deref(),
                    cost.variant.as_deref(),
                )?;
                let position = cost_positions.entry(&cost.tier).or_insert(0_i64);
                cost.insert(db, ingredient_id, item_id, step_id, *position)?;
                *position += 1;
                count += 1;
            }
        }
        for (position, recipe) in self.recipe.iter().enumerate() {
            let step_id = recipe
                .step
                .as_ref()
                .map(|name| {
                    use rusqlite::OptionalExtension;
                    db.query_row(
                        "SELECT id FROM crafting_steps WHERE system_id=?1 AND name=?2",
                        rusqlite::params![id, name],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()?
                    .with_context(|| {
                        format!(
                            "{}: recipe step {name:?} matched 0 rows; nearest: {}",
                            path.display(),
                            crate::tree::schemas::nearest_names(name, self.step.iter().map(|step| step.name.as_str()))
                                .join(", ")
                        )
                    })
                })
                .transpose()?;
            let yielded_ingredient = recipe
                .yields_ingredient
                .as_deref()
                .map(|name| crate::tree::crafting::ingredient_id(db, name, recipe.yields_variant.as_deref()))
                .transpose()?;
            recipe.insert(db, yielded_ingredient, None, id, position as i64, step_id)?;
            let recipe_id = db.last_insert_rowid();
            for option in &recipe.set_option {
                let option_id = crate::tree::crafting::recipe_set_option_id(
                    db,
                    step_id,
                    &option.option_set,
                    option.set_variant.as_deref(),
                    &option.name,
                )?;
                option.insert(db, recipe_id, option_id)?;
                count += 1;
            }
            for (position, unlock) in recipe.unlock.iter().enumerate() {
                unlock.write_unlock(db, CraftingUnlockOwner::Recipe(recipe_id), position as i64)?;
                count += 1;
            }
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
                let (ingredient_id, item_id) = crate::tree::crafting::cost_references(
                    db,
                    cost.ingredient.as_deref(),
                    cost.item.as_deref(),
                    cost.variant.as_deref(),
                )?;
                cost.insert(db, ingredient_id, item_id, recipe_id, position as i64)?;
                count += 1;
            }
            crate::tree::reject_repeated_step_costs(db, recipe_id)?;
        }
        Ok(count)
    }
}
