use super::quest_series::QuestSeriesTable;
use super::TableWriter;
use crate::map::drop_location::{
    chest_following, marks_rare_loot, names_chest_drop, names_quest_end_reward, names_saga, quest_name_spans,
    reward_giver_name, saga_tier_credited_to, segment_ranges, segment_spanning,
};
use anyhow::Result;
use ddo_model::enums::{LootType, RowSource, SagaTier};
use rusqlite::{params, Connection, Transaction};
use std::ops::Range;

const MATCHED_TEXT_MASK: &str = "\0";

struct DropTextQuest {
    name: String,
    lowercase_first_word: String,
    id: i64,
    is_raid: bool,
    is_wiki: bool,
}

struct DropTextRewardGiver {
    name: String,
    id: i64,
}

pub(crate) struct DropTextLinker {
    quests_longest_name_first: Vec<DropTextQuest>,
    quest_chains_longest_name_first: Vec<DropTextRewardGiver>,
    sagas_longest_name_first: Vec<DropTextRewardGiver>,
}

pub(super) struct RewardGiverLink {
    pub(super) table: QuestSeriesTable,
    pub(super) reward_giver_id: i64,
    pub(super) tier: Option<SagaTier>,
    pub(super) is_rare: bool,
}

struct DropTextQuestLink {
    quest_id: i64,
    loot_type: LootType,
    is_rare: bool,
    chest: Option<String>,
    is_wiki_quest: bool,
}

impl DropTextLinker {
    pub(super) fn from_written_tables(db: &Connection) -> Result<Self> {
        let mut statement = db.prepare("SELECT name, id, is_raid, source = ?1 FROM quests WHERE is_challenge = 0")?;
        let mut longest_name_first = statement
            .query_map(params![RowSource::Wiki.as_str()], |r| {
                let name: String = r.get(0)?;
                let lowercase_first_word = name.split_whitespace().next().unwrap_or_default().to_lowercase();
                Ok(DropTextQuest { name, lowercase_first_word, id: r.get(1)?, is_raid: r.get(2)?, is_wiki: r.get(3)? })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        longest_name_first.retain(|quest| !quest.lowercase_first_word.is_empty());
        longest_name_first.sort_by(|a, b| b.name.len().cmp(&a.name.len()).then_with(|| a.name.cmp(&b.name)));
        Ok(Self {
            quests_longest_name_first: longest_name_first,
            quest_chains_longest_name_first: reward_givers_longest_name_first(db, "SELECT name, id FROM quest_chains")?,
            sagas_longest_name_first: reward_givers_longest_name_first(db, "SELECT name, id FROM sagas")?,
        })
    }

    fn reward_givers_in_segment(&self, segment: &str) -> Vec<RewardGiverLink> {
        if reward_giver_name(segment).is_none() {
            return Vec::new();
        }
        let chains_first = [
            (QuestSeriesTable::QuestChains, &self.quest_chains_longest_name_first),
            (QuestSeriesTable::Sagas, &self.sagas_longest_name_first),
        ];
        let sagas_first = [chains_first[1], chains_first[0]];
        let preferred_tables = if names_saga(segment) { sagas_first } else { chains_first };
        for (table, reward_givers) in preferred_tables {
            let mut unmatched_segment = segment.to_string();
            let mut reward_giver_links = Vec::new();
            for reward_giver in reward_givers {
                let name_spans = quest_name_spans(&unmatched_segment, &reward_giver.name);
                if name_spans.is_empty() {
                    continue;
                }
                for span in &name_spans {
                    unmatched_segment.replace_range(span.clone(), &MATCHED_TEXT_MASK.repeat(span.len()));
                }
                reward_giver_links.push(RewardGiverLink {
                    table,
                    reward_giver_id: reward_giver.id,
                    tier: saga_tier_credited_to(segment, &reward_giver.name),
                    is_rare: marks_rare_loot(segment),
                });
            }
            if !reward_giver_links.is_empty() {
                return reward_giver_links;
            }
        }
        Vec::new()
    }

    pub(super) fn reward_giver_links_in(&self, drop_text: &str) -> Vec<RewardGiverLink> {
        self.segments_giving_no_quest_reward(drop_text)
            .into_iter()
            .flat_map(|segment| self.reward_givers_in_segment(segment))
            .collect()
    }

    pub(super) fn unlinked_reward_segments<'text>(&self, drop_text: &'text str) -> Vec<&'text str> {
        self.segments_giving_no_quest_reward(drop_text)
            .into_iter()
            .filter(|segment| reward_giver_name(segment).is_some() && self.reward_givers_in_segment(segment).is_empty())
            .collect()
    }

    fn quest_name_spans_in(&self, drop_text: &str) -> Vec<(&DropTextQuest, Vec<Range<usize>>)> {
        let mut unmatched_text = drop_text.to_string();
        let lowercase_drop_text = drop_text.to_lowercase();
        let mut quest_name_spans_by_quest = Vec::new();
        for quest in &self.quests_longest_name_first {
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
            quest_name_spans_by_quest.push((quest, quest_name_spans));
        }
        quest_name_spans_by_quest
    }

    fn segments_giving_no_quest_reward<'text>(&self, drop_text: &'text str) -> Vec<&'text str> {
        let quest_name_spans: Vec<Range<usize>> =
            self.quest_name_spans_in(drop_text).into_iter().flat_map(|(_, spans)| spans).collect();
        segment_ranges(drop_text)
            .into_iter()
            .filter(|segment| {
                let names_quest =
                    quest_name_spans.iter().any(|span| span.start >= segment.start && span.end <= segment.end);
                !(names_quest && names_quest_end_reward(&drop_text[segment.clone()]))
            })
            .map(|segment| &drop_text[segment])
            .collect()
    }

    fn quest_links_in(&self, drop_text: &str) -> Vec<DropTextQuestLink> {
        let mut quest_links = Vec::new();
        let mut quest_name_spans_by_quest = Vec::new();
        for (quest, quest_name_spans) in self.quest_name_spans_in(drop_text) {
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

fn reward_givers_longest_name_first(db: &Connection, sql: &str) -> Result<Vec<DropTextRewardGiver>> {
    let mut statement = db.prepare(sql)?;
    let mut reward_givers = statement
        .query_map([], |r| Ok(DropTextRewardGiver { name: r.get(0)?, id: r.get(1)? }))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    reward_givers.sort_by(|a, b| b.name.len().cmp(&a.name.len()).then_with(|| a.name.cmp(&b.name)));
    Ok(reward_givers)
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

impl DropTextLinker {
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
    pub(super) fn link_item_to_drop_text_reward_givers(
        &self,
        item_id: i64,
        drop_text: &str,
    ) -> Result<Vec<QuestSeriesTable>> {
        let mut linked_tables = Vec::new();
        for reward_giver_link in self.drop_text_linker.reward_giver_links_in(drop_text) {
            let changed_row_count = reward_giver_link.table.insert_reward(
                self.transaction,
                reward_giver_link.reward_giver_id,
                item_id,
                reward_giver_link.is_rare,
                reward_giver_link.tier,
            )?;
            if changed_row_count > 0 {
                linked_tables.push(reward_giver_link.table);
            }
        }
        Ok(linked_tables)
    }

    pub(super) fn link_to_drop_text_quests(
        &self,
        table: QuestLootTable,
        loot_id: i64,
        drop_text: &str,
    ) -> Result<Vec<LinkedDropTextQuest>> {
        self.drop_text_linker.link_loot_to_quests_named_in(self.transaction, table, loot_id, drop_text)
    }
}
