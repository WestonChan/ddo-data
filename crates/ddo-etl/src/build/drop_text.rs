use crate::map::drop_location::{marks_rare_loot, segment_containing};
use anyhow::Result;
use ddo_model::enums::{LootType, RowSource};
use rusqlite::{params, Transaction};

struct DropTextQuest {
    name: String,
    id: i64,
    is_raid: bool,
    is_wiki: bool,
}

pub(crate) struct DropTextQuests {
    longest_name_first: Vec<DropTextQuest>,
}

pub(super) struct DropTextQuestLink {
    pub(super) quest_id: i64,
    pub(super) loot_type: LootType,
    pub(super) is_rare: bool,
    pub(super) is_wiki_quest: bool,
}

impl DropTextQuests {
    pub(super) fn from_quests_table(transaction: &Transaction) -> Result<Self> {
        let mut statement =
            transaction.prepare("SELECT name, id, is_raid, source = ?1 FROM quests WHERE is_challenge = 0")?;
        let mut longest_name_first = statement
            .query_map(params![RowSource::Wiki.as_str()], |r| {
                Ok(DropTextQuest { name: r.get(0)?, id: r.get(1)?, is_raid: r.get(2)?, is_wiki: r.get(3)? })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        longest_name_first.sort_by(|a, b| b.name.len().cmp(&a.name.len()).then_with(|| a.name.cmp(&b.name)));
        Ok(Self { longest_name_first })
    }

    pub(super) fn quest_links_in(&self, drop_text: &str) -> Vec<DropTextQuestLink> {
        let mut unmatched_text = drop_text.to_string();
        let mentions_reward = drop_text.to_lowercase().contains("reward");
        let mut quest_links = Vec::new();
        for quest in &self.longest_name_first {
            let quest_name = quest.name.as_str();
            if quest_name.is_empty() || !unmatched_text.contains(quest_name) {
                continue;
            }
            let is_rare = unmatched_text
                .match_indices(quest_name)
                .any(|(byte_offset, _)| marks_rare_loot(segment_containing(drop_text, byte_offset)));
            unmatched_text = unmatched_text.replace(quest_name, &" ".repeat(quest_name.len()));
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
                is_wiki_quest: quest.is_wiki,
            });
        }
        quest_links
    }
}
