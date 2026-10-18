use super::ImportReportEntry;
use crate::map::buff::{BuffResolver, FamilyResolution};
use crate::map::effect_map::EFFECT_MAP;
use crate::xml::item_buffs::{self, ItemBuffDefinition};
use crate::xml::items::parse_item_file;
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

const MAPPING_PATH: &str = "crates/ddo-etl/data/effect_map.toml";

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
