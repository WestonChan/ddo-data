use super::TableWriter;
use crate::map::drop_location::{
    chest_following, marks_rare_loot, names_chest_drop, names_quest_end_reward, quest_name_spans, segment_spanning,
};
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
        let mut quest_links = Vec::new();
        let mut quest_name_spans_by_quest = Vec::new();
        for quest in &self.longest_name_first {
            if !lowercase_drop_text.contains(&quest.lowercase_first_word) {
                continue;
            }
            let quest_name_spans = quest_name_spans(&unmatched_text, &quest.name);
            if quest_name_spans.is_empty() {
                continue;
            }
            for span in &quest_name_spans {
                unmatched_text.replace_range(span.clone(), &MATCHED_TEXT_MASK.repeat(span.len()));
            }
            let mut rarity_by_loot_type: Vec<(LootType, bool)> = Vec::new();
            for span in &quest_name_spans {
                let segment = segment_spanning(drop_text, span);
                let is_rare = marks_rare_loot(segment);
                let loot_types_in_segment = if quest.is_raid {
                    [Some(LootType::Raid), None]
                } else {
                    [
                        names_chest_drop(segment).then_some(LootType::Chest),
                        names_quest_end_reward(segment).then_some(LootType::Reward),
                    ]
                };
                for loot_type in loot_types_in_segment.into_iter().flatten() {
                    match rarity_by_loot_type.iter_mut().find(|(known_type, _)| *known_type == loot_type) {
                        Some((_, is_known_rare)) => *is_known_rare |= is_rare,
                        None => rarity_by_loot_type.push((loot_type, is_rare)),
                    }
                }
            }
            for (loot_type, is_rare) in rarity_by_loot_type {
                quest_links.push(DropTextQuestLink {
                    quest_id: quest.id,
                    loot_type,
                    is_rare,
                    chest: None,
                    is_wiki_quest: quest.is_wiki,
                });
                quest_name_spans_by_quest.push((quest.id, quest_name_spans.clone()));
            }
        }
        let every_quest_name_span: Vec<Range<usize>> =
            quest_name_spans_by_quest.iter().flat_map(|(_, spans)| spans).cloned().collect();
        for (quest_link, (_, quest_name_spans)) in quest_links.iter_mut().zip(&quest_name_spans_by_quest) {
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
    pub(super) fn insert_link_unless_linked_sql(self) -> &'static str {
        match self {
            Self::Items => {
                "INSERT INTO quest_loot (quest_id, item_id, loot_type, chest) SELECT ?1, ?2, ?3, ?4
                  WHERE NOT EXISTS (SELECT 1 FROM quest_loot WHERE quest_id = ?1 AND item_id = ?2)"
            }
            Self::Augments => {
                "INSERT INTO quest_augment_loot (quest_id, augment_id, loot_type, chest) SELECT ?1, ?2, ?3, ?4
                  WHERE NOT EXISTS (SELECT 1 FROM quest_augment_loot WHERE quest_id = ?1 AND augment_id = ?2)"
            }
        }
    }

    pub(super) fn insert_link_unless_linked_as_sql(self) -> &'static str {
        match self {
            Self::Items => {
                "INSERT OR IGNORE INTO quest_loot (quest_id, item_id, loot_type, chest) VALUES (?1, ?2, ?3, ?4)"
            }
            Self::Augments => {
                "INSERT OR IGNORE INTO quest_augment_loot (quest_id, augment_id, loot_type, chest) VALUES (?1, ?2, ?3, ?4)"
            }
        }
    }

    pub(super) fn mark_rare_sql(self) -> &'static str {
        match self {
            Self::Items => {
                "UPDATE quest_loot SET is_rare = 1, chest = COALESCE(chest, ?3) WHERE quest_id = ?1 AND item_id = ?2
                    AND (loot_type <> 'reward' OR NOT EXISTS (SELECT 1 FROM quest_loot dropped
                          WHERE dropped.quest_id = ?1 AND dropped.item_id = ?2 AND dropped.loot_type <> 'reward'))"
            }
            Self::Augments => {
                "UPDATE quest_augment_loot SET is_rare = 1, chest = COALESCE(chest, ?3) WHERE quest_id = ?1 AND augment_id = ?2
                    AND (loot_type <> 'reward' OR NOT EXISTS (SELECT 1 FROM quest_augment_loot dropped
                          WHERE dropped.quest_id = ?1 AND dropped.augment_id = ?2 AND dropped.loot_type <> 'reward'))"
            }
        }
    }

    fn insert_link_sql(self) -> &'static str {
        match self {
            Self::Items => {
                "INSERT INTO quest_loot (quest_id, item_id, loot_type, is_rare, chest) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (quest_id, item_id, loot_type) DO UPDATE SET is_rare = 1 WHERE excluded.is_rare > quest_loot.is_rare"
            }
            Self::Augments => {
                "INSERT INTO quest_augment_loot (quest_id, augment_id, loot_type, is_rare, chest) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (quest_id, augment_id, loot_type) DO UPDATE SET is_rare = 1 WHERE excluded.is_rare > quest_augment_loot.is_rare"
            }
        }
    }
}

pub(super) struct LinkedDropTextQuest {
    pub(super) is_wiki_quest: bool,
    pub(super) is_newly_rare: bool,
}

pub(super) struct QuestLootLink<'a> {
    pub(super) table: QuestLootTable,
    pub(super) quest_id: i64,
    pub(super) loot_id: i64,
    pub(super) loot_type: LootType,
    pub(super) is_rare: bool,
    pub(super) chest: Option<&'a str>,
}

pub(super) fn insert_quest_loot_link(transaction: &Transaction, link: &QuestLootLink) -> Result<usize> {
    Ok(transaction.execute(
        link.table.insert_link_sql(),
        params![link.quest_id, link.loot_id, link.loot_type.as_str(), link.is_rare, link.chest],
    )?)
}

impl DropTextQuests {
    pub(super) fn link_loot_to_quests_named_in(
        &self,
        transaction: &Transaction,
        table: QuestLootTable,
        loot_id: i64,
        drop_text: &str,
    ) -> Result<Vec<LinkedDropTextQuest>> {
        let mut linked_quests = Vec::new();
        for quest_link in self.quest_links_in(drop_text) {
            let changed_row_count = insert_quest_loot_link(
                transaction,
                &QuestLootLink {
                    table,
                    quest_id: quest_link.quest_id,
                    loot_id,
                    loot_type: quest_link.loot_type,
                    is_rare: quest_link.is_rare,
                    chest: quest_link.chest.as_deref(),
                },
            )?;
            linked_quests.push(LinkedDropTextQuest {
                is_wiki_quest: quest_link.is_wiki_quest,
                is_newly_rare: quest_link.is_rare && changed_row_count > 0,
            });
        }
        Ok(linked_quests)
    }
}

impl TableWriter<'_> {
    pub(super) fn link_to_drop_text_quests(
        &self,
        table: QuestLootTable,
        loot_id: i64,
        drop_text: &str,
    ) -> Result<Vec<LinkedDropTextQuest>> {
        self.drop_text_quests.link_loot_to_quests_named_in(self.transaction, table, loot_id, drop_text)
    }
}
