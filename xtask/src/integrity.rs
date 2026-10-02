use anyhow::{Context, Result};
use ddo_etl::corrections::Corrections;
use rusqlite::Connection;
use std::collections::{BTreeMap, HashSet};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Hard,
    Warn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Passed,
    Failed,
    Warned,
    Skipped,
}

pub struct IntegrityOptions {
    pub corrections: Corrections,
    pub allowed_empty_tables: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offender {
    pub name: String,
    pub id: Option<i64>,
    pub detail: String,
}

enum OffenderQuery {
    Sql(&'static str),
    Built(fn(&Connection, &IntegrityOptions) -> Result<Findings>),
}

#[derive(Default)]
struct Findings {
    offenders: Option<Vec<Offender>>,
    notes: Vec<String>,
}

pub struct IntegrityCheck {
    pub name: &'static str,
    pub severity: Severity,
    pub description: &'static str,
    offender_query: OffenderQuery,
    shown_offender_limit: usize,
    top_detail_limit: usize,
}

const SHOWN_OFFENDER_LIMIT: usize = 10;

const DROP_LOCATION_HEAD_SQL: &str = "COALESCE(NULLIF(TRIM(CASE WHEN instr(i.drop_location, ',') > 0 \
     THEN substr(i.drop_location, 1, instr(i.drop_location, ',') - 1) ELSE i.drop_location END), ''), \
     '(no drop location)')";

impl IntegrityCheck {
    const fn hard(name: &'static str, description: &'static str, offender_query: OffenderQuery) -> Self {
        Self {
            name,
            severity: Severity::Hard,
            description,
            offender_query,
            shown_offender_limit: SHOWN_OFFENDER_LIMIT,
            top_detail_limit: 0,
        }
    }

    const fn warn(name: &'static str, description: &'static str, offender_query: OffenderQuery) -> Self {
        Self { severity: Severity::Warn, ..Self::hard(name, description, offender_query) }
    }

    const fn showing_every_offender(self) -> Self {
        Self { shown_offender_limit: usize::MAX, ..self }
    }

    const fn ranking_top_details(self, top_detail_limit: usize) -> Self {
        Self { top_detail_limit, ..self }
    }
}

pub const INTEGRITY_CHECKS: &[IntegrityCheck] = &[
    IntegrityCheck::hard(
        "legacy_items_hidden",
        "every item flagged is_legacy has a reason: a (legacy) or (historic) name, an is_legacy correction, or no \
         sources row (its only sources are retired)",
        OffenderQuery::Built(legacy_items_without_a_reason),
    ),
    IntegrityCheck::hard(
        "tables_not_empty",
        "every table has rows, except those --allow-empty-table names",
        OffenderQuery::Built(empty_tables),
    ),
    IntegrityCheck::hard(
        "weapon_and_armor_stats_match_category",
        "weapons have weapon stats only, armor has armor stats only, shields have both (a shield bashes), and \
         jewelry and clothing have neither",
        OffenderQuery::Sql(
            "SELECT i.name, i.id, i.item_category \
             || CASE WHEN w.item_id IS NULL THEN ' without' ELSE ' with' END || ' weapon stats and' \
             || CASE WHEN a.item_id IS NULL THEN ' without' ELSE ' with' END || ' armor stats' \
             FROM items i LEFT JOIN item_weapon_stats w ON w.item_id = i.id \
             LEFT JOIN item_armor_stats a ON a.item_id = i.id \
             WHERE (w.item_id IS NOT NULL) <> (i.item_category IN ('Weapon', 'Shield')) \
             OR (a.item_id IS NOT NULL) <> (i.item_category IN ('Armor', 'Shield'))",
        ),
    ),
    IntegrityCheck::hard(
        "raid_loot_only_on_raids",
        "a quest drop with loot type raid comes from a quest that is a raid",
        OffenderQuery::Sql(
            "SELECT q.name, d.id, 'raid loot ' || COALESCE(i.name, a.name, '') || ' from a quest that is not a raid' \
             FROM sources d JOIN quests q ON q.id = d.quest_id LEFT JOIN items i ON i.id = d.item_id \
             LEFT JOIN augments a ON a.id = d.augment_id \
             WHERE d.loot_type = 'raid' AND q.is_raid = 0",
        ),
    ),
    IntegrityCheck::hard(
        "item_sockets_use_known_labels",
        "every item socket is a slot type of a known family (standard, dino, lamordia, slavers, upgrade, crafting) \
         with a variant, and a standard socket is one of the nine colours",
        OffenderQuery::Sql(
            "SELECT i.name, i.id, 'socket ' || COALESCE(t.label, s.slot_id) || ' of family ' \
             || COALESCE(t.family, 'none') FROM item_augment_slots s JOIN items i ON i.id = s.item_id \
             LEFT JOIN augment_slot_types t ON t.id = s.slot_id \
             WHERE t.id IS NULL \
             OR t.family NOT IN ('standard', 'dino', 'lamordia', 'slavers', 'upgrade', 'crafting') \
             OR TRIM(t.variant) = '' \
             OR (t.family = 'standard' AND t.variant NOT IN \
                 ('red', 'green', 'blue', 'orange', 'purple', 'colorless', 'yellow', 'sun', 'moon'))",
        ),
    ),
    IntegrityCheck::hard(
        "no_stale_corrections",
        "every correction in the corrections files (--corrections, default the embedded ones) was applied; one that \
         was not has Maetrim's value changed under it or its rename done upstream, and cargo xtask wiki-check \
         says which",
        OffenderQuery::Built(unapplied_corrections),
    ),
    IntegrityCheck::hard(
        "trees_have_enhancements",
        "every enhancement tree has at least one enhancement",
        OffenderQuery::Sql(
            "SELECT t.name, t.id, 'no enhancements' FROM enhancement_trees t \
             WHERE NOT EXISTS (SELECT 1 FROM enhancements e WHERE e.tree_id = t.id)",
        ),
    ),
    IntegrityCheck::hard(
        "classes_have_full_progression",
        "every class has BAB and spell points for each of its 21 level entries (0 to 20); every heroic class has hit \
         points and skill points per level, a good or poor progression for each save, and class skills. Unknown, \
         Maetrim's placeholder for a level whose class is not chosen yet, is exempt",
        OffenderQuery::Sql(
            "SELECT name, id, CASE \
             WHEN json_array_length(COALESCE(bab, '[]')) <> 21 \
             THEN 'BAB has ' || json_array_length(COALESCE(bab, '[]')) || ' entries, not 21' \
             WHEN json_array_length(COALESCE(spell_points_per_level, '[]')) <> 21 \
             THEN 'spell points have ' || json_array_length(COALESCE(spell_points_per_level, '[]')) \
                  || ' entries, not 21' \
             WHEN COALESCE(hit_points, 0) <= 0 THEN 'no hit points per level' \
             WHEN COALESCE(skill_points, 0) <= 0 THEN 'no skill points per level' \
             WHEN NOT (COALESCE(fortitude, '') IN ('good', 'poor') AND COALESCE(reflex, '') IN ('good', 'poor') \
                       AND COALESCE(will, '') IN ('good', 'poor')) THEN 'a save with no progression' \
             ELSE 'no class skills' END \
             FROM classes c \
             WHERE json_array_length(COALESCE(bab, '[]')) <> 21 \
             OR json_array_length(COALESCE(spell_points_per_level, '[]')) <> 21 \
             OR (not_heroic = 0 AND name <> 'Unknown' AND ( \
                 COALESCE(hit_points, 0) <= 0 OR COALESCE(skill_points, 0) <= 0 \
                 OR NOT (COALESCE(fortitude, '') IN ('good', 'poor') AND COALESCE(reflex, '') IN ('good', 'poor') \
                         AND COALESCE(will, '') IN ('good', 'poor')) \
                 OR NOT EXISTS (SELECT 1 FROM class_skills s WHERE s.class_id = c.id)))",
        ),
    ),
    IntegrityCheck::warn(
        "items_without_a_source",
        "items (other than legacy ones) with no sources row. Every kind of source now has a table, but some remain \
         unmodelled: vendors and events no wiki file records yet (wiki-batch's vendor_names.txt and \
         event_names.txt), DDO Store purchases, wilderness and explorer areas that are no quest, and items with \
         no drop text. Becomes HARD once those are read or modelled; the drop_location heads below are the work list",
        OffenderQuery::Built(items_without_a_source),
    )
    .ranking_top_details(15),
    IntegrityCheck::warn(
        "effects_named_after_stats",
        "effects whose name equals a stat's ignoring case and spaces: buffs the buff map should turn into bonuses on \
         that stat",
        OffenderQuery::Sql(
            "SELECT e.name, e.id, 'named like the stat ' || s.name FROM effects e \
             JOIN stats s ON lower(replace(e.name, ' ', '')) = lower(replace(s.name, ' ', '')) ORDER BY e.name",
        ),
    )
    .showing_every_offender(),
    IntegrityCheck::warn(
        "items_have_minimum_level",
        "every item has a minimum level of 1 or more. WARN until the follow-up: the Cannith Crafted blanks take their \
         level from the crafting step, and Quiver of Alacrity has MinLevel 0 in Maetrim's file",
        OffenderQuery::Sql(
            "SELECT name, id, 'minimum level ' || COALESCE(minimum_level, 'null') FROM items \
             WHERE minimum_level IS NULL OR minimum_level < 1 ORDER BY name",
        ),
    )
    .showing_every_offender(),
    IntegrityCheck::warn(
        "augments_have_slot_and_family",
        "every augment has a family and fits at least one socket. WARN until the follow-up: Insightful Spell Focus \
         Mastery has no <Type> in Maetrim's file (a correction candidate), and No Augment is his empty-socket \
         placeholder",
        OffenderQuery::Sql(
            "SELECT name, id, CASE WHEN TRIM(family) = '' THEN 'blank family' \
             ELSE family || ' augment fits no socket' END FROM augments a \
             WHERE TRIM(family) = '' OR NOT EXISTS (SELECT 1 FROM augment_slots s WHERE s.augment_id = a.id)",
        ),
    )
    .showing_every_offender(),
    IntegrityCheck::hard(
        "chains_and_sagas_have_quests",
        "every quest chain and saga links at least two quests",
        OffenderQuery::Sql(
            "SELECT c.name, c.id, 'quest chain with ' || COUNT(q.quest_id) || ' quest(s)' FROM quest_chains c \
             LEFT JOIN quest_chain_quests q ON q.chain_id = c.id GROUP BY c.id HAVING COUNT(q.quest_id) < 2 \
             UNION ALL \
             SELECT s.name, s.id, 'saga with ' || COUNT(q.quest_id) || ' quest(s)' FROM sagas s \
             LEFT JOIN saga_quests q ON q.saga_id = s.id GROUP BY s.id HAVING COUNT(q.quest_id) < 2",
        ),
    )
    .showing_every_offender(),
    IntegrityCheck::warn(
        "items_without_enchantments",
        "items that carry nothing beyond their base weapon or armor stats: no bonus, effect, enhancement bonus, \
         modifier, clicky, augment slot or set membership. Maetrim's files give these items nothing more, so each is \
         either genuinely bare (starter and event gear, ritual components) or a correction candidate whose wiki page \
         lists enchantments; the top categories are listed",
        OffenderQuery::Sql(
            "SELECT i.name, i.id, i.item_category FROM items i \
             WHERE i.enhancement_bonus IS NULL \
             AND NOT EXISTS (SELECT 1 FROM item_bonuses b WHERE b.item_id = i.id) \
             AND NOT EXISTS (SELECT 1 FROM item_effects e WHERE e.item_id = i.id) \
             AND NOT EXISTS (SELECT 1 FROM modifiers m WHERE m.source_kind = 'item' AND m.source_id = i.id) \
             AND NOT EXISTS (SELECT 1 FROM item_clickies c WHERE c.item_id = i.id) \
             AND NOT EXISTS (SELECT 1 FROM item_augment_slots s WHERE s.item_id = i.id) \
             AND NOT EXISTS (SELECT 1 FROM set_bonus_items sbi WHERE sbi.item_id = i.id) ORDER BY i.name",
        ),
    )
    .ranking_top_details(5),
    IntegrityCheck::warn(
        "items_without_description",
        "items with a blank description; the blank_descriptions.txt list from cargo xtask wiki-batch is the work list",
        OffenderQuery::Sql(
            "SELECT name, id, item_category FROM items WHERE description IS NULL OR TRIM(description) = '' \
             ORDER BY name",
        ),
    )
    .ranking_top_details(5),
    IntegrityCheck::warn(
        "near_duplicate_item_names",
        "items whose names differ only by case, punctuation or spaces",
        OffenderQuery::Built(near_duplicate_item_names),
    )
    .showing_every_offender(),
    IntegrityCheck::warn(
        "raids_without_raid_loot",
        "raids with no sources row of loot type raid",
        OffenderQuery::Sql(
            "SELECT q.name, q.id, 'raid without raid loot' FROM quests q WHERE q.is_raid = 1 \
             AND NOT EXISTS (SELECT 1 FROM sources d WHERE d.quest_id = q.id AND d.loot_type = 'raid') ORDER BY q.name",
        ),
    )
    .showing_every_offender(),
    IntegrityCheck::warn(
        "quests_without_loot",
        "quests (not challenges) with no sources row; the database does not mark wilderness areas, so any among them \
         are listed too. The top packs are listed",
        OffenderQuery::Sql(
            "SELECT q.name, q.id, COALESCE(p.name, '(no pack)') FROM quests q \
             LEFT JOIN adventure_packs p ON p.id = q.pack_id WHERE q.is_challenge = 0 \
             AND NOT EXISTS (SELECT 1 FROM sources d WHERE d.quest_id = q.id) ORDER BY q.name",
        ),
    )
    .ranking_top_details(10),
    IntegrityCheck::warn(
        "unreferenced_effects",
        "effects no row references",
        OffenderQuery::Built(unreferenced_effects),
    )
    .showing_every_offender(),
    IntegrityCheck::warn(
        "unreferenced_bonuses",
        "bonuses no row references",
        OffenderQuery::Built(unreferenced_bonuses),
    )
    .showing_every_offender(),
    IntegrityCheck::warn(
        "unreferenced_stats",
        "stats that no bonus an item, augment, feat or set tier carries is on. The guard against a duplicate or \
         non-stat name in the seed is the every_seed_stat_has_a_source_or_is_kept_without_one test in \
         crates/ddo-etl/tests/stats_seed.rs, which fails on any seed stat no buff or effect map entry, fixture \
         bonus, wiki or correction bonus reaches unless its STATS_KEPT_WITHOUT_A_SOURCE list names it with a \
         reason. A full upstream build lists ten: that list's eight, and Ki and Maximum Caster Level, which \
         effect_map.toml targets but no effect of his carries as a plain number",
        OffenderQuery::Built(unreferenced_stats),
    )
    .showing_every_offender(),
    IntegrityCheck::warn(
        "slot_types_no_augment_fits",
        "socket types no augment fits, with how many item sockets carry each. A socket an augment slot option \
         grants, or one holding the options an item upgrades through (item_augment_slot_options), is filled by \
         those options and is not listed. Three stay on a full upstream build: Cannith Weapon Extra on the crafting \
         tutorial's Fire Touch Heavy Mace, which no Cannith augment in Maetrim's files fits, and Random Effect 1 and \
         2 on +5 Engraved Cormyrian Leather Armor, whose random enchantments his files do not model",
        OffenderQuery::Sql(
            "SELECT t.label, t.id, (SELECT COUNT(*) FROM item_augment_slots s WHERE s.slot_id = t.id) \
             || ' item socket(s)' FROM augment_slot_types t \
             WHERE NOT EXISTS (SELECT 1 FROM augment_slots a WHERE a.slot_id = t.id) \
             AND NOT EXISTS (SELECT 1 FROM item_augment_slot_option_grants g WHERE g.slot_id = t.id) \
             AND NOT EXISTS (SELECT 1 FROM item_augment_slots s JOIN item_augment_slot_options o \
             ON o.item_id = s.item_id AND o.slot_order = s.sort_order WHERE s.slot_id = t.id) ORDER BY t.label",
        ),
    )
    .showing_every_offender(),
    IntegrityCheck::warn(
        "sets_without_members",
        "set bonuses no item, augment, filigree or item augment slot option belongs to. Two stay on a full upstream \
         build: Anthem Melody, which no item names in Maetrim's files or on ddowiki (his dead data), and Magewright's \
         Expertise, which the Nearly Finished upgrade of Magewright's Cloak grants on the wiki while his option names \
         no set and no correction kind adds one to an option",
        OffenderQuery::Sql(
            "SELECT s.name, s.id, CASE WHEN s.is_filigree_set = 1 THEN 'filigree set' ELSE 'set' END \
             FROM set_bonuses s \
             WHERE NOT EXISTS (SELECT 1 FROM set_bonus_items i WHERE i.set_id = s.id) \
             AND NOT EXISTS (SELECT 1 FROM set_bonus_augments a WHERE a.set_id = s.id) \
             AND NOT EXISTS (SELECT 1 FROM item_augment_slot_option_sets o WHERE o.set_id = s.id) \
             AND NOT EXISTS (SELECT 1 FROM filigrees f WHERE f.set_id = s.id) ORDER BY s.name",
        ),
    )
    .showing_every_offender(),
];

fn has_column(db: &Connection, table: &str, column: &str) -> Result<bool> {
    let column_count: i64 =
        db.query_row("SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2", [table, column], |row| row.get(0))?;
    Ok(column_count > 0)
}

fn offenders_from_sql(db: &Connection, sql: &str) -> Result<Vec<Offender>> {
    let mut statement = db.prepare(sql).with_context(|| format!("preparing {sql}"))?;
    let offenders = statement
        .query_map([], |row| {
            Ok(Offender {
                name: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                id: row.get(1)?,
                detail: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(offenders)
}

fn legacy_items_without_a_reason(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    if !has_column(db, "items", "is_legacy")? {
        return Ok(Findings { offenders: None, notes: vec!["items.is_legacy is absent".to_string()] });
    }
    let offenders = offenders_from_sql(
        db,
        "SELECT i.name, i.id, 'is_legacy, yet it has a source, its name says neither (legacy) nor (historic), \
         and no correction sets is_legacy' FROM items i \
         WHERE i.is_legacy = 1 \
         AND lower(i.name) NOT LIKE '%(legacy)%' AND lower(i.name) NOT LIKE '%(historic)%' \
         AND NOT EXISTS (SELECT 1 FROM corrections c WHERE c.kind = 'item' AND c.name = i.name \
                         AND c.field = 'is_legacy') \
         AND EXISTS (SELECT 1 FROM sources d WHERE d.item_id = i.id)",
    )?;
    Ok(Findings { offenders: Some(offenders), notes: Vec::new() })
}

fn items_without_a_source(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let has_is_legacy = has_column(db, "items", "is_legacy")?;
    let legacy_filter = if has_is_legacy { "AND i.is_legacy = 0" } else { "" };
    let offenders = offenders_from_sql(
        db,
        &format!(
            "SELECT i.name, i.id, {DROP_LOCATION_HEAD_SQL} FROM items i \
             WHERE NOT EXISTS (SELECT 1 FROM sources d WHERE d.item_id = i.id) {legacy_filter} ORDER BY i.name"
        ),
    )?;
    let notes = if has_is_legacy { Vec::new() } else { vec!["items.is_legacy is absent; no item is excluded".into()] };
    Ok(Findings { offenders: Some(offenders), notes })
}

fn empty_tables(db: &Connection, options: &IntegrityOptions) -> Result<Findings> {
    let mut statement =
        db.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")?;
    let table_names = statement.query_map([], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut offenders = Vec::new();
    for table_name in table_names {
        let row_count: i64 = db.query_row(&format!("SELECT COUNT(*) FROM \"{table_name}\""), [], |row| row.get(0))?;
        if row_count == 0 && !options.allowed_empty_tables.contains(&table_name) {
            offenders.push(Offender { name: table_name, id: None, detail: "no rows".to_string() });
        }
    }
    let allowed_text = match options.allowed_empty_tables.as_slice() {
        [] => "none".to_string(),
        allowed_tables => allowed_tables.join(", "),
    };
    Ok(Findings { offenders: Some(offenders), notes: vec![format!("allowed empty: {allowed_text}")] })
}

fn unapplied_corrections(db: &Connection, options: &IntegrityOptions) -> Result<Findings> {
    let mut statement = db.prepare("SELECT kind, name, qualifier, field FROM corrections")?;
    let applied_keys = statement
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))?
        .collect::<rusqlite::Result<HashSet<(String, String, String, String)>>>()?;
    let offenders = options
        .corrections
        .entries
        .iter()
        .filter(|correction| {
            let key = (
                correction.kind.as_str().to_string(),
                correction.name.clone(),
                correction.qualifier(),
                correction.field.clone(),
            );
            !applied_keys.contains(&key)
        })
        .map(|correction| Offender {
            name: correction.label(),
            id: None,
            detail: format!("in {} but not applied", correction.file_name),
        })
        .collect();
    Ok(Findings { offenders: Some(offenders), notes: Vec::new() })
}

fn folded_item_name(item_name: &str) -> String {
    item_name.chars().filter(|character| character.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

fn near_duplicate_item_names(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let mut statement = db.prepare("SELECT name, id FROM items ORDER BY name")?;
    let items = statement
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut items_by_folded_name: BTreeMap<String, Vec<(String, i64)>> = BTreeMap::new();
    for (item_name, item_id) in items {
        items_by_folded_name.entry(folded_item_name(&item_name)).or_default().push((item_name, item_id));
    }
    let mut offenders = Vec::new();
    for same_named_items in items_by_folded_name.values().filter(|same_named_items| same_named_items.len() > 1) {
        for (item_name, item_id) in same_named_items {
            let other_names: Vec<String> = same_named_items
                .iter()
                .filter(|(other_name, _)| other_name != item_name)
                .map(|(other_name, _)| format!("{other_name:?}"))
                .collect();
            offenders.push(Offender {
                name: item_name.clone(),
                id: Some(*item_id),
                detail: format!("same as {}", other_names.join(", ")),
            });
        }
    }
    Ok(Findings { offenders: Some(offenders), notes: Vec::new() })
}

fn referencing_columns(db: &Connection, referenced_table: &str) -> Result<Vec<(String, String)>> {
    let mut statement = db.prepare(
        "SELECT m.name, f.\"from\" FROM sqlite_master m JOIN pragma_foreign_key_list(m.name) f \
         WHERE m.type = 'table' AND f.\"table\" = ?1 ORDER BY m.name",
    )?;
    let columns = statement
        .query_map([referenced_table], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(columns)
}

fn referenced_condition(referencing_columns: &[(String, String)], referenced_id: &str) -> String {
    let exists_clauses: Vec<String> = referencing_columns
        .iter()
        .map(|(table, column)| format!("EXISTS (SELECT 1 FROM \"{table}\" r WHERE r.\"{column}\" = {referenced_id})"))
        .collect();
    match exists_clauses.as_slice() {
        [] => "0".to_string(),
        _ => exists_clauses.join(" OR "),
    }
}

fn referencing_columns_note(referencing_columns: &[(String, String)]) -> String {
    let column_names: Vec<String> =
        referencing_columns.iter().map(|(table, column)| format!("{table}.{column}")).collect();
    format!("referenced from {}", column_names.join(", "))
}

fn unreferenced_rows(db: &Connection, table: &str) -> Result<Findings> {
    let referencing_columns = referencing_columns(db, table)?;
    let offenders = offenders_from_sql(
        db,
        &format!(
            "SELECT t.name, t.id, 'no row references it' FROM {table} t WHERE NOT ({}) ORDER BY t.name",
            referenced_condition(&referencing_columns, "t.id")
        ),
    )?;
    Ok(Findings { offenders: Some(offenders), notes: vec![referencing_columns_note(&referencing_columns)] })
}

fn unreferenced_effects(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    unreferenced_rows(db, "effects")
}

fn unreferenced_bonuses(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    unreferenced_rows(db, "bonuses")
}

fn unreferenced_stats(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let bonus_referencing_columns = referencing_columns(db, "bonuses")?;
    let offenders = offenders_from_sql(
        db,
        &format!(
            "SELECT s.name, s.id, s.category FROM stats s WHERE NOT EXISTS \
             (SELECT 1 FROM bonuses b WHERE b.stat_id = s.id AND ({})) ORDER BY s.name",
            referenced_condition(&bonus_referencing_columns, "b.id")
        ),
    )?;
    Ok(Findings { offenders: Some(offenders), notes: vec![referencing_columns_note(&bonus_referencing_columns)] })
}

fn findings_of(check: &IntegrityCheck, db: &Connection, options: &IntegrityOptions) -> Result<Findings> {
    match check.offender_query {
        OffenderQuery::Sql(sql) => Ok(Findings { offenders: Some(offenders_from_sql(db, sql)?), notes: Vec::new() }),
        OffenderQuery::Built(build_findings) => build_findings(db, options),
    }
}

fn top_details_of(offenders: &[Offender], limit: usize) -> Vec<(String, usize)> {
    let mut count_by_detail: Vec<(String, usize)> = Vec::new();
    for offender in offenders {
        match count_by_detail.iter_mut().find(|(detail, _)| *detail == offender.detail) {
            Some((_, count)) => *count += 1,
            None => count_by_detail.push((offender.detail.clone(), 1)),
        }
    }
    count_by_detail.sort_by(|(left_detail, left_count), (right_detail, right_count)| {
        right_count.cmp(left_count).then_with(|| left_detail.cmp(right_detail))
    });
    count_by_detail.truncate(limit);
    count_by_detail
}

pub struct CheckOutcome {
    pub name: &'static str,
    pub severity: Severity,
    pub description: &'static str,
    pub status: CheckStatus,
    pub offenders: Vec<Offender>,
    pub top_details: Vec<(String, usize)>,
    pub notes: Vec<String>,
    shown_offender_limit: usize,
}

fn outcome_of(check: &IntegrityCheck, db: &Connection, options: &IntegrityOptions) -> Result<CheckOutcome> {
    let findings = findings_of(check, db, options).with_context(|| format!("running check {}", check.name))?;
    let (status, offenders) = match findings.offenders {
        None => (CheckStatus::Skipped, Vec::new()),
        Some(offenders) if offenders.is_empty() => (CheckStatus::Passed, offenders),
        Some(offenders) => match check.severity {
            Severity::Hard => (CheckStatus::Failed, offenders),
            Severity::Warn => (CheckStatus::Warned, offenders),
        },
    };
    Ok(CheckOutcome {
        name: check.name,
        severity: check.severity,
        description: check.description,
        status,
        top_details: top_details_of(&offenders, check.top_detail_limit),
        offenders,
        notes: findings.notes,
        shown_offender_limit: check.shown_offender_limit,
    })
}

pub struct IntegrityReport {
    pub outcomes: Vec<CheckOutcome>,
}

impl IntegrityReport {
    pub fn outcome(&self, check_name: &str) -> Option<&CheckOutcome> {
        self.outcomes.iter().find(|outcome| outcome.name == check_name)
    }

    pub fn failed_hard_check_names(&self) -> Vec<&'static str> {
        self.outcomes
            .iter()
            .filter(|outcome| outcome.status == CheckStatus::Failed)
            .map(|outcome| outcome.name)
            .collect()
    }
}

fn offender_count_text(offender_count: usize) -> String {
    match offender_count {
        1 => "1 offender".to_string(),
        _ => format!("{offender_count} offenders"),
    }
}

impl fmt::Display for CheckOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status_text = match self.status {
            CheckStatus::Passed => "ok".to_string(),
            CheckStatus::Skipped => "skipped".to_string(),
            CheckStatus::Failed => format!("FAIL ({})", offender_count_text(self.offenders.len())),
            CheckStatus::Warned => format!("WARN ({})", offender_count_text(self.offenders.len())),
        };
        writeln!(formatter, "check {}: {status_text}", self.name)?;
        if !matches!(self.status, CheckStatus::Passed) {
            writeln!(formatter, "  {}", self.description)?;
        }
        for offender in self.offenders.iter().take(self.shown_offender_limit) {
            let id_text = offender.id.map(|id| format!(" #{id}")).unwrap_or_default();
            writeln!(formatter, "  {:?}{id_text}: {}", offender.name, offender.detail)?;
        }
        if self.offenders.len() > self.shown_offender_limit {
            writeln!(formatter, "  ... and {} more", self.offenders.len() - self.shown_offender_limit)?;
        }
        if !self.top_details.is_empty() {
            writeln!(formatter, "  top {}:", self.top_details.len())?;
            for (detail, count) in &self.top_details {
                writeln!(formatter, "  {count:>6}  {detail}")?;
            }
        }
        for note in &self.notes {
            writeln!(formatter, "  note: {note}")?;
        }
        Ok(())
    }
}

impl fmt::Display for IntegrityReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for outcome in &self.outcomes {
            write!(formatter, "{outcome}")?;
        }
        let warned_count = self.outcomes.iter().filter(|outcome| outcome.status == CheckStatus::Warned).count();
        match self.failed_hard_check_names().as_slice() {
            [] => write!(formatter, "integrity: every HARD check passed, {warned_count} WARN check(s) reported"),
            failed_names => write!(formatter, "integrity: HARD check(s) failed: {}", failed_names.join(", ")),
        }
    }
}

pub fn integrity_report(db: &Connection, options: &IntegrityOptions) -> Result<IntegrityReport> {
    let outcomes =
        INTEGRITY_CHECKS.iter().map(|check| outcome_of(check, db, options)).collect::<Result<Vec<CheckOutcome>>>()?;
    Ok(IntegrityReport { outcomes })
}
