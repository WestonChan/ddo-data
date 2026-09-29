use super::{trimmed_non_empty, BuildReport, TableWriter};
use crate::xml::sentient_gems;
use crate::xml::set_bonuses::parse_set_bonus_file;
use anyhow::Result;
use ddo_model::enums::ModifierSource;
use rusqlite::params;
use std::path::Path;

impl TableWriter<'_> {
    pub(super) fn write_set_bonus_file(
        &mut self,
        path: &Path,
        is_filigree_set: bool,
        report: &mut BuildReport,
    ) -> Result<()> {
        if !path.is_file() {
            return Ok(());
        }
        let set_bonus_file = parse_set_bonus_file(path)?;
        for set_bonus in &set_bonus_file.set_bonuses {
            let set_name = set_bonus.name.trim();
            self.transaction.execute(
                "INSERT OR IGNORE INTO set_bonuses (name, icon, is_filigree_set) VALUES (?1, ?2, ?3)",
                params![set_name, trimmed_non_empty(set_bonus.icon.as_deref()), is_filigree_set],
            )?;
            let set_id: i64 =
                self.transaction
                    .query_row("SELECT id FROM set_bonuses WHERE name = ?1", params![set_name], |r| r.get(0))?;
            if self.written.set_bonus_ids_by_name.insert(set_name.to_string(), set_id).is_some() {
                continue;
            }
            for tier in &set_bonus.tiers {
                self.transaction.execute(
                    "INSERT OR IGNORE INTO set_bonus_tiers (set_id, equipped_count, description) VALUES (?1, ?2, ?3)",
                    params![set_id, tier.equipped_count, trimmed_non_empty(tier.description.as_deref())],
                )?;
                let tier_id: i64 = self.transaction.query_row(
                    "SELECT id FROM set_bonus_tiers WHERE set_id = ?1 AND equipped_count = ?2",
                    params![set_id, tier.equipped_count],
                    |r| r.get(0),
                )?;
                self.write_modifiers(ModifierSource::SetBonusTier, tier_id, &tier.effects)?;
            }
        }
        for filigree in &set_bonus_file.filigrees {
            let set_id = match filigree.set_bonus_names.first().map(|s| s.trim()) {
                Some(set_name) => self.written.set_bonus_ids_by_name.get(set_name).copied(),
                None => None,
            };
            self.transaction.execute(
                "INSERT OR IGNORE INTO filigrees (name, description, icon, menu, set_id) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    filigree.name.trim(),
                    trimmed_non_empty(filigree.description.as_deref()),
                    trimmed_non_empty(filigree.icon.as_deref()),
                    trimmed_non_empty(filigree.menu.as_deref()),
                    set_id
                ],
            )?;
            let filigree_id: i64 = self.transaction.query_row(
                "SELECT id FROM filigrees WHERE name = ?1",
                params![filigree.name.trim()],
                |r| r.get(0),
            )?;
            self.write_modifiers(ModifierSource::Filigree, filigree_id, &filigree.effects)?;
            report.filigree_count += 1;
        }
        Ok(())
    }

    pub(super) fn write_sentient_gems(&mut self, path: &Path, report: &mut BuildReport) -> Result<()> {
        if !path.is_file() {
            return Ok(());
        }
        for gem in sentient_gems::parse(path)? {
            report.sentient_gem_count += self.transaction.execute(
                "INSERT OR IGNORE INTO sentient_gems (name, icon, description) VALUES (?1, ?2, ?3)",
                params![
                    gem.name,
                    trimmed_non_empty(gem.icon.as_deref()),
                    trimmed_non_empty(gem.description.as_deref())
                ],
            )?;
        }
        Ok(())
    }

    pub(super) fn link_pending_set_members(&mut self) -> Result<()> {
        let pending_item_links = std::mem::take(&mut self.pending_set_item_links);
        self.insert_set_member_links(
            pending_item_links,
            "INSERT OR IGNORE INTO set_bonus_items (set_id, item_id) VALUES (?1, ?2)",
        )?;
        let pending_augment_links = std::mem::take(&mut self.pending_set_augment_links);
        self.insert_set_member_links(
            pending_augment_links,
            "INSERT OR IGNORE INTO set_bonus_augments (set_id, augment_id) VALUES (?1, ?2)",
        )
    }

    fn insert_set_member_links(&self, pending_links: Vec<(i64, String)>, insert_sql: &str) -> Result<()> {
        for (member_id, set_name) in pending_links {
            if let Some(set_id) = self.written.set_bonus_ids_by_name.get(&set_name) {
                self.transaction.execute(insert_sql, params![set_id, member_id])?;
            }
        }
        Ok(())
    }
}
