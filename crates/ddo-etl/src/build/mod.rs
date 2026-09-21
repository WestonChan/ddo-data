//! Walk a DDOBuilderV2 `DataFiles` directory and write the database. Rebuilds from scratch every
//! run: the output is an artifact, not a store that gets patched.
//!
//! Stages, in order: seeds and versions; patrons and quests; clickies; items (which reference
//! clickies and quests); augments; gear sets and filigree sets; set membership (which needs both
//! items and sets).

mod augments;
mod characters;
mod items;
mod modifiers;
mod sets;

use crate::map::augment_slot::decode;
use crate::map::buff::BuffMap;
use crate::map::effect::EffectMap;
use crate::xml::items::parse_item_file;
use crate::xml::quests::Quest;
use crate::xml::{clickies, item_buffs, patrons, quests};
use anyhow::{Context, Result};
use ddo_model::enums::BonusType;
use ddo_model::stats::Stat;
use ddo_model::{seeds, DatasetVersion, SCHEMA_VERSION};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BuildReport {
    pub items_written: usize,
    pub items_skipped_cosmetic: usize,
    pub bonuses: usize,
    pub effects: usize,
    pub quests: usize,
    pub quest_loot_links: usize,
    pub augment_slot_types: usize,
    pub augments: usize,
    pub set_bonuses: usize,
    pub filigrees: usize,
    pub clickies: usize,
    pub modifiers: usize,
    pub feats: usize,
    pub races: usize,
    pub classes: usize,
    /// Effect types `derive` could not map onto a stat, with how often each occurred. They are
    /// still stored as modifiers; this is the to-do list for `data/effect_map.toml`.
    pub unmapped_effect_types: BTreeMap<String, usize>,
}

pub fn build(source: &Path, conn: &mut Connection, version: &DatasetVersion) -> Result<BuildReport> {
    conn.execute_batch(ddo_model::ddl()).context("applying DDL")?;
    seeds::insert_all(conn).context("inserting seed tables")?;

    let buff_map = BuffMap::load()?;
    let effect_map = EffectMap::load()?;
    let templates = item_buffs::parse(&source.join("ItemBuffs.xml"))?;
    let quest_list = quests::parse(&source.join("Quests.xml"))?;
    let patron_list = patrons::parse(&source.join("Patrons.xml"))?;
    let clickie_list = clickies::parse(&source.join("ItemClickies.xml"))?;

    let tx = conn.transaction()?;
    let mut report = BuildReport::default();

    tx.execute("DELETE FROM schema_version", [])?;
    tx.execute("INSERT INTO schema_version (version) VALUES (?1)", params![SCHEMA_VERSION])?;
    tx.execute("DELETE FROM dataset_version", [])?;
    tx.execute(
        "INSERT INTO dataset_version (upstream_sha, built_at) VALUES (?1, ?2)",
        params![version.upstream_sha, version.built_at],
    )?;

    for p in &patron_list {
        tx.execute("INSERT OR IGNORE INTO patrons (name) VALUES (?1)", params![p.name.trim()])?;
    }
    let quest_index = write_quests(&tx, &quest_list)?;
    report.quests = quest_index.len();

    let mut ctx = Ctx {
        tx: &tx,
        buff_map: &buff_map,
        effect_map: &effect_map,
        templates: &templates,
        quests: &quest_index,
        caches: Caches::default(),
        pending_set_items: Vec::new(),
    };

    ctx.write_standard_feats(&source.join("Feats.xml"), &mut report)?;
    for path in files_with_extension(&source.join("Races"), "xml")? {
        ctx.write_race_file(&path, &mut report).with_context(|| format!("{}", path.display()))?;
    }
    for path in files_with_extension(&source.join("Classes"), "xml")? {
        ctx.write_class_file(&path, &mut report).with_context(|| format!("{}", path.display()))?;
    }
    ctx.resolve_base_classes()?;

    for c in &clickie_list {
        ctx.write_clickie(c)?;
    }
    report.clickies = ctx.caches.clickies.len();

    for path in files_with_extension(&source.join("Items"), "item")? {
        let file = parse_item_file(&path)?;
        for item in &file.items {
            ctx.write_item(item, &mut report).with_context(|| format!("{}", path.display()))?;
        }
    }

    for path in files_with_extension(&source.join("Augments"), "xml")? {
        report.augments += ctx.write_augments_file(&path).with_context(|| format!("{}", path.display()))?;
    }

    ctx.write_set_file(&source.join("SetBonuses.xml"), false, &mut report)?;
    for path in files_with_extension(&source.join("FiligreeSets"), "xml")? {
        ctx.write_set_file(&path, true, &mut report).with_context(|| format!("{}", path.display()))?;
    }
    ctx.resolve_set_items()?;

    report.bonuses = ctx.caches.bonuses.len();
    report.effects = ctx.caches.effects.len();
    report.augment_slot_types = ctx.caches.slot_types.len();
    report.set_bonuses = ctx.caches.sets.len();
    report.modifiers = ctx.caches.modifiers_written;
    report.unmapped_effect_types = effect_map.unmapped_types();
    tx.commit()?;
    Ok(report)
}

/// Sorted paths of `*.{ext}` directly inside `dir`. A missing directory is an empty list, so a
/// trimmed fixture tree need not contain every family.
fn files_with_extension(dir: &Path, ext: &str) -> Result<Vec<PathBuf>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("listing {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == ext))
        .collect();
    paths.sort();
    Ok(paths)
}

/// Quests by name, longest first, so `DropLocation` matching prefers the most specific name.
pub(crate) struct QuestIndex {
    by_length: Vec<(String, i64, bool)>,
}

impl QuestIndex {
    fn len(&self) -> usize {
        self.by_length.len()
    }
}

fn write_quests(tx: &Transaction, quests: &[Quest]) -> Result<QuestIndex> {
    let mut by_length = Vec::with_capacity(quests.len());
    for q in quests {
        let pack_id = match &q.adventure_pack {
            Some(pack) => {
                tx.execute(
                    "INSERT OR IGNORE INTO adventure_packs (name, is_free_to_play) VALUES (?1, ?2)",
                    params![pack, pack == "Free to Play"],
                )?;
                Some(tx.query_row("SELECT id FROM adventure_packs WHERE name = ?1", params![pack], |r| {
                    r.get::<_, i64>(0)
                })?)
            }
            None => None,
        };
        let patron_id = match &q.patron {
            Some(p) => {
                tx.query_row("SELECT id FROM patrons WHERE name = ?1", params![p], |r| r.get::<_, i64>(0)).optional()?
            }
            None => None,
        };
        tx.execute(
            "INSERT OR IGNORE INTO quests (name, pack_id, patron_id, level, epic_level, favor, is_raid) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![q.name, pack_id, patron_id, q.levels.first(), q.levels.get(1), q.favor, q.is_raid],
        )?;
        let id: i64 = tx.query_row("SELECT id FROM quests WHERE name = ?1", params![q.name], |r| r.get(0))?;
        by_length.push((q.name.clone(), id, q.is_raid));
    }
    by_length.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
    Ok(QuestIndex { by_length })
}

/// (stat_id, bonus_type_id, value, value2): the identity of a `bonuses` row.
type BonusKey = (i64, Option<i64>, Option<i64>, Option<i64>);

#[derive(Default)]
pub(crate) struct Caches {
    materials: HashMap<String, i64>,
    slot_types: HashMap<String, i64>,
    bonuses: HashMap<BonusKey, i64>,
    effects: HashMap<String, i64>,
    clickies: HashMap<String, i64>,
    sets: HashMap<String, i64>,
    /// (name, source, source id) → feats.id
    feats: HashMap<(String, ddo_model::enums::FeatSource, Option<i64>), i64>,
    classes: HashMap<String, i64>,
    modifiers_written: usize,
}

pub(crate) struct Ctx<'a> {
    tx: &'a Transaction<'a>,
    buff_map: &'a BuffMap,
    effect_map: &'a EffectMap,
    templates: &'a HashMap<String, String>,
    quests: &'a QuestIndex,
    caches: Caches,
    /// (item_id, set name) pairs to link once the set tables exist.
    pending_set_items: Vec<(i64, String)>,
}

impl Ctx<'_> {
    /// The id of the `bonuses` row for this (stat, type, value, value2), inserting it on first use.
    fn bonus_id(
        &mut self,
        stat: &'static Stat,
        bonus_type: Option<BonusType>,
        value: Option<i64>,
        value2: Option<i64>,
        description: Option<&str>,
    ) -> Result<i64> {
        let key = (stat.id, bonus_type.map(BonusType::id), value, value2);
        if let Some(id) = self.caches.bonuses.get(&key) {
            return Ok(*id);
        }
        let label = match value {
            Some(v) if v < 0 => format!("{} {v}", stat.name),
            Some(v) => format!("{} +{v}", stat.name),
            None => stat.name.to_string(),
        };
        self.tx.execute(
            "INSERT INTO bonuses (name, description, stat_id, bonus_type_id, value, value2) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![label, description, stat.id, key.1, value, value2],
        )?;
        let id = self.tx.last_insert_rowid();
        self.caches.bonuses.insert(key, id);
        Ok(id)
    }

    fn material_id(&mut self, name: &str) -> Result<i64> {
        if let Some(id) = self.caches.materials.get(name) {
            return Ok(*id);
        }
        self.tx.execute("INSERT INTO item_materials (name) VALUES (?1)", params![name])?;
        let id = self.tx.last_insert_rowid();
        self.caches.materials.insert(name.to_string(), id);
        Ok(id)
    }

    fn slot_type_id(&mut self, raw: &str) -> Result<i64> {
        let spec = decode(raw);
        if let Some(id) = self.caches.slot_types.get(&spec.label) {
            return Ok(*id);
        }
        self.tx.execute(
            "INSERT INTO augment_slot_types (label, family, variant, qualifier) VALUES (?1, ?2, ?3, ?4)",
            params![spec.label, spec.family, spec.variant, spec.qualifier],
        )?;
        let id = self.tx.last_insert_rowid();
        self.caches.slot_types.insert(spec.label, id);
        Ok(id)
    }

    fn write_clickie(&mut self, c: &clickies::Clickie) -> Result<()> {
        let name = c.name.trim();
        self.tx.execute(
            "INSERT OR IGNORE INTO clickies (name, description, icon, school) VALUES (?1, ?2, ?3, ?4)",
            params![
                name,
                nonempty(c.description.as_deref()),
                nonempty(c.icon.as_deref()),
                nonempty(c.school.as_deref())
            ],
        )?;
        let id: i64 = self.tx.query_row("SELECT id FROM clickies WHERE name = ?1", params![name], |r| r.get(0))?;
        if self.caches.clickies.insert(name.to_string(), id).is_none() {
            self.write_modifiers(ddo_model::enums::ModifierSource::Clickie, id, &c.effects)?;
        }
        Ok(())
    }
}

/// Trimmed text, or `None` when absent or blank.
pub(crate) fn nonempty(s: Option<&str>) -> Option<&str> {
    s.map(str::trim).filter(|s| !s.is_empty())
}

/// A JSON array of numbers with integral values written without a fractional part: `[2,3.5]`.
pub(crate) fn json_numbers(values: &[f64]) -> Option<String> {
    if values.is_empty() {
        return None;
    }
    let parts: Vec<String> =
        values.iter().map(|v| if v.fract() == 0.0 { format!("{}", *v as i64) } else { v.to_string() }).collect();
    Some(format!("[{}]", parts.join(",")))
}

pub(crate) fn json_strings(values: &[String]) -> Option<String> {
    if values.is_empty() {
        None
    } else {
        serde_json::to_string(values).ok()
    }
}
