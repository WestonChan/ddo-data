use super::quest_series::QuestSeriesTable;
use super::{BuildReport, TableWriter};
use crate::map::drop_location::{
    chest_following, marks_rare_loot, names_chest_drop, names_quest_end_reward, names_saga, names_store_purchase,
    prerequisite_spans, quest_name_spans, reward_giver_name, saga_tier_credited_to, segment_head, segment_ranges,
    segment_spanning, starts_with_saga_tier_aside,
};
use crate::map::legacy_drop_source::LegacyDropSources;
use crate::map::source_alias::SourceAliases;
use anyhow::Result;
use ddo_model::enums::{LootType, Provenance, SagaTier, SourceKind};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::ops::{Range, RangeInclusive};
use std::sync::LazyLock;

const MATCHED_TEXT_MASK: &str = "\0";

struct DropTextQuest {
    name_in_drop_text: String,
    lowercase_first_word: String,
    id: i64,
    is_raid: bool,
    is_wiki: bool,
}

struct NamedDropSource {
    name: String,
    id: i64,
}

struct MatchedLootSource {
    source: LootSource,
    source_match: String,
}

struct AliasedDropSource {
    head: String,
    lowercase_contains: Option<String>,
    minimum_levels: Option<RangeInclusive<i64>>,
    id: i64,
}

impl AliasedDropSource {
    fn is_named_by(&self, segment: &str, loot_minimum_level: Option<i64>) -> bool {
        segment_head(segment).eq_ignore_ascii_case(&self.head)
            && self.lowercase_contains.as_ref().is_none_or(|contains| segment.to_lowercase().contains(contains))
            && self
                .minimum_levels
                .as_ref()
                .is_none_or(|levels| loot_minimum_level.is_some_and(|level| levels.contains(&level)))
    }
}

struct AliasTarget<'alias> {
    head: &'alias str,
    contains: Option<&'alias str>,
    minimum_levels: Option<RangeInclusive<i64>>,
    target_name: &'alias str,
}

pub(crate) struct DropTextLinker {
    quests_longest_name_first: Vec<DropTextQuest>,
    quest_chains_longest_name_first: Vec<NamedDropSource>,
    sagas_longest_name_first: Vec<NamedDropSource>,
    packs_longest_name_first: Vec<NamedDropSource>,
    crafting_systems_longest_name_first: Vec<NamedDropSource>,
    crafting_systems_by_station: Vec<AliasedDropSource>,
    challenge_packs_by_text: Vec<AliasedDropSource>,
    vendors_by_turn_in: Vec<AliasedDropSource>,
    vendors_longest_name_first: Vec<NamedDropSource>,
    events_longest_name_first: Vec<NamedDropSource>,
    unresolved_alias_texts: Vec<String>,
    legacy_drop_sources: LegacyDropSources,
}

pub(super) struct RewardGiverLink {
    pub(super) table: QuestSeriesTable,
    pub(super) reward_giver_id: i64,
    pub(super) tier: Option<SagaTier>,
    pub(super) is_rare: bool,
    source_match: String,
    drop_text_segment: String,
}

struct DropTextPackLink {
    pack_id: i64,
    loot_type: LootType,
    is_rare: bool,
    chest: Option<String>,
    source_match: String,
    drop_text_segment: String,
}

struct DropTextQuestLink {
    quest_id: i64,
    loot_type: LootType,
    is_rare: bool,
    chest: Option<String>,
    is_wiki_quest: bool,
    source_match: String,
    drop_text_segment: String,
}

impl DropTextLinker {
    pub(super) fn from_written_tables(
        db: &Connection,
        legacy_drop_sources: &LegacyDropSources,
        source_aliases: &SourceAliases,
    ) -> Result<Self> {
        let mut statement =
            db.prepare("SELECT name, epic_name, id, is_raid, provenance = ?1 FROM quests WHERE is_challenge = 0")?;
        let quest_rows = statement
            .query_map(params![Provenance::Wiki.as_str()], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut longest_name_first: Vec<DropTextQuest> = quest_rows
            .into_iter()
            .flat_map(|(name, epic_name, id, is_raid, is_wiki)| {
                std::iter::once(name).chain(epic_name).map(move |name_in_drop_text| {
                    let lowercase_first_word =
                        name_in_drop_text.split_whitespace().next().unwrap_or_default().to_lowercase();
                    DropTextQuest { name_in_drop_text, lowercase_first_word, id, is_raid, is_wiki }
                })
            })
            .collect();
        longest_name_first.retain(|quest| !quest.lowercase_first_word.is_empty());
        longest_name_first.sort_by(|a, b| {
            b.name_in_drop_text
                .len()
                .cmp(&a.name_in_drop_text.len())
                .then_with(|| a.is_wiki.cmp(&b.is_wiki))
                .then_with(|| a.name_in_drop_text.cmp(&b.name_in_drop_text))
        });
        let mut unresolved_alias_texts = Vec::new();
        let mut packs_longest_name_first =
            named_sources_longest_name_first(db, "SELECT name, id FROM adventure_packs")?;
        for alias in &source_aliases.adventure_packs {
            if let Some(pack_id) = db
                .query_row("SELECT id FROM adventure_packs WHERE name = ?1", [&alias.pack], |row| row.get(0))
                .optional()?
            {
                packs_longest_name_first.push(NamedDropSource { name: alias.text.clone(), id: pack_id });
            }
        }
        packs_longest_name_first
            .sort_by(|left, right| right.name.len().cmp(&left.name.len()).then_with(|| left.name.cmp(&right.name)));
        Ok(Self {
            quests_longest_name_first: longest_name_first,
            quest_chains_longest_name_first: named_sources_longest_name_first(db, "SELECT name, id FROM quest_chains")?,
            sagas_longest_name_first: named_sources_longest_name_first(db, "SELECT name, id FROM sagas")?,
            packs_longest_name_first,
            crafting_systems_longest_name_first: named_sources_longest_name_first(
                db,
                "SELECT name, id FROM crafting_systems",
            )?,
            crafting_systems_by_station: aliased_sources(
                db,
                "SELECT id FROM crafting_systems WHERE name = ?1",
                source_aliases.crafting_systems.iter().map(|alias| AliasTarget {
                    head: &alias.text,
                    contains: None,
                    minimum_levels: alias.minimum_levels(),
                    target_name: &alias.system,
                }),
                &mut unresolved_alias_texts,
            )?,
            challenge_packs_by_text: aliased_sources(
                db,
                "SELECT id FROM adventure_packs WHERE name = ?1",
                source_aliases.challenges.iter().map(|alias| AliasTarget {
                    head: &alias.text,
                    contains: None,
                    minimum_levels: None,
                    target_name: &alias.pack,
                }),
                &mut unresolved_alias_texts,
            )?,
            vendors_by_turn_in: aliased_sources(
                db,
                "SELECT id FROM vendors WHERE name = ?1",
                source_aliases.vendors.iter().map(|alias| AliasTarget {
                    head: &alias.text,
                    contains: alias.contains.as_deref(),
                    minimum_levels: None,
                    target_name: &alias.vendor,
                }),
                &mut unresolved_alias_texts,
            )?,
            unresolved_alias_texts,
            vendors_longest_name_first: named_sources_longest_name_first(db, "SELECT name, id FROM vendors")?,
            events_longest_name_first: named_sources_longest_name_first(db, "SELECT name, id FROM events")?,
            legacy_drop_sources: legacy_drop_sources.clone(),
        })
    }

    fn reward_givers_in_segment(&self, segment: &str) -> Vec<RewardGiverLink> {
        if !prerequisite_spans(segment).is_empty() && segment_head(segment).to_ascii_lowercase().starts_with("requires")
        {
            return Vec::new();
        }
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
            mask_matched_spans(&mut unmatched_segment, &prerequisite_spans(segment));
            let mut reward_giver_links = Vec::new();
            for reward_giver in reward_givers {
                let name_spans = quest_name_spans(&unmatched_segment, &reward_giver.name);
                if name_spans.is_empty() {
                    continue;
                }
                mask_matched_spans(&mut unmatched_segment, &name_spans);
                reward_giver_links.push(RewardGiverLink {
                    table,
                    reward_giver_id: reward_giver.id,
                    tier: match table {
                        QuestSeriesTable::Sagas => saga_tier_credited_to(segment, &reward_giver.name),
                        QuestSeriesTable::QuestChains => None,
                    },
                    is_rare: marks_rare_loot(segment),
                    source_match: segment[name_spans[0].clone()].to_string(),
                    drop_text_segment: segment.to_string(),
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

    pub(super) fn unlinked_reward_segments<'text>(
        &self,
        drop_text: &'text str,
        loot_minimum_level: Option<i64>,
    ) -> Vec<&'text str> {
        self.segments_giving_no_quest_reward(drop_text)
            .into_iter()
            .filter(|segment| {
                reward_giver_name(segment).is_some()
                    && self.reward_givers_in_segment(segment).is_empty()
                    && self.named_sources_in(segment, loot_minimum_level).is_empty()
            })
            .collect()
    }

    fn quest_name_spans_in(&self, drop_text: &str) -> Vec<(&DropTextQuest, Vec<Range<usize>>)> {
        let mut unmatched_text = drop_text.to_string();
        mask_matched_spans(&mut unmatched_text, &prerequisite_spans(drop_text));
        let lowercase_drop_text = drop_text.to_lowercase();
        let mut quest_name_spans_by_quest: Vec<(&DropTextQuest, Vec<Range<usize>>)> = Vec::new();
        let mut packs_to_mask = self.packs_longest_name_first.iter().peekable();
        for quest in &self.quests_longest_name_first {
            while let Some(longer_pack) = packs_to_mask.next_if(|pack| pack.name.len() > quest.name_in_drop_text.len())
            {
                let pack_name_spans = quest_name_spans(&unmatched_text, &longer_pack.name);
                mask_matched_spans(&mut unmatched_text, &pack_name_spans);
            }
            if !lowercase_drop_text.contains(&quest.lowercase_first_word) {
                continue;
            }
            let quest_name_spans = quest_name_spans(&unmatched_text, &quest.name_in_drop_text);
            if quest_name_spans.is_empty() {
                continue;
            }
            mask_matched_spans(&mut unmatched_text, &quest_name_spans);
            match quest_name_spans_by_quest.iter_mut().find(|(known_quest, _)| known_quest.id == quest.id) {
                Some((_, known_spans)) => {
                    known_spans.extend(quest_name_spans);
                    known_spans.sort_by_key(|span| span.start);
                }
                None => quest_name_spans_by_quest.push((quest, quest_name_spans)),
            }
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

    fn names_saga_reward_list_around(&self, segment: &str, pack_name_span: &Range<usize>) -> bool {
        self.sagas_longest_name_first.iter().any(|saga| {
            quest_name_spans(segment, &saga.name).iter().any(|saga_name_span| {
                saga_name_span.start <= pack_name_span.start
                    && saga_name_span.end >= pack_name_span.end
                    && starts_with_saga_tier_aside(&segment[saga_name_span.end..])
            })
        })
    }

    fn segments_naming_no_quest<'text>(&self, drop_text: &'text str) -> Vec<&'text str> {
        let every_quest_name_span: Vec<Range<usize>> =
            self.quest_name_spans_in(drop_text).into_iter().flat_map(|(_, spans)| spans).collect();
        segment_ranges(drop_text)
            .into_iter()
            .filter(|segment_range| {
                !every_quest_name_span
                    .iter()
                    .any(|span| span.start >= segment_range.start && span.end <= segment_range.end)
            })
            .map(|segment_range| &drop_text[segment_range])
            .collect()
    }

    fn pack_name_spans_in(&self, segment: &str) -> Vec<(i64, Vec<Range<usize>>)> {
        if names_saga(segment) || names_store_purchase(segment) || !self.reward_givers_in_segment(segment).is_empty() {
            return Vec::new();
        }
        let mut unmatched_segment = segment.to_string();
        mask_matched_spans(&mut unmatched_segment, &prerequisite_spans(segment));
        let mut pack_name_spans_by_pack = Vec::new();
        for pack in &self.packs_longest_name_first {
            let pack_name_spans = quest_name_spans(&unmatched_segment, &pack.name);
            let names_saga_reward_list =
                pack_name_spans.iter().any(|span| self.names_saga_reward_list_around(segment, span));
            mask_matched_spans(&mut unmatched_segment, &pack_name_spans);
            if !pack_name_spans.is_empty() && !names_saga_reward_list {
                pack_name_spans_by_pack.push((pack.id, pack_name_spans));
            }
        }
        pack_name_spans_by_pack
    }

    fn pack_links_in(&self, drop_text: &str) -> Vec<DropTextPackLink> {
        let mut pack_links: Vec<DropTextPackLink> = Vec::new();
        for segment in self.segments_naming_no_quest(drop_text) {
            if self.challenge_packs_by_text.iter().any(|alias| alias.is_named_by(segment, None)) {
                continue;
            }
            let pack_name_spans_by_pack = self.pack_name_spans_in(segment);
            let every_pack_name_span: Vec<Range<usize>> =
                pack_name_spans_by_pack.iter().flat_map(|(_, spans)| spans).cloned().collect();
            let loot_types = [
                names_chest_drop(segment).then_some(LootType::Chest),
                names_quest_end_reward(segment).then_some(LootType::Reward),
            ];
            let is_rare = marks_rare_loot(segment);
            for (pack_id, pack_name_spans) in &pack_name_spans_by_pack {
                for loot_type in loot_types.into_iter().flatten() {
                    let chest = match loot_type {
                        LootType::Reward => None,
                        _ => pack_name_spans
                            .iter()
                            .find_map(|span| chest_following(segment, span.end, &every_pack_name_span)),
                    };
                    match pack_links.iter_mut().find(|link| link.pack_id == *pack_id && link.loot_type == loot_type) {
                        Some(known_link) => {
                            known_link.is_rare |= is_rare;
                            known_link.chest = known_link.chest.take().or(chest);
                        }
                        None => {
                            let source_match = segment[pack_name_spans[0].clone()].to_string();
                            pack_links.push(DropTextPackLink {
                                pack_id: *pack_id,
                                loot_type,
                                is_rare,
                                chest,
                                source_match,
                                drop_text_segment: segment.to_string(),
                            });
                        }
                    }
                }
            }
        }
        pack_links
    }

    fn segments_naming_no_drop_source<'text>(
        &self,
        drop_text: &'text str,
        loot_minimum_level: Option<i64>,
    ) -> Vec<&'text str> {
        self.segments_naming_no_quest(drop_text)
            .into_iter()
            .filter(|segment| {
                !segment.trim().is_empty()
                    && self.reward_givers_in_segment(segment).is_empty()
                    && self.pack_name_spans_in(segment).is_empty()
                    && self.named_sources_in(segment, loot_minimum_level).is_empty()
            })
            .collect()
    }

    pub(super) fn unlinked_segments<'text>(
        &self,
        drop_text: &'text str,
        loot_minimum_level: Option<i64>,
    ) -> Vec<&'text str> {
        self.segments_naming_no_drop_source(drop_text, loot_minimum_level)
            .into_iter()
            .filter(|segment| !self.legacy_drop_sources.names_legacy_source(segment))
            .collect()
    }

    pub(super) fn legacy_texts_naming_every_segment(
        &self,
        drop_text: &str,
        loot_minimum_level: Option<i64>,
    ) -> Option<Vec<&str>> {
        let segments: Vec<&str> = segment_ranges(drop_text)
            .into_iter()
            .map(|range| &drop_text[range])
            .filter(|s| !s.trim().is_empty())
            .collect();
        let names_current_drop_source =
            segments.iter().any(|segment| self.segments_naming_no_drop_source(segment, loot_minimum_level).is_empty());
        if names_current_drop_source {
            return None;
        }
        self.legacy_drop_sources.texts_naming_every_segment(&segments)
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
                    source_match: drop_text[quest_name_spans[0].clone()].to_string(),
                    drop_text_segment: segment_spanning(drop_text, &quest_name_spans[0]).to_string(),
                });
                quest_name_spans_by_quest.push((quest.id, quest_name_spans.clone()));
            }
        }
        let every_quest_name_span: Vec<Range<usize>> =
            quest_name_spans_by_quest.iter().flat_map(|(_, spans)| spans).cloned().collect();
        for (quest_link, (_, quest_name_spans)) in quest_links.iter_mut().zip(&quest_name_spans_by_quest) {
            if quest_link.loot_type == LootType::Reward {
                continue;
            }
            quest_link.chest =
                quest_name_spans.iter().find_map(|span| chest_following(drop_text, span.end, &every_quest_name_span));
        }
        quest_links
    }
}

impl DropTextLinker {
    fn named_sources_in(&self, segment: &str, loot_minimum_level: Option<i64>) -> Vec<MatchedLootSource> {
        let original_segment = segment;
        let mut masked_segment = segment.to_string();
        mask_matched_spans(&mut masked_segment, &prerequisite_spans(segment));
        let segment = masked_segment.as_str();
        let mut named_sources = Vec::new();
        let head = segment_head(segment);
        if let Some(character_level) = starter_character_level(head) {
            named_sources.push(MatchedLootSource {
                source: LootSource::Starter(character_level),
                source_match: head.to_string(),
            });
        }
        let aliased_kinds: [(&[AliasedDropSource], LootSourceOfId); 3] = [
            (&self.challenge_packs_by_text, LootSource::Challenge),
            (&self.crafting_systems_by_station, LootSource::CraftingSystem),
            (&self.vendors_by_turn_in, LootSource::Vendor),
        ];
        for (aliased_sources, loot_source) in aliased_kinds {
            for aliased_source in
                aliased_sources.iter().filter(|aliased_source| aliased_source.is_named_by(segment, loot_minimum_level))
            {
                named_sources
                    .push(MatchedLootSource { source: loot_source(aliased_source.id), source_match: head.to_string() });
            }
        }
        let named_kinds: [(&[NamedDropSource], LootSourceOfId); 3] = [
            (&self.crafting_systems_longest_name_first, LootSource::CraftingSystem),
            (&self.vendors_longest_name_first, LootSource::Vendor),
            (&self.events_longest_name_first, LootSource::Event),
        ];
        for (sources_longest_name_first, loot_source) in named_kinds {
            let mut unmatched_segment = segment.to_string();
            for named_source in sources_longest_name_first {
                let name_spans = quest_name_spans(&unmatched_segment, &named_source.name);
                if name_spans.is_empty() {
                    continue;
                }
                mask_matched_spans(&mut unmatched_segment, &name_spans);
                named_sources.push(MatchedLootSource {
                    source: loot_source(named_source.id),
                    source_match: original_segment[name_spans[0].clone()].to_string(),
                });
            }
        }
        let mut distinct_sources: Vec<MatchedLootSource> = Vec::new();
        for named_source in named_sources {
            if !distinct_sources.iter().any(|known| known.source.same_source_as(named_source.source)) {
                distinct_sources.push(named_source);
            }
        }
        distinct_sources
    }

    pub(super) fn unresolved_alias_texts(&self) -> &[String] {
        &self.unresolved_alias_texts
    }

    pub(super) fn link_loot_to_sources_named_in(
        &self,
        transaction: &Transaction,
        loot: DroppedLoot,
        drop_text: &str,
    ) -> Result<Vec<SourceKind>> {
        let mut linked_kinds = Vec::new();
        let loot_minimum_level = loot.minimum_level(transaction)?;
        for segment in segment_ranges(drop_text).into_iter().map(|range| &drop_text[range]) {
            for named_source in self.named_sources_in(segment, loot_minimum_level) {
                let changed_row_count = insert_source_link(
                    transaction,
                    &SourceLink {
                        source: named_source.source,
                        loot,
                        loot_type: None,
                        is_rare: marks_rare_loot(segment),
                        chest: None,
                        tier: None,
                        cost: None,
                        drop_text_segment: Some(segment),
                        source_match: Some(&named_source.source_match),
                    },
                )?;
                if changed_row_count > 0 {
                    linked_kinds.push(named_source.source.kind());
                }
            }
        }
        Ok(linked_kinds)
    }
}

type LootSourceOfId = fn(i64) -> LootSource;

fn starter_character_level(segment_head: &str) -> Option<i64> {
    let level_text = segment_head
        .get(..STARTER_HEAD_PREFIX.len())
        .filter(|prefix| prefix.eq_ignore_ascii_case(STARTER_HEAD_PREFIX))?;
    segment_head[level_text.len()..].trim().parse().ok().filter(|level| *level >= 1)
}

const STARTER_HEAD_PREFIX: &str = "advance to level ";

fn aliased_sources<'alias>(
    db: &Connection,
    target_id_sql: &str,
    alias_targets: impl Iterator<Item = AliasTarget<'alias>>,
    unresolved_alias_texts: &mut Vec<String>,
) -> Result<Vec<AliasedDropSource>> {
    let mut statement = db.prepare(target_id_sql)?;
    let mut aliased_sources = Vec::new();
    for alias_target in alias_targets {
        match statement.query_row([alias_target.target_name], |r| r.get(0)).optional()? {
            Some(id) => aliased_sources.push(AliasedDropSource {
                head: alias_target.head.to_string(),
                lowercase_contains: alias_target.contains.map(str::to_lowercase),
                minimum_levels: alias_target.minimum_levels,
                id,
            }),
            None => unresolved_alias_texts.push(alias_target.head.to_string()),
        }
    }
    Ok(aliased_sources)
}

fn mask_matched_spans(text: &mut String, matched_spans: &[Range<usize>]) {
    for span in matched_spans {
        text.replace_range(span.clone(), &MATCHED_TEXT_MASK.repeat(span.len()));
    }
}

fn named_sources_longest_name_first(db: &Connection, sql: &str) -> Result<Vec<NamedDropSource>> {
    let mut statement = db.prepare(sql)?;
    let mut reward_givers = statement
        .query_map([], |r| Ok(NamedDropSource { name: r.get(0)?, id: r.get(1)? }))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    reward_givers.sort_by(|a, b| b.name.len().cmp(&a.name.len()).then_with(|| a.name.cmp(&b.name)));
    Ok(reward_givers)
}

#[derive(Clone, Copy)]
pub(super) enum LootSource {
    Quest(i64),
    QuestChain(i64),
    Saga(i64),
    AdventurePack(i64),
    Challenge(i64),
    CraftingSystem(i64),
    Vendor(i64),
    Event(i64),
    Starter(i64),
}

impl LootSource {
    fn kind(self) -> SourceKind {
        match self {
            Self::Quest(_) => SourceKind::Quest,
            Self::QuestChain(_) => SourceKind::QuestChain,
            Self::Saga(_) => SourceKind::Saga,
            Self::AdventurePack(_) => SourceKind::AdventurePack,
            Self::Challenge(_) => SourceKind::Challenge,
            Self::CraftingSystem(_) => SourceKind::CraftingSystem,
            Self::Vendor(_) => SourceKind::Vendor,
            Self::Event(_) => SourceKind::Event,
            Self::Starter(_) => SourceKind::Starter,
        }
    }

    fn identifying_value(self) -> i64 {
        let (Self::Quest(value)
        | Self::QuestChain(value)
        | Self::Saga(value)
        | Self::AdventurePack(value)
        | Self::Challenge(value)
        | Self::CraftingSystem(value)
        | Self::Vendor(value)
        | Self::Event(value)
        | Self::Starter(value)) = self;
        value
    }

    fn same_source_as(self, other: Self) -> bool {
        self.kind() == other.kind() && self.identifying_value() == other.identifying_value()
    }

    fn value_in_column(self, column: &str) -> Option<i64> {
        (self.kind().identifying_column() == column).then_some(self.identifying_value())
    }
}

#[derive(Clone, Copy)]
pub(super) enum DroppedLoot {
    Item(i64),
    Augment(i64),
}

impl DroppedLoot {
    pub(super) fn item_id(self) -> Option<i64> {
        match self {
            Self::Item(id) => Some(id),
            Self::Augment(_) => None,
        }
    }

    pub(super) fn augment_id(self) -> Option<i64> {
        match self {
            Self::Item(_) => None,
            Self::Augment(id) => Some(id),
        }
    }

    pub(super) fn minimum_level(self, db: &Connection) -> Result<Option<i64>> {
        let (sql, id) = match self {
            Self::Item(id) => ("SELECT minimum_level FROM items WHERE id = ?1", id),
            Self::Augment(id) => ("SELECT min_level FROM augments WHERE id = ?1", id),
        };
        Ok(db.query_row(sql, [id], |r| r.get(0)).optional()?.flatten())
    }
}

pub(super) struct SourceLink<'a> {
    pub(super) source: LootSource,
    pub(super) loot: DroppedLoot,
    pub(super) loot_type: Option<LootType>,
    pub(super) is_rare: bool,
    pub(super) chest: Option<&'a str>,
    pub(super) tier: Option<SagaTier>,
    pub(super) cost: Option<&'a str>,
    pub(super) drop_text_segment: Option<&'a str>,
    pub(super) source_match: Option<&'a str>,
}

impl SourceLink<'_> {
    pub(super) fn from_quest(quest_id: i64, loot: DroppedLoot, loot_type: LootType) -> Self {
        Self {
            source: LootSource::Quest(quest_id),
            loot,
            loot_type: Some(loot_type),
            is_rare: false,
            chest: None,
            tier: None,
            cost: None,
            drop_text_segment: None,
            source_match: None,
        }
    }

    fn execute(&self, transaction: &Transaction, sql: &str) -> Result<usize> {
        let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(self.source.kind().as_str())];
        for column in SourceKind::identifying_columns() {
            values.push(Box::new(self.source.value_in_column(column)));
        }
        values.push(Box::new(self.loot.item_id()));
        values.push(Box::new(self.loot.augment_id()));
        values.push(Box::new(self.loot_type.map(LootType::as_str)));
        values.push(Box::new(self.is_rare));
        values.push(Box::new(self.chest));
        values.push(Box::new(self.tier.map(SagaTier::as_str)));
        values.push(Box::new(self.cost));
        values.push(Box::new(self.drop_text_segment));
        values.push(Box::new(self.source_match));
        let mut statement = transaction.prepare(sql)?;
        let parameter_count = statement.parameter_count();
        Ok(statement.execute(rusqlite::params_from_iter(values.into_iter().take(parameter_count)))?)
    }
}

struct SourceLinkSql {
    columns: String,
    values: String,
    same_source_and_loot: String,
    same_source_loot_type_and_tier: String,
    same_drop_location: String,
}

static SOURCE_LINK_SQL: LazyLock<SourceLinkSql> = LazyLock::new(|| {
    let identifying_columns = SourceKind::identifying_columns();
    let columns: Vec<&str> = std::iter::once("kind")
        .chain(identifying_columns.iter().copied())
        .chain([
            "item_id",
            "augment_id",
            "loot_type",
            "is_rare",
            "chest",
            "tier",
            "cost",
            "drop_text_segment",
            "source_match",
        ])
        .collect();
    let values: Vec<String> = (1..=columns.len()).map(|position| format!("?{position}")).collect();
    let same_source_and_loot: Vec<String> = columns[..identifying_columns.len() + 3]
        .iter()
        .enumerate()
        .map(|(index, column)| format!("{column} IS ?{}", index + 1))
        .collect();
    let matching_columns = |excluded: &[&str]| {
        columns
            .iter()
            .enumerate()
            .filter(|(_, column)| !excluded.contains(column) && !["drop_text_segment", "source_match"].contains(column))
            .map(|(index, column)| format!("{column} IS ?{}", index + 1))
            .collect::<Vec<_>>()
            .join(" AND ")
    };
    SourceLinkSql {
        columns: format!("sources ({})", columns.join(", ")),
        values: values.join(", "),
        same_source_and_loot: same_source_and_loot.join(" AND "),
        same_source_loot_type_and_tier: matching_columns(&["is_rare", "chest", "cost"]),
        same_drop_location: matching_columns(&["is_rare"]),
    }
});

pub(super) fn insert_source_link(transaction: &Transaction, source_link: &SourceLink) -> Result<usize> {
    let sql = &*SOURCE_LINK_SQL;
    if source_link.is_rare {
        let updated = source_link.execute(
            transaction,
            &format!("UPDATE sources SET is_rare = 1 WHERE {} AND is_rare = 0", sql.same_drop_location),
        )?;
        if updated > 0 {
            return Ok(updated);
        }
    }
    source_link.execute(
        transaction,
        &format!(
            "INSERT INTO {} SELECT {} WHERE NOT EXISTS (SELECT 1 FROM sources WHERE {})",
            sql.columns, sql.values, sql.same_drop_location
        ),
    )
}

pub(super) fn insert_source_link_unless_loot_linked_there(
    transaction: &Transaction,
    source_link: &SourceLink,
) -> Result<usize> {
    let sql = &*SOURCE_LINK_SQL;
    source_link.execute(
        transaction,
        &format!(
            "INSERT INTO {} SELECT {} WHERE NOT EXISTS (SELECT 1 FROM sources WHERE {})",
            sql.columns, sql.values, sql.same_source_and_loot
        ),
    )
}

pub(super) fn insert_source_link_unless_linked_as(
    transaction: &Transaction,
    source_link: &SourceLink,
) -> Result<usize> {
    let sql = &*SOURCE_LINK_SQL;
    source_link.execute(
        transaction,
        &format!(
            "INSERT INTO {} SELECT {} WHERE NOT EXISTS (SELECT 1 FROM sources WHERE {})",
            sql.columns, sql.values, sql.same_source_loot_type_and_tier
        ),
    )
}

pub(super) fn mark_quest_source_link_rare(
    transaction: &Transaction,
    quest_id: i64,
    loot: DroppedLoot,
    chest: Option<&str>,
) -> Result<usize> {
    let same_quest_and_loot = "kind = 'quest' AND quest_id = ?1 AND item_id IS ?2 AND augment_id IS ?3";
    Ok(transaction.execute(
        &format!(
            "UPDATE sources SET is_rare = 1, chest = CASE loot_type WHEN 'reward' THEN NULL ELSE COALESCE(chest, ?4) END
              WHERE {same_quest_and_loot}
                AND (loot_type <> 'reward' OR NOT EXISTS (SELECT 1 FROM sources WHERE {same_quest_and_loot} AND loot_type <> 'reward'))"
        ),
        params![quest_id, loot.item_id(), loot.augment_id(), chest],
    )?)
}

pub(super) struct LinkedDropTextQuest {
    pub(super) is_wiki_quest: bool,
    pub(super) is_newly_rare: bool,
}

impl DropTextLinker {
    pub(super) fn link_loot_to_quests_named_in(
        &self,
        transaction: &Transaction,
        loot: DroppedLoot,
        drop_text: &str,
    ) -> Result<Vec<LinkedDropTextQuest>> {
        let mut linked_quests = Vec::new();
        for quest_link in self.quest_links_in(drop_text) {
            let changed_row_count = insert_source_link(
                transaction,
                &SourceLink {
                    is_rare: quest_link.is_rare,
                    chest: quest_link.chest.as_deref(),
                    drop_text_segment: Some(&quest_link.drop_text_segment),
                    source_match: Some(&quest_link.source_match),
                    ..SourceLink::from_quest(quest_link.quest_id, loot, quest_link.loot_type)
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

impl DropTextLinker {
    pub(super) fn link_loot_to_packs_named_in(
        &self,
        transaction: &Transaction,
        loot: DroppedLoot,
        drop_text: &str,
    ) -> Result<usize> {
        let pack_links = self.pack_links_in(drop_text);
        for pack_link in &pack_links {
            insert_source_link(
                transaction,
                &SourceLink {
                    source: LootSource::AdventurePack(pack_link.pack_id),
                    loot,
                    loot_type: Some(pack_link.loot_type),
                    is_rare: pack_link.is_rare,
                    chest: pack_link.chest.as_deref(),
                    tier: None,
                    cost: None,
                    drop_text_segment: Some(&pack_link.drop_text_segment),
                    source_match: Some(&pack_link.source_match),
                },
            )?;
        }
        Ok(pack_links.len())
    }
}

impl TableWriter<'_> {
    pub(super) fn link_to_drop_text_packs(&self, loot: DroppedLoot, drop_text: &str) -> Result<usize> {
        self.drop_text_linker.link_loot_to_packs_named_in(self.transaction, loot, drop_text)
    }

    pub(super) fn link_item_to_drop_text_reward_givers(
        &self,
        item_id: i64,
        drop_text: &str,
    ) -> Result<Vec<QuestSeriesTable>> {
        let mut linked_tables = Vec::new();
        for reward_giver_link in self.drop_text_linker.reward_giver_links_in(drop_text) {
            let changed_row_count = insert_source_link(
                self.transaction,
                &SourceLink {
                    source: reward_giver_link.table.loot_source(reward_giver_link.reward_giver_id),
                    loot: DroppedLoot::Item(item_id),
                    loot_type: None,
                    is_rare: reward_giver_link.is_rare,
                    chest: None,
                    tier: reward_giver_link.tier,
                    cost: None,
                    drop_text_segment: Some(&reward_giver_link.drop_text_segment),
                    source_match: Some(&reward_giver_link.source_match),
                },
            )?;
            if changed_row_count > 0 {
                linked_tables.push(reward_giver_link.table);
            }
        }
        Ok(linked_tables)
    }

    pub(super) fn link_to_sources_named_in_drop_text(
        &self,
        loot: DroppedLoot,
        drop_text: &str,
        report: &mut BuildReport,
    ) -> Result<()> {
        for linked_kind in self.drop_text_linker.link_loot_to_sources_named_in(self.transaction, loot, drop_text)? {
            match linked_kind {
                SourceKind::CraftingSystem => report.drop_text_crafting_system_source_count += 1,
                SourceKind::Challenge => report.drop_text_challenge_source_count += 1,
                SourceKind::Starter => report.drop_text_starter_source_count += 1,
                SourceKind::Vendor => report.drop_text_vendor_source_count += 1,
                SourceKind::Event => report.drop_text_event_source_count += 1,
                _ => {}
            }
        }
        Ok(())
    }

    pub(super) fn link_to_drop_text_quests(
        &self,
        loot: DroppedLoot,
        drop_text: &str,
    ) -> Result<Vec<LinkedDropTextQuest>> {
        self.drop_text_linker.link_loot_to_quests_named_in(self.transaction, loot, drop_text)
    }
}
