use anyhow::{ensure, Result};
use ddo_model::enums::SourceKind;
use rusqlite::{params_from_iter, types::Value, Connection, OptionalExtension};

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

    fn column_value(&self, column: &str) -> Value {
        let text = |value: Option<&str>| value.map_or(Value::Null, |text| Value::Text(text.to_string()));
        match column {
            "owner_kind" => Value::Text(self.owner_kind.to_string()),
            "item_id" | "owner_id" => Value::Integer(self.owner_id),
            "drop_text_segment" => text(self.drop_text_segment),
            "source_match" => text(self.source_match),
            "loot_type" => text(self.loot_type),
            "chest" => text(self.chest),
            "tier" => Value::Text(self.tier.unwrap_or("any").to_string()),
            "is_rare" => Value::Integer(i64::from(self.is_rare)),
            identifying if identifying == self.kind.identifying_column() => Value::Integer(self.target_id),
            unknown => unreachable!("source column {unknown} has no value"),
        }
    }

    pub(crate) fn insert(&self, db: &Connection) -> Result<i64> {
        ensure!(
            self.kind.has_loot_type() || (self.loot_type.is_none() && self.chest.is_none()),
            "{:?} cannot carry drop-only fields",
            self.kind
        );
        ensure!(self.kind == SourceKind::Saga || self.tier.is_none(), "only a saga source can carry a tier");
        if self.kind == SourceKind::Vendor {
            ensure!(self.owner_kind == "item", "a vendor offer must output an item");
        }
        let columns = insert_columns(self.kind);
        let placeholders = (1..=columns.len()).map(|index| format!("?{index}")).collect::<Vec<_>>().join(", ");
        let values = columns.iter().map(|column| self.column_value(column)).collect::<Vec<_>>();
        db.prepare_cached(&format!(
            "INSERT INTO {} ({}) VALUES ({placeholders})",
            self.kind.table_name(),
            columns.join(", ")
        ))?
        .execute(params_from_iter(values))?;
        Ok(db.last_insert_rowid())
    }
}

fn insert_columns(kind: SourceKind) -> Vec<&'static str> {
    let identifying = kind.identifying_column();
    match kind {
        SourceKind::Vendor => vec![identifying, "item_id", "drop_text_segment", "source_match", "is_rare"],
        SourceKind::Saga => {
            vec![identifying, "owner_kind", "owner_id", "drop_text_segment", "source_match", "tier", "is_rare"]
        }
        _ if kind.has_loot_type() => vec![
            identifying,
            "owner_kind",
            "owner_id",
            "drop_text_segment",
            "source_match",
            "loot_type",
            "chest",
            "is_rare",
        ],
        _ => vec![identifying, "owner_kind", "owner_id", "drop_text_segment", "source_match", "is_rare"],
    }
}

pub(crate) fn declared_columns() -> Vec<(&'static str, Vec<&'static str>)> {
    SourceKind::ALL.iter().map(|kind| (kind.table_name(), insert_columns(*kind))).collect()
}

pub(crate) fn reject_missing_source_references(db: &Connection) -> Result<()> {
    let mut select = db.prepare(ddo_model::sources::REFERENCE_VIOLATIONS)?;
    let mut rows = select.query([])?;
    if let Some(row) = rows.next()? {
        anyhow::bail!("source reference: {}: {}", row.get::<_, String>(0)?, row.get::<_, String>(2)?);
    }
    Ok(())
}
