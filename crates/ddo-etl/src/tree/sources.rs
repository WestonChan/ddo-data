use crate::map::drop_location::{quest_name_spans, segment_head, segment_ranges};
use crate::map::source_alias::SourceAliases;
use anyhow::Result;
use anyhow::{bail, Context};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub(super) struct SourceRecord {
    pub(super) chest: Option<String>,
    pub(super) price: Vec<crate::vendor_prices::VendorPrice>,
    cost: Option<String>,
    pub(super) drop_text_segment: Option<String>,
    pub(super) is_rare: bool,
    pub(super) kind: String,
    pub(super) tier: Option<String>,
    pub(super) loot_type: Option<String>,
    pub(super) name: String,
}

pub(super) struct SourcesByOwner {
    pub(super) items: BTreeMap<i64, Vec<SourceRecord>>,
    pub(super) augments: BTreeMap<i64, Vec<SourceRecord>>,
}

pub(super) fn sources_by_owner(db: &Connection) -> Result<SourcesByOwner> {
    let mut prices = crate::vendor_prices::prices_by_offer(db)?;
    let mut sources = SourcesByOwner { items: BTreeMap::new(), augments: BTreeMap::new() };
    let mut select = db.prepare(
        "SELECT source.item_id, source.augment_id, source.kind,
                CASE source.kind
                    WHEN 'quest' THEN quest.name
                    WHEN 'adventure_pack' THEN pack.name
                    WHEN 'challenge' THEN pack.name
                    WHEN 'quest_chain' THEN chain.name
                    WHEN 'saga' THEN saga.name
                    WHEN 'crafting_system' THEN system.name
                    WHEN 'vendor' THEN vendor.name
                    WHEN 'event' THEN event.name
                    WHEN 'starter' THEN 'Advance to level ' || source.character_level
                END,
                source.loot_type, source.chest, source.is_rare, source.cost, source.drop_text_segment, offer.id, source.tier
         FROM sources source
         LEFT JOIN quests quest ON quest.id = source.quest_id
         LEFT JOIN adventure_packs pack ON pack.id = source.pack_id
         LEFT JOIN quest_chains chain ON chain.id = source.chain_id
         LEFT JOIN sagas saga ON saga.id = source.saga_id
         LEFT JOIN crafting_systems system ON system.id = source.crafting_system_id
         LEFT JOIN vendors vendor ON vendor.id = source.vendor_id
         LEFT JOIN events event ON event.id = source.event_id
         LEFT JOIN vendor_offers offer ON source.kind = 'vendor' AND offer.vendor_id = source.vendor_id AND offer.item_id = source.item_id",
    )?;
    for row in select.query_map([], |row| {
        Ok((
            row.get::<_, Option<i64>>(0)?,
            row.get::<_, Option<i64>>(1)?,
            SourceRecord {
                kind: row.get(2)?,
                tier: row.get(10)?,
                name: row.get(3)?,
                loot_type: row.get(4)?,
                chest: row.get(5)?,
                is_rare: row.get(6)?,
                cost: row.get(7)?,
                price: Vec::new(),
                drop_text_segment: row.get(8)?,
            },
            row.get::<_, Option<i64>>(9)?,
        ))
    })? {
        let (item_id, augment_id, mut source, offer_id) = row?;
        if let Some(offer_id) = offer_id {
            source.price = prices.remove(&offer_id).unwrap_or_default();
        }
        let rows = match (item_id, augment_id) {
            (Some(id), None) => sources.items.entry(id).or_default(),
            (None, Some(id)) => sources.augments.entry(id).or_default(),
            _ => bail!("source {:?} must belong to one item or augment", source.name),
        };
        rows.push(source);
    }
    for entries in sources.items.values_mut().chain(sources.augments.values_mut()) {
        entries.sort_by_key(|source| {
            (
                source.kind.clone(),
                source.tier.clone(),
                source.name.clone(),
                source.loot_type.clone().unwrap_or_default(),
                source.chest.clone().unwrap_or_default(),
                source.is_rare,
                source.cost.clone().unwrap_or_default(),
            )
        });
    }
    Ok(sources)
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct SourceProvenance {
    drop_text_segment: Option<String>,
    source_match: Option<String>,
    character_level: Option<i64>,
}

pub(super) struct SourceOwner<'a> {
    pub(super) drop_text: Option<&'a str>,
    pub(super) minimum_level: Option<i64>,
    pub(super) explicit_wiki_sources: bool,
}

pub(super) struct SourceInput<'a> {
    pub(super) kind: &'a str,
    pub(super) tier: Option<&'a str>,
    pub(super) name: &'a str,
    pub(super) loot_type: Option<&'a str>,
    pub(super) chest: Option<&'a str>,
    pub(super) price: &'a [crate::vendor_prices::VendorPrice],
    pub(super) is_rare: bool,
}

pub(super) fn augment_drop_text(description: &str) -> Option<&str> {
    let (offset, label) = ["Drop Location:", "Drops in:", "Drops in;", "Drops in U48 Quest:"]
        .into_iter()
        .filter_map(|label| description.rfind(label).map(|offset| (offset, label)))
        .max_by_key(|(offset, _)| *offset)?;
    Some(description[offset + label.len()..].trim_start())
}

fn source_name_span(segment: &str, source_name: &str) -> Option<std::ops::Range<usize>> {
    quest_name_spans(segment, source_name).into_iter().next().or_else(|| {
        source_name.strip_prefix("The ").or_else(|| source_name.strip_prefix("A ")).and_then(|without_article| {
            quest_name_spans(segment, &format!("Return to {without_article}")).into_iter().next()
        })
    })
}

fn chosen_segment<'a>(
    drop_text: &'a str,
    kind: &str,
    name: &str,
    alternate_name: Option<&str>,
    loot_type: Option<&str>,
    chest: Option<&str>,
) -> Option<&'a str> {
    let segments = segment_ranges(drop_text)
        .into_iter()
        .map(|range| &drop_text[range])
        .filter(|segment| !segment.trim().is_empty())
        .collect::<Vec<_>>();
    let matching = segments
        .iter()
        .copied()
        .filter(|segment| {
            source_name_span(segment, name).is_some()
                || alternate_name.is_some_and(|alternate| source_name_span(segment, alternate).is_some())
        })
        .collect::<Vec<_>>();
    if matching.len() == 1 {
        return Some(matching[0]);
    }
    if matching.len() > 1 {
        let filtered = matching
            .iter()
            .copied()
            .filter(|segment| {
                let lowercase = segment.to_ascii_lowercase();
                match kind {
                    "saga" => lowercase.contains("saga") || lowercase.contains("end reward"),
                    "quest_chain" => lowercase.contains("chain") || lowercase.contains("end reward"),
                    "adventure_pack" if loot_type == Some("chest") => {
                        lowercase.contains("chest") && !lowercase.contains("reward")
                    }
                    "adventure_pack" if loot_type == Some("reward") => lowercase.contains("reward"),
                    "adventure_pack" => !lowercase.contains("saga") && !lowercase.contains("chain"),
                    _ if loot_type == Some("reward") => lowercase.contains("reward"),
                    _ if chest.is_some() => chest.is_some_and(|label| lowercase.contains(&label.to_ascii_lowercase())),
                    _ => false,
                }
            })
            .collect::<Vec<_>>();
        if filtered.len() == 1 {
            return Some(filtered[0]);
        }
        return None;
    }
    let wrapped_segment = drop_text.split(';').find(|segment| {
        segment.contains('\n')
            && (source_name_span(segment, name).is_some()
                || alternate_name.is_some_and(|alternate| source_name_span(segment, alternate).is_some()))
    });
    wrapped_segment
}

fn matched_source_text(segment: &str, kind: &str, name: &str, alternate_name: Option<&str>) -> Option<String> {
    let head = segment_head(segment);
    if kind == "starter" {
        return (!head.is_empty()).then(|| head.to_string());
    }
    let alias_span = alternate_name
        .filter(|alternate| kind != "quest" && head.eq_ignore_ascii_case(alternate))
        .and_then(|alternate| source_name_span(segment, alternate));
    let Some(name_span) = alias_span
        .or_else(|| alternate_name.and_then(|alternate| source_name_span(segment, alternate)))
        .or_else(|| source_name_span(segment, name))
    else {
        return (!head.is_empty()).then(|| head.to_string());
    };
    let before_name = &segment[..name_span.start];
    if kind == "quest" && before_name.ends_with("Return to ") {
        return Some(segment[name_span.start - "Return to ".len()..name_span.end].to_string());
    }
    Some(segment[name_span].to_string())
}

fn source_alias<'a>(
    kind: &str,
    name: &str,
    drop_text: &str,
    owner: &SourceOwner<'_>,
    aliases: &'a SourceAliases,
) -> Option<&'a str> {
    let level = owner.minimum_level;
    for segment in segment_ranges(drop_text).into_iter().map(|range| &drop_text[range]) {
        let head = segment_head(segment);
        match kind {
            "crafting_system" => {
                if let Some(alias) = aliases.crafting_systems.iter().find(|alias| {
                    alias.system == name
                        && head.eq_ignore_ascii_case(&alias.text)
                        && alias
                            .minimum_levels()
                            .is_none_or(|levels| level.is_some_and(|level| levels.contains(&level)))
                }) {
                    return Some(&alias.text);
                }
            }
            "vendor" => {
                if let Some(alias) = aliases.vendors.iter().find(|alias| {
                    alias.vendor == name
                        && head.eq_ignore_ascii_case(&alias.text)
                        && alias.contains.as_ref().is_none_or(|contains| {
                            segment.to_ascii_lowercase().contains(&contains.to_ascii_lowercase())
                        })
                }) {
                    return Some(&alias.text);
                }
            }
            "challenge" => {
                if let Some(alias) =
                    aliases.challenges.iter().find(|alias| alias.pack == name && head.eq_ignore_ascii_case(&alias.text))
                {
                    return Some(&alias.text);
                }
            }
            "adventure_pack" => {
                if let Some(alias) = aliases
                    .adventure_packs
                    .iter()
                    .find(|alias| alias.pack == name && !alias.placement_only && head.eq_ignore_ascii_case(&alias.text))
                {
                    return Some(&alias.text);
                }
            }
            _ => {}
        }
    }
    None
}

fn source_provenance(
    owner: &SourceOwner<'_>,
    source: &SourceInput<'_>,
    quest_epic_names: &BTreeMap<String, String>,
    aliases: &SourceAliases,
) -> SourceProvenance {
    let kind = source.kind;
    let name = source.name;
    let mut provenance = SourceProvenance::default();
    if kind == "starter" {
        provenance.character_level = name.strip_prefix("Advance to level ").and_then(|level| level.parse().ok());
    }
    if owner.explicit_wiki_sources {
        return provenance;
    }
    let Some(drop_text) = owner.drop_text else { return provenance };
    let alternate_name = if kind == "quest" {
        quest_epic_names.get(name).map(String::as_str)
    } else {
        source_alias(kind, name, drop_text, owner, aliases)
    };
    let Some(segment) = chosen_segment(drop_text, kind, name, alternate_name, source.loot_type, source.chest) else {
        return provenance;
    };
    let raw_segment =
        if segment == drop_text.trim_end() && drop_text.len() > segment.len() { drop_text } else { segment };
    provenance.drop_text_segment = Some(raw_segment.to_string());
    provenance.source_match = matched_source_text(segment, kind, name, alternate_name).or_else(|| {
        let head = segment_head(segment);
        (!head.is_empty()).then(|| head.to_string())
    });

    provenance
}

pub(super) fn source_table(kind: ddo_model::enums::SourceKind) -> &'static str {
    use ddo_model::enums::SourceKind;
    match kind {
        SourceKind::Quest => "quests",
        SourceKind::AdventurePack | SourceKind::Challenge => "adventure_packs",
        SourceKind::QuestChain => "quest_chains",
        SourceKind::Saga => "sagas",
        SourceKind::CraftingSystem => "crafting_systems",
        SourceKind::Vendor => "vendors",
        SourceKind::Event => "events",
        SourceKind::Starter => "",
    }
}

pub(super) fn insert_source(
    db: &Connection,
    owner_table: &str,
    owner_id: i64,
    owner: &SourceOwner<'_>,
    source: &SourceInput<'_>,
    quest_epic_names: &BTreeMap<String, String>,
    aliases: &SourceAliases,
) -> Result<()> {
    let provenance = source_provenance(owner, source, quest_epic_names, aliases);
    use ddo_model::enums::SourceKind;
    let kind = SourceKind::ALL
        .iter()
        .copied()
        .find(|kind| kind.as_str() == source.kind)
        .with_context(|| format!("unknown source kind {:?}", source.kind))?;
    anyhow::ensure!(
        kind != SourceKind::Saga || source.tier.is_some(),
        "saga source {:?} must state its tier, including any",
        source.name
    );
    let table = source_table(kind);
    let target_id = if kind == SourceKind::Starter {
        provenance.character_level.context("starter source has no level")?
    } else {
        super::id_for_name(db, table, source.name)?
    };
    let owner_kind = match owner_table {
        "items" => "item",
        "augments" => "augment",
        _ => bail!("{owner_table} cannot own a source"),
    };
    let row = crate::loot_sources::LootSource {
        kind,
        target_id,
        owner_kind,
        owner_id,
        loot_type: source.loot_type,
        chest: source.chest,
        is_rare: source.is_rare,
        tier: source.tier,
        drop_text_segment: provenance.drop_text_segment.as_deref(),
        source_match: provenance.source_match.as_deref(),
    };
    let id = row.insert(db).with_context(|| format!("source {:?}", source.name))?;
    anyhow::ensure!(source.price.is_empty() || kind == SourceKind::Vendor, "only a vendor source can have a price");
    if kind == SourceKind::Vendor {
        crate::vendor_prices::insert_prices(db, id, source.price)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_provenance_follows_the_original_drop_segment() {
        let cases = [
            (
                "A New Invasion, Any chest; Tower of Despair, Nythirios chest",
                "Tower of Despair",
                Some("nythirios chest"),
                Some(" Tower of Despair, Nythirios chest"),
                Some("Tower of Despair"),
            ),
            (", end chest", "The Proper Authorities", None, None, None),
            (
                "ToEE: First Level and Earth Temple, and ToEE: Lower\nTemple Complex, rare encounter chests",
                "ToEE: Lower Temple Complex",
                None,
                Some("ToEE: First Level and Earth Temple, and ToEE: Lower\nTemple Complex, rare encounter chests"),
                Some("ToEE: Lower\nTemple Complex"),
            ),
            (
                "Old Tomb, New Tenants, End Chest; Three-Barrel Cove (epic), All Rare Encounters",
                "Old Grey Garl",
                None,
                Some("Old Tomb, New Tenants, End Chest"),
                Some("Old Tomb, New Tenants"),
            ),
        ];
        let quest_names = BTreeMap::from([("Old Grey Garl".to_string(), "Old Tomb, New Tenants".to_string())]);
        for (drop_text, name, chest, expected_segment, expected_match) in cases {
            let owner = SourceOwner { drop_text: Some(drop_text), minimum_level: None, explicit_wiki_sources: false };
            let source = SourceInput {
                tier: None,
                kind: "quest",
                name,
                loot_type: Some("chest"),
                chest,
                price: &[],
                is_rare: false,
            };
            let provenance = source_provenance(&owner, &source, &quest_names, &SourceAliases::default());
            assert_eq!(provenance.drop_text_segment.as_deref(), expected_segment);
            assert_eq!(provenance.source_match.as_deref(), expected_match);
            let cited_owner = SourceOwner { explicit_wiki_sources: true, ..owner };
            assert_eq!(
                source_provenance(&cited_owner, &source, &quest_names, &SourceAliases::default()),
                SourceProvenance::default()
            );
        }
    }

    #[test]
    fn augment_drop_location_accepts_each_source_label() {
        assert_eq!(
            augment_drop_text("Artifact bonus.\nDrops in; Dinosaur Crisis, optional chest"),
            Some("Dinosaur Crisis, optional chest")
        );
        assert_eq!(
            augment_drop_text("Drops in: Drop Location: ToEE: Lower\nTemple Complex"),
            Some("ToEE: Lower\nTemple Complex")
        );
    }
}
