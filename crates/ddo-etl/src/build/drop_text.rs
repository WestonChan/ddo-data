use crate::map::drop_location::{chest_following, marks_rare_loot, segment_containing};
use anyhow::Result;
use ddo_model::enums::{LootType, RowSource};
use rusqlite::{params, Transaction};
use std::ops::Range;

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
    pub(super) chest: Option<String>,
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
        let mut quest_name_spans_by_link = Vec::new();
        for quest in &self.longest_name_first {
            let quest_name = quest.name.as_str();
            if quest_name.is_empty() || !unmatched_text.contains(quest_name) {
                continue;
            }
            let quest_name_spans: Vec<Range<usize>> = unmatched_text
                .match_indices(quest_name)
                .map(|(byte_offset, _)| byte_offset..byte_offset + quest_name.len())
                .collect();
            let is_rare =
                quest_name_spans.iter().any(|span| marks_rare_loot(segment_containing(drop_text, span.start)));
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
