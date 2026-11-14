use super::wiki::OverlayReferences;
use crate::wiki::{QuestSeriesReward, WikiOverrides, WikiQuestSeries};
use anyhow::Result;
use rusqlite::Transaction;
use std::collections::BTreeSet;

struct PreviousReferences {
    known: BTreeSet<(String, String)>,
    early: bool,
}

impl PreviousReferences {
    fn contains(&self, table: &str, name: &str) -> bool {
        (self.early && !matches!(table, "quests" | "adventure_packs" | "patrons"))
            || self.known.contains(&(table.into(), name.into()))
    }

    fn retain_known_name(&self, table: &str, name: &mut Option<String>) {
        if name.as_ref().is_some_and(|name| !self.contains(table, name)) {
            *name = None;
        }
    }

    fn retain_known_series_references<Reward: QuestSeriesReward>(&self, series: &mut WikiQuestSeries<Reward>) {
        self.retain_known_name("adventure_packs", &mut series.pack);
        series.quests.retain(|name| self.contains("quests", name));
        series.rewards.retain(|reward| self.contains("items", reward.name()));
    }

    fn has_unlock_references(&self, unlock: &crate::wiki::CraftingUnlock) -> bool {
        [
            ("quests", &unlock.quest),
            ("patrons", &unlock.patron),
            ("adventure_packs", &unlock.pack),
            ("items", &unlock.item),
            ("crafting_ingredients", &unlock.crafting_ingredient),
            ("feats", &unlock.feat),
        ]
        .into_iter()
        .all(|(table, name)| name.as_deref().is_none_or(|name| self.contains(table, name)))
    }
}

pub(super) fn omit_unresolved_previous_facts(
    transaction: &Transaction,
    references: &OverlayReferences,
    wiki: &mut WikiOverrides,
    early: bool,
) -> Result<()> {
    let mut known = references.known_names(transaction)?;
    known.extend(wiki.items.iter().map(|item| ("items".into(), item.name.clone())));
    known.extend(wiki.augments.iter().map(|augment| ("augments".into(), augment.name.clone())));
    known.extend(
        wiki.crafting_ingredients.iter().map(|ingredient| ("crafting_ingredients".into(), ingredient.name.clone())),
    );
    let mut references = PreviousReferences { known, early };
    wiki.quests.retain(|quest| {
        let valid = quest.pack.as_ref().is_none_or(|name| references.contains("adventure_packs", name))
            && quest.patron.as_ref().is_none_or(|name| references.contains("patrons", name));
        if valid {
            references.known.insert(("quests".into(), quest.name.clone()));
        }
        valid
    });
    wiki.quest_loot.retain_mut(|quest| {
        if !references.contains("quests", &quest.name) {
            return false;
        }
        for (drops, table) in [(&mut quest.items, "items"), (&mut quest.augments, "augments")] {
            drops.retain(|drop| references.contains(table, drop.name()));
        }
        for (drops, table) in [(&mut quest.rare, "items"), (&mut quest.rare_augments, "augments")] {
            drops.retain(|drop| references.contains(table, drop.name()));
        }
        true
    });
    for item in &mut wiki.items {
        references.retain_known_name("set_bonuses", &mut item.set);
        item.augment_slots.retain(|name| references.contains("augment_slot_types", name));
    }
    for augment in &mut wiki.augments {
        references.retain_known_name("set_bonuses", &mut augment.set);
        augment.slots.retain(|name| references.contains("augment_slot_types", name));
    }
    for station in &mut wiki.crafting_stations {
        references.retain_known_name("quests", &mut station.quest);
        station.unlocks.retain(|unlock| references.has_unlock_references(unlock));
    }
    for system in &mut wiki.crafting_systems {
        references.retain_known_name("adventure_packs", &mut system.pack);
        system.unlocks.retain(|unlock| references.has_unlock_references(unlock));
        for step in &mut system.steps {
            references.retain_known_name("augment_slot_types", &mut step.slot);
            step.costs.retain(|cost| cost.cost.item.as_deref().is_none_or(|name| references.contains("items", name)));
        }
        for recipe in &mut system.recipes {
            recipe.unlocks.retain(|unlock| references.has_unlock_references(unlock));
            recipe.cost.retain(|cost| cost.item.as_deref().is_none_or(|name| references.contains("items", name)));
            references.retain_known_name("augment_slot_types", &mut recipe.grants_slot);
            recipe.augments.retain(|name| references.contains("augments", name));
        }
    }
    for chain in &mut wiki.quest_chains {
        references.retain_known_series_references(chain);
    }
    for saga in &mut wiki.sagas {
        references.retain_known_series_references(saga);
    }
    for vendor in &mut wiki.vendors {
        references.retain_known_name("adventure_packs", &mut vendor.pack);
        vendor.items.retain(|item| references.contains("items", item.name()));
    }
    for event in &mut wiki.events {
        event.items.retain(|name| references.contains("items", name));
    }
    Ok(())
}
