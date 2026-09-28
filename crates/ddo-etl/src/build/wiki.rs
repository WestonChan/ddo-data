use super::BuildReport;
use crate::wiki::WikiOverrides;
use anyhow::{Context, Result};
use rusqlite::{params, OptionalExtension, Transaction};

pub(super) fn apply_wiki(tx: &Transaction, wiki: &WikiOverrides, report: &mut BuildReport) -> Result<()> {
    for entry in &wiki.quest_loot {
        let cited = format!("wiki quest_loot {:?} ({})", entry.name, entry.page);
        id_by_name(tx, "quests", &entry.name)?.with_context(|| {
            format!("{cited}: no quest has this name in Quests.xml or Challenges.xml; fix the name to match Maetrim's")
        })?;
        for item in &entry.rare {
            id_by_name(tx, "items", item)?.with_context(|| {
                format!("{cited}: rare item {item:?} is not in Maetrim's items; report it upstream rather than adding it here")
            })?;
            report.wiki_rare_drops += 1;
        }
        report.wiki_quest_loot_entries += 1;
    }
    Ok(())
}

fn id_by_name(tx: &Transaction, table: &str, name: &str) -> Result<Option<i64>> {
    Ok(tx.query_row(&format!("SELECT id FROM {table} WHERE name = ?1"), params![name], |r| r.get(0)).optional()?)
}
