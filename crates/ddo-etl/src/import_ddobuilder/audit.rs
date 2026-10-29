use super::ImportReportEntry;
use crate::map::buff::{BuffResolver, FamilyResolution};
use crate::map::effect_map::{EffectTargetQualifiers, EFFECT_MAP};
use crate::xml::item_buffs::{self, ItemBuffDefinition};
use crate::xml::items::parse_item_file;
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

const MAPPING_PATH: &str = "crates/ddo-etl/data/effect_map.toml";

pub(super) fn is_stat_home(entry: &ImportReportEntry) -> bool {
    matches!(entry.category, "NEW STAT HOME" | "STAT HOME MAJORITY")
}

pub(super) fn stat_homes(
    ours: &rusqlite::Connection,
    upstream: &rusqlite::Connection,
) -> Result<Vec<ImportReportEntry>> {
    let known_stats = crate::groupings::Groupings::from_database(ours)?;
    let majority_types = crate::build::effect_vocabulary::item_home_bonus_types(upstream)?;
    let bonus_types = upstream
        .prepare("SELECT id, name FROM bonus_types")?
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
    let paths = crate::tree::DataFilePaths::from_database(upstream)?;
    let mut statement =
        upstream.prepare("SELECT id, name, home_bonus_type_id FROM effects WHERE is_stat ORDER BY name")?;
    let mut entries = Vec::new();
    for row in statement
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<i64>>(2)?)))?
    {
        let (id, name, home) = row?;
        let declared = home.and_then(|id| bonus_types.get(&id)).map_or("none", String::as_str);
        let (category, detail) = if known_stats.effect_named(&name).is_none() {
            ("NEW STAT HOME", format!("{name}: inferred {declared} in upstream; existing deleted files stay deleted"))
        } else if let Some(majority) = majority_types.get(&id).filter(|majority| Some(**majority) != home) {
            ("STAT HOME MAJORITY", format!("{name}: item majority {}; declared {declared} kept", bonus_types[majority]))
        } else {
            continue;
        };
        entries.push(ImportReportEntry { path: paths.path("effects", id)?, category, detail });
    }
    Ok(entries)
}

pub(super) fn unmapped_damage_targets(db: &rusqlite::Connection) -> Result<Vec<ImportReportEntry>> {
    let mut statement = db.prepare(
        "SELECT effect_type, damage, COUNT(*) FROM (
            SELECT effect_type, damage FROM modifiers WHERE damage IS NOT NULL
            UNION ALL
            SELECT extra.value, m.damage FROM modifiers m, json_each(m.extra_types) extra WHERE m.damage IS NOT NULL
         ) GROUP BY effect_type, damage ORDER BY effect_type, damage",
    )?;
    let rows = statement
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?)))?;
    let qualifiers = EffectTargetQualifiers::default();
    let mut entries = Vec::new();
    for row in rows {
        let (effect_type, damage, count) = row?;
        if EFFECT_MAP.damage_as_target.contains(&effect_type)
            && EFFECT_MAP.damage_target_stats(&effect_type, &damage, &qualifiers).is_none()
        {
            entries.push(ImportReportEntry {
                path: MAPPING_PATH.into(),
                category: "UNMAPPED DAMAGE TARGET",
                detail: format!(
                    "{effect_type}: Damage {damage:?} maps to no stat on {count} modifiers; original targets kept"
                ),
            });
        }
    }
    Ok(entries)
}

pub(super) fn unmatched_set_tier_prose(
    previous: &BTreeSet<crate::build::SetTierProseMatch>,
    current: &BTreeSet<crate::build::SetTierProseMatch>,
) -> Vec<ImportReportEntry> {
    previous
        .difference(current)
        .map(|matched| ImportReportEntry {
            path: MAPPING_PATH.into(),
            category: "UNMATCHED SET TIER PROSE BONUS",
            detail: format!(
                "{:?} at {} pieces no longer matches {:?}",
                matched.set_name, matched.equipped_count, matched.description
            ),
        })
        .collect()
}

pub(super) fn entries(source: &Path) -> Result<Vec<ImportReportEntry>> {
    let definitions = item_buffs::parse(&source.join("ItemBuffs.xml"))?;
    let mut entries = unclassified_buff_types(source, &definitions)?;
    entries.extend(partially_mapped_targets(&definitions));
    entries.sort_by(|left, right| (left.category, &left.detail).cmp(&(right.category, &right.detail)));
    Ok(entries)
}

fn partially_mapped_targets(definitions: &HashMap<String, ItemBuffDefinition>) -> Vec<ImportReportEntry> {
    let resolver = BuffResolver::from_definitions(definitions);
    let mut entries = Vec::new();
    for (buff_kind, definition) in definitions {
        let target_count = definition
            .effects
            .iter()
            .map(|effect| effect.targets.iter().collect::<BTreeSet<_>>().len())
            .max()
            .unwrap_or_default();
        if target_count < 2 {
            continue;
        }
        let resolution = match resolver.family_resolution(buff_kind) {
            Ok(resolution) => resolution,
            Err(error) => {
                entries.push(ImportReportEntry {
                    path: MAPPING_PATH.into(),
                    category: "MAPPING AUDIT ERROR",
                    detail: format!("{buff_kind}: {error:#}"),
                });
                continue;
            }
        };
        let mapped_count = match resolution {
            FamilyResolution::MappedGroup { group_name, .. } => {
                EFFECT_MAP.group_members(&group_name).map_or(0, <[String]>::len)
            }
            FamilyResolution::Mapped { .. } => 1,
            FamilyResolution::EffectFallback(rows) => rows.len(),
            _ => 0,
        };
        if mapped_count < target_count {
            entries.push(ImportReportEntry {
                path: MAPPING_PATH.into(),
                category: "PARTIALLY MAPPED EFFECT",
                detail: format!("{buff_kind}: {target_count} source targets, {mapped_count} mapped stats"),
            });
        }
    }
    entries
}

fn unclassified_buff_types(
    source: &Path,
    definitions: &HashMap<String, ItemBuffDefinition>,
) -> Result<Vec<ImportReportEntry>> {
    let mut families_by_type: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut items_by_type: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut types_by_family: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (family, definition) in definitions {
        for effect_type in definition.effects.iter().flat_map(|effect| &effect.types) {
            if !EFFECT_MAP.is_classified_effect_type(effect_type) {
                families_by_type.entry(effect_type.clone()).or_default().insert(family.clone());
                types_by_family.entry(family.clone()).or_default().insert(effect_type.clone());
            }
        }
    }
    let items_dir = source.join("Items");
    if items_dir.is_dir() {
        for entry in walkdir::WalkDir::new(items_dir).into_iter().filter_map(Result::ok) {
            if !entry.file_type().is_file() || entry.path().extension().is_none_or(|extension| extension != "item") {
                continue;
            }
            for item in parse_item_file(entry.path())?.items {
                for buff in &item.buffs {
                    if let Some(effect_types) = types_by_family.get(buff.kind.trim()) {
                        for effect_type in effect_types {
                            items_by_type.entry(effect_type.clone()).or_default().insert(item.name.clone());
                        }
                    }
                }
            }
        }
    }
    Ok(families_by_type
        .into_iter()
        .map(|(effect_type, families)| {
            let item_count = items_by_type.get(&effect_type).map_or(0, BTreeSet::len);
            let family_word = if families.len() == 1 { "family" } else { "families" };
            let item_word = if item_count == 1 { "item" } else { "items" };
            ImportReportEntry {
                path: MAPPING_PATH.into(),
                category: "UNCLASSIFIED BUFF TYPE",
                detail: format!(
                    "{effect_type}: {} {family_word}; {item_count} {item_word}; {}",
                    families.len(),
                    families.into_iter().collect::<Vec<_>>().join(", ")
                ),
            }
        })
        .collect())
}
