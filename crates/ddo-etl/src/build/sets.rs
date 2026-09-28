use super::{nonempty, BuildReport, Ctx};
use crate::xml::sentient_gems;
use crate::xml::set_bonuses::parse_set_file;
use anyhow::Result;
use ddo_model::enums::ModifierSource;
use rusqlite::params;
use std::path::Path;

impl Ctx<'_> {
    pub(super) fn write_set_file(
        &mut self,
        path: &Path,
        is_filigree_set: bool,
        report: &mut BuildReport,
    ) -> Result<()> {
        if !path.is_file() {
            return Ok(());
        }
        let file = parse_set_file(path)?;
        for set in &file.sets {
            let name = set.name.trim();
            self.tx.execute(
                "INSERT OR IGNORE INTO set_bonuses (name, icon, is_filigree_set) VALUES (?1, ?2, ?3)",
                params![name, nonempty(set.icon.as_deref()), is_filigree_set],
            )?;
            let set_id: i64 =
                self.tx.query_row("SELECT id FROM set_bonuses WHERE name = ?1", params![name], |r| r.get(0))?;
            if self.caches.sets.insert(name.to_string(), set_id).is_some() {
                continue;
            }
            for tier in &set.tiers {
                self.tx.execute(
                    "INSERT OR IGNORE INTO set_bonus_tiers (set_id, equipped_count, description) VALUES (?1, ?2, ?3)",
                    params![set_id, tier.equipped_count, nonempty(tier.description.as_deref())],
                )?;
                let tier_id: i64 = self.tx.query_row(
                    "SELECT id FROM set_bonus_tiers WHERE set_id = ?1 AND equipped_count = ?2",
                    params![set_id, tier.equipped_count],
                    |r| r.get(0),
                )?;
                self.write_modifiers(ModifierSource::SetBonusTier, tier_id, &tier.effects)?;
            }
        }
        for f in &file.filigrees {
            let set_id = match f.set_bonus.first().map(|s| s.trim()) {
                Some(s) => self.caches.sets.get(s).copied(),
                None => None,
            };
            self.tx.execute(
                "INSERT OR IGNORE INTO filigrees (name, description, icon, menu, set_id) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    f.name.trim(),
                    nonempty(f.description.as_deref()),
                    nonempty(f.icon.as_deref()),
                    nonempty(f.menu.as_deref()),
                    set_id
                ],
            )?;
            let id: i64 =
                self.tx.query_row("SELECT id FROM filigrees WHERE name = ?1", params![f.name.trim()], |r| r.get(0))?;
            self.write_modifiers(ModifierSource::Filigree, id, &f.effects)?;
            report.filigrees += 1;
        }
        Ok(())
    }

    pub(super) fn write_sentient_gems(&mut self, path: &Path, report: &mut BuildReport) -> Result<()> {
        if !path.is_file() {
            return Ok(());
        }
        for g in sentient_gems::parse(path)? {
            report.sentient_gems += self.tx.execute(
                "INSERT OR IGNORE INTO sentient_gems (name, icon, description) VALUES (?1, ?2, ?3)",
                params![g.name, nonempty(g.icon.as_deref()), nonempty(g.description.as_deref())],
            )?;
        }
        Ok(())
    }

    pub(super) fn resolve_set_members(&mut self) -> Result<()> {
        let items = std::mem::take(&mut self.pending_set_items);
        self.link_set_members(items, "INSERT OR IGNORE INTO set_bonus_items (set_id, item_id) VALUES (?1, ?2)")?;
        let augments = std::mem::take(&mut self.pending_set_augments);
        self.link_set_members(augments, "INSERT OR IGNORE INTO set_bonus_augments (set_id, augment_id) VALUES (?1, ?2)")
    }

    fn link_set_members(&self, pending: Vec<(i64, String)>, insert: &str) -> Result<()> {
        for (member_id, set_name) in pending {
            if let Some(set_id) = self.caches.sets.get(&set_name) {
                self.tx.execute(insert, params![set_id, member_id])?;
            }
        }
        Ok(())
    }
}
