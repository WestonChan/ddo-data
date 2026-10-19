database_record! {
    #[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct VocabularyTerm in (table: &str) uses { "item_materials", "weapon_proficiencies", "triggers" } {
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

pub(super) mod augment;
pub(crate) use augment::AugmentFile;
pub(super) mod augment_slot_types;
pub(crate) use augment_slot_types::AugmentSlotTypesFile;
pub(super) mod bonus_types;
pub(crate) use bonus_types::BonusTypesFile;
pub(super) mod class;
pub(crate) use class::ClassFile;
pub(super) mod clickies;
pub(crate) use clickies::ClickiesFile;
pub(super) mod crafting_system;
pub(crate) use crafting_system::CraftingSystemFile;
pub(super) mod damage_types;
pub(crate) use damage_types::DamageTypesFile;
pub(super) mod effect;
pub(crate) use effect::EffectFile;
pub(super) mod effect_tier;
pub(crate) use effect_tier::EffectTierFile;
pub(super) mod enhancement_tree;
pub(crate) use enhancement_tree::EnhancementTreeFile;
pub(super) mod equipment_slots;
pub(crate) use equipment_slots::EquipmentSlotsFile;
pub(super) mod event;
pub(crate) use event::EventFile;
pub(super) mod exceptions;
pub use exceptions::{ExceptionsFile, PackException, QuestPackException};
pub(super) mod redirects;
pub(crate) use redirects::RedirectsFile;
pub use redirects::{Redirect, RetiredSlug};
pub(super) mod excluded_items;
pub(crate) use excluded_items::ExcludedItemsFile;
pub(super) mod feat_category;
pub(crate) use feat_category::FeatCategoryFile;
pub(super) mod filigree_set;
pub(crate) use filigree_set::FiligreeSetFile;
pub(super) mod guild_buffs;
pub(crate) use guild_buffs::GuildBuffsFile;
pub(super) mod item;
pub(crate) use item::ItemFile;
pub(super) mod item_materials;
pub(crate) use item_materials::ItemMaterialsFile;
pub(super) mod optional_buffs;
pub(crate) use optional_buffs::OptionalBuffsFile;
pub(super) mod pack;
pub(crate) use pack::PackFile;
pub(super) mod patron;
pub(crate) use patron::PatronFile;
pub(super) mod quest;
pub(crate) use quest::QuestFile;
pub(super) mod quest_series;
pub(crate) use quest_series::QuestSeriesFile;
pub(super) mod race;
pub(crate) use race::RaceFile;

pub(super) mod sentient_gem;
pub(crate) use sentient_gem::SentientGemFile;
pub(super) mod set;
pub(crate) use set::SetFile;
pub(super) mod source_aliases;
pub(crate) use source_aliases::SourceAliasesFile;
pub(super) mod spell_list;
pub(crate) use spell_list::SpellListFile;
pub(super) mod stances;
pub(crate) use stances::StancesFile;
pub(super) mod stat;
pub(crate) use stat::StatFile;
pub(super) mod stat_group;
pub(crate) use stat_group::StatGroupFile;
pub(super) mod triggers;
pub(crate) use triggers::TriggersFile;
pub(super) mod upstream_commit;
pub(crate) use upstream_commit::UpstreamCommitFile;
pub(super) mod vendor;
pub(crate) use vendor::VendorFile;
pub(super) mod vendor_currencies;
pub(crate) use vendor_currencies::{insert_xml_vendor_currencies, VendorCurrenciesFile};
pub(super) mod weapon_proficiencies;
pub(crate) use weapon_proficiencies::WeaponProficienciesFile;
pub(super) mod weapon_types;
pub(crate) use weapon_types::WeaponTypesFile;

pub(crate) fn declared_correctable_fields(
    kind: ddo_model::enums::CorrectionKind,
) -> Vec<crate::corrections::CorrectableField> {
    use ddo_model::enums::CorrectionKind;
    let direct = match kind {
        CorrectionKind::Item => ItemFile::CORRECTABLE_FIELDS,
        CorrectionKind::Augment => AugmentFile::CORRECTABLE_FIELDS,
        CorrectionKind::Quest => QuestFile::CORRECTABLE_FIELDS,
        CorrectionKind::Effect => EffectFile::CORRECTABLE_FIELDS,
        CorrectionKind::Feat => feat_category::Feat::CORRECTABLE_FIELDS,
        CorrectionKind::Enhancement => enhancement_tree::Enhancement::CORRECTABLE_FIELDS,
        CorrectionKind::Spell => spell_list::Spell::CORRECTABLE_FIELDS,
        CorrectionKind::EnhancementTree => EnhancementTreeFile::CORRECTABLE_FIELDS,
        CorrectionKind::OptionalBuff => optional_buffs::OptionalBuff::CORRECTABLE_FIELDS,
        CorrectionKind::Clickie => clickies::Clickie::CORRECTABLE_FIELDS,
        CorrectionKind::Filigree => filigree_set::Filigree::CORRECTABLE_FIELDS,
        CorrectionKind::Race => RaceFile::CORRECTABLE_FIELDS,
        CorrectionKind::Class => ClassFile::CORRECTABLE_FIELDS,
        CorrectionKind::AdventurePack => PackFile::CORRECTABLE_FIELDS,
        CorrectionKind::Patron => PatronFile::CORRECTABLE_FIELDS,
        CorrectionKind::SocketLabel => augment_slot_types::SocketType::CORRECTABLE_FIELDS,
        CorrectionKind::SetBonus => SetFile::CORRECTABLE_FIELDS,
        CorrectionKind::SetTier => set::SetTier::CORRECTABLE_FIELDS,
        _ => &[],
    };
    let mut fields = direct.to_vec();
    let derived = match kind {
        CorrectionKind::Item => ItemFile::MEMBERSHIP_CORRECTIONS,
        CorrectionKind::Quest => QuestFile::RUN_CORRECTIONS,
        CorrectionKind::ItemBonus
        | CorrectionKind::SetTierBonus
        | CorrectionKind::AugmentBonus
        | CorrectionKind::ItemEffect => crate::tree::tables::EffectGrant::GRANT_CORRECTIONS,
        _ => &[],
    };
    fields.extend_from_slice(derived);
    if matches!(kind, CorrectionKind::ItemBonus | CorrectionKind::SetTierBonus) {
        fields.extend_from_slice(effect::EffectBonus::SCALING_CORRECTIONS);
    }
    fields
}

pub(crate) fn selection_correctable_fields() -> &'static [crate::corrections::CorrectableField] {
    enhancement_tree::EnhancementSelection::CORRECTABLE_FIELDS
}
