use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct AdventurePackAlias in "source_aliases" {
        keys { kind: &str, row_index: i64 }
        fields {
            text: String => text,
            pack: String => target_name,
            #[serde(default, skip_serializing_if = "super::super::file_types::is_false")]
            placement_only: bool => placement_only,
            reason: String => reason,
            url: String => url,
            read: String => read,
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct ChallengeAlias in "source_aliases" {
        keys { kind: &str, row_index: i64, placement_only: bool }
        fields {
            text: String => text,
            pack: String => target_name,
            reason: String => reason,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            url: Option<String> => url,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            read: Option<String> => read,
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct CraftingSystemAlias in "source_aliases" {
        keys { kind: &str, row_index: i64, placement_only: bool }
        fields {
            text: String => text,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            min_minimum_level: Option<i64> => min_minimum_level,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            max_minimum_level: Option<i64> => max_minimum_level,
            system: String => target_name,
            reason: String => reason,
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct VendorAlias in "source_aliases" {
        keys { kind: &str, row_index: i64, placement_only: bool }
        fields {
            text: String => text,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            contains: Option<String> => contains,
            vendor: String => target_name,
            reason: String => reason,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceAliasesFile {
    #[serde(default, rename = "adventure_pack", skip_serializing_if = "Vec::is_empty")]
    pub(crate) adventure_packs: Vec<AdventurePackAlias>,
    #[serde(default, rename = "challenge", skip_serializing_if = "Vec::is_empty")]
    pub(crate) challenges: Vec<ChallengeAlias>,
    #[serde(default, rename = "crafting_system", skip_serializing_if = "Vec::is_empty")]
    pub(crate) crafting_systems: Vec<CraftingSystemAlias>,
    #[serde(default, rename = "vendor", skip_serializing_if = "Vec::is_empty")]
    pub(crate) vendors: Vec<VendorAlias>,
}

impl SourceAliasesFile {
    pub(in crate::tree) fn from_xml() -> anyhow::Result<Self> {
        let aliases = crate::map::source_alias::SourceAliases::embedded()?;
        Ok(Self {
            adventure_packs: aliases
                .adventure_packs
                .into_iter()
                .map(|alias| AdventurePackAlias {
                    text: alias.text,
                    pack: alias.pack,
                    placement_only: alias.placement_only,
                    reason: alias.reason,
                    url: alias.source,
                    read: alias.read,
                })
                .collect(),
            challenges: aliases
                .challenges
                .into_iter()
                .map(|alias| ChallengeAlias {
                    text: alias.text,
                    pack: alias.pack,
                    reason: alias.reason,
                    url: alias.source,
                    read: alias.read,
                })
                .collect(),
            crafting_systems: aliases
                .crafting_systems
                .into_iter()
                .map(|alias| CraftingSystemAlias {
                    text: alias.text,
                    min_minimum_level: alias.min_minimum_level,
                    max_minimum_level: alias.max_minimum_level,
                    system: alias.system,
                    reason: alias.reason,
                })
                .collect(),
            vendors: aliases
                .vendors
                .into_iter()
                .map(|alias| VendorAlias {
                    text: alias.text,
                    contains: alias.contains,
                    vendor: alias.vendor,
                    reason: alias.reason,
                })
                .collect(),
        })
    }

    pub(in crate::tree) fn from_rows(db: &rusqlite::Connection) -> anyhow::Result<Self> {
        let mut select = db.prepare(&format!(
            "SELECT {} FROM source_aliases WHERE kind = 'adventure_pack' ORDER BY row_index",
            AdventurePackAlias::select_columns()
        ))?;
        let adventure_packs =
            select.query_map([], AdventurePackAlias::from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut select = db.prepare(&format!(
            "SELECT {} FROM source_aliases WHERE kind = 'challenge' ORDER BY row_index",
            ChallengeAlias::select_columns()
        ))?;
        let challenges = select.query_map([], ChallengeAlias::from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut select = db.prepare(&format!(
            "SELECT {} FROM source_aliases WHERE kind = 'crafting_system' ORDER BY row_index",
            CraftingSystemAlias::select_columns()
        ))?;
        let crafting_systems =
            select.query_map([], CraftingSystemAlias::from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut select = db.prepare(&format!(
            "SELECT {} FROM source_aliases WHERE kind = 'vendor' ORDER BY row_index",
            VendorAlias::select_columns()
        ))?;
        let vendors = select.query_map([], VendorAlias::from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Self { adventure_packs, challenges, crafting_systems, vendors })
    }

    pub(in crate::tree) fn write_source_aliases(&self, db: &rusqlite::Connection) -> anyhow::Result<usize> {
        let mut count = 0;
        for (position, alias) in self.adventure_packs.iter().enumerate() {
            count += alias.insert(db, "adventure_pack", position as i64)?;
        }
        for (position, alias) in self.challenges.iter().enumerate() {
            count += alias.insert(db, "challenge", position as i64, false)?;
        }
        for (position, alias) in self.crafting_systems.iter().enumerate() {
            count += alias.insert(db, "crafting_system", position as i64, false)?;
        }
        for (position, alias) in self.vendors.iter().enumerate() {
            count += alias.insert(db, "vendor", position as i64, false)?;
        }
        Ok(count)
    }
}
