use super::drop_text::{insert_source_link, DroppedLoot, LootSource, SourceLink};
use super::BuildReport;
use crate::wiki::WikiOverrides;
use anyhow::{Context, Result};
use ddo_model::enums::Provenance;
use rusqlite::{params, OptionalExtension, Transaction};

pub(super) fn write_wiki_vendors_and_events(
    transaction: &Transaction,
    wiki_overrides: &WikiOverrides,
    report: &mut BuildReport,
) -> Result<()> {
    for vendor in &wiki_overrides.vendors {
        transaction.execute(
            "INSERT INTO vendors (name, location, provenance, wiki_url) VALUES (?1, ?2, ?3, ?4)",
            params![vendor.name, vendor.location, Provenance::Wiki.as_str(), vendor.page],
        )?;
        report.wiki_vendor_count += 1;
    }
    for event in &wiki_overrides.events {
        transaction.execute(
            "INSERT INTO events (name, provenance, wiki_url) VALUES (?1, ?2, ?3)",
            params![event.name, Provenance::Wiki.as_str(), event.page],
        )?;
        report.wiki_event_count += 1;
    }
    Ok(())
}

pub(super) fn write_wiki_vendor_and_event_items(
    transaction: &Transaction,
    wiki_overrides: &WikiOverrides,
    report: &mut BuildReport,
) -> Result<()> {
    for vendor in &wiki_overrides.vendors {
        let citation = format!("wiki {} vendor {:?} ({})", vendor.file_name, vendor.name, vendor.page);
        let vendor_id = id_named(transaction, "SELECT id FROM vendors WHERE name = ?1", &vendor.name)?
            .context("the vendor row is written before his items")?;
        if let Some(pack_name) = &vendor.pack {
            let pack_id = id_named(transaction, "SELECT id FROM adventure_packs WHERE name = ?1", pack_name)?
                .with_context(|| {
                    format!("{citation}: pack {pack_name:?} is not an adventure pack in Maetrim's Quests.xml or Challenges.xml; use his spelling")
                })?;
            transaction.execute("UPDATE vendors SET pack_id = ?2 WHERE id = ?1", params![vendor_id, pack_id])?;
        }
        for vendor_item in &vendor.items {
            let item_id = listed_item_id(transaction, &citation, vendor_item.name())?;
            insert_listed_item(transaction, LootSource::Vendor(vendor_id), item_id)?;
            if let Some(cost) = vendor_item.cost() {
                transaction.execute(
                    "UPDATE sources SET cost = ?3 WHERE kind = 'vendor' AND vendor_id = ?1 AND item_id = ?2",
                    params![vendor_id, item_id, cost],
                )?;
            }
            report.vendor_item_count += 1;
        }
    }
    for event in &wiki_overrides.events {
        let citation = format!("wiki {} event {:?} ({})", event.file_name, event.name, event.page);
        let event_id = id_named(transaction, "SELECT id FROM events WHERE name = ?1", &event.name)?
            .context("the event row is written before his items")?;
        for item_name in &event.items {
            let item_id = listed_item_id(transaction, &citation, item_name)?;
            insert_listed_item(transaction, LootSource::Event(event_id), item_id)?;
            report.event_item_count += 1;
        }
    }
    Ok(())
}

fn listed_item_id(transaction: &Transaction, citation: &str, item_name: &str) -> Result<i64> {
    id_named(transaction, "SELECT id FROM items WHERE name = ?1", item_name)?.with_context(|| {
        format!("{citation}: item {item_name:?} is not in Maetrim's items or a wiki item; names must match exactly")
    })
}

fn insert_listed_item(transaction: &Transaction, source: LootSource, item_id: i64) -> Result<usize> {
    insert_source_link(
        transaction,
        &SourceLink {
            source,
            loot: DroppedLoot::Item(item_id),
            loot_type: None,
            is_rare: false,
            chest: None,
            tier: None,
            cost: None,
        },
    )
}

fn id_named(transaction: &Transaction, sql: &str, name: &str) -> Result<Option<i64>> {
    Ok(transaction.query_row(sql, params![name], |r| r.get(0)).optional()?)
}
