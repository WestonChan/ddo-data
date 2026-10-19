mod catalog;
pub use catalog::find;
pub(super) use catalog::Catalog;

use super::kinds::*;
use anyhow::{ensure, Context, Result};
use schemars::schema_for;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

pub(super) const EFFECT_VOCABULARIES: &[(&str, &str)] = &[
    ("bonus-types.toml", "BonusTypes"),
    ("equipment-slots.toml", "EquipmentSlots"),
    ("weapon-types.toml", "WeaponTypes"),
    ("damage-types.toml", "DamageTypes"),
    ("item-materials.toml", "ItemMaterials"),
    ("triggers.toml", "Triggers"),
    ("augment-slot-types.toml", "AugmentSlotTypes"),
    ("weapon-proficiencies.toml", "WeaponProficiencies"),
    ("clickies.toml", "Clickies"),
];

static FILE_SCHEMAS: OnceLock<BTreeMap<String, serde_json::Value>> = OnceLock::new();

pub(super) fn file_schema(kind: &str) -> &'static serde_json::Value {
    let schemas = FILE_SCHEMAS.get_or_init(|| {
        schema_files()
            .expect("file schemas derive from Rust types")
            .into_iter()
            .map(|(file_name, json)| (file_name, serde_json::from_str(&json).expect("generated schema is JSON")))
            .collect()
    });
    let mut file_name = kebab_case(kind);
    file_name.push_str(".schema.json");
    schemas.get(&file_name).expect("every file kind has a generated schema")
}

fn collect_field_errors(
    root_schema: &serde_json::Value,
    definition: &serde_json::Value,
    value: &toml::Value,
    location: &str,
    errors: &mut Vec<String>,
) {
    let definition = definition["$ref"]
        .as_str()
        .and_then(|reference| reference.strip_prefix("#/$defs/"))
        .map_or(definition, |name| &root_schema["$defs"][name]);
    if let Some(fields) = value.as_table() {
        for field in definition["required"].as_array().into_iter().flatten().filter_map(serde_json::Value::as_str) {
            if !fields.contains_key(field) {
                errors.push(format!("{location}{field}"));
            }
        }
        for citation in fields.get("citation").and_then(toml::Value::as_array).into_iter().flatten() {
            let Some(cited_field) = citation.get("field").and_then(toml::Value::as_str).filter(|name| !name.is_empty())
            else {
                continue;
            };
            if definition["properties"].get(cited_field).is_none() {
                errors.push(format!(
                    "{location}citation field {cited_field:?} is neither empty nor a field or array of this record"
                ));
            }
        }
        for (name, child) in fields {
            if let Some(property) = definition["properties"].get(name) {
                if child.as_bool() == Some(false)
                    && property["type"] == "boolean"
                    && !definition["required"].as_array().into_iter().flatten().any(|required| required == name)
                {
                    errors.push(format!("{location}{name}: must omit false (the default)"));
                }
                let child_location =
                    if child.is_array() { format!("{location}{name}") } else { format!("{location}{name}.") };
                collect_field_errors(root_schema, property, child, &child_location, errors);
            }
        }
    } else if let Some(rows) = value.as_array() {
        for (index, row) in rows.iter().enumerate() {
            collect_field_errors(root_schema, &definition["items"], row, &format!("{location}[{index}]."), errors);
        }
    }
}

fn validate_source_value(path: &Path, value: &toml::Value) -> Result<()> {
    match value {
        toml::Value::Table(fields) => {
            for (name, child) in fields {
                ensure!(
                    name != "id"
                        && !name.ends_with("_id")
                        && !matches!(name.as_str(), "sort_order" | "owner_kind" | "source_kind"),
                    "{}: {name} is derived at build and cannot appear in a data file",
                    path.display()
                );
                validate_source_value(path, child)?;
            }
        }
        toml::Value::Array(rows) => {
            for row in rows {
                validate_source_value(path, row)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn validate_source_fields(root: &Path) -> Result<()> {
    for entry in walkdir::WalkDir::new(root) {
        let entry = entry?;
        if !entry.file_type().is_file()
            || entry.path().extension().is_none_or(|extension| extension != "toml")
            || in_generated_folder(entry.path().strip_prefix(root)?)
        {
            continue;
        }
        let path = entry.path();
        read_file(root, path, &std::fs::read_to_string(path)?)?;
    }
    Ok(())
}

pub(super) fn pack_local_kind(root: &Path, path: &Path) -> Result<Option<&'static str>> {
    let relative = path.strip_prefix(root)?;
    let segments =
        relative.iter().map(|part| part.to_str().context("data path is not UTF-8")).collect::<Result<Vec<_>>>()?;
    Ok(if segments.first() == Some(&"packs") {
        match segments.get(segments.len().saturating_sub(2)).copied() {
            Some("gear") => Some("Item"),
            Some("chains") => Some("QuestChain"),
            Some("sagas") => Some("Saga"),
            Some("sentient-gems") => Some("SentientGem"),
            Some("sets") => Some("Set"),
            Some("challenges") => Some("Quest"),
            Some("patrons") => Some("Patron"),
            Some("filigrees") => Some("FiligreeSet"),
            _ => None,
        }
    } else {
        None
    })
}

pub(crate) fn file_kind(root: &Path, path: &Path) -> Result<&'static str> {
    let relative = path.strip_prefix(root)?;
    let segments =
        relative.iter().map(|part| part.to_str().context("data path is not UTF-8")).collect::<Result<Vec<_>>>()?;
    let first = segments.first().copied().context("data path has no folder")?;
    let name = path.file_name().and_then(|name| name.to_str()).context("data file has no name")?;
    let kind = if relative == Path::new("redirects.toml") {
        "Redirects"
    } else if first == "effects" && segments.len() == 2 {
        EFFECT_VOCABULARIES.iter().find(|(filename, _)| *filename == name).map_or("Effect", |(_, kind)| kind)
    } else if let Some(kind) = pack_local_kind(root, path)? {
        kind
    } else if relative == Path::new("upstream/commit.toml") {
        "UpstreamCommit"
    } else if name == "exceptions.toml" {
        "Exceptions"
    } else {
        match first {
            "packs" => match name {
                "pack.toml" => "Pack",
                "quest.toml" => "Quest",
                "crafting.toml" => "CraftingSystem",
                "vendor.toml" => "Vendor",
                "event.toml" => "Event",
                "currencies.toml" if segments.as_slice() == ["packs", "shared", "vendors", "currencies.toml"] => {
                    "VendorCurrencies"
                }
                _ if segments.contains(&"augments") => "Augment",
                _ => "Item",
            },
            "characters" | "effects" | "upstream" => match segments.get(1).copied() {
                Some("tiers") => "EffectTier",
                Some("stats") => "Stat",
                Some("stat-groups") => "StatGroup",
                Some("classes") => "Class",
                Some("races") => "Race",
                Some("enhancements") => "EnhancementTree",
                Some("feats") => "FeatCategory",
                Some("spells") => "SpellList",
                Some("filigrees") => "FiligreeSet",
                _ => match name {
                    "augment-slot-types.toml" => "AugmentSlotTypes",
                    "bonus-types.toml" => "BonusTypes",
                    "clickies.toml" => "Clickies",
                    "damage-types.toml" => "DamageTypes",
                    "equipment-slots.toml" => "EquipmentSlots",
                    "excluded-items.toml" => "ExcludedItems",
                    "guild-buffs.toml" => "GuildBuffs",
                    "item-materials.toml" => "ItemMaterials",
                    "optional-buffs.toml" => "OptionalBuffs",
                    "source-aliases.toml" => "SourceAliases",
                    "stances.toml" => "Stances",
                    "triggers.toml" => "Triggers",
                    "weapon-proficiencies.toml" => "WeaponProficiencies",
                    "weapon-types.toml" => "WeaponTypes",
                    _ if first == "effects" => "Effect",
                    _ => anyhow::bail!("{}: unknown rule file", path.display()),
                },
            },
            _ => "Item",
        }
    };
    Ok(kind)
}

pub const SCHEMA_FOLDER: &str = ".schemas";
pub const TEMPLATE_FOLDER: &str = ".templates";
pub const GENERATED_FOLDERS: [&str; 2] = [SCHEMA_FOLDER, TEMPLATE_FOLDER];

pub fn generated_folder_contents() -> Result<[(&'static str, BTreeMap<String, String>); 2]> {
    Ok([(SCHEMA_FOLDER, schema_files()?), (TEMPLATE_FOLDER, template_files()?)])
}

pub fn in_generated_folder(path: &Path) -> bool {
    path.components().any(|component| GENERATED_FOLDERS.iter().any(|folder| component.as_os_str() == *folder))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    Entity(&'static str),
    Rows(&'static str),
    Dataset,
}

fn kebab_case(name: &str) -> String {
    let mut kebab = String::new();
    for (index, letter) in name.chars().enumerate() {
        if letter.is_uppercase() && index > 0 {
            kebab.push('-');
        }
        kebab.extend(letter.to_lowercase());
    }
    kebab
}

pub(super) trait FileTemplate: serde::de::DeserializeOwned {
    const EXEMPLAR: &'static str;

    fn template() -> Self {
        toml::from_str(Self::EXEMPLAR)
            .unwrap_or_else(|error| panic!("{} exemplar: {error}", std::any::type_name::<Self>()))
    }

    fn file_slug(exemplar: &toml::Value) -> String {
        super::slug(exemplar["name"].as_str().expect("an exemplar names itself, or its type names the file"))
    }
}

pub struct KindTemplate {
    pub kind: &'static str,
    pub file_name: String,
    pub text: String,
    pub slug: String,
}

macro_rules! template_text {
    (Dataset, $file:ty) => {
        None
    };
    (Rows, $file:ty) => {
        Some((super::typed_file_text(&<$file as FileTemplate>::template())?, String::new()))
    };
    ($placement:ident, $file:ty) => {{
        let exemplar = <$file as FileTemplate>::template();
        let slug = <$file as FileTemplate>::file_slug(&toml::Value::try_from(&exemplar)?);
        Some((super::typed_file_text(&exemplar)?, slug))
    }};
}

macro_rules! declare_file_kinds {
    ($($variant:ident($file:ty) => $placement:ident $(($path:literal))?,)+) => {
        #[derive(Debug)]
        pub(crate) enum SourceFile {
            $($variant($file),)+
        }

        fn read_typed_file(kind: &str, document: toml::Value) -> Result<SourceFile> {
            match kind {
                $(stringify!($variant) => Ok(SourceFile::$variant(document.try_into()?)),)+
                _ => anyhow::bail!("unknown data file kind {kind}"),
            }
        }

        impl SourceFile {
            #[cfg(test)]
            pub(crate) fn as_value(&self) -> Result<toml::Value> {
                match self {
                    $(Self::$variant(file) => toml::Value::try_from(file).map_err(Into::into),)+
                }
            }
        }

        pub fn schema_files() -> Result<BTreeMap<String, String>> {
            let schemas = [$((stringify!($variant), serde_json::to_value(schema_for!($file))?),)+];
            schemas
                .into_iter()
                .map(|(variant, schema)| {
                    Ok((
                        format!("{}.schema.json", kebab_case(variant)),
                        format!("{}\n", serde_json::to_string_pretty(&schema)?),
                    ))
                })
                .collect()
        }

        pub const FILE_KINDS: &[(&str, Placement)] = &[$((stringify!($variant), Placement::$placement $(($path))?),)+];

        pub fn kind_templates() -> Result<Vec<KindTemplate>> {
            let mut templates = Vec::new();
            $(
                if let Some((text, slug)) = template_text!($placement, $file) {
                    templates.push(KindTemplate {
                        kind: stringify!($variant),
                        file_name: format!("{}.toml", kebab_case(stringify!($variant))),
                        text,
                        slug,
                    });
                }
            )+
            Ok(templates)
        }

        pub fn template_files() -> Result<BTreeMap<String, String>> {
            Ok(kind_templates()?.into_iter().map(|template| (template.file_name, template.text)).collect())
        }
    };
}

declare_file_kinds! {
    Augment(AugmentFile) => Entity("packs/shared/augments/named/{slug}.toml"),
    AugmentSlotTypes(AugmentSlotTypesFile) => Rows("effects/augment-slot-types.toml"),
    BonusTypes(BonusTypesFile) => Rows("effects/bonus-types.toml"),
    Class(ClassFile) => Entity("characters/classes/{slug}.toml"),
    Clickies(ClickiesFile) => Rows("effects/clickies.toml"),
    CraftingSystem(CraftingSystemFile) => Entity("packs/shared/crafting/{slug}/crafting.toml"),
    DamageTypes(DamageTypesFile) => Rows("effects/damage-types.toml"),
    Effect(EffectFile) => Entity("effects/{slug}.toml"),
    EffectTier(EffectTierFile) => Entity("effects/tiers/{slug}.toml"),
    EnhancementTree(EnhancementTreeFile) => Entity("characters/enhancements/{slug}/tree.toml"),
    EquipmentSlots(EquipmentSlotsFile) => Rows("effects/equipment-slots.toml"),
    Event(EventFile) => Entity("packs/shared/events/{slug}/event.toml"),
    Exceptions(ExceptionsFile) => Dataset,
    ExcludedItems(ExcludedItemsFile) => Dataset,
    FeatCategory(FeatCategoryFile) => Rows("characters/feats/deity.toml"),
    FiligreeSet(FiligreeSetFile) => Rows("packs/shared/filigrees/electrocution.toml"),
    StatGroup(StatGroupFile) => Entity("effects/stat-groups/{slug}.toml"),
    GuildBuffs(GuildBuffsFile) => Rows("characters/guild-buffs.toml"),
    Item(ItemFile) => Entity("packs/unsourced/gear/{slug}.toml"),
    ItemMaterials(ItemMaterialsFile) => Rows("effects/item-materials.toml"),
    OptionalBuffs(OptionalBuffsFile) => Rows("characters/optional-buffs.toml"),
    Pack(PackFile) => Entity("packs/{slug}/pack.toml"),
    Patron(PatronFile) => Entity("packs/shared/patrons/{slug}.toml"),
    Quest(QuestFile) => Entity("packs/free-to-play/quests/{slug}/quest.toml"),
    QuestChain(QuestSeriesFile) => Entity("packs/free-to-play/chains/{slug}.toml"),
    Race(RaceFile) => Entity("characters/races/{slug}.toml"),
    Redirects(RedirectsFile) => Dataset,
    Saga(QuestSeriesFile) => Entity("packs/free-to-play/sagas/{slug}.toml"),
    SentientGem(SentientGemFile) => Entity("packs/shared/sentient-gems/{slug}.toml"),
    SourceAliases(SourceAliasesFile) => Dataset,
    Set(SetFile) => Entity("packs/shared/sets/{slug}.toml"),
    SpellList(SpellListFile) => Rows("characters/spells/unassigned.toml"),
    Stances(StancesFile) => Rows("characters/stances.toml"),
    Stat(StatFile) => Entity("effects/stats/{slug}.toml"),
    Triggers(TriggersFile) => Rows("effects/triggers.toml"),
    UpstreamCommit(super::UpstreamCommitFile) => Dataset,
    Vendor(VendorFile) => Entity("packs/free-to-play/vendors/{slug}/vendor.toml"),
    VendorCurrencies(VendorCurrenciesFile) => Rows("packs/shared/vendors/currencies.toml"),
    WeaponProficiencies(WeaponProficienciesFile) => Rows("effects/weapon-proficiencies.toml"),
    WeaponTypes(WeaponTypesFile) => Rows("effects/weapon-types.toml"),
}

impl SourceFile {
    pub(super) fn layout_entity(&self) -> Option<(&'static str, &str)> {
        match self {
            Self::Item(file) => Some(("items", &file.name)),
            Self::Augment(file) => Some(("augments", &file.name)),
            Self::Effect(file) => Some(("effects", &file.name)),
            Self::Stat(file) => Some(("effects", &file.name)),
            Self::StatGroup(file) => Some(("effects", &file.name)),
            Self::EffectTier(file) => Some(("effect_tier_groups", &file.name)),
            Self::Pack(file) => Some(("adventure_packs", &file.name)),
            Self::Class(file) => Some(("classes", &file.name)),
            Self::Race(file) => Some(("races", &file.name)),
            Self::EnhancementTree(file) => Some(("enhancement_trees", &file.name)),
            Self::Set(file) => Some(("set_bonuses", &file.name)),
            Self::Quest(file) => Some(("quests", &file.name)),
            Self::QuestChain(file) => Some(("quest_chains", &file.name)),
            Self::Saga(file) => Some(("sagas", &file.name)),
            Self::CraftingSystem(file) => Some(("crafting_systems", &file.name)),
            Self::Vendor(file) => Some(("vendors", &file.name)),
            Self::Event(file) => Some(("events", &file.name)),
            Self::Patron(file) => Some(("patrons", &file.name)),
            Self::SentientGem(file) => Some(("sentient_gems", &file.name)),
            _ => None,
        }
    }

    pub(super) fn icons<'a>(&'a self, kind: &'a str) -> Vec<(&'a str, &'a str, &'static str)> {
        use super::icon_references::IconReferences;
        let mut icons = Vec::new();
        match self {
            Self::Augment(file) => file.append_icons(kind, &mut icons),
            Self::Class(file) => file.append_icons(kind, &mut icons),
            Self::Effect(file) => file.append_icons(kind, &mut icons),
            Self::EnhancementTree(file) => file.append_icons(kind, &mut icons),
            Self::Item(file) => file.append_icons(kind, &mut icons),
            Self::Set(file) => file.append_icons(kind, &mut icons),
            Self::Stat(file) => file.append_icons(kind, &mut icons),
            Self::Pack(file) => file.append_icons(kind, &mut icons),
            Self::Patron(file) => file.append_icons(kind, &mut icons),
            Self::Quest(file) => file.append_icons(kind, &mut icons),
            Self::Vendor(file) => file.append_icons(kind, &mut icons),
            Self::Event(file) => file.append_icons(kind, &mut icons),
            Self::StatGroup(file) => file.append_icons(kind, &mut icons),
            Self::Race(file) => file.append_icons(kind, &mut icons),
            Self::Clickies(file) => file.row.append_icons(kind, &mut icons),
            Self::FeatCategory(file) => file.row.append_icons(kind, &mut icons),
            Self::FiligreeSet(file) => file.row.append_icons(kind, &mut icons),
            Self::GuildBuffs(file) => file.row.append_icons(kind, &mut icons),
            Self::OptionalBuffs(file) => file.row.append_icons(kind, &mut icons),
            Self::SpellList(file) => file.row.append_icons(kind, &mut icons),
            Self::Stances(file) => file.row.append_icons(kind, &mut icons),
            Self::SentientGem(file) => file.append_icons(kind, &mut icons),
            _ => {}
        }
        icons
    }
}

pub(crate) fn read_file(root: &Path, path: &Path, contents: &str) -> Result<SourceFile> {
    let kind = file_kind(root, path)?;
    let document: toml::Value = toml::from_str(contents)?;
    document.as_table().context("data document must be a table")?;
    validate_source_value(path, &document)?;
    if path.strip_prefix(root)?.starts_with("effects") {
        if let Some(name) = document.get("name").and_then(toml::Value::as_str) {
            let filename = format!("{}.toml", super::slug(name));
            ensure!(
                !EFFECT_VOCABULARIES.iter().any(|(reserved, _)| *reserved == filename),
                "{}: effect name {name:?} uses a reserved vocabulary filename; reserved names: {}",
                path.display(),
                EFFECT_VOCABULARIES.iter().map(|(filename, _)| *filename).collect::<Vec<_>>().join(", ")
            );
        }
    }
    if kind == "Pack" {
        let name = document.get("name").and_then(toml::Value::as_str).unwrap_or_default();
        ensure!(
            !matches!(super::slug(name).as_str(), "shared" | "general" | "unsourced"),
            "{}: {name:?} is a reserved pseudo-pack",
            path.display()
        );
    }
    let schema = file_schema(kind);
    let mut field_errors = Vec::new();
    collect_field_errors(schema, schema, &document, "", &mut field_errors);
    ensure!(field_errors.is_empty(), "{}: {kind} has invalid fields: {}", path.display(), field_errors.join(", "));
    let source = read_typed_file(kind, document)?;
    if let SourceFile::Pack(file) = &source {
        let folder = super::pack_folder_name(&super::slug(&file.name), file.update.as_deref())?;
        super::build::validate_entity_file_name(path, &folder, true, None)?;
    }
    Ok(source)
}

macro_rules! declared_records {
    ($($record:ty,)+) => {
        pub(crate) fn declared_record_columns() -> BTreeMap<String, std::collections::BTreeSet<String>> {
            let mut declared: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
            $(
                for (table, columns) in <$record as super::records::DeclaredRecord>::declared_columns() {
                    declared.entry(table.to_string()).or_default().extend(columns.into_iter().map(str::to_string));
                }
            )+
            declared
        }
    };
}

declared_records! {
    super::annotations::Citation,
    super::annotations::AppliedCorrection,
    super::annotations::EntityIdentity,
    super::kinds::augment::AugmentEffect,
    super::kinds::augment::AugmentSlot,
    super::kinds::augment::AugmentFile,
    super::kinds::augment_slot_types::SocketType,
    super::kinds::bonus_types::BonusType,
    super::kinds::class::ClassAutoFeat,
    super::kinds::class::ClassFeatSlot,
    super::kinds::class::ClassSpell,
    super::kinds::class::ClassSpellSlot,
    super::kinds::class::ClassFile,
    super::kinds::clickies::Clickie,
    super::kinds::crafting_system::CraftingSystemRecipeAugment,
    super::kinds::crafting_system::CraftingSystemRecipeIngredient,
    super::kinds::crafting_system::CraftingSystemFamily,
    super::kinds::crafting_system::CraftingSystemIngredient,
    super::kinds::crafting_system::CraftingSystemRecipe,
    super::kinds::crafting_system::CraftingSystemFile,
    super::kinds::damage_types::DamageType,
    super::kinds::effect::EffectBonus,
    super::kinds::effect::EffectDamage,
    super::kinds::effect::EffectFile,
    super::kinds::effect_tier::EffectTierFile,
    super::kinds::enhancement_tree::EnhancementSelection,
    super::kinds::enhancement_tree::SelectorExclusion,
    super::kinds::enhancement_tree::Enhancement,
    super::kinds::enhancement_tree::EnhancementTreeFile,
    super::kinds::equipment_slots::EquipmentSlot,
    super::kinds::event::EventFile,
    super::kinds::exceptions::PackException,
    super::kinds::exceptions::QuestPackException,
    super::kinds::excluded_items::ExcludedItem,
    super::kinds::feat_category::ConditionalFeatGroup,
    super::kinds::feat_category::FeatGroup,
    super::kinds::feat_category::FeatSubItem,
    super::kinds::feat_category::Feat,
    super::kinds::filigree_set::Filigree,
    super::kinds::guild_buffs::GuildBuff,
    super::kinds::item::SocketOptionEffect,
    super::kinds::item::SocketGrant,
    super::kinds::item::SocketSet,
    super::kinds::item::Armor,
    super::kinds::item::ItemClickie,
    super::kinds::item::ItemDrBypass,
    super::kinds::item::ItemEffect,
    super::kinds::item::SocketOption,
    super::kinds::item::Socket,
    super::kinds::item::Weapon,
    super::kinds::item::ItemFile,
    super::kinds::VocabularyTerm,
    super::kinds::optional_buffs::OptionalBuff,
    super::kinds::pack::PackFile,
    super::kinds::patron::PatronFile,
    super::kinds::quest::QuestFile,
    super::kinds::quest_series::QuestSeriesFile,
    super::kinds::quest_series::SeriesQuest,
    super::kinds::race::AbilityModifier,
    super::kinds::race::RaceFeatSlot,
    super::kinds::race::RaceGrantedFeat,
    super::kinds::race::RaceFile,
    super::kinds::redirects::Redirect,
    super::kinds::redirects::RetiredSlug,
    super::kinds::sentient_gem::SentientGemFile,
    super::kinds::set::SetAugment,
    super::kinds::set::SetItem,
    super::kinds::set::SetTier,
    super::kinds::set::SetFile,
    super::kinds::source_aliases::AdventurePackAlias,
    super::kinds::source_aliases::ChallengeAlias,
    super::kinds::source_aliases::CraftingSystemAlias,
    super::kinds::source_aliases::VendorAlias,
    super::kinds::spell_list::SpellDamage,
    super::kinds::spell_list::SpellDifficultyClass,
    super::kinds::spell_list::Spell,
    super::kinds::stat::StatFile,
    super::kinds::stat_group::StatGroupFile,
    super::kinds::upstream_commit::UpstreamCommitFile,
    super::kinds::vendor::VendorFile,
    super::kinds::vendor_currencies::VendorCurrency,
    super::kinds::weapon_types::WeaponType,
    super::RemovedEntry,
    super::tables::Requirement,
    super::tables::Modifier,
    super::tables::Attack,
    super::tables::DifficultyClass,
    super::tables::Stance,
    super::tables::Skill,
    super::tables::EffectGrant,
}

pub const UNPOPULATED_COLUMNS: &[(&str, &str)] = &[
    ("effects", "default_value2"),
    ("effects", "wiki_url"),
    ("effect_bonuses", "trigger_id"),
    ("triggers", "description"),
];

pub(crate) fn declared_columns_by_table() -> BTreeMap<String, std::collections::BTreeSet<String>> {
    let mut declared = declared_record_columns();
    for (table, columns) in crate::loot_sources::declared_columns() {
        declared.entry(table.to_string()).or_default().extend(columns.into_iter().map(str::to_string));
    }
    declared
        .entry("vendor_offer_costs".to_string())
        .or_default()
        .extend(crate::vendor_prices::OFFER_COST_COLUMNS.into_iter().map(str::to_string));
    declared
}

pub(crate) fn undeclared_columns(
    db: &rusqlite::Connection,
    declared: &BTreeMap<String, std::collections::BTreeSet<String>>,
) -> Result<Vec<String>> {
    use crate::delta::{is_rebuilt_column, table_merge, TableMerge};
    let mut findings = Vec::new();
    for table in crate::database_tables::table_names(db)? {
        if matches!(table_merge(&table), Some(TableMerge::Rederived | TableMerge::DatasetVersion)) {
            continue;
        }
        let mut statement = db.prepare(&format!("PRAGMA table_info(\"{table}\")"))?;
        let columns = statement
            .query_map([], |row| Ok((row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, i64>(5)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let primary_keys = columns.iter().filter(|(_, _, key)| *key > 0).count();
        for (column, column_type, key) in &columns {
            let is_rowid_alias = *key > 0 && primary_keys == 1 && column_type.eq_ignore_ascii_case("INTEGER");
            let covered = declared.get(&table).is_some_and(|names| names.contains(column))
                || super::annotations::ANNOTATION_COLUMNS.contains(&column.as_str())
                || UNPOPULATED_COLUMNS.contains(&(table.as_str(), column.as_str()));
            if !covered && !is_rowid_alias && !is_rebuilt_column(&table, column) {
                findings.push(format!("{table}.{column}"));
            }
        }
    }
    Ok(findings)
}

pub fn validate_column_coverage() -> Result<()> {
    let db = rusqlite::Connection::open_in_memory()?;
    db.execute_batch(ddo_model::ddl())?;
    let findings = undeclared_columns(&db, &declared_columns_by_table())?;
    ensure!(
        findings.is_empty(),
        "columns no record declaration writes, no build rebuilds and no list marks unpopulated: {}",
        findings.join(", ")
    );
    Ok(())
}

pub fn validate_unpopulated_columns(db: &rusqlite::Connection) -> Result<()> {
    let mut populated = Vec::new();
    for (table, column) in UNPOPULATED_COLUMNS {
        let rows: i64 =
            db.query_row(&format!("SELECT COUNT(*) FROM \"{table}\" WHERE \"{column}\" IS NOT NULL"), [], |row| {
                row.get(0)
            })?;
        if rows > 0 {
            populated.push(format!("{table}.{column} ({rows} rows)"));
        }
    }
    ensure!(populated.is_empty(), "columns listed as unpopulated hold values: {}", populated.join(", "));
    Ok(())
}

pub fn validate_taplo_config(workspace_root: &Path) -> Result<()> {
    let path = workspace_root.join("taplo.toml");
    let config: toml::Value =
        toml::from_str(&std::fs::read_to_string(&path)?).with_context(|| format!("parsing {}", path.display()))?;
    let rules = config.get("rule").and_then(toml::Value::as_array).context("taplo.toml has no path rules")?;
    ensure!(!rules.is_empty(), "taplo.toml has no path rules");
    for rule in rules {
        let includes = rule.get("include").and_then(toml::Value::as_array).context("Taplo rule has no includes")?;
        ensure!(!includes.is_empty(), "Taplo rule has empty includes");
        let schema_path = rule
            .get("schema")
            .and_then(|schema| schema.get("path"))
            .and_then(toml::Value::as_str)
            .context("Taplo rule has no schema path")?;
        ensure!(workspace_root.join(schema_path).is_file(), "Taplo schema {schema_path} does not exist");
    }
    let data_root = workspace_root.join("data");
    let root_entries = std::fs::read_dir(&data_root)?
        .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
        .collect::<std::io::Result<std::collections::BTreeSet<_>>>()?;
    let expected_entries: std::collections::BTreeSet<String> = GENERATED_FOLDERS
        .into_iter()
        .chain(["README.md", "characters", "icons", "effects", "packs", "upstream", "redirects.toml"])
        .map(str::to_string)
        .collect();
    ensure!(
        root_entries == expected_entries,
        "data root must contain only {}; found {root_entries:?}",
        expected_entries.iter().cloned().collect::<Vec<_>>().join(", ")
    );
    for entry in walkdir::WalkDir::new(&data_root) {
        let entry = entry?;
        if !entry.file_type().is_file() || entry.path().extension().is_none_or(|extension| extension != "toml") {
            continue;
        }
        let relative = entry.path().strip_prefix(workspace_root)?.to_string_lossy().replace('\\', "/");
        if in_generated_folder(entry.path().strip_prefix(&data_root)?) {
            continue;
        }
        let matching = matching_taplo_rules(rules, &relative).len();
        ensure!(
            matching == 1,
            "{}: matches {matching} Taplo schema rules, expected exactly one",
            entry.path().display()
        );
    }
    Ok(())
}

fn matching_taplo_rules<'a>(rules: &'a [toml::Value], relative: &str) -> Vec<&'a toml::Value> {
    rules
        .iter()
        .filter(|rule| {
            let includes = rule.get("include").and_then(toml::Value::as_array).into_iter().flatten();
            let excludes = rule.get("exclude").and_then(toml::Value::as_array).into_iter().flatten();
            includes.filter_map(toml::Value::as_str).any(|pattern| taplo_path_matches(pattern, relative))
                && !excludes.filter_map(toml::Value::as_str).any(|pattern| taplo_path_matches(pattern, relative))
        })
        .collect()
}

fn taplo_path_matches(pattern: &str, relative: &str) -> bool {
    let pattern_segments = pattern.split('/').collect::<Vec<_>>();
    let path_segments = relative.split('/').collect::<Vec<_>>();
    pattern_segments.len() == path_segments.len()
        && pattern_segments.iter().zip(path_segments).all(|(pattern, segment)| {
            *pattern == "*" || (*pattern == "*.toml" && segment.ends_with(".toml")) || *pattern == segment
        })
}

#[cfg(test)]
mod tests {
    fn ddl_database() -> rusqlite::Connection {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        db
    }

    #[test]
    fn every_database_column_is_declared_by_a_record_or_rebuilt() {
        super::validate_column_coverage().unwrap();
    }

    #[test]
    fn every_schema_x_columns_list_is_part_of_the_declared_columns() {
        let declared = super::declared_columns_by_table();
        let mut checked = 0;
        for schema in super::schema_files().unwrap().values() {
            let document: serde_json::Value = serde_json::from_str(schema).unwrap();
            for definition in document["$defs"].as_object().into_iter().flatten().map(|(_, definition)| definition) {
                let (Some(table), Some(columns)) = (definition["x-table"].as_str(), definition["x-columns"].as_array())
                else {
                    continue;
                };
                for column in columns.iter().filter_map(serde_json::Value::as_str) {
                    assert!(declared[table].contains(column), "{table}.{column}");
                }
                checked += 1;
            }
        }
        assert!(checked > 50, "{checked}");
    }

    #[test]
    fn a_ddl_column_no_declaration_writes_is_reported() {
        let db = ddl_database();
        db.execute_batch("ALTER TABLE patrons ADD COLUMN injected_column TEXT").unwrap();
        let findings = super::undeclared_columns(&db, &super::declared_columns_by_table()).unwrap();
        assert_eq!(findings, ["patrons.injected_column"]);
    }

    #[test]
    fn a_column_listed_unpopulated_that_holds_a_value_is_reported() {
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::tree::build_from_data(&fixtures.join("data"), &mut db).unwrap();
        super::validate_unpopulated_columns(&db).unwrap();
        db.execute("UPDATE effects SET wiki_url = 'https://ddowiki.com/page/Injected' WHERE id = (SELECT MIN(id) FROM effects)", [])
            .unwrap();
        let error = super::validate_unpopulated_columns(&db).unwrap_err().to_string();
        assert!(error.contains("effects.wiki_url (1 rows)"), "{error}");
    }

    #[test]
    fn every_placement_maps_back_to_its_own_kind() {
        let root = std::path::Path::new("data");
        for (kind, placement) in super::FILE_KINDS {
            let relative = match placement {
                super::Placement::Entity(pattern) => pattern.replace("{slug}", "example-entity"),
                super::Placement::Rows(path) => (*path).to_string(),
                super::Placement::Dataset => continue,
            };
            assert_eq!(super::file_kind(root, &root.join(&relative)).unwrap(), *kind, "{relative}");
        }
    }

    #[test]
    fn named_effects_live_at_the_effects_root() {
        let root = std::path::Path::new("data");
        assert_eq!(super::file_kind(root, &root.join("effects/example.toml")).unwrap(), "Effect");
    }

    use super::{read_file, taplo_path_matches, validate_source_value, validate_taplo_config, SourceFile};
    use std::path::Path;

    #[test]
    fn effect_names_cannot_take_reserved_vocabulary_filenames() {
        let root = Path::new("data");
        for (filename, kind) in super::EFFECT_VOCABULARIES {
            let path = root.join("effects").join(filename);
            assert_eq!(super::file_kind(root, &path).unwrap(), *kind);
            let name = filename.trim_end_matches(".toml").replace('-', " ");
            let error =
                read_file(root, &path, &format!("name = {name:?}\nverbose_name_template = {name:?}\n")).unwrap_err();
            let message = error.to_string();
            assert!(message.contains("reserved vocabulary filename"), "{message}");
            for (reserved, _) in super::EFFECT_VOCABULARIES {
                assert!(message.contains(reserved), "{message}");
            }
        }
    }

    #[test]
    fn item_and_augment_sources_use_singular_tables() {
        let root = Path::new("data");
        for (relative, fields) in [
            (
                "packs/unsourced/gear/example.toml",
                "name = 'Example'\nslot = 'Head'\nitem_category = 'Clothing'\nminimum_level = 1\n",
            ),
            ("packs/shared/augments/named/example.toml", "name = 'Example'\ndescription = ''\nfamily = 'Named'\n"),
        ] {
            let path = root.join(relative);
            let contents = format!("{fields}[[source]]\nkind = 'vendor'\nname = 'Example Vendor'\n");
            let file = read_file(root, &path, &contents).unwrap();
            assert_eq!(file.as_value().unwrap()["source"][0]["name"].as_str(), Some("Example Vendor"));
            assert!(read_file(root, &path, &contents.replace("[[source]]", "[[sources]]")).is_err());
        }
    }

    #[test]
    fn pack_local_entity_names_do_not_override_folder_kinds() {
        let root = Path::new("data");
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config: toml::Value =
            toml::from_str(&std::fs::read_to_string(workspace_root.join("taplo.toml")).unwrap()).unwrap();
        let rules = config["rule"].as_array().unwrap();
        for (folder, kind) in [
            ("sets", "Set"),
            ("challenges", "Quest"),
            ("patrons", "Patron"),
            ("chains", "QuestChain"),
            ("sagas", "Saga"),
            ("sentient-gems", "SentientGem"),
        ] {
            for name in ["Example", "Pack", "Quest", "Chain", "Exceptions"] {
                let path = root.join("packs/example").join(folder).join(format!("{}.toml", super::super::slug(name)));
                assert_eq!(super::file_kind(root, &path).unwrap(), kind, "{}", path.display());
                let matching = super::matching_taplo_rules(rules, path.to_str().unwrap());
                assert_eq!(matching.len(), 1, "{}", path.display());
                let schema_path = matching[0]["schema"]["path"].as_str().unwrap();
                let schema: serde_json::Value =
                    serde_json::from_str(&std::fs::read_to_string(workspace_root.join(schema_path)).unwrap()).unwrap();
                assert_eq!(&schema, super::file_schema(kind), "{}", path.display());
            }
        }
    }

    #[test]
    fn data_paths_share_reader_editor_and_icon_kinds() {
        let root = Path::new("data");
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config: toml::Value =
            toml::from_str(&std::fs::read_to_string(workspace_root.join("taplo.toml")).unwrap()).unwrap();
        let rules = config["rule"].as_array().unwrap();
        for (relative, kind, icon_kind) in [
            ("packs/example/pack.toml", "Pack", None),
            ("packs/u039-example/pack.toml", "Pack", None),
            ("packs/u039-example/filigrees/example.toml", "FiligreeSet", Some(("filigrees", "filigrees"))),
            ("packs/shared/sentient-gems/example.toml", "SentientGem", Some(("sentient_gems", "sentient-gems"))),
            ("characters/classes/example.toml", "Class", Some(("classes", "classes"))),
            ("characters/enhancements/example/tree.toml", "EnhancementTree", Some(("enhancements", "enhancements"))),
            ("upstream/commit.toml", "UpstreamCommit", None),
            ("packs/example/sagas/example.toml", "Saga", None),
            ("packs/shared/sagas/example.toml", "Saga", None),
            ("packs/example/crafting/example/crafting.toml", "CraftingSystem", None),
            ("packs/shared/crafting/example/crafting.toml", "CraftingSystem", None),
            ("packs/example/vendors/example/vendor.toml", "Vendor", None),
            ("packs/shared/vendors/example/vendor.toml", "Vendor", None),
            ("packs/example/events/example/event.toml", "Event", None),
            ("packs/shared/events/example/event.toml", "Event", None),
            ("packs/shared/vendors/currencies.toml", "VendorCurrencies", None),
            ("packs/example/challenges/example.toml", "Quest", None),
            ("packs/shared/sets/example.toml", "Set", Some(("set_bonuses", "sets"))),
            ("packs/shared/patrons/example.toml", "Patron", None),
            ("packs/shared/augments/named/example.toml", "Augment", Some(("augments", "augments"))),
            ("packs/example/crafting/example/augments/example.toml", "Augment", Some(("augments", "augments"))),
            ("packs/example/crafting/example/gear/example.toml", "Item", Some(("items", "items"))),
            ("packs/shared/vendors/example/gear/example.toml", "Item", Some(("items", "items"))),
            ("packs/general/gear/example.toml", "Item", Some(("items", "items"))),
            ("packs/unsourced/gear/example.toml", "Item", Some(("items", "items"))),
            ("effects/example.toml", "Effect", None),
            ("effects/stats/example.toml", "Stat", None),
            ("effects/stat-groups/example.toml", "StatGroup", None),
            ("effects/tiers/example.toml", "EffectTier", None),
            ("packs/example/chains/chain.toml", "QuestChain", None),
            ("packs/free-to-play/patron/chains/chain.toml", "QuestChain", None),
            ("packs/shared/chains/chain.toml", "QuestChain", None),
            ("packs/example/quests/quest/quest.toml", "Quest", None),
            ("packs/free-to-play/patron/quests/quest/quest.toml", "Quest", None),
            ("packs/example/gear/reward.toml", "Item", Some(("items", "items"))),
            ("packs/example/quests/quest/gear/item.toml", "Item", Some(("items", "items"))),
            ("packs/example/quests/quest/gear/heroic.toml", "Item", Some(("items", "items"))),
            ("packs/free-to-play/patron/gear/reward.toml", "Item", Some(("items", "items"))),
            ("packs/free-to-play/patron/quests/quest/gear/heroic.toml", "Item", Some(("items", "items"))),
        ] {
            let path = root.join(relative);
            assert_eq!(super::file_kind(root, &path).unwrap(), kind, "{relative}");
            assert_eq!(crate::icons::icon_kind_for_data_path(Path::new(relative)), icon_kind, "{relative}");
            let matching = super::matching_taplo_rules(rules, path.to_str().unwrap());
            assert_eq!(matching.len(), 1, "{relative}");
            let schema_path = matching[0]["schema"]["path"].as_str().unwrap();
            let schema: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(workspace_root.join(schema_path)).unwrap()).unwrap();
            assert_eq!(&schema, super::file_schema(kind), "{relative}");
        }
    }

    #[test]
    fn each_committed_data_file_has_one_schema() {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        validate_taplo_config(&workspace_root).unwrap();
    }

    #[test]
    fn reader_keeps_a_written_item_typed() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let path = root.join("packs/unsourced/gear/sir-squilliam-iv-esq.toml");
        let contents = std::fs::read_to_string(&path).unwrap();
        let SourceFile::Item(item) = read_file(&root, &path, &contents).unwrap() else {
            panic!("item path read as another kind");
        };
        assert_eq!(item.name, "Sir Squilliam IV, Esq.");
        assert_eq!(item.minimum_level, 20);
        assert_eq!(item.effect[0].name, "Maximum Charge Tier V");
    }

    #[test]
    fn missing_images_name_the_file_and_field() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let index = crate::icons::IconIndex::from_data(&root).unwrap();
        for (relative, field) in [
            ("characters/enhancements/archmage/tree.toml", "background"),
            ("characters/classes/fighter.toml", "small_icon"),
            ("characters/classes/fighter.toml", "large_icon"),
        ] {
            let path = root.join(relative);
            let mut parsed: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            parsed[field] = toml::Value::String("MissingImage".into());
            let file = read_file(&root, &path, &toml::to_string(&parsed).unwrap()).unwrap();
            let error = crate::tree::build::validate_file_icons(&root, &path, &file, &index, &mut Default::default())
                .unwrap_err();
            let message = format!("{error:#}");
            assert!(message.contains(relative) && message.contains(field), "{message}");
        }
    }

    #[test]
    fn image_qualification_matches_the_owners_contract() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let index = crate::icons::IconIndex::from_data(&root).unwrap();
        for (relative, contents, valid) in [
            (
                "characters/optional-buffs.toml",
                "[[row]]\nname = 'Example'\ndescription = ''\nicon = 'spells/Fireball'\n",
                true,
            ),
            (
                "characters/optional-buffs.toml",
                "[[row]]\nname = 'Example'\ndescription = ''\nicon = 'enhancements/spellsinger/SSArcaneAid'\n",
                true,
            ),
            (
                "characters/optional-buffs.toml",
                "[[row]]\nname = 'Example'\ndescription = ''\nicon = 'Fireball'\n",
                false,
            ),
            (
                "characters/optional-buffs.toml",
                "[[row]]\nname = 'Example'\ndescription = ''\nicon = 'spells/fireball'\n",
                false,
            ),
            (
                "characters/optional-buffs.toml",
                "[[row]]\nname = 'Example'\ndescription = ''\nicon = 'enhancements/warchanter/SSArcaneAid'\n",
                false,
            ),
            (
                "characters/optional-buffs.toml",
                "[[row]]\nname = 'Example'\ndescription = ''\nicon = '../spells/Fireball'\n",
                false,
            ),
        ] {
            let path = root.join(relative);
            let file = read_file(&root, &path, contents).unwrap();
            let result = crate::tree::build::validate_file_icons(&root, &path, &file, &index, &mut Default::default());
            if valid {
                result.unwrap();
            } else {
                let error = format!("{:#}", result.unwrap_err());
                assert!(error.contains(relative) && error.contains("icon"), "{error}");
            }
        }
        let path = root.join("characters/classes/fighter.toml");
        let mut document: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        document["large_icon"] = toml::Value::String(format!("classes/{}", document["large_icon"].as_str().unwrap()));
        let file = read_file(&root, &path, &toml::to_string(&document).unwrap()).unwrap();
        assert!(crate::tree::build::validate_file_icons(&root, &path, &file, &index, &mut Default::default()).is_err());
    }

    #[test]
    fn explicit_false_defaults_are_rejected_but_evidence_is_preserved() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let path = root.join("packs/unsourced/gear/sir-squilliam-iv-esq.toml");
        let contents = std::fs::read_to_string(&path).unwrap();
        for field in ["is_legacy", "accepts_sentience", "is_minor_artifact"] {
            let error = read_file(&root, &path, &format!("{field} = false\n{contents}")).unwrap_err().to_string();
            assert!(error.contains(field) && error.contains("must omit") && error.contains("sir-squilliam"), "{error}");
        }
        let evidence = format!("{contents}\n[[ddobuilder_correction]]\nkind = 'item'\nfield = 'is_legacy'\nupstream = false\nreason = 'Evidence'\nurl = 'https://ddowiki.com/page/Item:Example'\nread = '2026-10-08'\n");
        read_file(&root, &path, &evidence).unwrap();
    }

    #[test]
    fn typed_icon_references_cover_every_authored_icon() {
        fn authored_icons(
            root: &serde_json::Value,
            definition: &serde_json::Value,
            value: &toml::Value,
            kind: &str,
            icons: &mut Vec<(String, String)>,
        ) {
            let definition = definition["$ref"]
                .as_str()
                .and_then(|reference| root.pointer(reference.trim_start_matches('#')))
                .unwrap_or(definition);
            if let Some(image_kind) = definition["x-image-kind"].as_str() {
                if let Some(image) = value.as_str() {
                    icons.push((if image_kind.is_empty() { kind } else { image_kind }.to_string(), image.to_string()));
                }
            }
            if let Some(fields) = value.as_table() {
                for (field, value) in fields {
                    let kind = match field.as_str() {
                        "stance" => "stances",
                        "option" if kind == "items" => "augments",
                        _ => kind,
                    };
                    if let Some(property) = definition["properties"].get(field) {
                        authored_icons(root, property, value, kind, icons);
                    }
                }
            }
            if let Some(rows) = value.as_array() {
                for row in rows {
                    authored_icons(root, &definition["items"], row, kind, icons);
                }
            }
            for branch in ["anyOf", "allOf", "oneOf"] {
                for definition in definition[branch].as_array().into_iter().flatten() {
                    authored_icons(root, definition, value, kind, icons);
                }
            }
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        for entry in walkdir::WalkDir::new(&root) {
            let entry = entry.unwrap();
            if entry.path().extension().is_none_or(|extension| extension != "toml")
                || super::in_generated_folder(entry.path().strip_prefix(&root).unwrap())
            {
                continue;
            }
            let kind = crate::icons::icon_kind_for_data_path(entry.path().strip_prefix(&root).unwrap())
                .map_or("", |(_, kind)| kind);
            let contents = std::fs::read_to_string(entry.path()).unwrap();
            let file = read_file(&root, entry.path(), &contents).unwrap();
            let mut actual = file
                .icons(kind)
                .into_iter()
                .map(|(kind, icon, _)| (kind.to_string(), icon.to_string()))
                .collect::<Vec<_>>();
            let mut expected = Vec::new();
            let schema = super::file_schema(super::file_kind(&root, entry.path()).unwrap());
            authored_icons(schema, schema, &toml::from_str(&contents).unwrap(), kind, &mut expected);
            assert!(
                expected.iter().all(|(kind, _)| !kind.is_empty()),
                "{}: image owner has no kind",
                entry.path().display()
            );
            actual.sort();
            expected.sort();
            assert_eq!(actual, expected, "{}", entry.path().display());
        }
    }

    #[test]
    fn item_file_reports_wrong_scalar_types_and_all_missing_required_fields() {
        let root = Path::new("data");
        let path = root.join("packs/unsourced/gear/new-item.toml");
        let missing =
            read_file(root, &path, "name = 'New Item'\nminimum_level = 'seventeen'\n").unwrap_err().to_string();
        assert!(missing.contains("item_category"), "{missing}");
        assert!(missing.contains("slot"), "{missing}");
        let wrong_level = read_file(
            root,
            &path,
            "name = 'New Item'\nslot = 'Head'\nitem_category = 'Clothing'\nminimum_level = 'seventeen'\nwiki_url = 'https://ddowiki.com/page/New_Item'\n",
        )
        .unwrap_err()
        .to_string();
        assert!(wrong_level.contains("minimum_level") || wrong_level.contains("integer"), "{wrong_level}");
        let wrong_boolean = read_file(
            root,
            &path,
            "name = 'New Item'\nslot = 'Head'\nis_legacy = 0\nitem_category = 'Clothing'\nminimum_level = 1\nwiki_url = 'https://ddowiki.com/page/New_Item'\n",
        )
        .unwrap_err()
        .to_string();
        assert!(wrong_boolean.contains("is_legacy") || wrong_boolean.contains("boolean"), "{wrong_boolean}");
        let nested = read_file(
            root,
            &path,
            "name = 'New Item'\nslot = 'Head'\nitem_category = 'Clothing'\nminimum_level = 1\n[[modifier]]\n[[socket]]\n",
        )
        .unwrap_err()
        .to_string();
        assert!(nested.contains("socket[0].slot"), "{nested}");
        assert!(nested.contains("modifier[0].effect_type"), "{nested}");
    }

    #[test]
    fn taplo_pattern_matches_one_folder_segment() {
        assert!(taplo_path_matches("data/packs/*/quest.toml", "data/packs/example/quest.toml"));
        assert!(!taplo_path_matches("data/packs/*/quest.toml", "data/packs/example/gear/quest.toml"));
        assert!(taplo_path_matches("data/packs/*/*.toml", "data/packs/example/pack.toml"));
    }

    #[test]
    fn nested_database_fields_are_rejected_before_build() {
        let path = Path::new("packs/unsourced/gear/new-item.toml");
        let parsed: toml::Value =
            toml::from_str("name = 'New Item'\n[[modifier]]\neffect_type = 'Strength'\nsource_id = 4\n").unwrap();
        let error = validate_source_value(path, &parsed).unwrap_err().to_string();
        assert!(error.contains("source_id"), "{error}");
        assert!(error.contains("new-item.toml"), "{error}");
    }

    fn mirrored_arrays(
        schema: &serde_json::Value,
        node: &serde_json::Value,
        arrays: &mut std::collections::BTreeSet<(String, String, String)>,
    ) {
        if let Some(properties) = node["properties"].as_object() {
            for (property_name, property) in properties {
                let rows = &property["items"];
                let row = rows["$ref"]
                    .as_str()
                    .and_then(|reference| reference.strip_prefix("#/$defs/"))
                    .map_or(rows, |name| &schema["$defs"][name]);
                if let Some(table) = row["x-table"].as_str() {
                    let declared = row["x-array"].as_str().unwrap_or_default();
                    arrays.insert((table.to_string(), declared.to_string(), property_name.clone()));
                }
            }
        }
        for definition in node["$defs"].as_object().into_iter().flat_map(|definitions| definitions.values()) {
            mirrored_arrays(schema, definition, arrays);
        }
    }

    #[test]
    fn every_entity_record_declares_citations() {
        let has_citation_array = |schema: &serde_json::Value, node: &serde_json::Value| {
            let citation = &node["properties"]["citation"];
            let items = &citation["items"];
            let record = items["$ref"]
                .as_str()
                .and_then(|reference| reference.strip_prefix("#/$defs/"))
                .map_or(items, |name| &schema["$defs"][name]);
            citation["type"] == "array" && record["x-table"] == "citations" && record["x-array"] == "citation"
        };
        let entities = crate::delta::TABLE_MERGES
            .iter()
            .filter_map(|(table, merge)| matches!(merge, crate::delta::TableMerge::Entity { .. }).then_some(*table))
            .collect::<std::collections::BTreeSet<_>>();
        let mut covered = std::collections::BTreeSet::new();
        let mut missing = Vec::new();
        for (file, json) in super::schema_files().unwrap() {
            let schema: serde_json::Value = serde_json::from_str(&json).unwrap();
            for node in std::iter::once(&schema)
                .chain(schema["$defs"].as_object().into_iter().flat_map(|definitions| definitions.values()))
            {
                let Some(table) = node["x-table"].as_str().filter(|table| entities.contains(table)) else { continue };
                covered.insert(table.to_string());
                if !has_citation_array(&schema, node) {
                    missing.push(format!("{file}: {table} has no citation array"));
                }
            }
        }
        for (kind, table) in super::catalog::PARAMETERIZED_ENTITY_KINDS {
            let schema = super::file_schema(kind);
            for node in std::iter::once(schema)
                .chain(schema["$defs"].as_object().into_iter().flat_map(|definitions| definitions.values()))
            {
                if node["properties"]["name"]["type"] != "string" {
                    continue;
                }
                covered.insert(table.to_string());
                if !has_citation_array(schema, node) {
                    missing.push(format!("{kind}: {table} has no citation array"));
                }
            }
        }
        for table in entities {
            if !covered.contains(table) {
                missing.push(format!("{table} has no declared entity record"));
            }
        }
        assert!(missing.is_empty(), "{}", missing.join("\n"));
    }

    #[test]
    fn annotation_arrays_take_their_names_from_their_mirror_table_declarations() {
        let mut arrays = std::collections::BTreeSet::new();
        for json in super::schema_files().unwrap().values() {
            let schema: serde_json::Value = serde_json::from_str(json).unwrap();
            mirrored_arrays(&schema, &schema, &mut arrays);
        }
        for table in ["citations", "removed_entities", "corrections"] {
            assert!(
                arrays.iter().any(|(mirrored, declared, _)| mirrored == table && !declared.is_empty()),
                "no file array mirrors {table} with a declared array name: {arrays:?}"
            );
        }
        for (table, declared, property_name) in arrays.iter().filter(|(_, declared, _)| !declared.is_empty()) {
            assert_eq!(property_name, declared, "a file array mirroring {table} must be named {declared}");
            let merged_array = crate::delta::TABLE_MERGES.iter().find(|(merged, _)| merged == table).and_then(
                |(_, merge)| match merge {
                    crate::delta::TableMerge::Array { array, .. } | crate::delta::TableMerge::Corrections { array } => {
                        Some(*array)
                    }
                    _ => None,
                },
            );
            assert_eq!(merged_array, Some(declared.as_str()), "TABLE_MERGES must merge {table} as {declared}");
        }
    }

    #[test]
    fn stat_category_rejects_unknown_vocabulary_value() {
        let root = Path::new("data");
        let path = root.join("effects/stats/armor-class.toml");
        let error = read_file(root, &path, "name = 'Armor Class'\ncategory = 'magicall'\n").unwrap_err().to_string();
        assert!(error.contains("category") || error.contains("magicall"), "{error}");
    }
}
