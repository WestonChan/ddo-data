database_record! {
    #[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct VocabularyTerm in (table: &str) {
        keys { id: i64, sort_order: i64 }
        fields { name: String => name }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            citation: Vec<crate::tree::Citation>,
        }
    }
}

macro_rules! declare_vocabulary_file {
    ($table:literal, $file:ident) => {
        impl $file {
            pub(in crate::tree) fn write_vocabulary(
                &self,
                db: &rusqlite::Connection,
                path: &std::path::Path,
                ids: &mut super::super::ids::StableIds,
            ) -> anyhow::Result<usize> {
                for (position, term) in self.row.iter().enumerate() {
                    let identity = super::super::slug(&term.name);
                    let id = ids.issue($table, &identity, path)?;
                    term.insert(db, $table, id, position as i64)?;
                }
                Ok(self.row.len())
            }

            pub(in crate::tree) fn from_rows(
                db: &rusqlite::Connection,
                annotations: &crate::tree::annotations::EntityAnnotations,
            ) -> anyhow::Result<Self> {
                let mut select = db.prepare(&format!(
                    "SELECT {} FROM {} ORDER BY sort_order",
                    super::VocabularyTerm::select_columns($table),
                    $table
                ))?;
                let row = select
                    .query_map([], |row| {
                        let mut term = super::VocabularyTerm::from_row(row)?;
                        term.attach_citations(annotations, $table, row.get("id")?);
                        Ok(term)
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(Self { row })
            }
        }
    };
}

mod augment;
pub(crate) use augment::AugmentFile;
mod augment_slot_types;
pub(crate) use augment_slot_types::AugmentSlotTypesFile;
mod bonus_types;
pub(crate) use bonus_types::BonusTypesFile;
mod class;
pub(crate) use class::ClassFile;
mod clickies;
pub(crate) use clickies::ClickiesFile;
mod crafting_system;
pub(crate) use crafting_system::CraftingSystemFile;
mod damage_types;
pub(crate) use damage_types::DamageTypesFile;
mod effect;
pub(crate) use effect::EffectFile;
mod effect_tier;
pub(crate) use effect_tier::EffectTierFile;
mod enhancement_tree;
pub(crate) use enhancement_tree::EnhancementTreeFile;
mod equipment_slots;
pub(crate) use equipment_slots::EquipmentSlotsFile;
mod event;
pub(crate) use event::EventFile;
mod exceptions;
pub use exceptions::{ExceptionsFile, PackException, QuestPackException};
mod redirects;
pub(crate) use redirects::RedirectsFile;
mod excluded_items;
pub(crate) use excluded_items::ExcludedItemsFile;
mod feat_category;
pub(crate) use feat_category::FeatCategoryFile;
mod filigree_set;
pub(crate) use filigree_set::FiligreeSetFile;
mod guild_buffs;
pub(crate) use guild_buffs::GuildBuffsFile;
mod item;
pub(crate) use item::ItemFile;
mod item_materials;
pub(crate) use item_materials::ItemMaterialsFile;
mod optional_buffs;
pub(crate) use optional_buffs::OptionalBuffsFile;
mod pack;
pub(crate) use pack::PackFile;
mod patron;
pub(crate) use patron::PatronFile;
mod quest;
pub(crate) use quest::QuestFile;
mod quest_series;
pub(crate) use quest_series::QuestSeriesFile;
mod race;
pub(crate) use race::RaceFile;

mod sentient_gem;
pub(crate) use sentient_gem::SentientGemFile;
mod set;
pub(crate) use set::SetFile;
mod source_aliases;
pub(crate) use source_aliases::SourceAliasesFile;
mod spell_list;
pub(crate) use spell_list::SpellListFile;
mod stances;
pub(crate) use stances::StancesFile;
mod stat;
pub(crate) use stat::StatFile;
mod stat_group;
pub(crate) use stat_group::StatGroupFile;
mod triggers;
pub(crate) use triggers::TriggersFile;
mod upstream_commit;
pub(crate) use upstream_commit::UpstreamCommitFile;
mod vendor;
pub(crate) use vendor::VendorFile;
mod vendor_currencies;
pub(crate) use vendor_currencies::{insert_xml_vendor_currencies, VendorCurrenciesFile};
mod weapon_proficiencies;
pub(crate) use weapon_proficiencies::WeaponProficienciesFile;
mod weapon_types;
pub(crate) use weapon_types::WeaponTypesFile;
