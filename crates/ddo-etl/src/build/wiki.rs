use super::BuildReport;
use crate::wiki::WikiOverrides;
use anyhow::{Context, Result};
use ddo_model::enums::LootType;
use rusqlite::{params, OptionalExtension, Transaction};

pub(super) fn apply_wiki(tx: &Transaction, wiki: &WikiOverrides, report: &mut BuildReport) -> Result<()> {
    for entry in &wiki.quest_loot {
        let cited = format!("wiki quest_loot {:?} ({})", entry.name, entry.page);
        let quest_id = id_by_name(tx, "quests", &entry.name)?.with_context(|| {
            format!("{cited}: no quest has this name in Quests.xml or Challenges.xml; fix the name to match Maetrim's")
        })?;
        for item in &entry.rare {
            let item_id = id_by_name(tx, "items", item)?.with_context(|| {
                format!("{cited}: rare item {item:?} is not in Maetrim's items; report it upstream rather than adding it here")
            })?;
            report.wiki_quest_loot_links_added += tx.execute(
                "INSERT OR IGNORE INTO quest_loot (quest_id, item_id, loot_type) VALUES (?1, ?2, ?3)",
                params![quest_id, item_id, LootType::Chest.as_str()],
            )?;
            tx.execute(
                "UPDATE quest_loot SET is_rare = 1 WHERE quest_id = ?1 AND item_id = ?2",
                params![quest_id, item_id],
            )?;
            report.wiki_rare_drops += 1;
        }
        report.wiki_quest_loot_entries += 1;
    }
    for entry in &wiki.quests {
        let cited = format!("wiki quests {:?} ({})", entry.name, entry.page);
        let quest_id = id_by_name(tx, "quests", &entry.name)?.with_context(|| {
            format!("{cited}: no quest has this name in Quests.xml or Challenges.xml; fix the name to match Maetrim's")
        })?;
        tx.execute(
            "UPDATE quests SET duration = ?2, is_free_to_play = ?3, legendary_level = ?4, zone = ?5, bestowed_by = ?6,
                    flagging = ?7
              WHERE id = ?1",
            params![
                quest_id,
                entry.duration().map(|d| d.as_str()),
                entry.free_to_play,
                entry.legendary_level,
                entry.zone,
                entry.bestowed_by,
                entry.flagging
            ],
        )?;
        for (tier, xp) in entry.xp.tiers() {
            report.wiki_quest_xp_rows += tx.execute(
                "INSERT INTO quest_xp (quest_id, tier, casual, normal, hard, elite) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![quest_id, tier.as_str(), xp.casual, xp.normal, xp.hard, xp.elite],
            )?;
        }
        report.wiki_quest_entries += 1;
    }
    Ok(())
}

fn id_by_name(tx: &Transaction, table: &str, name: &str) -> Result<Option<i64>> {
    Ok(tx.query_row(&format!("SELECT id FROM {table} WHERE name = ?1"), params![name], |r| r.get(0)).optional()?)
}
