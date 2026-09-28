use super::{nonempty, BuildReport, Ctx};
use crate::xml::{guild_buffs, optional_buffs};
use anyhow::Result;
use ddo_model::enums::ModifierSource;
use rusqlite::params;
use std::path::Path;

impl Ctx<'_> {
    pub(super) fn write_guild_buffs(&mut self, path: &Path, report: &mut BuildReport) -> Result<()> {
        if !path.is_file() {
            return Ok(());
        }
        for b in guild_buffs::parse(path)? {
            let inserted = self.tx.execute(
                "INSERT OR IGNORE INTO guild_buffs (name, description, guild_level) VALUES (?1, ?2, ?3)",
                params![b.name, nonempty(b.description.as_deref()), b.guild_level],
            )?;
            if inserted == 0 {
                continue;
            }
            let id = self.tx.last_insert_rowid();
            self.write_modifiers(ModifierSource::GuildBuff, id, &b.effects)?;
            report.guild_buffs += 1;
        }
        Ok(())
    }

    pub(super) fn write_optional_buffs(&mut self, path: &Path, report: &mut BuildReport) -> Result<()> {
        if !path.is_file() {
            return Ok(());
        }
        for b in optional_buffs::parse(path)? {
            let inserted = self.tx.execute(
                "INSERT OR IGNORE INTO optional_buffs (name, icon, description) VALUES (?1, ?2, ?3)",
                params![b.name, nonempty(b.icon.as_deref()), nonempty(b.description.as_deref())],
            )?;
            if inserted == 0 {
                continue;
            }
            let id = self.tx.last_insert_rowid();
            self.write_modifiers(ModifierSource::OptionalBuff, id, &b.effects)?;
            report.optional_buffs += 1;
        }
        Ok(())
    }
}
