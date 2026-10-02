use anyhow::{bail, Result};
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WikiVendor {
    pub name: String,
    pub page: String,
    pub read: String,
    pub location: Option<String>,
    pub pack: Option<String>,
    #[serde(default)]
    pub items: Vec<VendorItem>,
    #[serde(skip)]
    pub file_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum VendorItem {
    Named(String),
    Priced(PricedVendorItem),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PricedVendorItem {
    pub name: String,
    pub cost: Option<String>,
}

impl VendorItem {
    pub fn name(&self) -> &str {
        match self {
            Self::Named(name) => name,
            Self::Priced(item) => &item.name,
        }
    }

    pub fn cost(&self) -> Option<&str> {
        match self {
            Self::Named(_) => None,
            Self::Priced(item) => item.cost.as_deref(),
        }
    }
}

impl WikiVendor {
    pub(super) fn validate(&self) -> Result<()> {
        reject_items_listed_twice(self.items.iter().map(VendorItem::name))
    }
}

pub(super) fn reject_items_listed_twice<'name>(item_names: impl Iterator<Item = &'name str>) -> Result<()> {
    let mut listed_item_names = HashSet::new();
    for item_name in item_names {
        if !listed_item_names.insert(item_name) {
            bail!("item {item_name:?} is listed twice; list each item once");
        }
    }
    Ok(())
}
