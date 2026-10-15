use anyhow::{Context, Result};
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::tree::annotations::EntityAnnotations;
use crate::tree::Citation;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct VendorCurrency in "vendor_currencies" {
        keys { id: i64, slug: &str, sort_order: i64 }
        fields { name: String => name }
        attached { citation: Vec<Citation> }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct VendorCurrenciesFile {
    pub(crate) row: Vec<VendorCurrency>,
}

impl VendorCurrenciesFile {
    pub(in crate::tree) fn from_rows(db: &Connection, annotations: &EntityAnnotations) -> Result<Self> {
        let mut select = db.prepare(&format!(
            "SELECT {} FROM vendor_currencies ORDER BY sort_order",
            VendorCurrency::select_columns()
        ))?;
        let mut currencies = Vec::new();
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("id")?, VendorCurrency::from_row(row)?)))? {
            let (id, mut currency) = row?;
            currency.citation = annotations
                .entity("vendor_currencies", id)
                .map(|annotation| annotation.citation.clone())
                .unwrap_or_default();
            currencies.push(currency);
        }
        Ok(Self { row: currencies })
    }

    pub(crate) fn write_vendor_currencies(&self, db: &Connection, path: &Path) -> Result<()> {
        let mut ids = super::super::ids::StableIds::default();
        for (position, currency) in self.row.iter().enumerate() {
            let identity = super::super::slug(&currency.name);
            let id = ids.issue("vendor_currencies", &identity, path)?;
            currency
                .insert(db, id, &identity, position as i64)
                .with_context(|| format!("{}: currency {:?}", path.display(), currency.name))?;
            crate::tree::annotations::insert_citations(db, "vendor_currencies", id, &currency.citation)?;
        }
        Ok(())
    }
}

pub(crate) fn insert_xml_vendor_currencies(db: &Connection) -> Result<()> {
    let file: VendorCurrenciesFile = toml::from_str(include_str!("../../../data/vendor_currencies.toml"))?;
    file.write_vendor_currencies(db, Path::new("data/vendor_currencies.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn currency_order_and_all_citations_round_trip() {
        let file: VendorCurrenciesFile = toml::from_str("[[row]]\nname = 'Z Token'\n[[row.citation]]\nurl = 'https://ddowiki.com/page/Z_Token'\nread = '2026-10-08'\n[[row.citation]]\nfield = 'name'\nurl = 'https://ddowiki.com/page/Z_Token_History'\nread = '2026-10-07'\n[[row]]\nname = 'A Token'\n[[row.citation]]\nurl = 'https://ddowiki.com/page/A_Token'\nread = '2026-10-08'\n").unwrap();
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        file.write_vendor_currencies(&db, Path::new("packs/shared/vendors/currencies.toml")).unwrap();
        let restored = VendorCurrenciesFile::from_rows(&db, &EntityAnnotations::from_database(&db).unwrap()).unwrap();
        assert_eq!(toml::to_string(&restored).unwrap(), toml::to_string(&file).unwrap());
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM citations WHERE owner_kind = 'vendor_currencies'", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            3
        );
    }
}
