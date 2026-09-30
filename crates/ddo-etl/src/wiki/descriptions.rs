use anyhow::{bail, Result};
use serde::Deserialize;

const AUGMENT_DROP_PLACEHOLDER: &str = "Drops in: ?";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DescriptionKind {
    Item,
    Augment,
    Race,
    Feat,
    Enhancement,
}

impl DescriptionKind {
    pub const ALL: [Self; 5] = [Self::Item, Self::Augment, Self::Race, Self::Feat, Self::Enhancement];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Item => "item",
            Self::Augment => "augment",
            Self::Race => "race",
            Self::Feat => "feat",
            Self::Enhancement => "enhancement",
        }
    }

    pub fn table_name(self) -> &'static str {
        match self {
            Self::Item => "items",
            Self::Augment => "augments",
            Self::Race => "races",
            Self::Feat => "feats",
            Self::Enhancement => "enhancements",
        }
    }

    pub fn is_awaiting_description(self, maetrim_description: Option<&str>) -> bool {
        self.text_kept_before_wiki_description(maetrim_description).is_some()
    }

    pub fn filled_description(self, maetrim_description: Option<&str>, wiki_description: &str) -> Option<String> {
        self.text_kept_before_wiki_description(maetrim_description).map(|kept_text| kept_text + wiki_description)
    }

    fn text_kept_before_wiki_description(self, maetrim_description: Option<&str>) -> Option<String> {
        let trimmed_description = maetrim_description.map(str::trim).unwrap_or_default();
        if trimmed_description.is_empty() {
            return Some(String::new());
        }
        if self != Self::Augment {
            return None;
        }
        trimmed_description
            .strip_suffix(AUGMENT_DROP_PLACEHOLDER)
            .filter(|text_before_placeholder| {
                text_before_placeholder.is_empty() || text_before_placeholder.ends_with('\n')
            })
            .map(str::to_owned)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WikiDescription {
    pub kind: DescriptionKind,
    pub name: String,
    pub page: String,
    pub read: String,
    pub description: String,
}

impl WikiDescription {
    pub(super) fn validate(&self) -> Result<()> {
        if self.description.trim().is_empty() {
            bail!("description is empty; leave the entry out until the page has text to copy");
        }
        if self.description.trim() != self.description {
            bail!("description {:?} starts or ends with whitespace; trim it", self.description);
        }
        Ok(())
    }
}
