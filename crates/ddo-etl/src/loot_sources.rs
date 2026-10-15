use anyhow::{ensure, Result};
use ddo_model::enums::SourceKind;
use rusqlite::{params, Connection, OptionalExtension};

pub(crate) struct LootSource<'a> {
    pub kind: SourceKind,
    pub target_id: i64,
    pub owner_kind: &'a str,
    pub owner_id: i64,
    pub loot_type: Option<&'a str>,
    pub chest: Option<&'a str>,
    pub is_rare: bool,
    pub tier: Option<&'a str>,
    pub drop_text_segment: Option<&'a str>,
    pub source_match: Option<&'a str>,
}

pub(crate) enum SourceMatch {
    Owner,
    LootType,
    Location,
}

impl LootSource<'_> {
    pub(crate) fn matching_id(&self, db: &Connection, matching: SourceMatch) -> Result<Option<i64>> {
        let table = self.kind.table_name();
        let target = self.kind.identifying_column();
        let owner = if self.kind == SourceKind::Vendor {
            "item_id = ?3 AND ?2 = 'item'"
        } else {
            "owner_kind = ?2 AND owner_id = ?3"
        };
        let mut predicate = format!("{target} = ?1 AND {owner}");
        let mut parameters: Vec<&dyn rusqlite::ToSql> = vec![&self.target_id, &self.owner_kind, &self.owner_id];
        let tier = self.tier.unwrap_or("any");
        if !matches!(matching, SourceMatch::Owner) {
            if self.kind.has_loot_type() {
                predicate.push_str(" AND loot_type = ?4");
                parameters.push(&self.loot_type);
                if matches!(matching, SourceMatch::Location) {
                    predicate.push_str(" AND chest IS ?5");
                    parameters.push(&self.chest);
                }
            } else if self.kind == SourceKind::Saga {
                predicate.push_str(" AND tier = ?4");
                parameters.push(&tier);
            }
        }
        Ok(db
            .query_row(&format!("SELECT id FROM {table} WHERE {predicate} LIMIT 1"), parameters.as_slice(), |row| {
                row.get(0)
            })
            .optional()?)
    }

    pub(crate) fn insert(&self, db: &Connection) -> Result<i64> {
        let table = self.kind.table_name();
        let target = self.kind.identifying_column();
        ensure!(
            self.kind.has_loot_type() || (self.loot_type.is_none() && self.chest.is_none()),
            "{:?} cannot carry drop-only fields",
            self.kind
        );
        ensure!(self.kind == SourceKind::Saga || self.tier.is_none(), "only a saga source can carry a tier");
        if self.kind == SourceKind::Vendor {
            ensure!(self.owner_kind == "item", "a vendor offer must output an item");
            db.prepare_cached("INSERT INTO vendor_offers (vendor_id, item_id, drop_text_segment, source_match, is_rare) VALUES (?1, ?2, ?3, ?4, ?5)")?.execute(params![self.target_id, self.owner_id, self.drop_text_segment, self.source_match, self.is_rare])?;
        } else if self.kind.has_loot_type() {
            db.prepare_cached(&format!("INSERT INTO {table} ({target}, owner_kind, owner_id, drop_text_segment, source_match, loot_type, chest, is_rare) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"))?.execute(params![self.target_id, self.owner_kind, self.owner_id, self.drop_text_segment, self.source_match, self.loot_type, self.chest, self.is_rare])?;
        } else if self.kind == SourceKind::Saga {
            db.prepare_cached("INSERT INTO saga_rewards (saga_id, owner_kind, owner_id, drop_text_segment, source_match, tier, is_rare) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)")?.execute(params![self.target_id, self.owner_kind, self.owner_id, self.drop_text_segment, self.source_match, self.tier.unwrap_or("any"), self.is_rare])?;
        } else {
            db.prepare_cached(&format!("INSERT INTO {table} ({target}, owner_kind, owner_id, drop_text_segment, source_match, is_rare) VALUES (?1, ?2, ?3, ?4, ?5, ?6)"))?.execute(params![self.target_id, self.owner_kind, self.owner_id, self.drop_text_segment, self.source_match, self.is_rare])?;
        }
        Ok(db.last_insert_rowid())
    }
}

pub(crate) fn reject_missing_source_references(db: &Connection) -> Result<()> {
    let mut select = db.prepare(ddo_model::sources::REFERENCE_VIOLATIONS)?;
    let mut rows = select.query([])?;
    if let Some(row) = rows.next()? {
        anyhow::bail!("source reference: {}: {}", row.get::<_, String>(0)?, row.get::<_, String>(2)?);
    }
    Ok(())
}
