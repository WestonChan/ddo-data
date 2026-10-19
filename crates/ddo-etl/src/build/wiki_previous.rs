use super::wiki::OverlayReferences;
use crate::wiki::{QuestSeriesReward, WikiOverrides, WikiQuestSeries};
use anyhow::Result;
use rusqlite::Transaction;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SkippedWikiFact {
    pub fact: String,
    pub file_name: String,
    pub unresolved_name: String,
}

struct PreviousReferences<'a> {
    known: BTreeSet<(String, String)>,
    early: bool,
    skipped: &'a mut Vec<SkippedWikiFact>,
}

impl PreviousReferences<'_> {
    fn fact(&mut self, table: &str, name: &str, file: &str, fact: String) -> bool {
        if self.early && !matches!(table, "quests" | "adventure_packs" | "patrons") {
            return true;
        }
        if self.known.contains(&(table.into(), name.into())) {
            return true;
        }
        self.skipped.push(SkippedWikiFact { fact, file_name: file.into(), unresolved_name: name.into() });
        false
    }

    fn optional(&mut self, table: &str, name: &mut Option<String>, file: &str, fact: String) {
        if name.as_ref().is_some_and(|name| !self.fact(table, name, file, fact)) {
            *name = None;
        }
    }

    fn series<Reward: QuestSeriesReward>(&mut self, series: &mut WikiQuestSeries<Reward>) {
        self.optional("adventure_packs", &mut series.pack, &series.file_name, format!("{} pack", series.name));
        series
            .quests
            .retain(|name| self.fact("quests", name, &series.file_name, format!("{} quest {name}", series.name)));
        series.rewards.retain(|reward| {
            self.fact("items", reward.name(), &series.file_name, format!("{} reward {}", series.name, reward.name()))
        });
    }
}

pub(super) fn omit_unresolved_previous_facts(
    transaction: &Transaction,
    references: &OverlayReferences,
    wiki: &mut WikiOverrides,
    early: bool,
    skipped: &mut Vec<SkippedWikiFact>,
) -> Result<()> {
    let mut known = references.known_names(transaction)?;
    known.extend(wiki.items.iter().map(|item| ("items".into(), item.name.clone())));
    known.extend(wiki.augments.iter().map(|augment| ("augments".into(), augment.name.clone())));
    let mut references = PreviousReferences { known, early, skipped };
    wiki.quests.retain(|quest| {
        let valid = quest.pack.as_ref().is_none_or(|name| {
            references.fact("adventure_packs", name, &quest.file_name, format!("{} quest", quest.name))
        }) && quest
            .patron
            .as_ref()
            .is_none_or(|name| references.fact("patrons", name, &quest.file_name, format!("{} quest", quest.name)));
        if valid {
            references.known.insert(("quests".into(), quest.name.clone()));
        }
        valid
    });
    wiki.quest_loot.retain_mut(|quest| {
        if !references.fact("quests", &quest.name, &quest.file_name, format!("{} loot", quest.name)) {
            return false;
        }
        for (drops, table) in [(&mut quest.items, "items"), (&mut quest.augments, "augments")] {
            drops.retain(|drop| {
                references.fact(table, drop.name(), &quest.file_name, format!("{} drop {}", quest.name, drop.name()))
            });
        }
        for (drops, table) in [(&mut quest.rare, "items"), (&mut quest.rare_augments, "augments")] {
            drops.retain(|drop| {
                references.fact(
                    table,
                    drop.name(),
                    &quest.file_name,
                    format!("{} rare drop {}", quest.name, drop.name()),
                )
            });
        }
        true
    });
    for item in &mut wiki.items {
        references.optional("set_bonuses", &mut item.set, &item.file_name, format!("{} set", item.name));
        item.augment_slots.retain(|name| {
            references.fact("augment_slot_types", name, &item.file_name, format!("{} socket {name}", item.name))
        });
    }
    for augment in &mut wiki.augments {
        references.optional("set_bonuses", &mut augment.set, &augment.file_name, format!("{} set", augment.name));
        augment.slots.retain(|name| {
            references.fact("augment_slot_types", name, &augment.file_name, format!("{} socket {name}", augment.name))
        });
    }
    for system in &mut wiki.crafting_systems {
        references.optional("adventure_packs", &mut system.pack, &system.file_name, format!("{} pack", system.name));
        for recipe in &mut system.recipes {
            references.optional(
                "augment_slot_types",
                &mut recipe.slot,
                &system.file_name,
                format!("{} recipe {} slot", system.name, recipe.option),
            );
            references.optional(
                "augment_slot_types",
                &mut recipe.grants_slot,
                &system.file_name,
                format!("{} recipe {} granted socket", system.name, recipe.option),
            );
            recipe.augments.retain(|name| {
                references.fact(
                    "augments",
                    name,
                    &system.file_name,
                    format!("{} recipe {} augment {name}", system.name, recipe.option),
                )
            });
        }
    }
    for chain in &mut wiki.quest_chains {
        references.series(chain);
    }
    for saga in &mut wiki.sagas {
        references.series(saga);
    }
    for vendor in &mut wiki.vendors {
        references.optional("adventure_packs", &mut vendor.pack, &vendor.file_name, format!("{} pack", vendor.name));
        vendor.items.retain(|item| {
            references.fact("items", item.name(), &vendor.file_name, format!("{} offer {}", vendor.name, item.name()))
        });
    }
    for event in &mut wiki.events {
        event
            .items
            .retain(|name| references.fact("items", name, &event.file_name, format!("{} offer {name}", event.name)));
    }
    Ok(())
}
