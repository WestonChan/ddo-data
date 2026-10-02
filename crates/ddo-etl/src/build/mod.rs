mod augments;
mod buffs;
mod characters;
mod corrections;
mod drop_text;
mod items;
mod modifiers;
mod quest_series;
mod sets;
mod trees_spells;
mod vendors_and_events;
mod wiki;

use crate::corrections::Corrections;
use crate::map::augment_slot::AugmentSlotType;
use crate::map::buff::BuffMap;
use crate::map::drop_location::{names_saga, reward_giver_name, segment_head};
use crate::map::effect::EffectMap;
use crate::map::legacy_drop_source::LegacyDropSources;
use crate::map::source_alias::SourceAliases;
use crate::wiki::WikiOverrides;
use crate::xml::challenges::{self, Challenge};
use crate::xml::items::parse_item_file;
use crate::xml::quests::Quest;
use crate::xml::{clickies, item_buffs, patrons, quests};
use anyhow::{Context, Result};
use ddo_model::enums::{BonusType, FeatSource, ModifierSource};
use ddo_model::stats::Stat;
use ddo_model::{seeds, DatasetVersion, SCHEMA_VERSION};
use drop_text::DropTextLinker;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BuildReport {
    pub written_item_count: usize,
    pub skipped_cosmetic_item_count: usize,
    pub legacy_item_count: usize,
    pub bonus_count: usize,
    pub effect_count: usize,
    pub quest_count: usize,
    pub challenge_count: usize,
    pub quest_loot_link_count: usize,
    pub drop_text_rare_link_count: usize,
    pub drop_text_wiki_quest_link_count: usize,
    pub quest_augment_loot_link_count: usize,
    pub drop_text_rare_augment_link_count: usize,
    pub pack_loot_link_count: usize,
    pub pack_augment_loot_link_count: usize,
    pub legacy_source_flagged_count: usize,
    pub augment_slot_type_count: usize,
    pub augment_count: usize,
    pub set_bonus_count: usize,
    pub filigree_count: usize,
    pub clickie_count: usize,
    pub modifier_count: usize,
    pub feat_count: usize,
    pub race_count: usize,
    pub class_count: usize,
    pub enhancement_tree_count: usize,
    pub enhancement_count: usize,
    pub skipped_duplicate_tree_count: usize,
    pub spell_count: usize,
    pub skipped_duplicate_spell_count: usize,
    pub standalone_stance_count: usize,
    pub guild_buff_count: usize,
    pub optional_buff_count: usize,
    pub sentient_gem_count: usize,
    pub wiki_quest_loot_entry_count: usize,
    pub wiki_rare_drop_count: usize,
    pub wiki_added_quest_loot_link_count: usize,
    pub wiki_loot_drop_count: usize,
    pub wiki_rare_augment_drop_count: usize,
    pub wiki_added_quest_augment_loot_link_count: usize,
    pub wiki_loot_augment_drop_count: usize,
    pub wiki_quest_entry_count: usize,
    pub wiki_quest_created_count: usize,
    pub wiki_quest_superseded_count: usize,
    pub wiki_quest_probable_duplicate_count: usize,
    pub superseded_wiki_quests: Vec<SupersededWikiEntry>,
    pub probable_duplicate_wiki_quests: Vec<ProbableDuplicateWikiEntry>,
    pub wiki_crafting_system_count: usize,
    pub wiki_crafting_recipe_count: usize,
    pub wiki_crafting_ingredient_count: usize,
    pub wiki_description_entry_count: usize,
    pub wiki_description_filled_count: usize,
    pub wiki_description_skipped_count: usize,
    pub wiki_description_augment_link_count: usize,
    pub wiki_item_written_count: usize,
    pub wiki_item_superseded_count: usize,
    pub wiki_item_probable_duplicate_count: usize,
    pub superseded_wiki_items: Vec<SupersededWikiEntry>,
    pub probable_duplicate_wiki_items: Vec<ProbableDuplicateWikiEntry>,
    pub wiki_augment_written_count: usize,
    pub wiki_augment_superseded_count: usize,
    pub wiki_augment_probable_duplicate_count: usize,
    pub superseded_wiki_augments: Vec<SupersededWikiEntry>,
    pub probable_duplicate_wiki_augments: Vec<ProbableDuplicateWikiEntry>,
    pub wiki_quest_chain_count: usize,
    pub quest_chain_quest_link_count: usize,
    pub quest_chain_reward_count: usize,
    pub wiki_saga_count: usize,
    pub saga_quest_link_count: usize,
    pub saga_reward_count: usize,
    pub drop_text_quest_chain_reward_count: usize,
    pub drop_text_saga_reward_count: usize,
    pub drop_text_crafting_system_source_count: usize,
    pub drop_text_challenge_source_count: usize,
    pub drop_text_starter_source_count: usize,
    pub wiki_vendor_count: usize,
    pub vendor_item_count: usize,
    pub drop_text_vendor_source_count: usize,
    pub wiki_event_count: usize,
    pub event_item_count: usize,
    pub drop_text_event_source_count: usize,
    pub unresolved_source_aliases: Vec<String>,
    pub correction_applied_count: usize,
    pub correction_stale_count: usize,
    pub stale_corrections: Vec<StaleCorrection>,
    pub unmapped_effect_type_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupersededWikiEntry {
    pub name: String,
    pub file_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleCorrection {
    pub kind: String,
    pub name: String,
    pub field: String,
    pub cause: StaleCorrectionCause,
    pub file_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StaleCorrectionCause {
    ValueChanged { expected_value: String, maetrim_value: String },
    RenameDoneUpstream { new_name: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbableDuplicateWikiEntry {
    pub name: String,
    pub maetrim_name: String,
}

pub fn build_database(
    data_files_dir: &Path,
    wiki_overrides: &WikiOverrides,
    corrections: &Corrections,
    db: &mut Connection,
    dataset_version: &DatasetVersion,
) -> Result<BuildReport> {
    db.execute_batch(ddo_model::ddl()).context("applying DDL")?;
    seeds::insert_all(db).context("inserting seed tables")?;

    let item_buff_definitions = item_buffs::parse(&data_files_dir.join("ItemBuffs.xml"))?;
    let buff_map = BuffMap::load(&item_buff_definitions)?;
    let effect_map = EffectMap::load()?;
    let buff_description_templates: HashMap<String, String> =
        item_buff_definitions.into_iter().map(|(buff_kind, definition)| (buff_kind, definition.display_text)).collect();
    let parsed_quests = quests::parse(&data_files_dir.join("Quests.xml"))?;
    let parsed_patrons = patrons::parse(&data_files_dir.join("Patrons.xml"))?;
    let challenges_path = data_files_dir.join("Challenges.xml");
    let parsed_challenges = if challenges_path.is_file() { challenges::parse(&challenges_path)? } else { Vec::new() };
    let parsed_clickies = clickies::parse(&data_files_dir.join("ItemClickies.xml"))?;

    let transaction = db.transaction()?;
    let mut report = BuildReport::default();

    transaction.execute("DELETE FROM schema_version", [])?;
    transaction.execute("INSERT INTO schema_version (version) VALUES (?1)", params![SCHEMA_VERSION])?;
    transaction.execute("DELETE FROM dataset_version", [])?;
    transaction.execute(
        "INSERT INTO dataset_version (upstream_sha, built_at) VALUES (?1, ?2)",
        params![dataset_version.upstream_sha, dataset_version.built_at],
    )?;

    for patron in &parsed_patrons {
        transaction.execute("INSERT OR IGNORE INTO patrons (name) VALUES (?1)", params![patron.name.trim()])?;
    }
    report.quest_count = write_quests(&transaction, &parsed_quests)?;
    report.challenge_count = write_challenges(&transaction, &parsed_challenges)?;
    corrections::apply_quest_corrections(&transaction, corrections, &mut report)?;
    wiki::write_wiki_quests(&transaction, &wiki_overrides.quests, &mut report)?;
    quest_series::write_wiki_quest_series(&transaction, wiki_overrides, &mut report)?;
    wiki::write_wiki_crafting_systems(&transaction, wiki_overrides)?;
    vendors_and_events::write_wiki_vendors_and_events(&transaction, wiki_overrides, &mut report)?;
    let drop_text_linker = DropTextLinker::from_written_tables(
        &transaction,
        &LegacyDropSources::embedded()?,
        &SourceAliases::embedded()?,
    )?;
    report.unresolved_source_aliases = drop_text_linker.unresolved_alias_texts().to_vec();

    let mut writer = TableWriter {
        transaction: &transaction,
        buff_map: &buff_map,
        effect_map: &effect_map,
        buff_description_templates: &buff_description_templates,
        drop_text_linker: &drop_text_linker,
        written: WrittenRows::default(),
        pending_set_item_links: Vec::new(),
        pending_set_augment_links: Vec::new(),
    };

    writer.write_standard_feats(&data_files_dir.join("Feats.xml"), &mut report)?;
    for path in files_with_extension(&data_files_dir.join("Races"), "xml")? {
        writer.write_race_file(&path, &mut report).with_context(|| format!("{}", path.display()))?;
    }
    for path in files_with_extension(&data_files_dir.join("Classes"), "xml")? {
        writer.write_class_file(&path, &mut report).with_context(|| format!("{}", path.display()))?;
    }
    writer.link_base_classes()?;
    for path in files_with_extension(&data_files_dir.join("EnhancementTrees"), "xml")?
        .into_iter()
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().ends_with(".tree.xml")))
    {
        writer.write_tree_file(&path, &mut report).with_context(|| format!("{}", path.display()))?;
    }
    writer.write_spells(&data_files_dir.join("Spells.xml"), &mut report)?;

    for clickie in &parsed_clickies {
        writer.write_clickie(clickie)?;
    }
    report.clickie_count = writer.written.clickie_ids_by_name.len();

    for path in files_with_extension(&data_files_dir.join("Items"), "item")? {
        let item_file = parse_item_file(&path)?;
        for item in &item_file.items {
            writer.write_item(item, &mut report).with_context(|| format!("{}", path.display()))?;
        }
    }

    for path in files_with_extension(&data_files_dir.join("Augments"), "xml")? {
        writer.write_augments_file(&path, &mut report).with_context(|| format!("{}", path.display()))?;
    }

    writer.write_set_bonus_file(&data_files_dir.join("SetBonuses.xml"), false, &mut report)?;
    for path in files_with_extension(&data_files_dir.join("FiligreeSets"), "xml")? {
        writer.write_set_bonus_file(&path, true, &mut report).with_context(|| format!("{}", path.display()))?;
    }
    writer.write_wiki_items(&wiki_overrides.items, &mut report)?;
    writer.write_wiki_augments(&wiki_overrides.augments, &mut report)?;
    writer.link_pending_set_members()?;
    writer.write_sentient_gems(&data_files_dir.join("Sentient.gems.xml"), &mut report)?;
    writer.link_spell_references()?;
    writer.write_standalone_stances(&data_files_dir.join("Stances.xml"), &mut report)?;
    writer.write_guild_buffs(&data_files_dir.join("GuildBuffs.xml"), &mut report)?;
    writer.write_optional_buffs(&data_files_dir.join("SelfAndPartyBuffs.xml"), &mut report)?;
    corrections::apply_non_quest_corrections(&transaction, corrections, &mut report)?;
    wiki::apply_wiki_overrides(&transaction, wiki_overrides, &drop_text_linker, &mut report)?;
    quest_series::write_wiki_quest_series_rewards(&transaction, wiki_overrides, &mut report)?;
    vendors_and_events::write_wiki_vendor_and_event_items(&transaction, wiki_overrides, &mut report)?;
    delete_bonuses_nothing_carries(&transaction)?;

    report.legacy_item_count =
        transaction.query_row("SELECT COUNT(*) FROM items WHERE is_legacy", [], |r| r.get::<_, i64>(0))? as usize;
    report.bonus_count = transaction.query_row("SELECT COUNT(*) FROM bonuses", [], |r| r.get::<_, i64>(0))? as usize;
    report.effect_count = writer.written.effect_ids_by_name.len();
    report.augment_slot_type_count =
        transaction.query_row("SELECT COUNT(*) FROM augment_slot_types", [], |r| r.get::<_, i64>(0))? as usize;
    report.set_bonus_count = writer.written.set_bonus_ids_by_name.len();
    report.modifier_count = writer.written.modifier_count;
    report.unmapped_effect_type_counts = effect_map.unmapped_type_counts();
    transaction.commit()?;
    Ok(report)
}

fn delete_bonuses_nothing_carries(transaction: &Transaction) -> Result<()> {
    transaction.execute(
        "DELETE FROM bonuses WHERE NOT EXISTS (SELECT 1 FROM item_bonuses r WHERE r.bonus_id = bonuses.id)
            AND NOT EXISTS (SELECT 1 FROM augment_bonuses r WHERE r.bonus_id = bonuses.id)
            AND NOT EXISTS (SELECT 1 FROM feat_bonuses r WHERE r.bonus_id = bonuses.id)
            AND NOT EXISTS (SELECT 1 FROM set_bonus_tier_bonuses r WHERE r.bonus_id = bonuses.id)",
        [],
    )?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnlinkedRewardGiver {
    pub name: String,
    pub is_saga: bool,
    pub item_count: usize,
}

pub fn unlinked_reward_givers(db: &Connection) -> Result<Vec<UnlinkedRewardGiver>> {
    let drop_text_linker =
        DropTextLinker::from_written_tables(db, &LegacyDropSources::embedded()?, &SourceAliases::embedded()?)?;
    let drop_texts = maetrim_item_drop_texts(db)?;
    let mut item_counts_by_reward_giver: BTreeMap<(bool, String), usize> = BTreeMap::new();
    for drop_text in &drop_texts {
        let mut reward_givers_in_item: Vec<(bool, String)> = drop_text_linker
            .unlinked_reward_segments(drop_text)
            .into_iter()
            .filter_map(|segment| reward_giver_name(segment).map(|name| (names_saga(segment), name)))
            .collect();
        reward_givers_in_item.sort();
        reward_givers_in_item.dedup();
        for reward_giver in reward_givers_in_item {
            *item_counts_by_reward_giver.entry(reward_giver).or_default() += 1;
        }
    }
    Ok(item_counts_by_reward_giver
        .into_iter()
        .map(|((is_saga, name), item_count)| UnlinkedRewardGiver { name, is_saga, item_count })
        .collect())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnlinkedDropSegmentHead {
    pub head: String,
    pub item_count: usize,
}

pub fn unlinked_drop_segment_heads(db: &Connection) -> Result<Vec<UnlinkedDropSegmentHead>> {
    unlinked_segment_heads(db, |segment| Some(segment.split([',', '(']).next().unwrap_or(segment).trim().to_string()))
}

pub fn unlinked_trade_segment_heads(db: &Connection) -> Result<Vec<UnlinkedDropSegmentHead>> {
    unlinked_segment_heads(db, |segment| {
        let lowercase_segment = segment.to_lowercase();
        if !TRADE_MARKERS.iter().any(|marker| lowercase_segment.contains(marker)) {
            return None;
        }
        let head = segment_head(segment);
        Some(head.split(" sells ").next().unwrap_or(head).trim().to_string())
    })
}

const TRADE_MARKERS: [&str; 4] = ["turn in", "sells", "exchange", "trade"];

fn unlinked_segment_heads(
    db: &Connection,
    head_of_segment: impl Fn(&str) -> Option<String>,
) -> Result<Vec<UnlinkedDropSegmentHead>> {
    let drop_text_linker =
        DropTextLinker::from_written_tables(db, &LegacyDropSources::embedded()?, &SourceAliases::embedded()?)?;
    let mut item_counts_by_head: BTreeMap<String, usize> = BTreeMap::new();
    for drop_text in &maetrim_item_drop_texts(db)? {
        let mut heads_in_item: Vec<String> = drop_text_linker
            .unlinked_segments(drop_text)
            .into_iter()
            .filter_map(&head_of_segment)
            .filter(|head| !head.is_empty())
            .collect();
        heads_in_item.sort();
        heads_in_item.dedup();
        for head in heads_in_item {
            *item_counts_by_head.entry(head).or_default() += 1;
        }
    }
    let mut unlinked_heads: Vec<UnlinkedDropSegmentHead> = item_counts_by_head
        .into_iter()
        .map(|(head, item_count)| UnlinkedDropSegmentHead { head, item_count })
        .collect();
    unlinked_heads.sort_by(|a, b| b.item_count.cmp(&a.item_count).then_with(|| a.head.cmp(&b.head)));
    Ok(unlinked_heads)
}

fn maetrim_item_drop_texts(db: &Connection) -> Result<Vec<String>> {
    let mut statement =
        db.prepare("SELECT drop_location FROM items WHERE provenance = ?1 AND drop_location IS NOT NULL ORDER BY id")?;
    let drop_texts = statement
        .query_map(params![ddo_model::enums::Provenance::Maetrim.as_str()], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(drop_texts)
}

fn files_with_extension(dir: &Path, extension: &str) -> Result<Vec<PathBuf>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("listing {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == extension))
        .collect();
    paths.sort();
    Ok(paths)
}

fn ensure_adventure_pack(transaction: &Transaction, pack_name: Option<&str>) -> Result<Option<i64>> {
    let Some(pack_name) = pack_name else {
        return Ok(None);
    };
    transaction.execute(
        "INSERT OR IGNORE INTO adventure_packs (name, is_free_to_play) VALUES (?1, ?2)",
        params![pack_name, pack_name == "Free to Play"],
    )?;
    Ok(Some(transaction.query_row("SELECT id FROM adventure_packs WHERE name = ?1", params![pack_name], |r| r.get(0))?))
}

fn patron_id(transaction: &Transaction, patron_name: Option<&str>) -> Result<Option<i64>> {
    match patron_name {
        Some(name) => Ok(transaction
            .query_row("SELECT id FROM patrons WHERE name = ?1", params![name], |r| r.get(0))
            .optional()?),
        None => Ok(None),
    }
}

fn write_quests(transaction: &Transaction, quests: &[Quest]) -> Result<usize> {
    for quest in quests {
        transaction.execute(
            "INSERT OR IGNORE INTO quests (name, pack_id, patron_id, level, epic_level, favor, is_raid, epic_name, difficulties)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                quest.name,
                ensure_adventure_pack(transaction, quest.adventure_pack.as_deref())?,
                patron_id(transaction, quest.patron.as_deref())?,
                quest.levels.first(),
                quest.levels.get(1),
                quest.favor,
                quest.is_raid,
                quest.epic_name,
                serde_json::to_string(&quest.difficulties.iter().map(|d| d.as_str()).collect::<Vec<_>>())?,
            ],
        )?;
    }
    Ok(quests.len())
}

fn write_challenges(transaction: &Transaction, challenges: &[Challenge]) -> Result<usize> {
    let mut inserted_count = 0;
    for challenge in challenges {
        inserted_count += transaction.execute(
            "INSERT OR IGNORE INTO quests (name, pack_id, patron_id, level, max_level, is_challenge) VALUES (?1, ?2, ?3, ?4, ?5, 1)",
            params![
                challenge.name,
                ensure_adventure_pack(transaction, challenge.adventure_pack.as_deref())?,
                patron_id(transaction, challenge.patron.as_deref())?,
                challenge.level_range.first(),
                challenge.level_range.get(1),
            ],
        )?;
    }
    Ok(inserted_count)
}

type BonusKey = (i64, Option<i64>, Option<i64>, Option<i64>);

type FeatKey = (String, FeatSource, Option<i64>);

#[derive(Default)]
pub(crate) struct WrittenRows {
    material_ids_by_name: HashMap<String, i64>,
    augment_slot_type_ids_by_label: HashMap<String, i64>,
    bonus_ids_by_key: HashMap<BonusKey, i64>,
    effect_ids_by_name: HashMap<String, i64>,
    clickie_ids_by_name: HashMap<String, i64>,
    set_bonus_ids_by_name: HashMap<String, i64>,
    feat_ids_by_key: HashMap<FeatKey, i64>,
    modifier_count: usize,
}

pub(crate) struct TableWriter<'a> {
    transaction: &'a Transaction<'a>,
    buff_map: &'a BuffMap,
    effect_map: &'a EffectMap,
    buff_description_templates: &'a HashMap<String, String>,
    drop_text_linker: &'a DropTextLinker,
    written: WrittenRows,
    pending_set_item_links: Vec<(i64, String)>,
    pending_set_augment_links: Vec<(i64, String)>,
}

impl TableWriter<'_> {
    fn ensure_bonus(
        &mut self,
        stat: &'static Stat,
        bonus_type: Option<BonusType>,
        value: Option<i64>,
        second_value: Option<i64>,
        description: Option<&str>,
    ) -> Result<i64> {
        let key = (stat.id, bonus_type.map(BonusType::id), value, second_value);
        if let Some(id) = self.written.bonus_ids_by_key.get(&key) {
            return Ok(*id);
        }
        let bonus_name = bonus_name(stat.name, value);
        self.transaction.execute(
            "INSERT INTO bonuses (name, description, stat_id, bonus_type_id, value, value2) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![bonus_name, description, stat.id, key.1, value, second_value],
        )?;
        let id = self.transaction.last_insert_rowid();
        self.written.bonus_ids_by_key.insert(key, id);
        Ok(id)
    }

    fn ensure_material(&mut self, name: &str) -> Result<i64> {
        if let Some(id) = self.written.material_ids_by_name.get(name) {
            return Ok(*id);
        }
        self.transaction.execute("INSERT INTO item_materials (name) VALUES (?1)", params![name])?;
        let id = self.transaction.last_insert_rowid();
        self.written.material_ids_by_name.insert(name.to_string(), id);
        Ok(id)
    }

    fn ensure_augment_slot_type(&mut self, upstream_name: &str) -> Result<i64> {
        let slot_type = AugmentSlotType::parse(upstream_name);
        if let Some(id) = self.written.augment_slot_type_ids_by_label.get(&slot_type.label) {
            return Ok(*id);
        }
        self.transaction.execute(
            "INSERT INTO augment_slot_types (label, family, variant, qualifier) VALUES (?1, ?2, ?3, ?4)",
            params![slot_type.label, slot_type.family, slot_type.variant, slot_type.qualifier],
        )?;
        let id = self.transaction.last_insert_rowid();
        self.written.augment_slot_type_ids_by_label.insert(slot_type.label, id);
        Ok(id)
    }

    fn write_clickie(&mut self, clickie: &clickies::Clickie) -> Result<()> {
        let name = clickie.name.trim();
        self.transaction.execute(
            "INSERT OR IGNORE INTO clickies (name, description, icon, school) VALUES (?1, ?2, ?3, ?4)",
            params![
                name,
                trimmed_non_empty(clickie.description.as_deref()),
                trimmed_non_empty(clickie.icon.as_deref()),
                trimmed_non_empty(clickie.school.as_deref())
            ],
        )?;
        let id: i64 =
            self.transaction.query_row("SELECT id FROM clickies WHERE name = ?1", params![name], |r| r.get(0))?;
        if self.written.clickie_ids_by_name.insert(name.to_string(), id).is_none() {
            self.write_modifiers(ModifierSource::Clickie, id, &clickie.effects)?;
        }
        Ok(())
    }
}

pub(crate) fn bonus_name(stat_name: &str, value: Option<i64>) -> String {
    match value {
        Some(v) if v < 0 => format!("{stat_name} {v}"),
        Some(v) => format!("{stat_name} +{v}"),
        None => stat_name.to_string(),
    }
}

pub(crate) fn trimmed_non_empty(text: Option<&str>) -> Option<&str> {
    text.map(str::trim).filter(|s| !s.is_empty())
}

pub(crate) fn joined_non_empty(parts: &[String], separator: &str) -> Option<String> {
    let non_empty_parts: Vec<&str> = parts.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    if non_empty_parts.is_empty() {
        None
    } else {
        Some(non_empty_parts.join(separator))
    }
}

pub(crate) fn json_number_array(numbers: &[f64]) -> Option<String> {
    if numbers.is_empty() {
        return None;
    }
    let formatted_numbers: Vec<String> =
        numbers.iter().map(|v| if v.fract() == 0.0 { format!("{}", *v as i64) } else { v.to_string() }).collect();
    Some(format!("[{}]", formatted_numbers.join(",")))
}

pub(crate) fn json_string_array(strings: &[String]) -> Option<String> {
    if strings.is_empty() {
        None
    } else {
        serde_json::to_string(strings).ok()
    }
}
