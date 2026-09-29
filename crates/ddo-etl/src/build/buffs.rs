use super::{trimmed_non_empty, BuildReport, TableWriter};
use crate::xml::{guild_buffs, optional_buffs};
use anyhow::Result;
use ddo_model::enums::ModifierSource;
use rusqlite::params;
use std::path::Path;

impl TableWriter<'_> {
    pub(super) fn write_guild_buffs(&mut self, path: &Path, report: &mut BuildReport) -> Result<()> {
        if !path.is_file() {
            return Ok(());
        }
        for guild_buff in guild_buffs::parse(path)? {
            let inserted_row_count = self.transaction.execute(
                "INSERT OR IGNORE INTO guild_buffs (name, description, guild_level) VALUES (?1, ?2, ?3)",
                params![guild_buff.name, trimmed_non_empty(guild_buff.description.as_deref()), guild_buff.guild_level],
            )?;
            if inserted_row_count == 0 {
                continue;
            }
            let guild_buff_id = self.transaction.last_insert_rowid();
            self.write_modifiers(ModifierSource::GuildBuff, guild_buff_id, &guild_buff.effects)?;
            report.guild_buff_count += 1;
        }
        Ok(())
    }

    pub(super) fn write_optional_buffs(&mut self, path: &Path, report: &mut BuildReport) -> Result<()> {
        if !path.is_file() {
            return Ok(());
        }
        for optional_buff in optional_buffs::parse(path)? {
            let inserted_row_count = self.transaction.execute(
                "INSERT OR IGNORE INTO optional_buffs (name, icon, description) VALUES (?1, ?2, ?3)",
                params![
                    optional_buff.name,
                    trimmed_non_empty(optional_buff.icon.as_deref()),
                    trimmed_non_empty(optional_buff.description.as_deref())
                ],
            )?;
            if inserted_row_count == 0 {
                continue;
            }
            let optional_buff_id = self.transaction.last_insert_rowid();
            self.write_modifiers(ModifierSource::OptionalBuff, optional_buff_id, &optional_buff.effects)?;
            report.optional_buff_count += 1;
        }
        Ok(())
    }
}
