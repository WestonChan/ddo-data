//! Walk a DDOBuilderV2 `DataFiles` directory and write the database. Rebuilds from scratch every
//! run: the output is an artifact, not a store that gets patched.

use crate::map::augment_slot::decode;
use crate::map::buff::{BuffMap, Resolved};
use crate::map::material;
use crate::map::placement::{classify, Placement};
use crate::xml::items::{parse_item_file, Item};
use crate::xml::quests::Quest;
use crate::xml::{item_buffs, patrons, quests};
use anyhow::{Context, Result};
use ddo_model::enums::{ArmorType, LootType};
use ddo_model::{seeds, DatasetVersion, SCHEMA_VERSION};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BuildReport {
    pub items_written: usize,
    pub items_skipped_cosmetic: usize,
    pub bonuses: usize,
    pub effects: usize,
    pub quests: usize,
    pub quest_loot_links: usize,
    pub augment_slot_types: usize,
}

pub fn build(source: &Path, conn: &mut Connection, version: &DatasetVersion) -> Result<BuildReport> {
    conn.execute_batch(ddo_model::ddl()).context("applying DDL")?;
    seeds::insert_all(conn).context("inserting seed tables")?;

    let buff_map = BuffMap::load()?;
    let templates = item_buffs::parse(&source.join("ItemBuffs.xml"))?;
    let quest_list = quests::parse(&source.join("Quests.xml"))?;
    let patron_list = patrons::parse(&source.join("Patrons.xml"))?;

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

    let mut ctx =
        Ctx { tx: &tx, buff_map: &buff_map, templates: &templates, quests: &quest_index, caches: Caches::default() };

    let mut paths: Vec<_> = std::fs::read_dir(source.join("Items"))
        .with_context(|| format!("listing {}", source.join("Items").display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "item"))
        .collect();
    paths.sort();

    for path in &paths {
        let file = parse_item_file(path)?;
        for item in &file.items {
            let placement = classify(&item.equipment_slot, item.weapon.as_deref(), item.armor.as_deref())
                .with_context(|| format!("{}", path.display()))?;
            let Some(placement) = placement else {
                tx.execute(
                    "INSERT OR REPLACE INTO excluded_items (name, reason) VALUES (?1, ?2)",
                    params![item.name.trim(), "cosmetic-only slots"],
                )?;
                report.items_skipped_cosmetic += 1;
                continue;
            };
            ctx.write_item(item, &placement, &mut report).with_context(|| format!("{}", path.display()))?;
            report.items_written += 1;
        }
    }

    report.bonuses = ctx.caches.bonuses.len();
    report.effects = ctx.caches.effects.len();
    report.augment_slot_types = ctx.caches.slot_types.len();
    tx.commit()?;
    Ok(report)
}

/// Quests by name, longest first, so `DropLocation` matching prefers the most specific name.
struct QuestIndex {
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
struct Caches {
    materials: HashMap<String, i64>,
    slot_types: HashMap<String, i64>,
    bonuses: HashMap<BonusKey, i64>,
    effects: HashMap<String, i64>,
}

struct Ctx<'a> {
    tx: &'a Transaction<'a>,
    buff_map: &'a BuffMap,
    templates: &'a HashMap<String, String>,
    quests: &'a QuestIndex,
    caches: Caches,
}

impl Ctx<'_> {
    fn write_item(&mut self, item: &Item, placement: &Placement, report: &mut BuildReport) -> Result<()> {
        let mut enhancement_bonus: Option<i64> = None;
        let mut resolved = Vec::with_capacity(item.buffs.len());
        for buff in &item.buffs {
            match self.buff_map.resolve(buff)? {
                Resolved::Enhancement(v) => enhancement_bonus = Some(v),
                other => resolved.push((buff, other)),
            }
        }

        let material_id = match material::normalize(item.material.as_deref()) {
            Some(name) => Some(self.material_id(&name)?),
            None => None,
        };
        let name = item.name.trim();
        self.tx.execute(
            "INSERT INTO items (name, slot_id, item_category, item_type, minimum_level, enhancement_bonus, material_id, race_required,
                                icon, description, drop_location, set_bonus, accepts_sentience, is_minor_artifact, wiki_url)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                name,
                placement.slot.id(),
                placement.category.as_str(),
                placement.item_type,
                item.min_level,
                enhancement_bonus,
                material_id,
                item.race_required(),
                item.icon.as_deref().map(str::trim),
                item.description.as_deref().map(str::trim).filter(|s| !s.is_empty()),
                item.drop_location.as_deref().map(str::trim).filter(|s| !s.is_empty()),
                item.set_bonus.first().map(|s| s.trim()),
                item.accepts_sentience,
                item.minor_artifact,
                wiki_url(name),
            ],
        )?;
        let item_id = self.tx.last_insert_rowid();

        if let Some(wt) = placement.weapon_type() {
            self.write_weapon_stats(item_id, item, placement, wt.id, enhancement_bonus)?;
            if wt.is_shield {
                self.write_armor_stats(item_id, item, ArmorType::Shield)?;
            }
        }
        if let Some(armor) = item.armor.as_deref() {
            let Some(armor_type) = ArmorType::parse(armor.trim()) else {
                anyhow::bail!("unknown <Armor> type {armor:?}");
            };
            self.write_armor_stats(item_id, item, armor_type)?;
        }

        for (sort_order, (buff, res)) in resolved.into_iter().enumerate() {
            let template = self.templates.get(buff.kind.trim()).map(String::as_str).unwrap_or("");
            match res {
                Resolved::Bonus { stat, bonus_type, value, value2 } => {
                    let description =
                        if template.is_empty() { None } else { Some(self.buff_map.describe(template, buff)) };
                    let key = (stat.id, bonus_type.map(|b| b.id()), value, value2);
                    let bonus_id = match self.caches.bonuses.get(&key) {
                        Some(id) => *id,
                        None => {
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
                            id
                        }
                    };
                    self.tx.execute(
                        "INSERT INTO item_bonuses (item_id, bonus_id, sort_order) VALUES (?1, ?2, ?3)",
                        params![item_id, bonus_id, sort_order as i64],
                    )?;
                }
                Resolved::Effect { name: effect_name, value, target } => {
                    let effect_id = match self.caches.effects.get(&effect_name) {
                        Some(id) => *id,
                        None => {
                            let description = if template.is_empty() { None } else { Some(template) };
                            self.tx.execute(
                                "INSERT INTO effects (name, description) VALUES (?1, ?2)",
                                params![effect_name, description],
                            )?;
                            let id = self.tx.last_insert_rowid();
                            self.caches.effects.insert(effect_name.clone(), id);
                            id
                        }
                    };
                    self.tx.execute(
                        "INSERT INTO item_effects (item_id, effect_id, sort_order, value, target) VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![item_id, effect_id, sort_order as i64, value, target],
                    )?;
                }
                Resolved::Enhancement(_) => unreachable!("filtered above"),
            }
        }

        for (sort_order, aug) in item.augments.iter().enumerate() {
            let slot_id = self.slot_type_id(&aug.kind)?;
            self.tx.execute(
                "INSERT INTO item_augment_slots (item_id, sort_order, slot_id) VALUES (?1, ?2, ?3)",
                params![item_id, sort_order as i64, slot_id],
            )?;
            for (option_order, opt) in aug.options.iter().enumerate() {
                self.tx.execute(
                    "INSERT INTO item_augment_slot_options (item_id, slot_order, option_order, name, description, min_level)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        item_id,
                        sort_order as i64,
                        option_order as i64,
                        opt.name.trim(),
                        Some(opt.description.trim()).filter(|s| !s.is_empty()),
                        opt.min_level
                    ],
                )?;
            }
        }

        if let Some(drop) = item.drop_location.as_deref() {
            report.quest_loot_links += self.link_quests(item_id, drop)?;
        }
        Ok(())
    }

    fn write_weapon_stats(
        &self,
        item_id: i64,
        item: &Item,
        placement: &Placement,
        weapon_type_id: i64,
        enhancement: Option<i64>,
    ) -> Result<()> {
        let dice = item.base_dice.as_ref();
        let damage = render_damage(
            item.weapon_damage,
            dice.and_then(|d| Some((d.number?, d.sides?, d.bonus))),
            enhancement,
            &item.dr_bypass,
        );
        let critical = match (item.critical_threat_range, item.critical_multiplier) {
            (Some(range), Some(mult)) if range <= 1 => Some(format!("20 / x{mult}")),
            (Some(range), Some(mult)) => Some(format!("{}-20 / x{mult}", 21 - range)),
            _ => None,
        };
        self.tx.execute(
            "INSERT INTO item_weapon_stats (item_id, weapon_type_id, base_dice_count, base_dice_sides, base_dice_bonus, damage_multiplier,
                                            critical_threat_range, critical_multiplier, attack_modifier, damage_modifier, handedness, damage, critical)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                item_id,
                weapon_type_id,
                dice.and_then(|d| d.number),
                dice.and_then(|d| d.sides),
                dice.and_then(|d| d.bonus),
                item.weapon_damage,
                item.critical_threat_range,
                item.critical_multiplier,
                join_nonempty(&item.attack_modifier),
                join_nonempty(&item.damage_modifier),
                placement.handedness.map(|h| h.as_str()),
                damage,
                critical,
            ],
        )?;
        for bypass in &item.dr_bypass {
            let bypass = bypass.trim();
            if bypass.is_empty() || bypass == "-" {
                continue;
            }
            self.tx.execute(
                "INSERT OR IGNORE INTO item_dr_bypass (item_id, bypass) VALUES (?1, ?2)",
                params![item_id, bypass],
            )?;
        }
        Ok(())
    }

    fn write_armor_stats(&self, item_id: i64, item: &Item, armor_type: ArmorType) -> Result<()> {
        self.tx.execute(
            "INSERT OR REPLACE INTO item_armor_stats (item_id, armor_type, armor_bonus, max_dex_bonus, arcane_spell_failure, armor_check_penalty,
                                                      shield_bonus, damage_reduction, mithral_body, adamantine_body)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                item_id,
                armor_type.as_str(),
                item.armor_bonus,
                item.maximum_dexterity_bonus,
                item.arcane_spell_failure,
                item.armor_check_penalty,
                item.shield_bonus,
                item.damage_reduction,
                item.mithral_body,
                item.adamantine_body,
            ],
        )?;
        Ok(())
    }

    /// Link the item to every quest named in its drop text. Longer names are matched first and
    /// blanked out so a shorter quest name nested inside them does not also match.
    fn link_quests(&self, item_id: i64, drop: &str) -> Result<usize> {
        let mut text = drop.to_string();
        let lower = drop.to_lowercase();
        let mut links = 0;
        for (name, quest_id, is_raid) in &self.quests.by_length {
            if name.is_empty() || !text.contains(name.as_str()) {
                continue;
            }
            text = text.replace(name.as_str(), &" ".repeat(name.len()));
            let loot_type = if *is_raid {
                LootType::Raid
            } else if lower.contains("reward") {
                LootType::Reward
            } else {
                LootType::Chest
            };
            self.tx.execute(
                "INSERT OR IGNORE INTO quest_loot (quest_id, item_id, loot_type) VALUES (?1, ?2, ?3)",
                params![quest_id, item_id, loot_type.as_str()],
            )?;
            links += 1;
        }
        Ok(links)
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
}

fn join_nonempty(parts: &[String]) -> Option<String> {
    let joined: Vec<&str> = parts.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    if joined.is_empty() {
        None
    } else {
        Some(joined.join(", "))
    }
}

/// `3.6[1d10] + 7 Good, Magic, Pierce, Slash`, the legacy display form.
fn render_damage(
    multiplier: Option<f64>,
    dice: Option<(i64, i64, Option<i64>)>,
    enhancement: Option<i64>,
    bypass: &[String],
) -> Option<String> {
    let (n, s, b) = dice?;
    let mut out = String::new();
    if let Some(m) = multiplier {
        out.push_str(&m.to_string());
    }
    out.push_str(&format!("[{n}d{s}"));
    if let Some(b) = b.filter(|b| *b != 0) {
        out.push_str(&format!("{b:+}"));
    }
    out.push(']');
    if let Some(e) = enhancement {
        out.push_str(&format!(" + {e}"));
    }
    let bypass: Vec<&str> = bypass.iter().map(|s| s.trim()).filter(|s| !s.is_empty() && *s != "-").collect();
    if !bypass.is_empty() {
        out.push(' ');
        out.push_str(&bypass.join(", "));
    }
    Some(out)
}

fn wiki_url(name: &str) -> String {
    format!("https://ddowiki.com/page/Item:{}", name.replace(' ', "_").replace('+', "%2B"))
}
