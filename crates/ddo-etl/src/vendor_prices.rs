use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct VendorPrice {
    #[schemars(extend("x-reference-kind" = "vendor_currencies|crafting_ingredients"))]
    pub(crate) ingredient: String,
    pub(crate) quantity: i64,
}

pub(crate) fn prices_by_offer(db: &Connection) -> Result<BTreeMap<i64, Vec<VendorPrice>>> {
    let mut select = db.prepare("SELECT cost.offer_id, COALESCE(currency.name, ingredient.name), cost.quantity
        FROM vendor_offer_costs cost
        LEFT JOIN vendor_currencies currency ON cost.ingredient_kind = 'vendor_currency' AND currency.id = cost.ingredient_id
        LEFT JOIN crafting_ingredients ingredient ON cost.ingredient_kind = 'crafting_ingredient' AND ingredient.id = cost.ingredient_id
        ORDER BY cost.offer_id, cost.sort_order")?;
    let mut prices = BTreeMap::<i64, Vec<VendorPrice>>::new();
    for row in select.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, VendorPrice { ingredient: row.get(1)?, quantity: row.get(2)? }))
    })? {
        let (offer, price) = row?;
        prices.entry(offer).or_default().push(price);
    }
    Ok(prices)
}

fn ingredient_matches(db: &Connection, name: &str) -> Result<Vec<(String, i64)>> {
    let mut select = db.prepare(
        "SELECT 'vendor_currency', id FROM vendor_currencies WHERE name = ?1
        UNION ALL SELECT 'crafting_ingredient', id FROM crafting_ingredients WHERE name = ?1",
    )?;
    let matches = select.query_map([name], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
    Ok(matches)
}

pub(crate) fn insert_prices(db: &Connection, offer_id: i64, prices: &[VendorPrice]) -> Result<()> {
    let (vendor, item): (String, String) = db.query_row("SELECT vendor.name, item.name FROM vendor_offers offer
        JOIN vendors vendor ON vendor.id = offer.vendor_id JOIN items item ON item.id = offer.item_id WHERE offer.id = ?1", [offer_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
    for (position, price) in prices.iter().enumerate() {
        let matches = ingredient_matches(db, &price.ingredient)?;
        ensure!(
            matches.len() == 1,
            "vendor {vendor:?}, item {item:?}: price ingredient {:?} matches {} currencies or crafting ingredients",
            price.ingredient,
            matches.len()
        );
        ensure!(price.quantity > 0, "vendor {vendor:?}, item {item:?}: price quantity must be positive");
        db.prepare_cached("INSERT INTO vendor_offer_costs (offer_id, ingredient_kind, ingredient_id, quantity, sort_order) VALUES (?1, ?2, ?3, ?4, ?5)")?
            .execute(params![offer_id, matches[0].0, matches[0].1, price.quantity, position as i64])?;
    }
    Ok(())
}

pub(crate) fn legacy_price(db: &Connection, text: &str) -> Result<Vec<VendorPrice>> {
    let mut select = db.prepare("SELECT name FROM vendor_currencies UNION SELECT name FROM crafting_ingredients")?;
    let names = select.query_map([], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let shared_suffix = text.split_whitespace().last().unwrap_or("");
    text.split(',')
        .map(|part| {
            let part = part.trim().strip_prefix("and ").unwrap_or(part.trim());
            let (quantity, ingredient) = part
                .split_once(' ')
                .and_then(|(quantity, ingredient)| quantity.parse::<i64>().ok().map(|quantity| (quantity, ingredient)))
                .unwrap_or((1, part));
            let exact = names.iter().filter(|name| name.as_str() == ingredient).collect::<Vec<_>>();
            let matching = if exact.is_empty() {
                let plural_matches = names
                    .iter()
                    .filter(|name| ingredient.trim_end_matches('s') == name.trim_end_matches('s'))
                    .collect::<Vec<_>>();
                if plural_matches.is_empty() {
                    names
                        .iter()
                        .filter(|name| name.as_str() == format!("{ingredient} {shared_suffix}"))
                        .collect::<Vec<_>>()
                } else {
                    plural_matches
                }
            } else {
                exact
            };
            ensure!(
                matching.len() == 1,
                "price ingredient {ingredient:?} in {text:?} matches {} names",
                matching.len()
            );
            Ok(VendorPrice { ingredient: (*matching.first().context("price has no ingredient")?).clone(), quantity })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{insert_prices, legacy_price, prices_by_offer, VendorPrice};
    use crate::corrections::Corrections;
    use crate::wiki::WikiOverrides;
    use ddo_model::DatasetVersion;
    use rusqlite::Connection;
    use std::path::Path;

    fn fixture_offers() -> (Connection, i64) {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let wiki = WikiOverrides::from_dir(&fixtures.join("wiki")).unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        crate::build::build_database(
            &fixtures.join("DataFiles"),
            &wiki,
            &Corrections::default(),
            &mut db,
            &DatasetVersion { upstream_sha: "fixture".into(), built_at: "2026-10-08T00:00:00Z".into() },
        )
        .unwrap();
        let offer = db.query_row("SELECT offer.id FROM vendor_offers offer JOIN items item ON item.id = offer.item_id WHERE item.name = 'Ethereal Great Crossbow'", [], |row| row.get(0)).unwrap();
        (db, offer)
    }

    #[test]
    fn price_rows_preserve_order_and_the_largest_positive_quantity() {
        let (db, offer) = fixture_offers();
        let price = vec![
            VendorPrice { ingredient: "Silver Ingot".into(), quantity: i64::MAX },
            VendorPrice { ingredient: "Arcane Ingot".into(), quantity: 1 },
        ];
        insert_prices(&db, offer, &price).unwrap();
        assert_eq!(prices_by_offer(&db).unwrap()[&offer], price);
        let cost: String = db.query_row("SELECT cost FROM sources WHERE kind = 'vendor' AND item_id = (SELECT id FROM items WHERE name = 'Ethereal Great Crossbow')", [], |row| row.get(0)).unwrap();
        assert_eq!(cost, "9223372036854775807 Silver Ingot, 1 Arcane Ingot");
        assert_eq!(legacy_price(&db, &cost).unwrap(), price);
    }

    #[test]
    fn invalid_prices_name_the_vendor_and_item() {
        let (db, offer) = fixture_offers();
        for price in [
            VendorPrice { ingredient: "Planar Shard".into(), quantity: 0 },
            VendorPrice { ingredient: "Planar Shard".into(), quantity: -1 },
            VendorPrice { ingredient: "Unknown Ingredient".into(), quantity: 1 },
        ] {
            let error = insert_prices(&db, offer, &[price]).unwrap_err().to_string();
            assert!(error.contains("Morten Edgewright") && error.contains("Ethereal Great Crossbow"), "{error}");
        }
        assert!(!prices_by_offer(&db).unwrap().contains_key(&offer));
    }

    #[test]
    fn legacy_vendor_prices_resolve_powders_plurals_and_shared_ingredient_suffixes() {
        let (db, _) = fixture_offers();
        for (text, expected) in [
            ("Eerie Scarab Powder", vec![("Eerie Scarab Powder", 1)]),
            ("10 Planar Shards", vec![("Planar Shard", 10)]),
            (
                "1 Ethereal, 1 Arcane, and 1 Silver Ingot",
                vec![("Ethereal Ingot", 1), ("Arcane Ingot", 1), ("Silver Ingot", 1)],
            ),
        ] {
            assert_eq!(
                legacy_price(&db, text).unwrap(),
                expected
                    .into_iter()
                    .map(|(ingredient, quantity)| VendorPrice { ingredient: ingredient.into(), quantity })
                    .collect::<Vec<_>>()
            );
        }
        for invalid in ["", "1", "unknown", "1 Unknown Ingot", "9223372036854775808 Planar Shard"] {
            assert!(legacy_price(&db, invalid).is_err(), "{invalid}");
        }
    }
}
