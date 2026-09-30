use super::TableWriter;
use crate::map::drop_location::{chest_following, marks_rare_loot, quest_name_spans, segment_containing};
use anyhow::Result;
use ddo_model::enums::{LootType, RowSource};
use rusqlite::{params, Transaction};
use std::ops::Range;

const MATCHED_TEXT_MASK: &str = "\0";

struct DropTextQuest {
    name: String,
    lowercase_first_word: String,
    id: i64,
    is_raid: bool,
    is_wiki: bool,
}

pub(crate) struct DropTextQuests {
    longest_name_first: Vec<DropTextQuest>,
}

struct DropTextQuestLink {
    quest_id: i64,
    loot_type: LootType,
    is_rare: bool,
    chest: Option<String>,
    is_wiki_quest: bool,
}

impl DropTextQuests {
    pub(super) fn from_quests_table(transaction: &Transaction) -> Result<Self> {
        let mut statement =
            transaction.prepare("SELECT name, id, is_raid, source = ?1 FROM quests WHERE is_challenge = 0")?;
        let mut longest_name_first = statement
            .query_map(params![RowSource::Wiki.as_str()], |r| {
                let name: String = r.get(0)?;
                let lowercase_first_word = name.split_whitespace().next().unwrap_or_default().to_lowercase();
                Ok(DropTextQuest { name, lowercase_first_word, id: r.get(1)?, is_raid: r.get(2)?, is_wiki: r.get(3)? })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        longest_name_first.retain(|quest| !quest.lowercase_first_word.is_empty());
        longest_name_first.sort_by(|a, b| b.name.len().cmp(&a.name.len()).then_with(|| a.name.cmp(&b.name)));
        Ok(Self { longest_name_first })
    }

    fn quest_links_in(&self, drop_text: &str) -> Vec<DropTextQuestLink> {
        let mut unmatched_text = drop_text.to_string();
        let lowercase_drop_text = drop_text.to_lowercase();
        let mentions_reward = lowercase_drop_text.contains("reward");
        let mut quest_links = Vec::new();
        let mut quest_name_spans_by_link = Vec::new();
        for quest in &self.longest_name_first {
            if !lowercase_drop_text.contains(&quest.lowercase_first_word) {
                continue;
            }
            let quest_name_spans = quest_name_spans(&unmatched_text, &quest.name);
            if quest_name_spans.is_empty() {
                continue;
            }
            let is_rare =
                quest_name_spans.iter().any(|span| marks_rare_loot(segment_containing(drop_text, span.start)));
            for span in &quest_name_spans {
                unmatched_text.replace_range(span.clone(), &MATCHED_TEXT_MASK.repeat(span.len()));
            }
            let loot_type = if quest.is_raid {
                LootType::Raid
            } else if mentions_reward {
                LootType::Reward
            } else {
                LootType::Chest
            };
            quest_links.push(DropTextQuestLink {
                quest_id: quest.id,
                loot_type,
                is_rare,
                chest: None,
                is_wiki_quest: quest.is_wiki,
            });
            quest_name_spans_by_link.push(quest_name_spans);
        }
        let every_quest_name_span: Vec<Range<usize>> = quest_name_spans_by_link.iter().flatten().cloned().collect();
        for (quest_link, quest_name_spans) in quest_links.iter_mut().zip(&quest_name_spans_by_link) {
            quest_link.chest =
                quest_name_spans.iter().find_map(|span| chest_following(drop_text, span.end, &every_quest_name_span));
        }
        quest_links
    }
}

#[derive(Clone, Copy)]
pub(super) enum QuestLootTable {
    Items,
    Augments,
}

impl QuestLootTable {
    pub(super) fn insert_missing_link_sql(self) -> &'static str {
        match self {
            Self::Items => "INSERT OR IGNORE INTO quest_loot (quest_id, item_id, loot_type) VALUES (?1, ?2, ?3)",
            Self::Augments => {
                "INSERT OR IGNORE INTO quest_augment_loot (quest_id, augment_id, loot_type) VALUES (?1, ?2, ?3)"
            }
        }
    }

    pub(super) fn mark_rare_sql(self) -> &'static str {
        match self {
            Self::Items => {
                "UPDATE quest_loot SET is_rare = 1, chest = COALESCE(chest, ?3) WHERE quest_id = ?1 AND item_id = ?2"
            }
            Self::Augments => {
                "UPDATE quest_augment_loot SET is_rare = 1, chest = COALESCE(chest, ?3) WHERE quest_id = ?1 AND augment_id = ?2"
            }
        }
    }

    fn insert_link_sql(self) -> &'static str {
        match self {
            Self::Items => {
                "INSERT INTO quest_loot (quest_id, item_id, loot_type, is_rare, chest) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (quest_id, item_id) DO UPDATE SET is_rare = 1 WHERE excluded.is_rare > quest_loot.is_rare"
            }
            Self::Augments => {
                "INSERT INTO quest_augment_loot (quest_id, augment_id, loot_type, is_rare, chest) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (quest_id, augment_id) DO UPDATE SET is_rare = 1 WHERE excluded.is_rare > quest_augment_loot.is_rare"
            }
        }
    }
}

pub(super) struct LinkedDropTextQuest {
    pub(super) is_wiki_quest: bool,
    pub(super) is_newly_rare: bool,
}

impl TableWriter<'_> {
    pub(super) fn insert_quest_loot_link(
        &self,
        table: QuestLootTable,
        quest_id: i64,
        loot_id: i64,
        loot_type: LootType,
        is_rare: bool,
        chest: Option<&str>,
    ) -> Result<usize> {
        Ok(self
            .transaction
            .execute(table.insert_link_sql(), params![quest_id, loot_id, loot_type.as_str(), is_rare, chest])?)
    }

    pub(super) fn link_to_drop_text_quests(
        &self,
        table: QuestLootTable,
        loot_id: i64,
        drop_text: &str,
    ) -> Result<Vec<LinkedDropTextQuest>> {
        let mut linked_quests = Vec::new();
        for quest_link in self.drop_text_quests.quest_links_in(drop_text) {
            let changed_row_count = self.insert_quest_loot_link(
                table,
                quest_link.quest_id,
                loot_id,
                quest_link.loot_type,
                quest_link.is_rare,
                quest_link.chest.as_deref(),
            )?;
            linked_quests.push(LinkedDropTextQuest {
                is_wiki_quest: quest_link.is_wiki_quest,
                is_newly_rare: quest_link.is_rare && changed_row_count > 0,
            });
        }
        Ok(linked_quests)
    }
}
