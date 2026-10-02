use super::vendors::reject_items_listed_twice;
use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WikiEvent {
    pub name: String,
    pub page: String,
    pub read: String,
    #[serde(default)]
    pub items: Vec<String>,
    #[serde(skip)]
    pub file_name: String,
}

impl WikiEvent {
    pub(super) fn validate(&self) -> Result<()> {
        reject_items_listed_twice(self.items.iter().map(String::as_str))
    }
}
