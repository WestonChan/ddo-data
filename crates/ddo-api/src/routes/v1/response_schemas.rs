#![allow(dead_code)]

use utoipa::openapi::schema::{AllOfBuilder, ArrayBuilder, ObjectBuilder, Schema, SchemaType};
use utoipa::openapi::RefOr;
use utoipa::PartialSchema;

pub(crate) fn described_schema<T: PartialSchema>(description: &str) -> RefOr<Schema> {
    match T::schema() {
        RefOr::T(mut schema) => {
            let slot = match &mut schema {
                Schema::Array(array) => &mut array.description,
                Schema::Object(object) => &mut object.description,
                Schema::OneOf(one_of) => &mut one_of.description,
                Schema::AllOf(all_of) => &mut all_of.description,
                Schema::AnyOf(any_of) => &mut any_of.description,
                _ => return AllOfBuilder::new().item(schema).description(Some(description)).into(),
            };
            *slot = Some(description.to_string());
            RefOr::T(schema)
        }
        reference => AllOfBuilder::new().item(reference).description(Some(description)).into(),
    }
}

fn described_field_0() -> RefOr<Schema> {
    described_schema::<i64>("Numeric identifier for this record.")
}
fn described_field_1() -> RefOr<Schema> {
    described_schema::<bool>("Is free to play for this record.")
}
fn described_field_2() -> RefOr<Schema> {
    described_schema::<String>("Display name of this record.")
}
fn described_field_3() -> RefOr<Schema> {
    described_schema::<Vec<AdventurePacksPageResponseAdventurePacksEntry>>("Adventure packs for this record.")
}
fn described_field_4() -> RefOr<Schema> {
    described_schema::<i64>("Maximum rows in this page.")
}
fn described_field_5() -> RefOr<Schema> {
    described_schema::<i64>("Matching rows skipped before this page.")
}
fn described_field_6() -> RefOr<Schema> {
    described_schema::<i64>("Total matching rows before paging.")
}
fn described_field_7() -> RefOr<Schema> {
    ArrayBuilder::new()
        .items(ObjectBuilder::new().schema_type(SchemaType::AnyValue))
        .description(Some("Augments carrying this family or stat."))
        .into()
}
fn described_field_8() -> RefOr<Schema> {
    described_schema::<String>("Chest for this record.")
}
fn described_field_9() -> RefOr<Schema> {
    described_schema::<bool>("Is rare for this record.")
}
fn described_field_10() -> RefOr<Schema> {
    described_schema::<String>("Loot type for this record.")
}
fn described_field_11() -> RefOr<Schema> {
    described_schema::<i64>("Minimum level for this record.")
}
fn described_field_12() -> RefOr<Schema> {
    described_schema::<String>("Slot for this record.")
}
fn described_field_13() -> RefOr<Schema> {
    described_schema::<Vec<AdventurePacksDetailResponseItemsEntry>>("Items carrying this family or stat.")
}
fn described_field_14() -> RefOr<Schema> {
    described_schema::<String>("Family for this record.")
}
fn described_field_15() -> RefOr<Schema> {
    described_schema::<String>("Label for this record.")
}
fn described_field_16() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Qualifier for this record.")).into()
}
fn described_field_17() -> RefOr<Schema> {
    described_schema::<String>("Variant for this record.")
}
fn described_field_18() -> RefOr<Schema> {
    described_schema::<Vec<AugmentSlotTypesPageResponseAugmentSlotTypesEntry>>("Augment slot types for this record.")
}
fn described_field_19() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Adds augment for this record.")).into()
}
fn described_field_20() -> RefOr<Schema> {
    described_schema::<bool>("Choose level for this record.")
}
fn described_field_21() -> RefOr<Schema> {
    ArrayBuilder::new()
        .items(ObjectBuilder::new().schema_type(SchemaType::AnyValue))
        .description(Some("Crafting for this record."))
        .into()
}
fn described_field_22() -> RefOr<Schema> {
    described_schema::<String>("Descriptive text for this record.")
}
fn described_field_23() -> RefOr<Schema> {
    described_schema::<bool>("Dual values for this record.")
}
fn described_field_24() -> RefOr<Schema> {
    ObjectBuilder::new()
        .schema_type(SchemaType::AnyValue)
        .description(Some("Effect description for this record."))
        .into()
}
fn described_field_25() -> RefOr<Schema> {
    described_schema::<String>("Bonus type carried by this link or stat row.")
}
fn described_field_26() -> RefOr<Schema> {
    described_schema::<String>("Stat for this record.")
}
fn described_field_27() -> RefOr<Schema> {
    described_schema::<String>("Stat category for this record.")
}
fn described_field_28() -> RefOr<Schema> {
    described_schema::<i64>("First amount stated by this owner link.")
}
fn described_field_29() -> RefOr<Schema> {
    described_schema::<Vec<EnchantmentStatBonus>>("Stat bonuses derived from this enchantment link.")
}
fn described_field_30() -> RefOr<Schema> {
    described_schema::<i64>("Enchantment id for this record.")
}
fn described_field_31() -> RefOr<Schema> {
    ObjectBuilder::new()
        .schema_type(SchemaType::AnyValue)
        .description(Some("Ladder and ordered steps for this family, if any."))
        .into()
}
fn described_field_32() -> RefOr<Schema> {
    described_schema::<String>("Rendered enchantment line for this owner.")
}
fn described_field_33() -> RefOr<Schema> {
    ObjectBuilder::new()
        .schema_type(SchemaType::AnyValue)
        .description(Some("Second amount stated by this owner link."))
        .into()
}
fn described_field_34() -> RefOr<Schema> {
    described_schema::<Vec<EnchantmentLine>>("Enchantment lines in their owner order.")
}
fn described_field_35() -> RefOr<Schema> {
    described_schema::<bool>("Enter value for this record.")
}
fn described_field_36() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Grants augment for this record.")).into()
}
fn described_field_37() -> RefOr<Schema> {
    described_schema::<String>("Icon for this record.")
}
fn described_field_38() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Level values for this record.")).into()
}
fn described_field_39() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Level values2 for this record.")).into()
}
fn described_field_40() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Levels for this record.")).into()
}
fn described_field_41() -> RefOr<Schema> {
    described_schema::<i64>("Min level for this record.")
}
fn described_field_42() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Set bonus for this record.")).into()
}
fn described_field_43() -> RefOr<Schema> {
    described_schema::<Vec<String>>("Slots for this record.")
}
fn described_field_44() -> RefOr<Schema> {
    described_schema::<bool>("Suppress set bonus for this record.")
}
fn described_field_45() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Weapon class for this record.")).into()
}
fn described_field_46() -> RefOr<Schema> {
    described_schema::<Vec<AugmentsPageResponseAugmentsEntry>>("Augments carrying this family or stat.")
}
fn described_field_47() -> RefOr<Schema> {
    described_schema::<Option<String>>("Adds augment for this record.")
}
fn described_field_48() -> RefOr<Schema> {
    described_schema::<Option<String>>("Chest for this record.")
}
fn described_field_49() -> RefOr<Schema> {
    described_schema::<String>("Wiki url for this record.")
}
fn described_field_50() -> RefOr<Schema> {
    described_schema::<Vec<AugmentsDetailResponseAdventurePacksEntry>>("Adventure packs for this record.")
}
fn described_field_51() -> RefOr<Schema> {
    described_schema::<String>("Ingredient for this record.")
}
fn described_field_52() -> RefOr<Schema> {
    described_schema::<i64>("Quantity for this record.")
}
fn described_field_53() -> RefOr<Schema> {
    described_schema::<String>("Tier for this record.")
}
fn described_field_54() -> RefOr<Schema> {
    described_schema::<Vec<AugmentsDetailResponseCraftingEntryCostEntry>>("Cost for this record.")
}
fn described_field_55() -> RefOr<Schema> {
    described_schema::<String>("Option for this record.")
}
fn described_field_56() -> RefOr<Schema> {
    described_schema::<String>("System for this record.")
}
fn described_field_57() -> RefOr<Schema> {
    described_schema::<Vec<AugmentsDetailResponseCraftingEntry>>("Crafting for this record.")
}
fn described_field_58() -> RefOr<Schema> {
    described_schema::<Option<String>>("Effect description for this record.")
}
fn described_field_59() -> RefOr<Schema> {
    described_schema::<Vec<EnchantmentStatBonus>>("Stat bonuses derived from this enchantment link.")
}
fn described_field_60() -> RefOr<Schema> {
    described_schema::<Option<String>>("Descriptive text for this record.")
}
fn described_field_61() -> RefOr<Schema> {
    described_schema::<Vec<EnchantmentLine>>("Enchantment lines in their owner order.")
}
fn described_field_62() -> RefOr<Schema> {
    described_schema::<Option<String>>("Icon for this record.")
}
fn described_field_63() -> RefOr<Schema> {
    described_schema::<Option<Vec<i64>>>("Level values for this record.")
}
fn described_field_64() -> RefOr<Schema> {
    described_schema::<Option<Vec<i64>>>("Levels for this record.")
}
fn described_field_65() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Min level for this record.")
}
fn described_field_66() -> RefOr<Schema> {
    described_schema::<String>("Amount type for this record.")
}
fn described_field_67() -> RefOr<Schema> {
    described_schema::<Option<Vec<i64>>>("Amounts for this record.")
}
fn described_field_68() -> RefOr<Schema> {
    described_schema::<bool>("Apply as item effect for this record.")
}
fn described_field_69() -> RefOr<Schema> {
    described_schema::<String>("Bonus for this record.")
}
fn described_field_70() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Cap for this record.")).into()
}
fn described_field_71() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Damage for this record.")).into()
}
fn described_field_72() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Dice bonus for this record.")).into()
}
fn described_field_73() -> RefOr<Schema> {
    described_schema::<Option<String>>("Dice damage for this record.")
}
fn described_field_74() -> RefOr<Schema> {
    described_schema::<Option<Vec<i64>>>("Dice number for this record.")
}
fn described_field_75() -> RefOr<Schema> {
    described_schema::<Option<Vec<i64>>>("Dice sides for this record.")
}
fn described_field_76() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Display name for this record.")).into()
}
fn described_field_77() -> RefOr<Schema> {
    described_schema::<String>("Effect type for this record.")
}
fn described_field_78() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Extra types for this record.")).into()
}
fn described_field_79() -> RefOr<Schema> {
    described_schema::<bool>("Is item specific for this record.")
}
fn described_field_80() -> RefOr<Schema> {
    described_schema::<bool>("Percent for this record.")
}
fn described_field_81() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Rank for this record.")).into()
}
fn described_field_82() -> RefOr<Schema> {
    described_schema::<i64>("Group index for this record.")
}
fn described_field_83() -> RefOr<Schema> {
    described_schema::<String>("Group kind for this record.")
}
fn described_field_84() -> RefOr<Schema> {
    described_schema::<Vec<String>>("Items carrying this family or stat.")
}
fn described_field_85() -> RefOr<Schema> {
    described_schema::<String>("Req type for this record.")
}
fn described_field_86() -> RefOr<Schema> {
    ObjectBuilder::new()
        .schema_type(SchemaType::AnyValue)
        .description(Some("First amount stated by this owner link."))
        .into()
}
fn described_field_87() -> RefOr<Schema> {
    described_schema::<Vec<AugmentsDetailResponseModifiersEntryRequirementsEntry>>("Requirements for this record.")
}
fn described_field_88() -> RefOr<Schema> {
    described_schema::<i64>("Sort order for this record.")
}
fn described_field_89() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Stack source for this record.")).into()
}
fn described_field_90() -> RefOr<Schema> {
    described_schema::<Option<Vec<String>>>("Targets for this record.")
}
fn described_field_91() -> RefOr<Schema> {
    described_schema::<Vec<AugmentsDetailResponseModifiersEntry>>("Modifiers for this record.")
}
fn described_field_92() -> RefOr<Schema> {
    described_schema::<Vec<String>>("Difficulties for this record.")
}
fn described_field_93() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Epic level for this record.")
}
fn described_field_94() -> RefOr<Schema> {
    described_schema::<bool>("Is raid for this record.")
}
fn described_field_95() -> RefOr<Schema> {
    described_schema::<i64>("Level for this record.")
}
fn described_field_96() -> RefOr<Schema> {
    described_schema::<String>("Pack for this record.")
}
fn described_field_97() -> RefOr<Schema> {
    described_schema::<Option<String>>("Patron for this record.")
}
fn described_field_98() -> RefOr<Schema> {
    described_schema::<Vec<AugmentsDetailResponseQuestsEntry>>("Quests for this record.")
}
fn described_field_99() -> RefOr<Schema> {
    described_schema::<Option<String>>("Set bonus for this record.")
}
fn described_field_100() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Character level for this record.")).into()
}
fn described_field_101() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Cost for this record.")).into()
}
fn described_field_102() -> RefOr<Schema> {
    described_schema::<String>("Resource kind used to select the detail route.")
}
fn described_field_103() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Tier for this record.")).into()
}
fn described_field_104() -> RefOr<Schema> {
    described_schema::<Vec<AugmentsDetailResponseSourcesEntry>>("Sources for this record.")
}
fn described_field_105() -> RefOr<Schema> {
    described_schema::<bool>("Stacks with self for this record.")
}
fn described_field_106() -> RefOr<Schema> {
    described_schema::<Vec<BonusTypesPageResponseBonusTypesEntry>>("Bonus types occurring with this family or stat.")
}
fn described_field_107() -> RefOr<Schema> {
    described_schema::<Vec<String>>("Alignments for this record.")
}
fn described_field_108() -> RefOr<Schema> {
    described_schema::<Vec<f64>>("Bab for this record.")
}
fn described_field_109() -> RefOr<Schema> {
    described_schema::<Option<String>>("Base class for this record.")
}
fn described_field_110() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Base class id for this record.")
}
fn described_field_111() -> RefOr<Schema> {
    described_schema::<Vec<String>>("Casting stats for this record.")
}
fn described_field_112() -> RefOr<Schema> {
    described_schema::<Vec<String>>("Class specific feat types for this record.")
}
fn described_field_113() -> RefOr<Schema> {
    described_schema::<String>("Fortitude for this record.")
}
fn described_field_114() -> RefOr<Schema> {
    described_schema::<i64>("Hit points for this record.")
}
fn described_field_115() -> RefOr<Schema> {
    described_schema::<String>("Large icon for this record.")
}
fn described_field_116() -> RefOr<Schema> {
    described_schema::<bool>("Not heroic for this record.")
}
fn described_field_117() -> RefOr<Schema> {
    described_schema::<String>("Reflex for this record.")
}
fn described_field_118() -> RefOr<Schema> {
    described_schema::<i64>("Skill points for this record.")
}
fn described_field_119() -> RefOr<Schema> {
    described_schema::<String>("Small icon for this record.")
}
fn described_field_120() -> RefOr<Schema> {
    described_schema::<Vec<i64>>("Spell points per level for this record.")
}
fn described_field_121() -> RefOr<Schema> {
    described_schema::<String>("Will for this record.")
}
fn described_field_122() -> RefOr<Schema> {
    described_schema::<Vec<ClassesPageResponseClassesEntry>>("Classes for this record.")
}
fn described_field_123() -> RefOr<Schema> {
    described_schema::<Vec<String>>("Auto buy skills for this record.")
}
fn described_field_124() -> RefOr<Schema> {
    described_schema::<i64>("Feat id for this record.")
}
fn described_field_125() -> RefOr<Schema> {
    described_schema::<Vec<ClassesDetailResponseAutomaticFeatsEntry>>("Automatic feats for this record.")
}
fn described_field_126() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Base class for this record.")).into()
}
fn described_field_127() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Base class id for this record.")).into()
}
fn described_field_128() -> RefOr<Schema> {
    described_schema::<Vec<String>>("Class skills for this record.")
}
fn described_field_129() -> RefOr<Schema> {
    described_schema::<bool>("Auto populate for this record.")
}
fn described_field_130() -> RefOr<Schema> {
    described_schema::<String>("Feat type for this record.")
}
fn described_field_131() -> RefOr<Schema> {
    described_schema::<bool>("Singular for this record.")
}
fn described_field_132() -> RefOr<Schema> {
    described_schema::<Option<Vec<String>>>("Update list for this record.")
}
fn described_field_133() -> RefOr<Schema> {
    described_schema::<Vec<ClassesDetailResponseFeatSlotsEntry>>("Feat slots for this record.")
}
fn described_field_134() -> RefOr<Schema> {
    described_schema::<String>("Acquire for this record.")
}
fn described_field_135() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Max times acquire for this record.")
}
fn described_field_136() -> RefOr<Schema> {
    described_schema::<Vec<ClassesDetailResponseFeatsEntry>>("Feats for this record.")
}
fn described_field_137() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Spell slots for this record.")).into()
}
fn described_field_138() -> RefOr<Schema> {
    described_schema::<i64>("Cost for this record.")
}
fn described_field_139() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Max caster level for this record.")
}
fn described_field_140() -> RefOr<Schema> {
    described_schema::<i64>("Spell id for this record.")
}
fn described_field_141() -> RefOr<Schema> {
    described_schema::<i64>("Spell level for this record.")
}
fn described_field_142() -> RefOr<Schema> {
    described_schema::<Vec<ClassesDetailResponseSpellsEntry>>("Spells for this record.")
}
fn described_field_143() -> RefOr<Schema> {
    ArrayBuilder::new()
        .items(ObjectBuilder::new().schema_type(SchemaType::AnyValue))
        .description(Some("Modifiers for this record."))
        .into()
}
fn described_field_144() -> RefOr<Schema> {
    described_schema::<Option<String>>("School for this record.")
}
fn described_field_145() -> RefOr<Schema> {
    described_schema::<Vec<ClickiesPageResponseClickiesEntry>>("Clickies for this record.")
}
fn described_field_146() -> RefOr<Schema> {
    described_schema::<Vec<String>>("Families for this record.")
}
fn described_field_147() -> RefOr<Schema> {
    described_schema::<i64>("Ingredient count for this record.")
}
fn described_field_148() -> RefOr<Schema> {
    described_schema::<String>("Npc for this record.")
}
fn described_field_149() -> RefOr<Schema> {
    described_schema::<Option<String>>("Pack for this record.")
}
fn described_field_150() -> RefOr<Schema> {
    described_schema::<String>("Page for this record.")
}
fn described_field_151() -> RefOr<Schema> {
    described_schema::<i64>("Recipe count for this record.")
}
fn described_field_152() -> RefOr<Schema> {
    described_schema::<Vec<CraftingSystemsPageResponseCraftingSystemsEntry>>("Crafting systems for this record.")
}
fn described_field_153() -> RefOr<Schema> {
    described_schema::<String>("Bind for this record.")
}
fn described_field_154() -> RefOr<Schema> {
    described_schema::<String>("Source for this record.")
}
fn described_field_155() -> RefOr<Schema> {
    described_schema::<Vec<CraftingSystemsDetailResponseIngredientsEntry>>("Ingredients for this record.")
}
fn described_field_156() -> RefOr<Schema> {
    described_schema::<Vec<CraftingSystemsDetailResponseRecipesEntryAugmentsEntry>>(
        "Augments carrying this family or stat.",
    )
}
fn described_field_157() -> RefOr<Schema> {
    described_schema::<Vec<CraftingSystemsDetailResponseRecipesEntryCostEntry>>("Cost for this record.")
}
fn described_field_158() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Grants slot for this record.")).into()
}
fn described_field_159() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Note for this record.")).into()
}
fn described_field_160() -> RefOr<Schema> {
    described_schema::<Vec<CraftingSystemsDetailResponseRecipesEntry>>("Recipes for this record.")
}
fn described_field_161() -> RefOr<Schema> {
    described_schema::<String>("Category for this record.")
}
fn described_field_162() -> RefOr<Schema> {
    described_schema::<Vec<DamageTypesPageResponseDamageTypesEntry>>("Damage types for this record.")
}
fn described_field_163() -> RefOr<Schema> {
    described_schema::<i64>("Number of distinct augments carrying this entry.")
}
fn described_field_164() -> RefOr<Schema> {
    described_schema::<i64>("Number of distinct nonlegacy items carrying this entry.")
}
fn described_field_165() -> RefOr<Schema> {
    described_schema::<Vec<EnchantmentsPageResponseEnchantmentsEntryBonusTypesEntry>>(
        "Bonus types occurring with this family or stat.",
    )
}
fn described_field_166() -> RefOr<Schema> {
    described_schema::<String>("Detail route for this row kind and id.")
}
fn described_field_167() -> RefOr<Schema> {
    described_schema::<i64>("Number of distinct sets carrying this entry.")
}
fn described_field_168() -> RefOr<Schema> {
    described_schema::<Vec<EnchantmentsPageResponseEnchantmentsEntry>>("Enchantment lines in their owner order.")
}
fn described_field_169() -> RefOr<Schema> {
    described_schema::<i64>("Number of amount slots in the family template.")
}
fn described_field_170() -> RefOr<Schema> {
    described_schema::<Vec<EnchantmentsDetailResponseAugmentsAugmentsEntry>>("Augments carrying this family or stat.")
}
fn described_field_171() -> RefOr<Schema> {
    described_schema::<EnchantmentsDetailResponseAugments>("Augments carrying this family or stat.")
}
fn described_field_172() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Default value for this record.")).into()
}
fn described_field_173() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Default value2 for this record.")).into()
}
fn described_field_174() -> RefOr<Schema> {
    described_schema::<String>("Template used to render an owner description.")
}
fn described_field_175() -> RefOr<Schema> {
    described_schema::<Vec<EnchantmentsDetailResponseItemsItemsEntry>>("Items carrying this family or stat.")
}
fn described_field_176() -> RefOr<Schema> {
    described_schema::<EnchantmentsDetailResponseItems>("Items carrying this family or stat.")
}
fn described_field_177() -> RefOr<Schema> {
    ArrayBuilder::new()
        .items(ObjectBuilder::new().schema_type(SchemaType::AnyValue))
        .description(Some("Set tiers carrying this family or stat."))
        .into()
}
fn described_field_178() -> RefOr<Schema> {
    described_schema::<EnchantmentsDetailResponseSetTiers>("Set tiers carrying this family or stat.")
}
fn described_field_179() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Stacking note for this record.")).into()
}
fn described_field_180() -> RefOr<Schema> {
    described_schema::<i64>("Amount slot read by this stat row, or zero for a constant.")
}
fn described_field_181() -> RefOr<Schema> {
    ObjectBuilder::new()
        .schema_type(SchemaType::AnyValue)
        .description(Some("Bonus type carried by this link or stat row."))
        .into()
}
fn described_field_182() -> RefOr<Schema> {
    ObjectBuilder::new()
        .schema_type(SchemaType::AnyValue)
        .description(Some("Final amount of a static stat row."))
        .into()
}
fn described_field_183() -> RefOr<Schema> {
    described_schema::<String>("Rounding rule applied after scaling.")
}
fn described_field_184() -> RefOr<Schema> {
    described_schema::<f64>("Multiplier applied to a linked amount.")
}
fn described_field_185() -> RefOr<Schema> {
    described_schema::<Vec<EnchantmentsDetailResponseStatsEntry>>("Stat rules derived from this family.")
}
fn described_field_186() -> RefOr<Schema> {
    described_schema::<String>("Template used to render an owner line.")
}
fn described_field_187() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Wiki url for this record.")).into()
}
fn described_field_188() -> RefOr<Schema> {
    described_schema::<String>("Background for this record.")
}
fn described_field_189() -> RefOr<Schema> {
    described_schema::<i64>("Enhancement count for this record.")
}
fn described_field_190() -> RefOr<Schema> {
    described_schema::<bool>("Is legacy for this record.")
}
fn described_field_191() -> RefOr<Schema> {
    described_schema::<Vec<EnhancementTreesPageResponseEnhancementTreesEntryRequirementsEntry>>(
        "Requirements for this record.",
    )
}
fn described_field_192() -> RefOr<Schema> {
    described_schema::<i64>("Version for this record.")
}
fn described_field_193() -> RefOr<Schema> {
    described_schema::<Vec<EnhancementTreesPageResponseEnhancementTreesEntry>>("Enhancement trees for this record.")
}
fn described_field_194() -> RefOr<Schema> {
    described_schema::<Vec<String>>("Arrows for this record.")
}
fn described_field_195() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Attack for this record.")).into()
}
fn described_field_196() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Cooldown seconds for this record.")).into()
}
fn described_field_197() -> RefOr<Schema> {
    described_schema::<Vec<i64>>("Cost per rank for this record.")
}
fn described_field_198() -> RefOr<Schema> {
    described_schema::<Vec<i64>>("Amount for this record.")
}
fn described_field_199() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Base class level for this record.")).into()
}
fn described_field_200() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Class level for this record.")).into()
}
fn described_field_201() -> RefOr<Schema> {
    described_schema::<String>("Dc type for this record.")
}
fn described_field_202() -> RefOr<Schema> {
    described_schema::<String>("Dc versus for this record.")
}
fn described_field_203() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Mod ability for this record.")).into()
}
fn described_field_204() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Other for this record.")).into()
}
fn described_field_205() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Skill for this record.")).into()
}
fn described_field_206() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Tactical for this record.")).into()
}
fn described_field_207() -> RefOr<Schema> {
    described_schema::<Vec<EnhancementTreesDetailResponseEnhancementsEntryDcsEntry>>("Dcs for this record.")
}
fn described_field_208() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Duration seconds for this record.")).into()
}
fn described_field_209() -> RefOr<Schema> {
    ArrayBuilder::new()
        .items(ObjectBuilder::new().schema_type(SchemaType::AnyValue))
        .description(Some("Exclusions for this record."))
        .into()
}
fn described_field_210() -> RefOr<Schema> {
    ArrayBuilder::new()
        .items(ObjectBuilder::new().schema_type(SchemaType::AnyValue))
        .description(Some("Follow on modifiers for this record."))
        .into()
}
fn described_field_211() -> RefOr<Schema> {
    described_schema::<String>("Internal name for this record.")
}
fn described_field_212() -> RefOr<Schema> {
    described_schema::<bool>("Is clickie for this record.")
}
fn described_field_213() -> RefOr<Schema> {
    described_schema::<bool>("Is tier5 for this record.")
}
fn described_field_214() -> RefOr<Schema> {
    described_schema::<i64>("Min spent for this record.")
}
fn described_field_215() -> RefOr<Schema> {
    described_schema::<Vec<i64>>("Amounts for this record.")
}
fn described_field_216() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Dice damage for this record.")).into()
}
fn described_field_217() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Dice number for this record.")).into()
}
fn described_field_218() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Dice sides for this record.")).into()
}
fn described_field_219() -> RefOr<Schema> {
    ArrayBuilder::new()
        .items(ObjectBuilder::new().schema_type(SchemaType::AnyValue))
        .description(Some("Requirements for this record."))
        .into()
}
fn described_field_220() -> RefOr<Schema> {
    described_schema::<Option<String>>("Stack source for this record.")
}
fn described_field_221() -> RefOr<Schema> {
    described_schema::<Vec<EnhancementTreesDetailResponseEnhancementsEntryModifiersEntry>>("Modifiers for this record.")
}
fn described_field_222() -> RefOr<Schema> {
    described_schema::<i64>("Ranks for this record.")
}
fn described_field_223() -> RefOr<Schema> {
    described_schema::<Option<String>>("First amount stated by this owner link.")
}
fn described_field_224() -> RefOr<Schema> {
    described_schema::<Vec<EnhancementTreesDetailResponseEnhancementsEntryRequirementsEntry>>(
        "Requirements for this record.",
    )
}
fn described_field_225() -> RefOr<Schema> {
    ArrayBuilder::new()
        .items(ObjectBuilder::new().schema_type(SchemaType::AnyValue))
        .description(Some("Selections for this record."))
        .into()
}
fn described_field_226() -> RefOr<Schema> {
    ArrayBuilder::new()
        .items(ObjectBuilder::new().schema_type(SchemaType::AnyValue))
        .description(Some("Stances for this record."))
        .into()
}
fn described_field_227() -> RefOr<Schema> {
    ArrayBuilder::new()
        .items(ObjectBuilder::new().schema_type(SchemaType::AnyValue))
        .description(Some("This attack modifiers for this record."))
        .into()
}
fn described_field_228() -> RefOr<Schema> {
    described_schema::<i64>("X for this record.")
}
fn described_field_229() -> RefOr<Schema> {
    described_schema::<i64>("Y for this record.")
}
fn described_field_230() -> RefOr<Schema> {
    described_schema::<Vec<EnhancementTreesDetailResponseEnhancementsEntry>>("Enhancements for this record.")
}
fn described_field_231() -> RefOr<Schema> {
    described_schema::<Vec<EnhancementTreesDetailResponseRequirementsEntry>>("Requirements for this record.")
}
fn described_field_232() -> RefOr<Schema> {
    described_schema::<Vec<EquipmentSlotsPageResponseEquipmentSlotsEntry>>("Equipment slots for this record.")
}
fn described_field_233() -> RefOr<Schema> {
    described_schema::<Vec<EventsPageResponseEventsEntry>>("Events for this record.")
}
fn described_field_234() -> RefOr<Schema> {
    described_schema::<Vec<EventsDetailResponseItemsEntry>>("Items carrying this family or stat.")
}
fn described_field_235() -> RefOr<Schema> {
    described_schema::<bool>("Auto acquire ignores requirements for this record.")
}
fn described_field_236() -> RefOr<Schema> {
    described_schema::<Vec<String>>("Groups for this record.")
}
fn described_field_237() -> RefOr<Schema> {
    ObjectBuilder::new()
        .schema_type(SchemaType::AnyValue)
        .description(Some("Max times acquire for this record."))
        .into()
}
fn described_field_238() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Source id for this record.")).into()
}
fn described_field_239() -> RefOr<Schema> {
    described_schema::<String>("Source kind for this record.")
}
fn described_field_240() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Source name for this record.")).into()
}
fn described_field_241() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Sphere for this record.")).into()
}
fn described_field_242() -> RefOr<Schema> {
    described_schema::<Vec<FeatsPageResponseFeatsEntry>>("Feats for this record.")
}
fn described_field_243() -> RefOr<Schema> {
    described_schema::<i64>("Cooldown seconds for this record.")
}
fn described_field_244() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Duration seconds for this record.")
}
fn described_field_245() -> RefOr<Schema> {
    described_schema::<Option<FeatsDetailResponseAttack>>("Attack for this record.")
}
fn described_field_246() -> RefOr<Schema> {
    ObjectBuilder::new()
        .schema_type(SchemaType::AnyValue)
        .description(Some("Items carrying this family or stat."))
        .into()
}
fn described_field_247() -> RefOr<Schema> {
    described_schema::<String>("First amount stated by this owner link.")
}
fn described_field_248() -> RefOr<Schema> {
    described_schema::<Vec<FeatsDetailResponseAutoAcquireRequirementsEntry>>(
        "Auto acquire requirements for this record.",
    )
}
fn described_field_249() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Descriptive text for this record.")).into()
}
fn described_field_250() -> RefOr<Schema> {
    described_schema::<Vec<FeatsDetailResponseBonusesEntry>>("Stat bonuses derived from this enchantment link.")
}
fn described_field_251() -> RefOr<Schema> {
    described_schema::<Vec<FeatsDetailResponseConditionalGroupsEntryRequirementsEntry>>("Requirements for this record.")
}
fn described_field_252() -> RefOr<Schema> {
    described_schema::<Vec<FeatsDetailResponseConditionalGroupsEntry>>("Conditional groups for this record.")
}
fn described_field_253() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Amount for this record.")).into()
}
fn described_field_254() -> RefOr<Schema> {
    described_schema::<String>("Other for this record.")
}
fn described_field_255() -> RefOr<Schema> {
    described_schema::<String>("Skill for this record.")
}
fn described_field_256() -> RefOr<Schema> {
    described_schema::<Vec<FeatsDetailResponseDcsEntry>>("Dcs for this record.")
}
fn described_field_257() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Amounts for this record.")).into()
}
fn described_field_258() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Bonus for this record.")).into()
}
fn described_field_259() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Targets for this record.")).into()
}
fn described_field_260() -> RefOr<Schema> {
    described_schema::<Vec<FeatsDetailResponseFollowOnModifiersEntry>>("Follow on modifiers for this record.")
}
fn described_field_261() -> RefOr<Schema> {
    described_schema::<Option<Vec<f64>>>("Amounts for this record.")
}
fn described_field_262() -> RefOr<Schema> {
    described_schema::<Option<String>>("Display name for this record.")
}
fn described_field_263() -> RefOr<Schema> {
    described_schema::<Option<Vec<String>>>("Extra types for this record.")
}
fn described_field_264() -> RefOr<Schema> {
    described_schema::<Vec<FeatsDetailResponseModifiersEntryRequirementsEntry>>("Requirements for this record.")
}
fn described_field_265() -> RefOr<Schema> {
    described_schema::<Vec<FeatsDetailResponseModifiersEntry>>("Modifiers for this record.")
}
fn described_field_266() -> RefOr<Schema> {
    described_schema::<Option<Vec<String>>>("Items carrying this family or stat.")
}
fn described_field_267() -> RefOr<Schema> {
    described_schema::<Vec<FeatsDetailResponseRequirementsEntry>>("Requirements for this record.")
}
fn described_field_268() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Source id for this record.")
}
fn described_field_269() -> RefOr<Schema> {
    described_schema::<Option<String>>("Source name for this record.")
}
fn described_field_270() -> RefOr<Schema> {
    described_schema::<bool>("Auto controlled for this record.")
}
fn described_field_271() -> RefOr<Schema> {
    described_schema::<Option<String>>("Group name for this record.")
}
fn described_field_272() -> RefOr<Schema> {
    described_schema::<Option<Vec<String>>>("Incompatible for this record.")
}
fn described_field_273() -> RefOr<Schema> {
    described_schema::<Vec<FeatsDetailResponseStancesEntry>>("Stances for this record.")
}
fn described_field_274() -> RefOr<Schema> {
    described_schema::<Vec<FeatsDetailResponseSubItemsEntry>>("Sub items for this record.")
}
fn described_field_275() -> RefOr<Schema> {
    described_schema::<Vec<FeatsDetailResponseThisAttackModifiersEntry>>("This attack modifiers for this record.")
}
fn described_field_276() -> RefOr<Schema> {
    described_schema::<String>("Menu for this record.")
}
fn described_field_277() -> RefOr<Schema> {
    described_schema::<Vec<FiligreesPageResponseFiligreesEntryModifiersEntry>>("Modifiers for this record.")
}
fn described_field_278() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Set id for this record.")
}
fn described_field_279() -> RefOr<Schema> {
    described_schema::<Option<String>>("Set name for this record.")
}
fn described_field_280() -> RefOr<Schema> {
    described_schema::<Vec<FiligreesPageResponseFiligreesEntry>>("Filigrees for this record.")
}
fn described_field_281() -> RefOr<Schema> {
    described_schema::<i64>("Guild level for this record.")
}
fn described_field_282() -> RefOr<Schema> {
    described_schema::<Vec<String>>("Targets for this record.")
}
fn described_field_283() -> RefOr<Schema> {
    described_schema::<Vec<GuildBuffsPageResponseGuildBuffsEntryModifiersEntry>>("Modifiers for this record.")
}
fn described_field_284() -> RefOr<Schema> {
    described_schema::<Vec<GuildBuffsPageResponseGuildBuffsEntry>>("Guild buffs for this record.")
}
fn described_field_285() -> RefOr<Schema> {
    ObjectBuilder::new()
        .schema_type(SchemaType::AnyValue)
        .description(Some("Enhancement bonus for this record."))
        .into()
}
fn described_field_286() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Item type for this record.")).into()
}
fn described_field_287() -> RefOr<Schema> {
    described_schema::<Vec<ItemsPageResponseItemsEntry>>("Items carrying this family or stat.")
}
fn described_field_288() -> RefOr<Schema> {
    described_schema::<bool>("Accepts sentience for this record.")
}
fn described_field_289() -> RefOr<Schema> {
    described_schema::<Option<String>>("Loot type for this record.")
}
fn described_field_290() -> RefOr<Schema> {
    described_schema::<Vec<ItemsDetailResponseAdventurePacksEntry>>("Adventure packs for this record.")
}
fn described_field_291() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Adamantine body for this record.")
}
fn described_field_292() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Arcane spell failure for this record.")
}
fn described_field_293() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Armor bonus for this record.")
}
fn described_field_294() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Armor check penalty for this record.")
}
fn described_field_295() -> RefOr<Schema> {
    described_schema::<String>("Armor type for this record.")
}
fn described_field_296() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Damage reduction for this record.")
}
fn described_field_297() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Max dex bonus for this record.")
}
fn described_field_298() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Mithral body for this record.")
}
fn described_field_299() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Shield bonus for this record.")
}
fn described_field_300() -> RefOr<Schema> {
    described_schema::<Option<ItemsDetailResponseArmor>>("Armor for this record.")
}
fn described_field_301() -> RefOr<Schema> {
    described_schema::<Option<String>>("Bonus type carried by this link or stat row.")
}
fn described_field_302() -> RefOr<Schema> {
    described_schema::<Vec<EnchantmentStatBonus>>("Stat bonuses derived from this enchantment link.")
}
fn described_field_303() -> RefOr<Schema> {
    described_schema::<Vec<EnchantmentLine>>("Enchantment lines in their owner order.")
}
fn described_field_304() -> RefOr<Schema> {
    described_schema::<Option<String>>("Grants slot for this record.")
}
fn described_field_305() -> RefOr<Schema> {
    described_schema::<Option<String>>("Amount type for this record.")
}
fn described_field_306() -> RefOr<Schema> {
    described_schema::<Vec<f64>>("Amounts for this record.")
}
fn described_field_307() -> RefOr<Schema> {
    described_schema::<Vec<ItemsDetailResponseAugmentSlotsEntryOptionsEntryModifiersEntry>>(
        "Modifiers for this record.",
    )
}
fn described_field_308() -> RefOr<Schema> {
    described_schema::<Vec<ItemsDetailResponseAugmentSlotsEntryOptionsEntrySetsEntry>>("Sets for this record.")
}
fn described_field_309() -> RefOr<Schema> {
    described_schema::<Vec<ItemsDetailResponseAugmentSlotsEntryOptionsEntry>>("Options for this record.")
}
fn described_field_310() -> RefOr<Schema> {
    described_schema::<Option<String>>("Qualifier for this record.")
}
fn described_field_311() -> RefOr<Schema> {
    described_schema::<i64>("Slot type id for this record.")
}
fn described_field_312() -> RefOr<Schema> {
    described_schema::<Vec<ItemsDetailResponseAugmentSlotsEntry>>("Augment slots for this record.")
}
fn described_field_313() -> RefOr<Schema> {
    described_schema::<Vec<ItemsDetailResponseChallengePacksEntry>>("Challenge packs for this record.")
}
fn described_field_314() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Clickie id for this record.")
}
fn described_field_315() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Spell id for this record.")).into()
}
fn described_field_316() -> RefOr<Schema> {
    described_schema::<Vec<ItemsDetailResponseClickiesEntry>>("Clickies for this record.")
}
fn described_field_317() -> RefOr<Schema> {
    described_schema::<Vec<ItemsDetailResponseCraftingSystemsEntry>>("Crafting systems for this record.")
}
fn described_field_318() -> RefOr<Schema> {
    described_schema::<Option<String>>("Drop location for this record.")
}
fn described_field_319() -> RefOr<Schema> {
    described_schema::<Vec<EnchantmentStatBonus>>("Stat bonuses derived from this enchantment link.")
}
fn described_field_320() -> RefOr<Schema> {
    described_schema::<i64>("Rank for this record.")
}
fn described_field_321() -> RefOr<Schema> {
    described_schema::<Option<EnchantmentLadderPosition>>("Ladder and ordered steps for this family, if any.")
}
fn described_field_322() -> RefOr<Schema> {
    described_schema::<Option<i64>>("First amount stated by this owner link.")
}
fn described_field_323() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Second amount stated by this owner link.")
}
fn described_field_324() -> RefOr<Schema> {
    described_schema::<Vec<EnchantmentLine>>("Enchantment lines in their owner order.")
}
fn described_field_325() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Enhancement bonus for this record.")
}
fn described_field_326() -> RefOr<Schema> {
    described_schema::<Vec<ItemsDetailResponseEventsEntry>>("Events for this record.")
}
fn described_field_327() -> RefOr<Schema> {
    described_schema::<bool>("Is minor artifact for this record.")
}
fn described_field_328() -> RefOr<Schema> {
    described_schema::<Option<String>>("Item type for this record.")
}
fn described_field_329() -> RefOr<Schema> {
    described_schema::<Option<String>>("Material for this record.")
}
fn described_field_330() -> RefOr<Schema> {
    described_schema::<Vec<ItemsDetailResponseModifiersEntry>>("Modifiers for this record.")
}
fn described_field_331() -> RefOr<Schema> {
    described_schema::<Vec<ItemsDetailResponseQuestChainsEntry>>("Quest chains for this record.")
}
fn described_field_332() -> RefOr<Schema> {
    described_schema::<Vec<ItemsDetailResponseQuestsEntry>>("Quests for this record.")
}
fn described_field_333() -> RefOr<Schema> {
    described_schema::<Option<String>>("Race required for this record.")
}
fn described_field_334() -> RefOr<Schema> {
    described_schema::<Option<String>>("Tier for this record.")
}
fn described_field_335() -> RefOr<Schema> {
    described_schema::<Vec<ItemsDetailResponseSagasEntry>>("Sagas for this record.")
}
fn described_field_336() -> RefOr<Schema> {
    described_schema::<Option<ItemsDetailResponseSet>>("Set for this record.")
}
fn described_field_337() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Character level for this record.")
}
fn described_field_338() -> RefOr<Schema> {
    described_schema::<Option<String>>("Cost for this record.")
}
fn described_field_339() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Numeric identifier for this record.")
}
fn described_field_340() -> RefOr<Schema> {
    described_schema::<Option<String>>("Wiki url for this record.")
}
fn described_field_341() -> RefOr<Schema> {
    described_schema::<Vec<ItemsDetailResponseSourcesEntry>>("Sources for this record.")
}
fn described_field_342() -> RefOr<Schema> {
    described_schema::<i64>("Character level for this record.")
}
fn described_field_343() -> RefOr<Schema> {
    described_schema::<Vec<ItemsDetailResponseStarterRewardsEntry>>("Starter rewards for this record.")
}
fn described_field_344() -> RefOr<Schema> {
    described_schema::<String>("Location for this record.")
}
fn described_field_345() -> RefOr<Schema> {
    described_schema::<Vec<ItemsDetailResponseVendorsEntry>>("Vendors for this record.")
}
fn described_field_346() -> RefOr<Schema> {
    described_schema::<Option<String>>("Attack modifier for this record.")
}
fn described_field_347() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Base dice bonus for this record.")
}
fn described_field_348() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Base dice count for this record.")
}
fn described_field_349() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Base dice sides for this record.")
}
fn described_field_350() -> RefOr<Schema> {
    described_schema::<Option<String>>("Critical for this record.")
}
fn described_field_351() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Critical multiplier for this record.")
}
fn described_field_352() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Critical threat range for this record.")
}
fn described_field_353() -> RefOr<Schema> {
    described_schema::<Option<String>>("Damage for this record.")
}
fn described_field_354() -> RefOr<Schema> {
    described_schema::<Option<String>>("Damage modifier for this record.")
}
fn described_field_355() -> RefOr<Schema> {
    described_schema::<Option<f64>>("Damage multiplier for this record.")
}
fn described_field_356() -> RefOr<Schema> {
    described_schema::<Vec<String>>("Dr bypass for this record.")
}
fn described_field_357() -> RefOr<Schema> {
    described_schema::<String>("Handedness for this record.")
}
fn described_field_358() -> RefOr<Schema> {
    described_schema::<Option<String>>("Proficiency for this record.")
}
fn described_field_359() -> RefOr<Schema> {
    described_schema::<String>("Weapon type for this record.")
}
fn described_field_360() -> RefOr<Schema> {
    described_schema::<Option<ItemsDetailResponseWeapon>>("Weapon for this record.")
}
fn described_field_361() -> RefOr<Schema> {
    described_schema::<Vec<OptionalBuffsPageResponseOptionalBuffsEntryModifiersEntryRequirementsEntry>>(
        "Requirements for this record.",
    )
}
fn described_field_362() -> RefOr<Schema> {
    described_schema::<Vec<OptionalBuffsPageResponseOptionalBuffsEntryModifiersEntry>>("Modifiers for this record.")
}
fn described_field_363() -> RefOr<Schema> {
    described_schema::<Vec<OptionalBuffsPageResponseOptionalBuffsEntry>>("Optional buffs for this record.")
}
fn described_field_364() -> RefOr<Schema> {
    described_schema::<Vec<PatronsPageResponsePatronsEntry>>("Patrons for this record.")
}
fn described_field_365() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Pack for this record.")).into()
}
fn described_field_366() -> RefOr<Schema> {
    described_schema::<i64>("Quest count for this record.")
}
fn described_field_367() -> RefOr<Schema> {
    described_schema::<i64>("Reward count for this record.")
}
fn described_field_368() -> RefOr<Schema> {
    described_schema::<Vec<QuestChainsPageResponseQuestChainsEntry>>("Quest chains for this record.")
}
fn described_field_369() -> RefOr<Schema> {
    described_schema::<Vec<QuestChainsDetailResponseQuestsEntry>>("Quests for this record.")
}
fn described_field_370() -> RefOr<Schema> {
    described_schema::<Vec<QuestChainsDetailResponseRewardsEntry>>("Rewards for this record.")
}
fn described_field_371() -> RefOr<Schema> {
    described_schema::<String>("Bestowed by for this record.")
}
fn described_field_372() -> RefOr<Schema> {
    described_schema::<i64>("Epic level for this record.")
}
fn described_field_373() -> RefOr<Schema> {
    described_schema::<Option<String>>("Epic name for this record.")
}
fn described_field_374() -> RefOr<Schema> {
    described_schema::<i64>("Favor for this record.")
}
fn described_field_375() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Flagging for this record.")).into()
}
fn described_field_376() -> RefOr<Schema> {
    described_schema::<bool>("Is challenge for this record.")
}
fn described_field_377() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Legendary level for this record.")
}
fn described_field_378() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Max level for this record.")).into()
}
fn described_field_379() -> RefOr<Schema> {
    described_schema::<String>("Patron for this record.")
}
fn described_field_380() -> RefOr<Schema> {
    described_schema::<String>("Zone for this record.")
}
fn described_field_381() -> RefOr<Schema> {
    described_schema::<Vec<QuestsPageResponseQuestsEntry>>("Quests for this record.")
}
fn described_field_382() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Epic level for this record.")).into()
}
fn described_field_383() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Epic name for this record.")).into()
}
fn described_field_384() -> RefOr<Schema> {
    ArrayBuilder::new()
        .items(ObjectBuilder::new().schema_type(SchemaType::AnyValue))
        .description(Some("Items carrying this family or stat."))
        .into()
}
fn described_field_385() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Legendary level for this record.")).into()
}
fn described_field_386() -> RefOr<Schema> {
    described_schema::<Vec<QuestsDetailResponseQuestChainsEntry>>("Quest chains for this record.")
}
fn described_field_387() -> RefOr<Schema> {
    ArrayBuilder::new()
        .items(ObjectBuilder::new().schema_type(SchemaType::AnyValue))
        .description(Some("Sagas for this record."))
        .into()
}
fn described_field_388() -> RefOr<Schema> {
    described_schema::<Vec<i64>>("Build points for this record.")
}
fn described_field_389() -> RefOr<Schema> {
    described_schema::<Option<String>>("Iconic class for this record.")
}
fn described_field_390() -> RefOr<Schema> {
    described_schema::<bool>("Is construct for this record.")
}
fn described_field_391() -> RefOr<Schema> {
    described_schema::<bool>("No past life for this record.")
}
fn described_field_392() -> RefOr<Schema> {
    described_schema::<String>("Short name for this record.")
}
fn described_field_393() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Skill points for this record.")).into()
}
fn described_field_394() -> RefOr<Schema> {
    described_schema::<String>("Starting world for this record.")
}
fn described_field_395() -> RefOr<Schema> {
    described_schema::<Vec<RacesPageResponseRacesEntry>>("Races for this record.")
}
fn described_field_396() -> RefOr<Schema> {
    described_schema::<i64>("Modifier for this record.")
}
fn described_field_397() -> RefOr<Schema> {
    described_schema::<Vec<RacesDetailResponseAbilityModifiersEntry>>("Ability modifiers for this record.")
}
fn described_field_398() -> RefOr<Schema> {
    ArrayBuilder::new()
        .items(ObjectBuilder::new().schema_type(SchemaType::AnyValue))
        .description(Some("Auto buy skills for this record."))
        .into()
}
fn described_field_399() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Update list for this record.")).into()
}
fn described_field_400() -> RefOr<Schema> {
    described_schema::<Vec<RacesDetailResponseFeatSlotsEntry>>("Feat slots for this record.")
}
fn described_field_401() -> RefOr<Schema> {
    described_schema::<Vec<RacesDetailResponseFeatsEntry>>("Feats for this record.")
}
fn described_field_402() -> RefOr<Schema> {
    described_schema::<Vec<RacesDetailResponseGrantedFeatsEntry>>("Granted feats for this record.")
}
fn described_field_403() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Iconic class for this record.")).into()
}
fn described_field_404() -> RefOr<Schema> {
    described_schema::<Vec<SagasPageResponseSagasEntry>>("Sagas for this record.")
}
fn described_field_405() -> RefOr<Schema> {
    described_schema::<Vec<SagasDetailResponseQuestsEntry>>("Quests for this record.")
}
fn described_field_406() -> RefOr<Schema> {
    described_schema::<Vec<SagasDetailResponseRewardsEntry>>("Rewards for this record.")
}
fn described_field_407() -> RefOr<Schema> {
    described_schema::<Vec<SentientGemsPageResponseSentientGemsEntry>>("Sentient gems for this record.")
}
fn described_field_408() -> RefOr<Schema> {
    described_schema::<bool>("Is filigree set for this record.")
}
fn described_field_409() -> RefOr<Schema> {
    described_schema::<i64>("Tier count for this record.")
}
fn described_field_410() -> RefOr<Schema> {
    described_schema::<Vec<SetsPageResponseSetsEntry>>("Sets for this record.")
}
fn described_field_411() -> RefOr<Schema> {
    described_schema::<Vec<SetsDetailResponseAugmentsEntry>>("Augments carrying this family or stat.")
}
fn described_field_412() -> RefOr<Schema> {
    described_schema::<Vec<SetsDetailResponseFiligreesEntryModifiersEntry>>("Modifiers for this record.")
}
fn described_field_413() -> RefOr<Schema> {
    described_schema::<i64>("Set id for this record.")
}
fn described_field_414() -> RefOr<Schema> {
    described_schema::<String>("Set name for this record.")
}
fn described_field_415() -> RefOr<Schema> {
    described_schema::<Vec<SetsDetailResponseFiligreesEntry>>("Filigrees for this record.")
}
fn described_field_416() -> RefOr<Schema> {
    described_schema::<Vec<SetsDetailResponseItemsEntry>>("Items carrying this family or stat.")
}
fn described_field_417() -> RefOr<Schema> {
    described_schema::<Vec<EnchantmentStatBonus>>("Stat bonuses derived from this enchantment link.")
}
fn described_field_418() -> RefOr<Schema> {
    described_schema::<Vec<EnchantmentLine>>("Enchantment lines in their owner order.")
}
fn described_field_419() -> RefOr<Schema> {
    described_schema::<i64>("Equipped count for this record.")
}
fn described_field_420() -> RefOr<Schema> {
    described_schema::<Vec<SetsDetailResponseTiersEntryModifiersEntry>>("Modifiers for this record.")
}
fn described_field_421() -> RefOr<Schema> {
    described_schema::<Vec<SetsDetailResponseTiersEntry>>("Tiers for this record.")
}
fn described_field_422() -> RefOr<Schema> {
    described_schema::<i64>("Max caster level for this record.")
}
fn described_field_423() -> RefOr<Schema> {
    described_schema::<Vec<String>>("Metamagics for this record.")
}
fn described_field_424() -> RefOr<Schema> {
    described_schema::<Vec<String>>("Schools for this record.")
}
fn described_field_425() -> RefOr<Schema> {
    described_schema::<Vec<SpellsPageResponseSpellsEntry>>("Spells for this record.")
}
fn described_field_426() -> RefOr<Schema> {
    described_schema::<String>("Class for this record.")
}
fn described_field_427() -> RefOr<Schema> {
    described_schema::<i64>("Class id for this record.")
}
fn described_field_428() -> RefOr<Schema> {
    described_schema::<Vec<SpellsDetailResponseClassesEntry>>("Classes for this record.")
}
fn described_field_429() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Base dice bonus for this record.")).into()
}
fn described_field_430() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Base dice number for this record.")).into()
}
fn described_field_431() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Base dice sides for this record.")).into()
}
fn described_field_432() -> RefOr<Schema> {
    described_schema::<i64>("Bonus dice bonus for this record.")
}
fn described_field_433() -> RefOr<Schema> {
    described_schema::<i64>("Bonus dice number for this record.")
}
fn described_field_434() -> RefOr<Schema> {
    described_schema::<i64>("Bonus dice sides for this record.")
}
fn described_field_435() -> RefOr<Schema> {
    described_schema::<String>("Damage for this record.")
}
fn described_field_436() -> RefOr<Schema> {
    ObjectBuilder::new()
        .schema_type(SchemaType::AnyValue)
        .description(Some("Per caster levels for this record."))
        .into()
}
fn described_field_437() -> RefOr<Schema> {
    described_schema::<String>("Spell power for this record.")
}
fn described_field_438() -> RefOr<Schema> {
    described_schema::<Vec<SpellsDetailResponseDamageEntry>>("Damage for this record.")
}
fn described_field_439() -> RefOr<Schema> {
    described_schema::<bool>("Casting stat mod for this record.")
}
fn described_field_440() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Mod abilities for this record.")).into()
}
fn described_field_441() -> RefOr<Schema> {
    described_schema::<Vec<SpellsDetailResponseDcsEntry>>("Dcs for this record.")
}
fn described_field_442() -> RefOr<Schema> {
    described_schema::<String>("Group name for this record.")
}
fn described_field_443() -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(SchemaType::AnyValue).description(Some("Incompatible for this record.")).into()
}
fn described_field_444() -> RefOr<Schema> {
    described_schema::<Vec<StancesPageResponseStancesEntryRequirementsEntry>>("Requirements for this record.")
}
fn described_field_445() -> RefOr<Schema> {
    described_schema::<Vec<StancesPageResponseStancesEntry>>("Stances for this record.")
}
fn described_field_446() -> RefOr<Schema> {
    described_schema::<Vec<StatsPageResponseStatsEntry>>("Stat rules derived from this family.")
}
fn described_field_447() -> RefOr<Schema> {
    described_schema::<Vec<StatsDetailResponseAugmentsAugmentsEntry>>("Augments carrying this family or stat.")
}
fn described_field_448() -> RefOr<Schema> {
    described_schema::<StatsDetailResponseAugments>("Augments carrying this family or stat.")
}
fn described_field_449() -> RefOr<Schema> {
    described_schema::<Vec<StatsDetailResponseItemsItemsEntry>>("Items carrying this family or stat.")
}
fn described_field_450() -> RefOr<Schema> {
    described_schema::<StatsDetailResponseItems>("Items carrying this family or stat.")
}
fn described_field_451() -> RefOr<Schema> {
    described_schema::<Vec<StatsDetailResponseSetTiersSetTiersEntry>>("Set tiers carrying this family or stat.")
}
fn described_field_452() -> RefOr<Schema> {
    described_schema::<StatsDetailResponseSetTiers>("Set tiers carrying this family or stat.")
}
fn described_field_453() -> RefOr<Schema> {
    described_schema::<Vec<VendorsPageResponseVendorsEntry>>("Vendors for this record.")
}
fn described_field_454() -> RefOr<Schema> {
    described_schema::<String>("Cost for this record.")
}
fn described_field_455() -> RefOr<Schema> {
    described_schema::<Vec<VendorsDetailResponseItemsEntry>>("Items carrying this family or stat.")
}
fn described_field_456() -> RefOr<Schema> {
    described_schema::<bool>("Is shield for this record.")
}
fn described_field_457() -> RefOr<Schema> {
    described_schema::<Vec<WeaponTypesPageResponseWeaponTypesEntry>>("Weapon types for this record.")
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnchantmentStatBonus {
    #[schema(schema_with = schema_stat_name)]
    pub(crate) stat: String,
    #[schema(schema_with = schema_stat_category)]
    pub(crate) stat_category: String,
    #[schema(schema_with = schema_stat_bonus_type)]
    pub(crate) bonus_type: String,
    #[schema(schema_with = schema_stat_value)]
    pub(crate) value: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnchantmentLadderPosition {
    #[schema(schema_with = schema_ladder_id)]
    pub(crate) id: i64,
    #[schema(schema_with = schema_ladder_name)]
    pub(crate) name: String,
    #[schema(schema_with = schema_ladder_rank)]
    pub(crate) rank: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnchantmentLine {
    #[schema(schema_with = schema_line_id)]
    pub(crate) enchantment_id: i64,
    #[schema(schema_with = schema_line_name)]
    pub(crate) name: String,
    #[schema(schema_with = schema_line_ladder)]
    pub(crate) ladder: Option<EnchantmentLadderPosition>,
    #[schema(schema_with = schema_line_text)]
    pub(crate) text: String,
    #[schema(schema_with = schema_line_description)]
    pub(crate) description: Option<String>,
    #[schema(schema_with = schema_line_value)]
    pub(crate) value: Option<i64>,
    #[schema(schema_with = schema_line_value2)]
    pub(crate) value2: Option<i64>,
    #[schema(schema_with = schema_line_bonus_type)]
    pub(crate) bonus_type: Option<String>,
    #[schema(schema_with = schema_line_bonuses)]
    pub(crate) bonuses: Vec<EnchantmentStatBonus>,
}

fn schema_stat_name() -> RefOr<Schema> {
    described_schema::<String>("Stat affected by this enchantment.")
}
fn schema_stat_category() -> RefOr<Schema> {
    described_schema::<String>("Category of the affected stat.")
}
fn schema_stat_bonus_type() -> RefOr<Schema> {
    described_schema::<String>("Fixed or linked type of this stat bonus.")
}
fn schema_stat_value() -> RefOr<Schema> {
    described_schema::<i64>("Final amount after defaults, scaling and rounding.")
}
fn schema_ladder_id() -> RefOr<Schema> {
    described_schema::<i64>("Identifier of this ladder.")
}
fn schema_ladder_name() -> RefOr<Schema> {
    described_schema::<String>("Display name of this ladder.")
}
fn schema_ladder_rank() -> RefOr<Schema> {
    described_schema::<i64>("Rank of this family within the ladder.")
}
fn schema_line_id() -> RefOr<Schema> {
    described_schema::<i64>("Identifier of the shared enchantment family.")
}
fn schema_line_name() -> RefOr<Schema> {
    described_schema::<String>("Display name of the shared family.")
}
fn schema_line_ladder() -> RefOr<Schema> {
    described_schema::<Option<EnchantmentLadderPosition>>("Ladder membership and rank, if any.")
}
fn schema_line_text() -> RefOr<Schema> {
    described_schema::<String>("Line rendered from the family template and effective amounts.")
}
fn schema_line_description() -> RefOr<Schema> {
    described_schema::<Option<String>>("Description rendered from the family template, if any.")
}
fn schema_line_value() -> RefOr<Schema> {
    described_schema::<Option<i64>>("First amount supplied by this owner link.")
}
fn schema_line_value2() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Second amount supplied by this owner link.")
}
fn schema_line_bonus_type() -> RefOr<Schema> {
    described_schema::<Option<String>>("Link type when the family template has a type slot.")
}
fn schema_line_bonuses() -> RefOr<Schema> {
    described_schema::<Vec<EnchantmentStatBonus>>("Stat bonuses derived from this owner link.")
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AdventurePacksPageResponseAdventurePacksEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_1)]
    pub(crate) is_free_to_play: bool,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AdventurePacksPageResponse {
    #[schema(schema_with = described_field_3)]
    pub(crate) adventure_packs: Vec<AdventurePacksPageResponseAdventurePacksEntry>,
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AdventurePacksDetailResponseItemsEntry {
    #[schema(schema_with = described_field_8)]
    pub(crate) chest: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_10)]
    pub(crate) loot_type: String,
    #[schema(schema_with = described_field_11)]
    pub(crate) minimum_level: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_12)]
    pub(crate) slot: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AdventurePacksDetailResponse {
    #[schema(schema_with = described_field_7)]
    pub(crate) augments: Vec<serde_json::Value>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_1)]
    pub(crate) is_free_to_play: bool,
    #[schema(schema_with = described_field_13)]
    pub(crate) items: Vec<AdventurePacksDetailResponseItemsEntry>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AugmentSlotTypesPageResponseAugmentSlotTypesEntry {
    #[schema(schema_with = described_field_14)]
    pub(crate) family: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_15)]
    pub(crate) label: String,
    #[schema(schema_with = described_field_16)]
    pub(crate) qualifier: Option<serde_json::Value>,
    #[schema(schema_with = described_field_17)]
    pub(crate) variant: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AugmentSlotTypesPageResponse {
    #[schema(schema_with = described_field_18)]
    pub(crate) augment_slot_types: Vec<AugmentSlotTypesPageResponseAugmentSlotTypesEntry>,
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AugmentsPageResponseAugmentsEntry {
    #[schema(schema_with = described_field_19)]
    pub(crate) adds_augment: Option<serde_json::Value>,
    #[schema(schema_with = described_field_20)]
    pub(crate) choose_level: bool,
    #[schema(schema_with = described_field_21)]
    pub(crate) crafting: Vec<serde_json::Value>,
    #[schema(schema_with = described_field_22)]
    pub(crate) description: String,
    #[schema(schema_with = described_field_23)]
    pub(crate) dual_values: bool,
    #[schema(schema_with = described_field_24)]
    pub(crate) effect_description: Option<serde_json::Value>,
    #[schema(schema_with = described_field_34)]
    pub(crate) enchantments: Vec<EnchantmentLine>,
    #[schema(schema_with = described_field_35)]
    pub(crate) enter_value: bool,
    #[schema(schema_with = described_field_14)]
    pub(crate) family: String,
    #[schema(schema_with = described_field_36)]
    pub(crate) grants_augment: Option<serde_json::Value>,
    #[schema(schema_with = described_field_37)]
    pub(crate) icon: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_38)]
    pub(crate) level_values: Option<serde_json::Value>,
    #[schema(schema_with = described_field_39)]
    pub(crate) level_values2: Option<serde_json::Value>,
    #[schema(schema_with = described_field_40)]
    pub(crate) levels: Option<serde_json::Value>,
    #[schema(schema_with = described_field_41)]
    pub(crate) min_level: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_42)]
    pub(crate) set_bonus: Option<serde_json::Value>,
    #[schema(schema_with = described_field_43)]
    pub(crate) slots: Vec<String>,
    #[schema(schema_with = described_field_44)]
    pub(crate) suppress_set_bonus: bool,
    #[schema(schema_with = described_field_45)]
    pub(crate) weapon_class: Option<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AugmentsPageResponse {
    #[schema(schema_with = described_field_46)]
    pub(crate) augments: Vec<AugmentsPageResponseAugmentsEntry>,
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AugmentsDetailResponseAdventurePacksEntry {
    #[schema(schema_with = described_field_48)]
    pub(crate) chest: Option<String>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_10)]
    pub(crate) loot_type: String,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_49)]
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AugmentsDetailResponseCraftingEntryCostEntry {
    #[schema(schema_with = described_field_51)]
    pub(crate) ingredient: String,
    #[schema(schema_with = described_field_52)]
    pub(crate) quantity: i64,
    #[schema(schema_with = described_field_53)]
    pub(crate) tier: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AugmentsDetailResponseCraftingEntry {
    #[schema(schema_with = described_field_54)]
    pub(crate) cost: Vec<AugmentsDetailResponseCraftingEntryCostEntry>,
    #[schema(schema_with = described_field_55)]
    pub(crate) option: String,
    #[schema(schema_with = described_field_56)]
    pub(crate) system: String,
    #[schema(schema_with = described_field_53)]
    pub(crate) tier: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AugmentsDetailResponseModifiersEntryRequirementsEntry {
    #[schema(schema_with = described_field_82)]
    pub(crate) group_index: i64,
    #[schema(schema_with = described_field_83)]
    pub(crate) group_kind: String,
    #[schema(schema_with = described_field_84)]
    pub(crate) items: Vec<String>,
    #[schema(schema_with = described_field_85)]
    pub(crate) req_type: String,
    #[schema(schema_with = described_field_86)]
    pub(crate) value: Option<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AugmentsDetailResponseModifiersEntry {
    #[schema(schema_with = described_field_66)]
    pub(crate) amount_type: String,
    #[schema(schema_with = described_field_67)]
    pub(crate) amounts: Option<Vec<i64>>,
    #[schema(schema_with = described_field_68)]
    pub(crate) apply_as_item_effect: bool,
    #[schema(schema_with = described_field_69)]
    pub(crate) bonus: String,
    #[schema(schema_with = described_field_25)]
    pub(crate) bonus_type: String,
    #[schema(schema_with = described_field_70)]
    pub(crate) cap: Option<serde_json::Value>,
    #[schema(schema_with = described_field_71)]
    pub(crate) damage: Option<serde_json::Value>,
    #[schema(schema_with = described_field_72)]
    pub(crate) dice_bonus: Option<serde_json::Value>,
    #[schema(schema_with = described_field_73)]
    pub(crate) dice_damage: Option<String>,
    #[schema(schema_with = described_field_74)]
    pub(crate) dice_number: Option<Vec<i64>>,
    #[schema(schema_with = described_field_75)]
    pub(crate) dice_sides: Option<Vec<i64>>,
    #[schema(schema_with = described_field_76)]
    pub(crate) display_name: Option<serde_json::Value>,
    #[schema(schema_with = described_field_77)]
    pub(crate) effect_type: String,
    #[schema(schema_with = described_field_78)]
    pub(crate) extra_types: Option<serde_json::Value>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_79)]
    pub(crate) is_item_specific: bool,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_80)]
    pub(crate) percent: bool,
    #[schema(schema_with = described_field_81)]
    pub(crate) rank: Option<serde_json::Value>,
    #[schema(schema_with = described_field_87)]
    pub(crate) requirements: Vec<AugmentsDetailResponseModifiersEntryRequirementsEntry>,
    #[schema(schema_with = described_field_88)]
    pub(crate) sort_order: i64,
    #[schema(schema_with = described_field_89)]
    pub(crate) stack_source: Option<serde_json::Value>,
    #[schema(schema_with = described_field_90)]
    pub(crate) targets: Option<Vec<String>>,
    #[schema(schema_with = described_field_86)]
    pub(crate) value: Option<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AugmentsDetailResponseQuestsEntry {
    #[schema(schema_with = described_field_48)]
    pub(crate) chest: Option<String>,
    #[schema(schema_with = described_field_92)]
    pub(crate) difficulties: Vec<String>,
    #[schema(schema_with = described_field_93)]
    pub(crate) epic_level: Option<i64>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_1)]
    pub(crate) is_free_to_play: bool,
    #[schema(schema_with = described_field_94)]
    pub(crate) is_raid: bool,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_95)]
    pub(crate) level: i64,
    #[schema(schema_with = described_field_10)]
    pub(crate) loot_type: String,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_96)]
    pub(crate) pack: String,
    #[schema(schema_with = described_field_97)]
    pub(crate) patron: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AugmentsDetailResponseSourcesEntry {
    #[schema(schema_with = described_field_100)]
    pub(crate) character_level: Option<serde_json::Value>,
    #[schema(schema_with = described_field_48)]
    pub(crate) chest: Option<String>,
    #[schema(schema_with = described_field_101)]
    pub(crate) cost: Option<serde_json::Value>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_102)]
    pub(crate) kind: String,
    #[schema(schema_with = described_field_10)]
    pub(crate) loot_type: String,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_103)]
    pub(crate) tier: Option<serde_json::Value>,
    #[schema(schema_with = described_field_49)]
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AugmentsDetailResponse {
    #[schema(schema_with = described_field_47)]
    pub(crate) adds_augment: Option<String>,
    #[schema(schema_with = described_field_50)]
    pub(crate) adventure_packs: Vec<AugmentsDetailResponseAdventurePacksEntry>,
    #[schema(schema_with = described_field_20)]
    pub(crate) choose_level: bool,
    #[schema(schema_with = described_field_57)]
    pub(crate) crafting: Vec<AugmentsDetailResponseCraftingEntry>,
    #[schema(schema_with = described_field_22)]
    pub(crate) description: String,
    #[schema(schema_with = described_field_23)]
    pub(crate) dual_values: bool,
    #[schema(schema_with = described_field_58)]
    pub(crate) effect_description: Option<String>,
    #[schema(schema_with = described_field_61)]
    pub(crate) enchantments: Vec<EnchantmentLine>,
    #[schema(schema_with = described_field_35)]
    pub(crate) enter_value: bool,
    #[schema(schema_with = described_field_14)]
    pub(crate) family: String,
    #[schema(schema_with = described_field_36)]
    pub(crate) grants_augment: Option<serde_json::Value>,
    #[schema(schema_with = described_field_62)]
    pub(crate) icon: Option<String>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_63)]
    pub(crate) level_values: Option<Vec<i64>>,
    #[schema(schema_with = described_field_39)]
    pub(crate) level_values2: Option<serde_json::Value>,
    #[schema(schema_with = described_field_64)]
    pub(crate) levels: Option<Vec<i64>>,
    #[schema(schema_with = described_field_65)]
    pub(crate) min_level: Option<i64>,
    #[schema(schema_with = described_field_91)]
    pub(crate) modifiers: Vec<AugmentsDetailResponseModifiersEntry>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_98)]
    pub(crate) quests: Vec<AugmentsDetailResponseQuestsEntry>,
    #[schema(schema_with = described_field_99)]
    pub(crate) set_bonus: Option<String>,
    #[schema(schema_with = described_field_43)]
    pub(crate) slots: Vec<String>,
    #[schema(schema_with = described_field_104)]
    pub(crate) sources: Vec<AugmentsDetailResponseSourcesEntry>,
    #[schema(schema_with = described_field_44)]
    pub(crate) suppress_set_bonus: bool,
    #[schema(schema_with = described_field_45)]
    pub(crate) weapon_class: Option<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct BonusTypesPageResponseBonusTypesEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_105)]
    pub(crate) stacks_with_self: bool,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct BonusTypesPageResponse {
    #[schema(schema_with = described_field_106)]
    pub(crate) bonus_types: Vec<BonusTypesPageResponseBonusTypesEntry>,
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ClassesPageResponseClassesEntry {
    #[schema(schema_with = described_field_107)]
    pub(crate) alignments: Vec<String>,
    #[schema(schema_with = described_field_108)]
    pub(crate) bab: Vec<f64>,
    #[schema(schema_with = described_field_109)]
    pub(crate) base_class: Option<String>,
    #[schema(schema_with = described_field_110)]
    pub(crate) base_class_id: Option<i64>,
    #[schema(schema_with = described_field_111)]
    pub(crate) casting_stats: Vec<String>,
    #[schema(schema_with = described_field_112)]
    pub(crate) class_specific_feat_types: Vec<String>,
    #[schema(schema_with = described_field_22)]
    pub(crate) description: String,
    #[schema(schema_with = described_field_113)]
    pub(crate) fortitude: String,
    #[schema(schema_with = described_field_114)]
    pub(crate) hit_points: i64,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_115)]
    pub(crate) large_icon: String,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_116)]
    pub(crate) not_heroic: bool,
    #[schema(schema_with = described_field_117)]
    pub(crate) reflex: String,
    #[schema(schema_with = described_field_118)]
    pub(crate) skill_points: i64,
    #[schema(schema_with = described_field_119)]
    pub(crate) small_icon: String,
    #[schema(schema_with = described_field_120)]
    pub(crate) spell_points_per_level: Vec<i64>,
    #[schema(schema_with = described_field_121)]
    pub(crate) will: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ClassesPageResponse {
    #[schema(schema_with = described_field_122)]
    pub(crate) classes: Vec<ClassesPageResponseClassesEntry>,
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ClassesDetailResponseAutomaticFeatsEntry {
    #[schema(schema_with = described_field_124)]
    pub(crate) feat_id: i64,
    #[schema(schema_with = described_field_95)]
    pub(crate) level: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ClassesDetailResponseFeatSlotsEntry {
    #[schema(schema_with = described_field_129)]
    pub(crate) auto_populate: bool,
    #[schema(schema_with = described_field_130)]
    pub(crate) feat_type: String,
    #[schema(schema_with = described_field_95)]
    pub(crate) level: i64,
    #[schema(schema_with = described_field_131)]
    pub(crate) singular: bool,
    #[schema(schema_with = described_field_132)]
    pub(crate) update_list: Option<Vec<String>>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ClassesDetailResponseFeatsEntry {
    #[schema(schema_with = described_field_134)]
    pub(crate) acquire: String,
    #[schema(schema_with = described_field_22)]
    pub(crate) description: String,
    #[schema(schema_with = described_field_37)]
    pub(crate) icon: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_135)]
    pub(crate) max_times_acquire: Option<i64>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ClassesDetailResponseSpellsEntry {
    #[schema(schema_with = described_field_138)]
    pub(crate) cost: i64,
    #[schema(schema_with = described_field_139)]
    pub(crate) max_caster_level: Option<i64>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_140)]
    pub(crate) spell_id: i64,
    #[schema(schema_with = described_field_141)]
    pub(crate) spell_level: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ClassesDetailResponse {
    #[schema(schema_with = described_field_107)]
    pub(crate) alignments: Vec<String>,
    #[schema(schema_with = described_field_123)]
    pub(crate) auto_buy_skills: Vec<String>,
    #[schema(schema_with = described_field_125)]
    pub(crate) automatic_feats: Vec<ClassesDetailResponseAutomaticFeatsEntry>,
    #[schema(schema_with = described_field_108)]
    pub(crate) bab: Vec<f64>,
    #[schema(schema_with = described_field_126)]
    pub(crate) base_class: Option<serde_json::Value>,
    #[schema(schema_with = described_field_127)]
    pub(crate) base_class_id: Option<serde_json::Value>,
    #[schema(schema_with = described_field_111)]
    pub(crate) casting_stats: Vec<String>,
    #[schema(schema_with = described_field_128)]
    pub(crate) class_skills: Vec<String>,
    #[schema(schema_with = described_field_112)]
    pub(crate) class_specific_feat_types: Vec<String>,
    #[schema(schema_with = described_field_22)]
    pub(crate) description: String,
    #[schema(schema_with = described_field_133)]
    pub(crate) feat_slots: Vec<ClassesDetailResponseFeatSlotsEntry>,
    #[schema(schema_with = described_field_136)]
    pub(crate) feats: Vec<ClassesDetailResponseFeatsEntry>,
    #[schema(schema_with = described_field_113)]
    pub(crate) fortitude: String,
    #[schema(schema_with = described_field_114)]
    pub(crate) hit_points: i64,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_115)]
    pub(crate) large_icon: String,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_116)]
    pub(crate) not_heroic: bool,
    #[schema(schema_with = described_field_117)]
    pub(crate) reflex: String,
    #[schema(schema_with = described_field_118)]
    pub(crate) skill_points: i64,
    #[schema(schema_with = described_field_119)]
    pub(crate) small_icon: String,
    #[schema(schema_with = described_field_120)]
    pub(crate) spell_points_per_level: Vec<i64>,
    #[schema(schema_with = described_field_137)]
    pub(crate) spell_slots: serde_json::Value,
    #[schema(schema_with = described_field_142)]
    pub(crate) spells: Vec<ClassesDetailResponseSpellsEntry>,
    #[schema(schema_with = described_field_121)]
    pub(crate) will: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ClickiesPageResponseClickiesEntry {
    #[schema(schema_with = described_field_22)]
    pub(crate) description: String,
    #[schema(schema_with = described_field_37)]
    pub(crate) icon: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_143)]
    pub(crate) modifiers: Vec<serde_json::Value>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_144)]
    pub(crate) school: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ClickiesPageResponse {
    #[schema(schema_with = described_field_145)]
    pub(crate) clickies: Vec<ClickiesPageResponseClickiesEntry>,
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct CraftingSystemsPageResponseCraftingSystemsEntry {
    #[schema(schema_with = described_field_146)]
    pub(crate) families: Vec<String>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_147)]
    pub(crate) ingredient_count: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_148)]
    pub(crate) npc: String,
    #[schema(schema_with = described_field_149)]
    pub(crate) pack: Option<String>,
    #[schema(schema_with = described_field_150)]
    pub(crate) page: String,
    #[schema(schema_with = described_field_151)]
    pub(crate) recipe_count: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct CraftingSystemsPageResponse {
    #[schema(schema_with = described_field_152)]
    pub(crate) crafting_systems: Vec<CraftingSystemsPageResponseCraftingSystemsEntry>,
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct CraftingSystemsDetailResponseIngredientsEntry {
    #[schema(schema_with = described_field_153)]
    pub(crate) bind: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_154)]
    pub(crate) source: String,
    #[schema(schema_with = described_field_53)]
    pub(crate) tier: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct CraftingSystemsDetailResponseRecipesEntryAugmentsEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_41)]
    pub(crate) min_level: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct CraftingSystemsDetailResponseRecipesEntryCostEntry {
    #[schema(schema_with = described_field_51)]
    pub(crate) ingredient: String,
    #[schema(schema_with = described_field_52)]
    pub(crate) quantity: i64,
    #[schema(schema_with = described_field_53)]
    pub(crate) tier: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct CraftingSystemsDetailResponseRecipesEntry {
    #[schema(schema_with = described_field_156)]
    pub(crate) augments: Vec<CraftingSystemsDetailResponseRecipesEntryAugmentsEntry>,
    #[schema(schema_with = described_field_157)]
    pub(crate) cost: Vec<CraftingSystemsDetailResponseRecipesEntryCostEntry>,
    #[schema(schema_with = described_field_158)]
    pub(crate) grants_slot: Option<serde_json::Value>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_159)]
    pub(crate) note: Option<serde_json::Value>,
    #[schema(schema_with = described_field_55)]
    pub(crate) option: String,
    #[schema(schema_with = described_field_12)]
    pub(crate) slot: String,
    #[schema(schema_with = described_field_53)]
    pub(crate) tier: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct CraftingSystemsDetailResponse {
    #[schema(schema_with = described_field_146)]
    pub(crate) families: Vec<String>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_147)]
    pub(crate) ingredient_count: i64,
    #[schema(schema_with = described_field_155)]
    pub(crate) ingredients: Vec<CraftingSystemsDetailResponseIngredientsEntry>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_148)]
    pub(crate) npc: String,
    #[schema(schema_with = described_field_96)]
    pub(crate) pack: String,
    #[schema(schema_with = described_field_150)]
    pub(crate) page: String,
    #[schema(schema_with = described_field_151)]
    pub(crate) recipe_count: i64,
    #[schema(schema_with = described_field_160)]
    pub(crate) recipes: Vec<CraftingSystemsDetailResponseRecipesEntry>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct DamageTypesPageResponseDamageTypesEntry {
    #[schema(schema_with = described_field_161)]
    pub(crate) category: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct DamageTypesPageResponse {
    #[schema(schema_with = described_field_162)]
    pub(crate) damage_types: Vec<DamageTypesPageResponseDamageTypesEntry>,
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnchantmentsPageResponseEnchantmentsEntryBonusTypesEntry {
    #[schema(schema_with = described_field_164)]
    pub(crate) item_count: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnchantmentsPageResponseEnchantmentsEntry {
    #[schema(schema_with = described_field_163)]
    pub(crate) augment_count: i64,
    #[schema(schema_with = described_field_165)]
    pub(crate) bonus_types: Vec<EnchantmentsPageResponseEnchantmentsEntryBonusTypesEntry>,
    #[schema(schema_with = described_field_166)]
    pub(crate) detail_path: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_164)]
    pub(crate) item_count: i64,
    #[schema(schema_with = described_field_102)]
    pub(crate) kind: String,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_167)]
    pub(crate) set_count: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnchantmentsPageResponse {
    #[schema(schema_with = described_field_168)]
    pub(crate) enchantments: Vec<EnchantmentsPageResponseEnchantmentsEntry>,
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

fn schema_carrier_value() -> RefOr<Schema> {
    described_schema::<Option<i64>>("First amount stated by this owner link, if any.")
}
fn schema_carrier_value2() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Second amount stated by this owner link, if any.")
}
fn schema_set_tier_carriers() -> RefOr<Schema> {
    described_schema::<Vec<EnchantmentSetTierCarrier>>("Set tiers carrying this family or stat.")
}
fn schema_stat_rule_bonus_type() -> RefOr<Schema> {
    described_schema::<Option<String>>("Fixed type of this stat row, or the link type when absent.")
}
fn schema_stat_rule_constant() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Final amount for a static stat row.")
}
fn schema_family_default_value() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Definition amount used when the link omits its first amount.")
}
fn schema_family_default_value2() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Definition amount used when the link omits its second amount.")
}
fn schema_family_description_template() -> RefOr<Schema> {
    described_schema::<Option<String>>("Template for the rendered description, if defined.")
}
fn schema_family_ladder() -> RefOr<Schema> {
    described_schema::<Option<EnchantmentLadderDetail>>("Ladder and ordered steps for this family, if any.")
}
fn schema_family_stacking_note() -> RefOr<Schema> {
    described_schema::<Option<String>>("Stacking guidance for this family, if known.")
}
fn schema_family_wiki_url() -> RefOr<Schema> {
    described_schema::<Option<String>>("Wiki page for this family, if known.")
}
fn schema_ladder_steps() -> RefOr<Schema> {
    described_schema::<Vec<EnchantmentLadderStep>>("Families in this ladder ordered by rank.")
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnchantmentsDetailResponseAugmentsAugmentsEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = schema_carrier_value)]
    pub(crate) value: Option<i64>,
    #[schema(schema_with = schema_carrier_value2)]
    pub(crate) value2: Option<i64>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnchantmentsDetailResponseAugments {
    #[schema(schema_with = described_field_170)]
    pub(crate) augments: Vec<EnchantmentsDetailResponseAugmentsAugmentsEntry>,
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnchantmentsDetailResponseItemsItemsEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = schema_carrier_value)]
    pub(crate) value: Option<i64>,
    #[schema(schema_with = schema_carrier_value2)]
    pub(crate) value2: Option<i64>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnchantmentsDetailResponseItems {
    #[schema(schema_with = described_field_175)]
    pub(crate) items: Vec<EnchantmentsDetailResponseItemsItemsEntry>,
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnchantmentSetTierCarrier {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_413)]
    pub(crate) set_id: i64,
    #[schema(schema_with = described_field_414)]
    pub(crate) set_name: String,
    #[schema(schema_with = described_field_419)]
    pub(crate) equipped_count: i64,
    #[schema(schema_with = schema_carrier_value)]
    pub(crate) value: Option<i64>,
    #[schema(schema_with = schema_carrier_value2)]
    pub(crate) value2: Option<i64>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnchantmentsDetailResponseSetTiers {
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = schema_set_tier_carriers)]
    pub(crate) set_tiers: Vec<EnchantmentSetTierCarrier>,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnchantmentsDetailResponseStatsEntry {
    #[schema(schema_with = described_field_180)]
    pub(crate) amount_from: i64,
    #[schema(schema_with = schema_stat_rule_bonus_type)]
    pub(crate) bonus_type: Option<String>,
    #[schema(schema_with = schema_stat_rule_constant)]
    pub(crate) constant: Option<i64>,
    #[schema(schema_with = described_field_183)]
    pub(crate) rounding: String,
    #[schema(schema_with = described_field_184)]
    pub(crate) scale: f64,
    #[schema(schema_with = described_field_26)]
    pub(crate) stat: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnchantmentLadderStep {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = schema_ladder_rank)]
    pub(crate) rank: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnchantmentLadderDetail {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = schema_ladder_rank)]
    pub(crate) rank: i64,
    #[schema(schema_with = schema_ladder_steps)]
    pub(crate) steps: Vec<EnchantmentLadderStep>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnchantmentsDetailResponse {
    #[schema(schema_with = described_field_169)]
    pub(crate) amount_count: i64,
    #[schema(schema_with = described_field_171)]
    pub(crate) augments: EnchantmentsDetailResponseAugments,
    #[schema(schema_with = schema_family_default_value)]
    pub(crate) default_value: Option<i64>,
    #[schema(schema_with = schema_family_default_value2)]
    pub(crate) default_value2: Option<i64>,
    #[schema(schema_with = schema_family_description_template)]
    pub(crate) description_template: Option<String>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_176)]
    pub(crate) items: EnchantmentsDetailResponseItems,
    #[schema(schema_with = schema_family_ladder)]
    pub(crate) ladder: Option<EnchantmentLadderDetail>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_178)]
    pub(crate) set_tiers: EnchantmentsDetailResponseSetTiers,
    #[schema(schema_with = schema_family_stacking_note)]
    pub(crate) stacking_note: Option<String>,
    #[schema(schema_with = described_field_185)]
    pub(crate) stats: Vec<EnchantmentsDetailResponseStatsEntry>,
    #[schema(schema_with = described_field_186)]
    pub(crate) text_template: String,
    #[schema(schema_with = schema_family_wiki_url)]
    pub(crate) wiki_url: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnhancementTreesPageResponseEnhancementTreesEntryRequirementsEntry {
    #[schema(schema_with = described_field_82)]
    pub(crate) group_index: i64,
    #[schema(schema_with = described_field_83)]
    pub(crate) group_kind: String,
    #[schema(schema_with = described_field_84)]
    pub(crate) items: Vec<String>,
    #[schema(schema_with = described_field_85)]
    pub(crate) req_type: String,
    #[schema(schema_with = described_field_86)]
    pub(crate) value: Option<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnhancementTreesPageResponseEnhancementTreesEntry {
    #[schema(schema_with = described_field_188)]
    pub(crate) background: String,
    #[schema(schema_with = described_field_189)]
    pub(crate) enhancement_count: i64,
    #[schema(schema_with = described_field_37)]
    pub(crate) icon: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_190)]
    pub(crate) is_legacy: bool,
    #[schema(schema_with = described_field_102)]
    pub(crate) kind: String,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_191)]
    pub(crate) requirements: Vec<EnhancementTreesPageResponseEnhancementTreesEntryRequirementsEntry>,
    #[schema(schema_with = described_field_192)]
    pub(crate) version: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnhancementTreesPageResponse {
    #[schema(schema_with = described_field_193)]
    pub(crate) enhancement_trees: Vec<EnhancementTreesPageResponseEnhancementTreesEntry>,
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnhancementTreesDetailResponseEnhancementsEntryDcsEntry {
    #[schema(schema_with = described_field_198)]
    pub(crate) amount: Vec<i64>,
    #[schema(schema_with = described_field_199)]
    pub(crate) base_class_level: Option<serde_json::Value>,
    #[schema(schema_with = described_field_200)]
    pub(crate) class_level: Option<serde_json::Value>,
    #[schema(schema_with = described_field_201)]
    pub(crate) dc_type: String,
    #[schema(schema_with = described_field_202)]
    pub(crate) dc_versus: String,
    #[schema(schema_with = described_field_22)]
    pub(crate) description: String,
    #[schema(schema_with = described_field_37)]
    pub(crate) icon: String,
    #[schema(schema_with = described_field_203)]
    pub(crate) mod_ability: Option<serde_json::Value>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_204)]
    pub(crate) other: Option<serde_json::Value>,
    #[schema(schema_with = described_field_205)]
    pub(crate) skill: Option<serde_json::Value>,
    #[schema(schema_with = described_field_206)]
    pub(crate) tactical: Option<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnhancementTreesDetailResponseEnhancementsEntryModifiersEntry {
    #[schema(schema_with = described_field_66)]
    pub(crate) amount_type: String,
    #[schema(schema_with = described_field_215)]
    pub(crate) amounts: Vec<i64>,
    #[schema(schema_with = described_field_68)]
    pub(crate) apply_as_item_effect: bool,
    #[schema(schema_with = described_field_69)]
    pub(crate) bonus: String,
    #[schema(schema_with = described_field_25)]
    pub(crate) bonus_type: String,
    #[schema(schema_with = described_field_70)]
    pub(crate) cap: Option<serde_json::Value>,
    #[schema(schema_with = described_field_71)]
    pub(crate) damage: Option<serde_json::Value>,
    #[schema(schema_with = described_field_72)]
    pub(crate) dice_bonus: Option<serde_json::Value>,
    #[schema(schema_with = described_field_216)]
    pub(crate) dice_damage: Option<serde_json::Value>,
    #[schema(schema_with = described_field_217)]
    pub(crate) dice_number: Option<serde_json::Value>,
    #[schema(schema_with = described_field_218)]
    pub(crate) dice_sides: Option<serde_json::Value>,
    #[schema(schema_with = described_field_76)]
    pub(crate) display_name: Option<serde_json::Value>,
    #[schema(schema_with = described_field_77)]
    pub(crate) effect_type: String,
    #[schema(schema_with = described_field_78)]
    pub(crate) extra_types: Option<serde_json::Value>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_79)]
    pub(crate) is_item_specific: bool,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_80)]
    pub(crate) percent: bool,
    #[schema(schema_with = described_field_81)]
    pub(crate) rank: Option<serde_json::Value>,
    #[schema(schema_with = described_field_219)]
    pub(crate) requirements: Vec<serde_json::Value>,
    #[schema(schema_with = described_field_88)]
    pub(crate) sort_order: i64,
    #[schema(schema_with = described_field_220)]
    pub(crate) stack_source: Option<String>,
    #[schema(schema_with = described_field_90)]
    pub(crate) targets: Option<Vec<String>>,
    #[schema(schema_with = described_field_86)]
    pub(crate) value: Option<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnhancementTreesDetailResponseEnhancementsEntryRequirementsEntry {
    #[schema(schema_with = described_field_82)]
    pub(crate) group_index: i64,
    #[schema(schema_with = described_field_83)]
    pub(crate) group_kind: String,
    #[schema(schema_with = described_field_84)]
    pub(crate) items: Vec<String>,
    #[schema(schema_with = described_field_85)]
    pub(crate) req_type: String,
    #[schema(schema_with = described_field_223)]
    pub(crate) value: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnhancementTreesDetailResponseEnhancementsEntry {
    #[schema(schema_with = described_field_194)]
    pub(crate) arrows: Vec<String>,
    #[schema(schema_with = described_field_195)]
    pub(crate) attack: Option<serde_json::Value>,
    #[schema(schema_with = described_field_196)]
    pub(crate) cooldown_seconds: Option<serde_json::Value>,
    #[schema(schema_with = described_field_197)]
    pub(crate) cost_per_rank: Vec<i64>,
    #[schema(schema_with = described_field_207)]
    pub(crate) dcs: Vec<EnhancementTreesDetailResponseEnhancementsEntryDcsEntry>,
    #[schema(schema_with = described_field_22)]
    pub(crate) description: String,
    #[schema(schema_with = described_field_208)]
    pub(crate) duration_seconds: Option<serde_json::Value>,
    #[schema(schema_with = described_field_209)]
    pub(crate) exclusions: Vec<serde_json::Value>,
    #[schema(schema_with = described_field_210)]
    pub(crate) follow_on_modifiers: Vec<serde_json::Value>,
    #[schema(schema_with = described_field_37)]
    pub(crate) icon: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_211)]
    pub(crate) internal_name: String,
    #[schema(schema_with = described_field_212)]
    pub(crate) is_clickie: bool,
    #[schema(schema_with = described_field_213)]
    pub(crate) is_tier5: bool,
    #[schema(schema_with = described_field_214)]
    pub(crate) min_spent: i64,
    #[schema(schema_with = described_field_221)]
    pub(crate) modifiers: Vec<EnhancementTreesDetailResponseEnhancementsEntryModifiersEntry>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_222)]
    pub(crate) ranks: i64,
    #[schema(schema_with = described_field_224)]
    pub(crate) requirements: Vec<EnhancementTreesDetailResponseEnhancementsEntryRequirementsEntry>,
    #[schema(schema_with = described_field_225)]
    pub(crate) selections: Vec<serde_json::Value>,
    #[schema(schema_with = described_field_226)]
    pub(crate) stances: Vec<serde_json::Value>,
    #[schema(schema_with = described_field_227)]
    pub(crate) this_attack_modifiers: Vec<serde_json::Value>,
    #[schema(schema_with = described_field_228)]
    pub(crate) x: i64,
    #[schema(schema_with = described_field_229)]
    pub(crate) y: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnhancementTreesDetailResponseRequirementsEntry {
    #[schema(schema_with = described_field_82)]
    pub(crate) group_index: i64,
    #[schema(schema_with = described_field_83)]
    pub(crate) group_kind: String,
    #[schema(schema_with = described_field_84)]
    pub(crate) items: Vec<String>,
    #[schema(schema_with = described_field_85)]
    pub(crate) req_type: String,
    #[schema(schema_with = described_field_86)]
    pub(crate) value: Option<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnhancementTreesDetailResponse {
    #[schema(schema_with = described_field_188)]
    pub(crate) background: String,
    #[schema(schema_with = described_field_189)]
    pub(crate) enhancement_count: i64,
    #[schema(schema_with = described_field_230)]
    pub(crate) enhancements: Vec<EnhancementTreesDetailResponseEnhancementsEntry>,
    #[schema(schema_with = described_field_37)]
    pub(crate) icon: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_190)]
    pub(crate) is_legacy: bool,
    #[schema(schema_with = described_field_102)]
    pub(crate) kind: String,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_231)]
    pub(crate) requirements: Vec<EnhancementTreesDetailResponseRequirementsEntry>,
    #[schema(schema_with = described_field_192)]
    pub(crate) version: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EquipmentSlotsPageResponseEquipmentSlotsEntry {
    #[schema(schema_with = described_field_161)]
    pub(crate) category: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_88)]
    pub(crate) sort_order: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EquipmentSlotsPageResponse {
    #[schema(schema_with = described_field_232)]
    pub(crate) equipment_slots: Vec<EquipmentSlotsPageResponseEquipmentSlotsEntry>,
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EventsPageResponseEventsEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_164)]
    pub(crate) item_count: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_49)]
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EventsPageResponse {
    #[schema(schema_with = described_field_233)]
    pub(crate) events: Vec<EventsPageResponseEventsEntry>,
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EventsDetailResponseItemsEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_11)]
    pub(crate) minimum_level: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_12)]
    pub(crate) slot: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EventsDetailResponse {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_164)]
    pub(crate) item_count: i64,
    #[schema(schema_with = described_field_234)]
    pub(crate) items: Vec<EventsDetailResponseItemsEntry>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_49)]
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsPageResponseFeatsEntry {
    #[schema(schema_with = described_field_134)]
    pub(crate) acquire: String,
    #[schema(schema_with = described_field_235)]
    pub(crate) auto_acquire_ignores_requirements: bool,
    #[schema(schema_with = described_field_22)]
    pub(crate) description: String,
    #[schema(schema_with = described_field_236)]
    pub(crate) groups: Vec<String>,
    #[schema(schema_with = described_field_37)]
    pub(crate) icon: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_237)]
    pub(crate) max_times_acquire: Option<serde_json::Value>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_238)]
    pub(crate) source_id: Option<serde_json::Value>,
    #[schema(schema_with = described_field_239)]
    pub(crate) source_kind: String,
    #[schema(schema_with = described_field_240)]
    pub(crate) source_name: Option<serde_json::Value>,
    #[schema(schema_with = described_field_241)]
    pub(crate) sphere: Option<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsPageResponse {
    #[schema(schema_with = described_field_242)]
    pub(crate) feats: Vec<FeatsPageResponseFeatsEntry>,
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponseAttack {
    #[schema(schema_with = described_field_243)]
    pub(crate) cooldown_seconds: i64,
    #[schema(schema_with = described_field_22)]
    pub(crate) description: String,
    #[schema(schema_with = described_field_244)]
    pub(crate) duration_seconds: Option<i64>,
    #[schema(schema_with = described_field_37)]
    pub(crate) icon: String,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponseAutoAcquireRequirementsEntry {
    #[schema(schema_with = described_field_82)]
    pub(crate) group_index: i64,
    #[schema(schema_with = described_field_83)]
    pub(crate) group_kind: String,
    #[schema(schema_with = described_field_246)]
    pub(crate) items: Option<serde_json::Value>,
    #[schema(schema_with = described_field_85)]
    pub(crate) req_type: String,
    #[schema(schema_with = described_field_247)]
    pub(crate) value: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponseBonusesEntry {
    #[schema(schema_with = described_field_25)]
    pub(crate) bonus_type: String,
    #[schema(schema_with = described_field_249)]
    pub(crate) description: Option<serde_json::Value>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_26)]
    pub(crate) stat: String,
    #[schema(schema_with = described_field_27)]
    pub(crate) stat_category: String,
    #[schema(schema_with = described_field_28)]
    pub(crate) value: i64,
    #[schema(schema_with = described_field_33)]
    pub(crate) value2: Option<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponseConditionalGroupsEntryRequirementsEntry {
    #[schema(schema_with = described_field_82)]
    pub(crate) group_index: i64,
    #[schema(schema_with = described_field_83)]
    pub(crate) group_kind: String,
    #[schema(schema_with = described_field_84)]
    pub(crate) items: Vec<String>,
    #[schema(schema_with = described_field_85)]
    pub(crate) req_type: String,
    #[schema(schema_with = described_field_247)]
    pub(crate) value: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponseConditionalGroupsEntry {
    #[schema(schema_with = described_field_236)]
    pub(crate) groups: Vec<String>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_251)]
    pub(crate) requirements: Vec<FeatsDetailResponseConditionalGroupsEntryRequirementsEntry>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponseDcsEntry {
    #[schema(schema_with = described_field_253)]
    pub(crate) amount: Option<serde_json::Value>,
    #[schema(schema_with = described_field_199)]
    pub(crate) base_class_level: Option<serde_json::Value>,
    #[schema(schema_with = described_field_200)]
    pub(crate) class_level: Option<serde_json::Value>,
    #[schema(schema_with = described_field_201)]
    pub(crate) dc_type: String,
    #[schema(schema_with = described_field_202)]
    pub(crate) dc_versus: String,
    #[schema(schema_with = described_field_22)]
    pub(crate) description: String,
    #[schema(schema_with = described_field_37)]
    pub(crate) icon: String,
    #[schema(schema_with = described_field_203)]
    pub(crate) mod_ability: Option<serde_json::Value>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_254)]
    pub(crate) other: String,
    #[schema(schema_with = described_field_255)]
    pub(crate) skill: String,
    #[schema(schema_with = described_field_206)]
    pub(crate) tactical: Option<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponseFollowOnModifiersEntry {
    #[schema(schema_with = described_field_66)]
    pub(crate) amount_type: String,
    #[schema(schema_with = described_field_257)]
    pub(crate) amounts: Option<serde_json::Value>,
    #[schema(schema_with = described_field_68)]
    pub(crate) apply_as_item_effect: bool,
    #[schema(schema_with = described_field_258)]
    pub(crate) bonus: Option<serde_json::Value>,
    #[schema(schema_with = described_field_181)]
    pub(crate) bonus_type: Option<serde_json::Value>,
    #[schema(schema_with = described_field_70)]
    pub(crate) cap: Option<serde_json::Value>,
    #[schema(schema_with = described_field_71)]
    pub(crate) damage: Option<serde_json::Value>,
    #[schema(schema_with = described_field_72)]
    pub(crate) dice_bonus: Option<serde_json::Value>,
    #[schema(schema_with = described_field_216)]
    pub(crate) dice_damage: Option<serde_json::Value>,
    #[schema(schema_with = described_field_217)]
    pub(crate) dice_number: Option<serde_json::Value>,
    #[schema(schema_with = described_field_218)]
    pub(crate) dice_sides: Option<serde_json::Value>,
    #[schema(schema_with = described_field_76)]
    pub(crate) display_name: Option<serde_json::Value>,
    #[schema(schema_with = described_field_77)]
    pub(crate) effect_type: String,
    #[schema(schema_with = described_field_78)]
    pub(crate) extra_types: Option<serde_json::Value>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_79)]
    pub(crate) is_item_specific: bool,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_80)]
    pub(crate) percent: bool,
    #[schema(schema_with = described_field_81)]
    pub(crate) rank: Option<serde_json::Value>,
    #[schema(schema_with = described_field_219)]
    pub(crate) requirements: Vec<serde_json::Value>,
    #[schema(schema_with = described_field_88)]
    pub(crate) sort_order: i64,
    #[schema(schema_with = described_field_89)]
    pub(crate) stack_source: Option<serde_json::Value>,
    #[schema(schema_with = described_field_259)]
    pub(crate) targets: Option<serde_json::Value>,
    #[schema(schema_with = described_field_86)]
    pub(crate) value: Option<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponseModifiersEntryRequirementsEntry {
    #[schema(schema_with = described_field_82)]
    pub(crate) group_index: i64,
    #[schema(schema_with = described_field_83)]
    pub(crate) group_kind: String,
    #[schema(schema_with = described_field_84)]
    pub(crate) items: Vec<String>,
    #[schema(schema_with = described_field_85)]
    pub(crate) req_type: String,
    #[schema(schema_with = described_field_223)]
    pub(crate) value: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponseModifiersEntry {
    #[schema(schema_with = described_field_66)]
    pub(crate) amount_type: String,
    #[schema(schema_with = described_field_261)]
    pub(crate) amounts: Option<Vec<f64>>,
    #[schema(schema_with = described_field_68)]
    pub(crate) apply_as_item_effect: bool,
    #[schema(schema_with = described_field_69)]
    pub(crate) bonus: String,
    #[schema(schema_with = described_field_25)]
    pub(crate) bonus_type: String,
    #[schema(schema_with = described_field_70)]
    pub(crate) cap: Option<serde_json::Value>,
    #[schema(schema_with = described_field_71)]
    pub(crate) damage: Option<serde_json::Value>,
    #[schema(schema_with = described_field_72)]
    pub(crate) dice_bonus: Option<serde_json::Value>,
    #[schema(schema_with = described_field_216)]
    pub(crate) dice_damage: Option<serde_json::Value>,
    #[schema(schema_with = described_field_217)]
    pub(crate) dice_number: Option<serde_json::Value>,
    #[schema(schema_with = described_field_218)]
    pub(crate) dice_sides: Option<serde_json::Value>,
    #[schema(schema_with = described_field_262)]
    pub(crate) display_name: Option<String>,
    #[schema(schema_with = described_field_77)]
    pub(crate) effect_type: String,
    #[schema(schema_with = described_field_263)]
    pub(crate) extra_types: Option<Vec<String>>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_79)]
    pub(crate) is_item_specific: bool,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_80)]
    pub(crate) percent: bool,
    #[schema(schema_with = described_field_81)]
    pub(crate) rank: Option<serde_json::Value>,
    #[schema(schema_with = described_field_264)]
    pub(crate) requirements: Vec<FeatsDetailResponseModifiersEntryRequirementsEntry>,
    #[schema(schema_with = described_field_88)]
    pub(crate) sort_order: i64,
    #[schema(schema_with = described_field_220)]
    pub(crate) stack_source: Option<String>,
    #[schema(schema_with = described_field_90)]
    pub(crate) targets: Option<Vec<String>>,
    #[schema(schema_with = described_field_86)]
    pub(crate) value: Option<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponseRequirementsEntry {
    #[schema(schema_with = described_field_82)]
    pub(crate) group_index: i64,
    #[schema(schema_with = described_field_83)]
    pub(crate) group_kind: String,
    #[schema(schema_with = described_field_266)]
    pub(crate) items: Option<Vec<String>>,
    #[schema(schema_with = described_field_85)]
    pub(crate) req_type: String,
    #[schema(schema_with = described_field_223)]
    pub(crate) value: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponseStancesEntry {
    #[schema(schema_with = described_field_270)]
    pub(crate) auto_controlled: bool,
    #[schema(schema_with = described_field_22)]
    pub(crate) description: String,
    #[schema(schema_with = described_field_271)]
    pub(crate) group_name: Option<String>,
    #[schema(schema_with = described_field_37)]
    pub(crate) icon: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_272)]
    pub(crate) incompatible: Option<Vec<String>>,
    #[schema(schema_with = described_field_143)]
    pub(crate) modifiers: Vec<serde_json::Value>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_219)]
    pub(crate) requirements: Vec<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponseSubItemsEntry {
    #[schema(schema_with = described_field_22)]
    pub(crate) description: String,
    #[schema(schema_with = described_field_37)]
    pub(crate) icon: String,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponseThisAttackModifiersEntry {
    #[schema(schema_with = described_field_66)]
    pub(crate) amount_type: String,
    #[schema(schema_with = described_field_215)]
    pub(crate) amounts: Vec<i64>,
    #[schema(schema_with = described_field_68)]
    pub(crate) apply_as_item_effect: bool,
    #[schema(schema_with = described_field_258)]
    pub(crate) bonus: Option<serde_json::Value>,
    #[schema(schema_with = described_field_181)]
    pub(crate) bonus_type: Option<serde_json::Value>,
    #[schema(schema_with = described_field_70)]
    pub(crate) cap: Option<serde_json::Value>,
    #[schema(schema_with = described_field_71)]
    pub(crate) damage: Option<serde_json::Value>,
    #[schema(schema_with = described_field_72)]
    pub(crate) dice_bonus: Option<serde_json::Value>,
    #[schema(schema_with = described_field_216)]
    pub(crate) dice_damage: Option<serde_json::Value>,
    #[schema(schema_with = described_field_217)]
    pub(crate) dice_number: Option<serde_json::Value>,
    #[schema(schema_with = described_field_218)]
    pub(crate) dice_sides: Option<serde_json::Value>,
    #[schema(schema_with = described_field_76)]
    pub(crate) display_name: Option<serde_json::Value>,
    #[schema(schema_with = described_field_77)]
    pub(crate) effect_type: String,
    #[schema(schema_with = described_field_78)]
    pub(crate) extra_types: Option<serde_json::Value>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_79)]
    pub(crate) is_item_specific: bool,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_80)]
    pub(crate) percent: bool,
    #[schema(schema_with = described_field_81)]
    pub(crate) rank: Option<serde_json::Value>,
    #[schema(schema_with = described_field_219)]
    pub(crate) requirements: Vec<serde_json::Value>,
    #[schema(schema_with = described_field_88)]
    pub(crate) sort_order: i64,
    #[schema(schema_with = described_field_89)]
    pub(crate) stack_source: Option<serde_json::Value>,
    #[schema(schema_with = described_field_259)]
    pub(crate) targets: Option<serde_json::Value>,
    #[schema(schema_with = described_field_86)]
    pub(crate) value: Option<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponse {
    #[schema(schema_with = described_field_134)]
    pub(crate) acquire: String,
    #[schema(schema_with = described_field_245)]
    pub(crate) attack: Option<FeatsDetailResponseAttack>,
    #[schema(schema_with = described_field_235)]
    pub(crate) auto_acquire_ignores_requirements: bool,
    #[schema(schema_with = described_field_248)]
    pub(crate) auto_acquire_requirements: Vec<FeatsDetailResponseAutoAcquireRequirementsEntry>,
    #[schema(schema_with = described_field_250)]
    pub(crate) bonuses: Vec<FeatsDetailResponseBonusesEntry>,
    #[schema(schema_with = described_field_252)]
    pub(crate) conditional_groups: Vec<FeatsDetailResponseConditionalGroupsEntry>,
    #[schema(schema_with = described_field_256)]
    pub(crate) dcs: Vec<FeatsDetailResponseDcsEntry>,
    #[schema(schema_with = described_field_22)]
    pub(crate) description: String,
    #[schema(schema_with = described_field_260)]
    pub(crate) follow_on_modifiers: Vec<FeatsDetailResponseFollowOnModifiersEntry>,
    #[schema(schema_with = described_field_236)]
    pub(crate) groups: Vec<String>,
    #[schema(schema_with = described_field_37)]
    pub(crate) icon: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_135)]
    pub(crate) max_times_acquire: Option<i64>,
    #[schema(schema_with = described_field_265)]
    pub(crate) modifiers: Vec<FeatsDetailResponseModifiersEntry>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_267)]
    pub(crate) requirements: Vec<FeatsDetailResponseRequirementsEntry>,
    #[schema(schema_with = described_field_268)]
    pub(crate) source_id: Option<i64>,
    #[schema(schema_with = described_field_239)]
    pub(crate) source_kind: String,
    #[schema(schema_with = described_field_269)]
    pub(crate) source_name: Option<String>,
    #[schema(schema_with = described_field_241)]
    pub(crate) sphere: Option<serde_json::Value>,
    #[schema(schema_with = described_field_273)]
    pub(crate) stances: Vec<FeatsDetailResponseStancesEntry>,
    #[schema(schema_with = described_field_274)]
    pub(crate) sub_items: Vec<FeatsDetailResponseSubItemsEntry>,
    #[schema(schema_with = described_field_275)]
    pub(crate) this_attack_modifiers: Vec<FeatsDetailResponseThisAttackModifiersEntry>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FiligreesPageResponseFiligreesEntryModifiersEntry {
    #[schema(schema_with = described_field_66)]
    pub(crate) amount_type: String,
    #[schema(schema_with = described_field_215)]
    pub(crate) amounts: Vec<i64>,
    #[schema(schema_with = described_field_68)]
    pub(crate) apply_as_item_effect: bool,
    #[schema(schema_with = described_field_69)]
    pub(crate) bonus: String,
    #[schema(schema_with = described_field_25)]
    pub(crate) bonus_type: String,
    #[schema(schema_with = described_field_70)]
    pub(crate) cap: Option<serde_json::Value>,
    #[schema(schema_with = described_field_71)]
    pub(crate) damage: Option<serde_json::Value>,
    #[schema(schema_with = described_field_72)]
    pub(crate) dice_bonus: Option<serde_json::Value>,
    #[schema(schema_with = described_field_216)]
    pub(crate) dice_damage: Option<serde_json::Value>,
    #[schema(schema_with = described_field_217)]
    pub(crate) dice_number: Option<serde_json::Value>,
    #[schema(schema_with = described_field_218)]
    pub(crate) dice_sides: Option<serde_json::Value>,
    #[schema(schema_with = described_field_262)]
    pub(crate) display_name: Option<String>,
    #[schema(schema_with = described_field_77)]
    pub(crate) effect_type: String,
    #[schema(schema_with = described_field_78)]
    pub(crate) extra_types: Option<serde_json::Value>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_79)]
    pub(crate) is_item_specific: bool,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_80)]
    pub(crate) percent: bool,
    #[schema(schema_with = described_field_81)]
    pub(crate) rank: Option<serde_json::Value>,
    #[schema(schema_with = described_field_219)]
    pub(crate) requirements: Vec<serde_json::Value>,
    #[schema(schema_with = described_field_88)]
    pub(crate) sort_order: i64,
    #[schema(schema_with = described_field_89)]
    pub(crate) stack_source: Option<serde_json::Value>,
    #[schema(schema_with = described_field_90)]
    pub(crate) targets: Option<Vec<String>>,
    #[schema(schema_with = described_field_86)]
    pub(crate) value: Option<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FiligreesPageResponseFiligreesEntry {
    #[schema(schema_with = described_field_22)]
    pub(crate) description: String,
    #[schema(schema_with = described_field_37)]
    pub(crate) icon: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_276)]
    pub(crate) menu: String,
    #[schema(schema_with = described_field_277)]
    pub(crate) modifiers: Vec<FiligreesPageResponseFiligreesEntryModifiersEntry>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_278)]
    pub(crate) set_id: Option<i64>,
    #[schema(schema_with = described_field_279)]
    pub(crate) set_name: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FiligreesPageResponse {
    #[schema(schema_with = described_field_280)]
    pub(crate) filigrees: Vec<FiligreesPageResponseFiligreesEntry>,
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct GuildBuffsPageResponseGuildBuffsEntryModifiersEntry {
    #[schema(schema_with = described_field_66)]
    pub(crate) amount_type: String,
    #[schema(schema_with = described_field_215)]
    pub(crate) amounts: Vec<i64>,
    #[schema(schema_with = described_field_68)]
    pub(crate) apply_as_item_effect: bool,
    #[schema(schema_with = described_field_69)]
    pub(crate) bonus: String,
    #[schema(schema_with = described_field_25)]
    pub(crate) bonus_type: String,
    #[schema(schema_with = described_field_70)]
    pub(crate) cap: Option<serde_json::Value>,
    #[schema(schema_with = described_field_71)]
    pub(crate) damage: Option<serde_json::Value>,
    #[schema(schema_with = described_field_72)]
    pub(crate) dice_bonus: Option<serde_json::Value>,
    #[schema(schema_with = described_field_216)]
    pub(crate) dice_damage: Option<serde_json::Value>,
    #[schema(schema_with = described_field_217)]
    pub(crate) dice_number: Option<serde_json::Value>,
    #[schema(schema_with = described_field_218)]
    pub(crate) dice_sides: Option<serde_json::Value>,
    #[schema(schema_with = described_field_76)]
    pub(crate) display_name: Option<serde_json::Value>,
    #[schema(schema_with = described_field_77)]
    pub(crate) effect_type: String,
    #[schema(schema_with = described_field_78)]
    pub(crate) extra_types: Option<serde_json::Value>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_79)]
    pub(crate) is_item_specific: bool,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_80)]
    pub(crate) percent: bool,
    #[schema(schema_with = described_field_81)]
    pub(crate) rank: Option<serde_json::Value>,
    #[schema(schema_with = described_field_219)]
    pub(crate) requirements: Vec<serde_json::Value>,
    #[schema(schema_with = described_field_88)]
    pub(crate) sort_order: i64,
    #[schema(schema_with = described_field_89)]
    pub(crate) stack_source: Option<serde_json::Value>,
    #[schema(schema_with = described_field_282)]
    pub(crate) targets: Vec<String>,
    #[schema(schema_with = described_field_86)]
    pub(crate) value: Option<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct GuildBuffsPageResponseGuildBuffsEntry {
    #[schema(schema_with = described_field_22)]
    pub(crate) description: String,
    #[schema(schema_with = described_field_281)]
    pub(crate) guild_level: i64,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_283)]
    pub(crate) modifiers: Vec<GuildBuffsPageResponseGuildBuffsEntryModifiersEntry>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct GuildBuffsPageResponse {
    #[schema(schema_with = described_field_284)]
    pub(crate) guild_buffs: Vec<GuildBuffsPageResponseGuildBuffsEntry>,
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsPageResponseItemsEntry {
    #[schema(schema_with = described_field_161)]
    pub(crate) category: String,
    #[schema(schema_with = described_field_285)]
    pub(crate) enhancement_bonus: Option<serde_json::Value>,
    #[schema(schema_with = described_field_37)]
    pub(crate) icon: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_190)]
    pub(crate) is_legacy: bool,
    #[schema(schema_with = described_field_94)]
    pub(crate) is_raid: bool,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_286)]
    pub(crate) item_type: Option<serde_json::Value>,
    #[schema(schema_with = described_field_11)]
    pub(crate) minimum_level: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_96)]
    pub(crate) pack: String,
    #[schema(schema_with = described_field_12)]
    pub(crate) slot: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsPageResponse {
    #[schema(schema_with = described_field_287)]
    pub(crate) items: Vec<ItemsPageResponseItemsEntry>,
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseAdventurePacksEntry {
    #[schema(schema_with = described_field_48)]
    pub(crate) chest: Option<String>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_289)]
    pub(crate) loot_type: Option<String>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_49)]
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseArmor {
    #[schema(schema_with = described_field_291)]
    pub(crate) adamantine_body: Option<i64>,
    #[schema(schema_with = described_field_292)]
    pub(crate) arcane_spell_failure: Option<i64>,
    #[schema(schema_with = described_field_293)]
    pub(crate) armor_bonus: Option<i64>,
    #[schema(schema_with = described_field_294)]
    pub(crate) armor_check_penalty: Option<i64>,
    #[schema(schema_with = described_field_295)]
    pub(crate) armor_type: String,
    #[schema(schema_with = described_field_296)]
    pub(crate) damage_reduction: Option<i64>,
    #[schema(schema_with = described_field_297)]
    pub(crate) max_dex_bonus: Option<i64>,
    #[schema(schema_with = described_field_298)]
    pub(crate) mithral_body: Option<i64>,
    #[schema(schema_with = described_field_299)]
    pub(crate) shield_bonus: Option<i64>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseAugmentSlotsEntryOptionsEntryModifiersEntry {
    #[schema(schema_with = described_field_305)]
    pub(crate) amount_type: Option<String>,
    #[schema(schema_with = described_field_306)]
    pub(crate) amounts: Vec<f64>,
    #[schema(schema_with = described_field_68)]
    pub(crate) apply_as_item_effect: bool,
    #[schema(schema_with = described_field_69)]
    pub(crate) bonus: String,
    #[schema(schema_with = described_field_25)]
    pub(crate) bonus_type: String,
    #[schema(schema_with = described_field_70)]
    pub(crate) cap: Option<serde_json::Value>,
    #[schema(schema_with = described_field_71)]
    pub(crate) damage: Option<serde_json::Value>,
    #[schema(schema_with = described_field_72)]
    pub(crate) dice_bonus: Option<serde_json::Value>,
    #[schema(schema_with = described_field_216)]
    pub(crate) dice_damage: Option<serde_json::Value>,
    #[schema(schema_with = described_field_217)]
    pub(crate) dice_number: Option<serde_json::Value>,
    #[schema(schema_with = described_field_218)]
    pub(crate) dice_sides: Option<serde_json::Value>,
    #[schema(schema_with = described_field_76)]
    pub(crate) display_name: Option<serde_json::Value>,
    #[schema(schema_with = described_field_77)]
    pub(crate) effect_type: String,
    #[schema(schema_with = described_field_78)]
    pub(crate) extra_types: Option<serde_json::Value>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_79)]
    pub(crate) is_item_specific: bool,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_80)]
    pub(crate) percent: bool,
    #[schema(schema_with = described_field_81)]
    pub(crate) rank: Option<serde_json::Value>,
    #[schema(schema_with = described_field_219)]
    pub(crate) requirements: Vec<serde_json::Value>,
    #[schema(schema_with = described_field_88)]
    pub(crate) sort_order: i64,
    #[schema(schema_with = described_field_89)]
    pub(crate) stack_source: Option<serde_json::Value>,
    #[schema(schema_with = described_field_90)]
    pub(crate) targets: Option<Vec<String>>,
    #[schema(schema_with = described_field_86)]
    pub(crate) value: Option<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseAugmentSlotsEntryOptionsEntrySetsEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseAugmentSlotsEntryOptionsEntry {
    #[schema(schema_with = described_field_22)]
    pub(crate) description: String,
    #[schema(schema_with = described_field_303)]
    pub(crate) enchantments: Vec<EnchantmentLine>,
    #[schema(schema_with = described_field_304)]
    pub(crate) grants_slot: Option<String>,
    #[schema(schema_with = described_field_62)]
    pub(crate) icon: Option<String>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_41)]
    pub(crate) min_level: i64,
    #[schema(schema_with = described_field_307)]
    pub(crate) modifiers: Vec<ItemsDetailResponseAugmentSlotsEntryOptionsEntryModifiersEntry>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_308)]
    pub(crate) sets: Vec<ItemsDetailResponseAugmentSlotsEntryOptionsEntrySetsEntry>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseAugmentSlotsEntry {
    #[schema(schema_with = described_field_14)]
    pub(crate) family: String,
    #[schema(schema_with = described_field_15)]
    pub(crate) label: String,
    #[schema(schema_with = described_field_309)]
    pub(crate) options: Vec<ItemsDetailResponseAugmentSlotsEntryOptionsEntry>,
    #[schema(schema_with = described_field_310)]
    pub(crate) qualifier: Option<String>,
    #[schema(schema_with = described_field_311)]
    pub(crate) slot_type_id: i64,
    #[schema(schema_with = described_field_88)]
    pub(crate) sort_order: i64,
    #[schema(schema_with = described_field_17)]
    pub(crate) variant: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseChallengePacksEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_49)]
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseClickiesEntry {
    #[schema(schema_with = described_field_314)]
    pub(crate) clickie_id: Option<i64>,
    #[schema(schema_with = described_field_60)]
    pub(crate) description: Option<String>,
    #[schema(schema_with = described_field_62)]
    pub(crate) icon: Option<String>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_315)]
    pub(crate) spell_id: Option<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseCraftingSystemsEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_49)]
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseEventsEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_49)]
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseModifiersEntry {
    #[schema(schema_with = described_field_66)]
    pub(crate) amount_type: String,
    #[schema(schema_with = described_field_215)]
    pub(crate) amounts: Vec<i64>,
    #[schema(schema_with = described_field_68)]
    pub(crate) apply_as_item_effect: bool,
    #[schema(schema_with = described_field_69)]
    pub(crate) bonus: String,
    #[schema(schema_with = described_field_25)]
    pub(crate) bonus_type: String,
    #[schema(schema_with = described_field_70)]
    pub(crate) cap: Option<serde_json::Value>,
    #[schema(schema_with = described_field_71)]
    pub(crate) damage: Option<serde_json::Value>,
    #[schema(schema_with = described_field_72)]
    pub(crate) dice_bonus: Option<serde_json::Value>,
    #[schema(schema_with = described_field_216)]
    pub(crate) dice_damage: Option<serde_json::Value>,
    #[schema(schema_with = described_field_217)]
    pub(crate) dice_number: Option<serde_json::Value>,
    #[schema(schema_with = described_field_218)]
    pub(crate) dice_sides: Option<serde_json::Value>,
    #[schema(schema_with = described_field_76)]
    pub(crate) display_name: Option<serde_json::Value>,
    #[schema(schema_with = described_field_77)]
    pub(crate) effect_type: String,
    #[schema(schema_with = described_field_78)]
    pub(crate) extra_types: Option<serde_json::Value>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_79)]
    pub(crate) is_item_specific: bool,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_80)]
    pub(crate) percent: bool,
    #[schema(schema_with = described_field_81)]
    pub(crate) rank: Option<serde_json::Value>,
    #[schema(schema_with = described_field_219)]
    pub(crate) requirements: Vec<serde_json::Value>,
    #[schema(schema_with = described_field_88)]
    pub(crate) sort_order: i64,
    #[schema(schema_with = described_field_89)]
    pub(crate) stack_source: Option<serde_json::Value>,
    #[schema(schema_with = described_field_282)]
    pub(crate) targets: Vec<String>,
    #[schema(schema_with = described_field_86)]
    pub(crate) value: Option<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseQuestChainsEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_49)]
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseQuestsEntry {
    #[schema(schema_with = described_field_48)]
    pub(crate) chest: Option<String>,
    #[schema(schema_with = described_field_92)]
    pub(crate) difficulties: Vec<String>,
    #[schema(schema_with = described_field_93)]
    pub(crate) epic_level: Option<i64>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_1)]
    pub(crate) is_free_to_play: bool,
    #[schema(schema_with = described_field_94)]
    pub(crate) is_raid: bool,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_95)]
    pub(crate) level: i64,
    #[schema(schema_with = described_field_10)]
    pub(crate) loot_type: String,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_96)]
    pub(crate) pack: String,
    #[schema(schema_with = described_field_97)]
    pub(crate) patron: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseSagasEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_334)]
    pub(crate) tier: Option<String>,
    #[schema(schema_with = described_field_49)]
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseSet {
    #[schema(schema_with = described_field_37)]
    pub(crate) icon: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseSourcesEntry {
    #[schema(schema_with = described_field_337)]
    pub(crate) character_level: Option<i64>,
    #[schema(schema_with = described_field_48)]
    pub(crate) chest: Option<String>,
    #[schema(schema_with = described_field_338)]
    pub(crate) cost: Option<String>,
    #[schema(schema_with = described_field_339)]
    pub(crate) id: Option<i64>,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_102)]
    pub(crate) kind: String,
    #[schema(schema_with = described_field_289)]
    pub(crate) loot_type: Option<String>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_334)]
    pub(crate) tier: Option<String>,
    #[schema(schema_with = described_field_340)]
    pub(crate) wiki_url: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseStarterRewardsEntry {
    #[schema(schema_with = described_field_342)]
    pub(crate) character_level: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseVendorsEntry {
    #[schema(schema_with = described_field_338)]
    pub(crate) cost: Option<String>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_344)]
    pub(crate) location: String,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_49)]
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseWeapon {
    #[schema(schema_with = described_field_346)]
    pub(crate) attack_modifier: Option<String>,
    #[schema(schema_with = described_field_347)]
    pub(crate) base_dice_bonus: Option<i64>,
    #[schema(schema_with = described_field_348)]
    pub(crate) base_dice_count: Option<i64>,
    #[schema(schema_with = described_field_349)]
    pub(crate) base_dice_sides: Option<i64>,
    #[schema(schema_with = described_field_350)]
    pub(crate) critical: Option<String>,
    #[schema(schema_with = described_field_351)]
    pub(crate) critical_multiplier: Option<i64>,
    #[schema(schema_with = described_field_352)]
    pub(crate) critical_threat_range: Option<i64>,
    #[schema(schema_with = described_field_353)]
    pub(crate) damage: Option<String>,
    #[schema(schema_with = described_field_354)]
    pub(crate) damage_modifier: Option<String>,
    #[schema(schema_with = described_field_355)]
    pub(crate) damage_multiplier: Option<f64>,
    #[schema(schema_with = described_field_356)]
    pub(crate) dr_bypass: Vec<String>,
    #[schema(schema_with = described_field_357)]
    pub(crate) handedness: String,
    #[schema(schema_with = described_field_358)]
    pub(crate) proficiency: Option<String>,
    #[schema(schema_with = described_field_359)]
    pub(crate) weapon_type: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponse {
    #[schema(schema_with = described_field_288)]
    pub(crate) accepts_sentience: bool,
    #[schema(schema_with = described_field_290)]
    pub(crate) adventure_packs: Vec<ItemsDetailResponseAdventurePacksEntry>,
    #[schema(schema_with = described_field_300)]
    pub(crate) armor: Option<ItemsDetailResponseArmor>,
    #[schema(schema_with = described_field_312)]
    pub(crate) augment_slots: Vec<ItemsDetailResponseAugmentSlotsEntry>,
    #[schema(schema_with = described_field_161)]
    pub(crate) category: String,
    #[schema(schema_with = described_field_313)]
    pub(crate) challenge_packs: Vec<ItemsDetailResponseChallengePacksEntry>,
    #[schema(schema_with = described_field_316)]
    pub(crate) clickies: Vec<ItemsDetailResponseClickiesEntry>,
    #[schema(schema_with = described_field_317)]
    pub(crate) crafting_systems: Vec<ItemsDetailResponseCraftingSystemsEntry>,
    #[schema(schema_with = described_field_60)]
    pub(crate) description: Option<String>,
    #[schema(schema_with = described_field_318)]
    pub(crate) drop_location: Option<String>,
    #[schema(schema_with = described_field_324)]
    pub(crate) enchantments: Vec<EnchantmentLine>,
    #[schema(schema_with = described_field_325)]
    pub(crate) enhancement_bonus: Option<i64>,
    #[schema(schema_with = described_field_326)]
    pub(crate) events: Vec<ItemsDetailResponseEventsEntry>,
    #[schema(schema_with = described_field_62)]
    pub(crate) icon: Option<String>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_190)]
    pub(crate) is_legacy: bool,
    #[schema(schema_with = described_field_327)]
    pub(crate) is_minor_artifact: bool,
    #[schema(schema_with = described_field_328)]
    pub(crate) item_type: Option<String>,
    #[schema(schema_with = described_field_329)]
    pub(crate) material: Option<String>,
    #[schema(schema_with = described_field_11)]
    pub(crate) minimum_level: i64,
    #[schema(schema_with = described_field_330)]
    pub(crate) modifiers: Vec<ItemsDetailResponseModifiersEntry>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_331)]
    pub(crate) quest_chains: Vec<ItemsDetailResponseQuestChainsEntry>,
    #[schema(schema_with = described_field_332)]
    pub(crate) quests: Vec<ItemsDetailResponseQuestsEntry>,
    #[schema(schema_with = described_field_333)]
    pub(crate) race_required: Option<String>,
    #[schema(schema_with = described_field_335)]
    pub(crate) sagas: Vec<ItemsDetailResponseSagasEntry>,
    #[schema(schema_with = described_field_336)]
    pub(crate) set: Option<ItemsDetailResponseSet>,
    #[schema(schema_with = described_field_279)]
    pub(crate) set_name: Option<String>,
    #[schema(schema_with = described_field_12)]
    pub(crate) slot: String,
    #[schema(schema_with = described_field_341)]
    pub(crate) sources: Vec<ItemsDetailResponseSourcesEntry>,
    #[schema(schema_with = described_field_343)]
    pub(crate) starter_rewards: Vec<ItemsDetailResponseStarterRewardsEntry>,
    #[schema(schema_with = described_field_345)]
    pub(crate) vendors: Vec<ItemsDetailResponseVendorsEntry>,
    #[schema(schema_with = described_field_360)]
    pub(crate) weapon: Option<ItemsDetailResponseWeapon>,
    #[schema(schema_with = described_field_49)]
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct OptionalBuffsPageResponseOptionalBuffsEntryModifiersEntryRequirementsEntry {
    #[schema(schema_with = described_field_82)]
    pub(crate) group_index: i64,
    #[schema(schema_with = described_field_83)]
    pub(crate) group_kind: String,
    #[schema(schema_with = described_field_84)]
    pub(crate) items: Vec<String>,
    #[schema(schema_with = described_field_85)]
    pub(crate) req_type: String,
    #[schema(schema_with = described_field_86)]
    pub(crate) value: Option<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct OptionalBuffsPageResponseOptionalBuffsEntryModifiersEntry {
    #[schema(schema_with = described_field_66)]
    pub(crate) amount_type: String,
    #[schema(schema_with = described_field_215)]
    pub(crate) amounts: Vec<i64>,
    #[schema(schema_with = described_field_68)]
    pub(crate) apply_as_item_effect: bool,
    #[schema(schema_with = described_field_69)]
    pub(crate) bonus: String,
    #[schema(schema_with = described_field_25)]
    pub(crate) bonus_type: String,
    #[schema(schema_with = described_field_70)]
    pub(crate) cap: Option<serde_json::Value>,
    #[schema(schema_with = described_field_71)]
    pub(crate) damage: Option<serde_json::Value>,
    #[schema(schema_with = described_field_72)]
    pub(crate) dice_bonus: Option<serde_json::Value>,
    #[schema(schema_with = described_field_216)]
    pub(crate) dice_damage: Option<serde_json::Value>,
    #[schema(schema_with = described_field_217)]
    pub(crate) dice_number: Option<serde_json::Value>,
    #[schema(schema_with = described_field_218)]
    pub(crate) dice_sides: Option<serde_json::Value>,
    #[schema(schema_with = described_field_76)]
    pub(crate) display_name: Option<serde_json::Value>,
    #[schema(schema_with = described_field_77)]
    pub(crate) effect_type: String,
    #[schema(schema_with = described_field_78)]
    pub(crate) extra_types: Option<serde_json::Value>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_79)]
    pub(crate) is_item_specific: bool,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_80)]
    pub(crate) percent: bool,
    #[schema(schema_with = described_field_81)]
    pub(crate) rank: Option<serde_json::Value>,
    #[schema(schema_with = described_field_361)]
    pub(crate) requirements: Vec<OptionalBuffsPageResponseOptionalBuffsEntryModifiersEntryRequirementsEntry>,
    #[schema(schema_with = described_field_88)]
    pub(crate) sort_order: i64,
    #[schema(schema_with = described_field_89)]
    pub(crate) stack_source: Option<serde_json::Value>,
    #[schema(schema_with = described_field_90)]
    pub(crate) targets: Option<Vec<String>>,
    #[schema(schema_with = described_field_86)]
    pub(crate) value: Option<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct OptionalBuffsPageResponseOptionalBuffsEntry {
    #[schema(schema_with = described_field_22)]
    pub(crate) description: String,
    #[schema(schema_with = described_field_37)]
    pub(crate) icon: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_362)]
    pub(crate) modifiers: Vec<OptionalBuffsPageResponseOptionalBuffsEntryModifiersEntry>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct OptionalBuffsPageResponse {
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_363)]
    pub(crate) optional_buffs: Vec<OptionalBuffsPageResponseOptionalBuffsEntry>,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct PatronsPageResponsePatronsEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct PatronsPageResponse {
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_364)]
    pub(crate) patrons: Vec<PatronsPageResponsePatronsEntry>,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct QuestChainsPageResponseQuestChainsEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_365)]
    pub(crate) pack: Option<serde_json::Value>,
    #[schema(schema_with = described_field_366)]
    pub(crate) quest_count: i64,
    #[schema(schema_with = described_field_367)]
    pub(crate) reward_count: i64,
    #[schema(schema_with = described_field_49)]
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct QuestChainsPageResponse {
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_368)]
    pub(crate) quest_chains: Vec<QuestChainsPageResponseQuestChainsEntry>,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct QuestChainsDetailResponseQuestsEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_95)]
    pub(crate) level: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct QuestChainsDetailResponseRewardsEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_11)]
    pub(crate) minimum_level: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_12)]
    pub(crate) slot: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct QuestChainsDetailResponse {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_365)]
    pub(crate) pack: Option<serde_json::Value>,
    #[schema(schema_with = described_field_366)]
    pub(crate) quest_count: i64,
    #[schema(schema_with = described_field_369)]
    pub(crate) quests: Vec<QuestChainsDetailResponseQuestsEntry>,
    #[schema(schema_with = described_field_367)]
    pub(crate) reward_count: i64,
    #[schema(schema_with = described_field_370)]
    pub(crate) rewards: Vec<QuestChainsDetailResponseRewardsEntry>,
    #[schema(schema_with = described_field_49)]
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct QuestsPageResponseQuestsEntry {
    #[schema(schema_with = described_field_371)]
    pub(crate) bestowed_by: String,
    #[schema(schema_with = described_field_92)]
    pub(crate) difficulties: Vec<String>,
    #[schema(schema_with = described_field_372)]
    pub(crate) epic_level: i64,
    #[schema(schema_with = described_field_373)]
    pub(crate) epic_name: Option<String>,
    #[schema(schema_with = described_field_374)]
    pub(crate) favor: i64,
    #[schema(schema_with = described_field_375)]
    pub(crate) flagging: Option<serde_json::Value>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_376)]
    pub(crate) is_challenge: bool,
    #[schema(schema_with = described_field_1)]
    pub(crate) is_free_to_play: bool,
    #[schema(schema_with = described_field_94)]
    pub(crate) is_raid: bool,
    #[schema(schema_with = described_field_377)]
    pub(crate) legendary_level: Option<i64>,
    #[schema(schema_with = described_field_95)]
    pub(crate) level: i64,
    #[schema(schema_with = described_field_378)]
    pub(crate) max_level: Option<serde_json::Value>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_96)]
    pub(crate) pack: String,
    #[schema(schema_with = described_field_379)]
    pub(crate) patron: String,
    #[schema(schema_with = described_field_380)]
    pub(crate) zone: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct QuestsPageResponse {
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_381)]
    pub(crate) quests: Vec<QuestsPageResponseQuestsEntry>,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct QuestsDetailResponseQuestChainsEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_49)]
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct QuestsDetailResponse {
    #[schema(schema_with = described_field_7)]
    pub(crate) augments: Vec<serde_json::Value>,
    #[schema(schema_with = described_field_371)]
    pub(crate) bestowed_by: String,
    #[schema(schema_with = described_field_92)]
    pub(crate) difficulties: Vec<String>,
    #[schema(schema_with = described_field_382)]
    pub(crate) epic_level: Option<serde_json::Value>,
    #[schema(schema_with = described_field_383)]
    pub(crate) epic_name: Option<serde_json::Value>,
    #[schema(schema_with = described_field_374)]
    pub(crate) favor: i64,
    #[schema(schema_with = described_field_375)]
    pub(crate) flagging: Option<serde_json::Value>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_376)]
    pub(crate) is_challenge: bool,
    #[schema(schema_with = described_field_1)]
    pub(crate) is_free_to_play: bool,
    #[schema(schema_with = described_field_94)]
    pub(crate) is_raid: bool,
    #[schema(schema_with = described_field_384)]
    pub(crate) items: Vec<serde_json::Value>,
    #[schema(schema_with = described_field_385)]
    pub(crate) legendary_level: Option<serde_json::Value>,
    #[schema(schema_with = described_field_95)]
    pub(crate) level: i64,
    #[schema(schema_with = described_field_378)]
    pub(crate) max_level: Option<serde_json::Value>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_96)]
    pub(crate) pack: String,
    #[schema(schema_with = described_field_379)]
    pub(crate) patron: String,
    #[schema(schema_with = described_field_386)]
    pub(crate) quest_chains: Vec<QuestsDetailResponseQuestChainsEntry>,
    #[schema(schema_with = described_field_387)]
    pub(crate) sagas: Vec<serde_json::Value>,
    #[schema(schema_with = described_field_380)]
    pub(crate) zone: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct RacesPageResponseRacesEntry {
    #[schema(schema_with = described_field_388)]
    pub(crate) build_points: Vec<i64>,
    #[schema(schema_with = described_field_22)]
    pub(crate) description: String,
    #[schema(schema_with = described_field_389)]
    pub(crate) iconic_class: Option<String>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_390)]
    pub(crate) is_construct: bool,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_391)]
    pub(crate) no_past_life: bool,
    #[schema(schema_with = described_field_392)]
    pub(crate) short_name: String,
    #[schema(schema_with = described_field_393)]
    pub(crate) skill_points: Option<serde_json::Value>,
    #[schema(schema_with = described_field_394)]
    pub(crate) starting_world: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct RacesPageResponse {
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_395)]
    pub(crate) races: Vec<RacesPageResponseRacesEntry>,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct RacesDetailResponseAbilityModifiersEntry {
    #[schema(schema_with = described_field_396)]
    pub(crate) modifier: i64,
    #[schema(schema_with = described_field_26)]
    pub(crate) stat: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct RacesDetailResponseFeatSlotsEntry {
    #[schema(schema_with = described_field_130)]
    pub(crate) feat_type: String,
    #[schema(schema_with = described_field_95)]
    pub(crate) level: i64,
    #[schema(schema_with = described_field_399)]
    pub(crate) update_list: Option<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct RacesDetailResponseFeatsEntry {
    #[schema(schema_with = described_field_134)]
    pub(crate) acquire: String,
    #[schema(schema_with = described_field_22)]
    pub(crate) description: String,
    #[schema(schema_with = described_field_37)]
    pub(crate) icon: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_237)]
    pub(crate) max_times_acquire: Option<serde_json::Value>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct RacesDetailResponseGrantedFeatsEntry {
    #[schema(schema_with = described_field_124)]
    pub(crate) feat_id: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct RacesDetailResponse {
    #[schema(schema_with = described_field_397)]
    pub(crate) ability_modifiers: Vec<RacesDetailResponseAbilityModifiersEntry>,
    #[schema(schema_with = described_field_398)]
    pub(crate) auto_buy_skills: Vec<serde_json::Value>,
    #[schema(schema_with = described_field_388)]
    pub(crate) build_points: Vec<i64>,
    #[schema(schema_with = described_field_22)]
    pub(crate) description: String,
    #[schema(schema_with = described_field_400)]
    pub(crate) feat_slots: Vec<RacesDetailResponseFeatSlotsEntry>,
    #[schema(schema_with = described_field_401)]
    pub(crate) feats: Vec<RacesDetailResponseFeatsEntry>,
    #[schema(schema_with = described_field_402)]
    pub(crate) granted_feats: Vec<RacesDetailResponseGrantedFeatsEntry>,
    #[schema(schema_with = described_field_403)]
    pub(crate) iconic_class: Option<serde_json::Value>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_390)]
    pub(crate) is_construct: bool,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_391)]
    pub(crate) no_past_life: bool,
    #[schema(schema_with = described_field_392)]
    pub(crate) short_name: String,
    #[schema(schema_with = described_field_393)]
    pub(crate) skill_points: Option<serde_json::Value>,
    #[schema(schema_with = described_field_394)]
    pub(crate) starting_world: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SagasPageResponseSagasEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_96)]
    pub(crate) pack: String,
    #[schema(schema_with = described_field_366)]
    pub(crate) quest_count: i64,
    #[schema(schema_with = described_field_367)]
    pub(crate) reward_count: i64,
    #[schema(schema_with = described_field_49)]
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SagasPageResponse {
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_404)]
    pub(crate) sagas: Vec<SagasPageResponseSagasEntry>,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SagasDetailResponseQuestsEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_95)]
    pub(crate) level: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SagasDetailResponseRewardsEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_11)]
    pub(crate) minimum_level: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_12)]
    pub(crate) slot: String,
    #[schema(schema_with = described_field_53)]
    pub(crate) tier: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SagasDetailResponse {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_96)]
    pub(crate) pack: String,
    #[schema(schema_with = described_field_366)]
    pub(crate) quest_count: i64,
    #[schema(schema_with = described_field_405)]
    pub(crate) quests: Vec<SagasDetailResponseQuestsEntry>,
    #[schema(schema_with = described_field_367)]
    pub(crate) reward_count: i64,
    #[schema(schema_with = described_field_406)]
    pub(crate) rewards: Vec<SagasDetailResponseRewardsEntry>,
    #[schema(schema_with = described_field_49)]
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SentientGemsPageResponseSentientGemsEntry {
    #[schema(schema_with = described_field_22)]
    pub(crate) description: String,
    #[schema(schema_with = described_field_37)]
    pub(crate) icon: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SentientGemsPageResponse {
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_407)]
    pub(crate) sentient_gems: Vec<SentientGemsPageResponseSentientGemsEntry>,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SetsPageResponseSetsEntry {
    #[schema(schema_with = described_field_163)]
    pub(crate) augment_count: i64,
    #[schema(schema_with = described_field_37)]
    pub(crate) icon: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_408)]
    pub(crate) is_filigree_set: bool,
    #[schema(schema_with = described_field_164)]
    pub(crate) item_count: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_409)]
    pub(crate) tier_count: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SetsPageResponse {
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_410)]
    pub(crate) sets: Vec<SetsPageResponseSetsEntry>,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SetsDetailResponseAugmentsEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_41)]
    pub(crate) min_level: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SetsDetailResponseFiligreesEntryModifiersEntry {
    #[schema(schema_with = described_field_66)]
    pub(crate) amount_type: String,
    #[schema(schema_with = described_field_215)]
    pub(crate) amounts: Vec<i64>,
    #[schema(schema_with = described_field_68)]
    pub(crate) apply_as_item_effect: bool,
    #[schema(schema_with = described_field_69)]
    pub(crate) bonus: String,
    #[schema(schema_with = described_field_25)]
    pub(crate) bonus_type: String,
    #[schema(schema_with = described_field_70)]
    pub(crate) cap: Option<serde_json::Value>,
    #[schema(schema_with = described_field_71)]
    pub(crate) damage: Option<serde_json::Value>,
    #[schema(schema_with = described_field_72)]
    pub(crate) dice_bonus: Option<serde_json::Value>,
    #[schema(schema_with = described_field_216)]
    pub(crate) dice_damage: Option<serde_json::Value>,
    #[schema(schema_with = described_field_217)]
    pub(crate) dice_number: Option<serde_json::Value>,
    #[schema(schema_with = described_field_218)]
    pub(crate) dice_sides: Option<serde_json::Value>,
    #[schema(schema_with = described_field_262)]
    pub(crate) display_name: Option<String>,
    #[schema(schema_with = described_field_77)]
    pub(crate) effect_type: String,
    #[schema(schema_with = described_field_78)]
    pub(crate) extra_types: Option<serde_json::Value>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_79)]
    pub(crate) is_item_specific: bool,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_80)]
    pub(crate) percent: bool,
    #[schema(schema_with = described_field_81)]
    pub(crate) rank: Option<serde_json::Value>,
    #[schema(schema_with = described_field_219)]
    pub(crate) requirements: Vec<serde_json::Value>,
    #[schema(schema_with = described_field_88)]
    pub(crate) sort_order: i64,
    #[schema(schema_with = described_field_89)]
    pub(crate) stack_source: Option<serde_json::Value>,
    #[schema(schema_with = described_field_90)]
    pub(crate) targets: Option<Vec<String>>,
    #[schema(schema_with = described_field_86)]
    pub(crate) value: Option<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SetsDetailResponseFiligreesEntry {
    #[schema(schema_with = described_field_22)]
    pub(crate) description: String,
    #[schema(schema_with = described_field_37)]
    pub(crate) icon: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_276)]
    pub(crate) menu: String,
    #[schema(schema_with = described_field_412)]
    pub(crate) modifiers: Vec<SetsDetailResponseFiligreesEntryModifiersEntry>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_413)]
    pub(crate) set_id: i64,
    #[schema(schema_with = described_field_414)]
    pub(crate) set_name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SetsDetailResponseItemsEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_11)]
    pub(crate) minimum_level: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_12)]
    pub(crate) slot: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SetsDetailResponseTiersEntryModifiersEntry {
    #[schema(schema_with = described_field_66)]
    pub(crate) amount_type: String,
    #[schema(schema_with = described_field_215)]
    pub(crate) amounts: Vec<i64>,
    #[schema(schema_with = described_field_68)]
    pub(crate) apply_as_item_effect: bool,
    #[schema(schema_with = described_field_69)]
    pub(crate) bonus: String,
    #[schema(schema_with = described_field_25)]
    pub(crate) bonus_type: String,
    #[schema(schema_with = described_field_70)]
    pub(crate) cap: Option<serde_json::Value>,
    #[schema(schema_with = described_field_71)]
    pub(crate) damage: Option<serde_json::Value>,
    #[schema(schema_with = described_field_72)]
    pub(crate) dice_bonus: Option<serde_json::Value>,
    #[schema(schema_with = described_field_216)]
    pub(crate) dice_damage: Option<serde_json::Value>,
    #[schema(schema_with = described_field_217)]
    pub(crate) dice_number: Option<serde_json::Value>,
    #[schema(schema_with = described_field_218)]
    pub(crate) dice_sides: Option<serde_json::Value>,
    #[schema(schema_with = described_field_76)]
    pub(crate) display_name: Option<serde_json::Value>,
    #[schema(schema_with = described_field_77)]
    pub(crate) effect_type: String,
    #[schema(schema_with = described_field_263)]
    pub(crate) extra_types: Option<Vec<String>>,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_79)]
    pub(crate) is_item_specific: bool,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_80)]
    pub(crate) percent: bool,
    #[schema(schema_with = described_field_81)]
    pub(crate) rank: Option<serde_json::Value>,
    #[schema(schema_with = described_field_219)]
    pub(crate) requirements: Vec<serde_json::Value>,
    #[schema(schema_with = described_field_88)]
    pub(crate) sort_order: i64,
    #[schema(schema_with = described_field_89)]
    pub(crate) stack_source: Option<serde_json::Value>,
    #[schema(schema_with = described_field_90)]
    pub(crate) targets: Option<Vec<String>>,
    #[schema(schema_with = described_field_86)]
    pub(crate) value: Option<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SetsDetailResponseTiersEntry {
    #[schema(schema_with = described_field_418)]
    pub(crate) enchantments: Vec<EnchantmentLine>,
    #[schema(schema_with = described_field_419)]
    pub(crate) equipped_count: i64,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_420)]
    pub(crate) modifiers: Vec<SetsDetailResponseTiersEntryModifiersEntry>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SetsDetailResponse {
    #[schema(schema_with = described_field_411)]
    pub(crate) augments: Vec<SetsDetailResponseAugmentsEntry>,
    #[schema(schema_with = described_field_415)]
    pub(crate) filigrees: Vec<SetsDetailResponseFiligreesEntry>,
    #[schema(schema_with = described_field_37)]
    pub(crate) icon: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_408)]
    pub(crate) is_filigree_set: bool,
    #[schema(schema_with = described_field_416)]
    pub(crate) items: Vec<SetsDetailResponseItemsEntry>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_421)]
    pub(crate) tiers: Vec<SetsDetailResponseTiersEntry>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SpellsPageResponseSpellsEntry {
    #[schema(schema_with = described_field_101)]
    pub(crate) cost: Option<serde_json::Value>,
    #[schema(schema_with = described_field_22)]
    pub(crate) description: String,
    #[schema(schema_with = described_field_37)]
    pub(crate) icon: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_422)]
    pub(crate) max_caster_level: i64,
    #[schema(schema_with = described_field_423)]
    pub(crate) metamagics: Vec<String>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_424)]
    pub(crate) schools: Vec<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SpellsPageResponse {
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_425)]
    pub(crate) spells: Vec<SpellsPageResponseSpellsEntry>,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SpellsDetailResponseClassesEntry {
    #[schema(schema_with = described_field_426)]
    pub(crate) class: String,
    #[schema(schema_with = described_field_427)]
    pub(crate) class_id: i64,
    #[schema(schema_with = described_field_138)]
    pub(crate) cost: i64,
    #[schema(schema_with = described_field_422)]
    pub(crate) max_caster_level: i64,
    #[schema(schema_with = described_field_141)]
    pub(crate) spell_level: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SpellsDetailResponseDamageEntry {
    #[schema(schema_with = described_field_429)]
    pub(crate) base_dice_bonus: Option<serde_json::Value>,
    #[schema(schema_with = described_field_430)]
    pub(crate) base_dice_number: Option<serde_json::Value>,
    #[schema(schema_with = described_field_431)]
    pub(crate) base_dice_sides: Option<serde_json::Value>,
    #[schema(schema_with = described_field_432)]
    pub(crate) bonus_dice_bonus: i64,
    #[schema(schema_with = described_field_433)]
    pub(crate) bonus_dice_number: i64,
    #[schema(schema_with = described_field_434)]
    pub(crate) bonus_dice_sides: i64,
    #[schema(schema_with = described_field_435)]
    pub(crate) damage: String,
    #[schema(schema_with = described_field_436)]
    pub(crate) per_caster_levels: Option<serde_json::Value>,
    #[schema(schema_with = described_field_437)]
    pub(crate) spell_power: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SpellsDetailResponseDcsEntry {
    #[schema(schema_with = described_field_253)]
    pub(crate) amount: Option<serde_json::Value>,
    #[schema(schema_with = described_field_439)]
    pub(crate) casting_stat_mod: bool,
    #[schema(schema_with = described_field_201)]
    pub(crate) dc_type: String,
    #[schema(schema_with = described_field_202)]
    pub(crate) dc_versus: String,
    #[schema(schema_with = described_field_440)]
    pub(crate) mod_abilities: Option<serde_json::Value>,
    #[schema(schema_with = described_field_424)]
    pub(crate) schools: Vec<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SpellsDetailResponse {
    #[schema(schema_with = described_field_428)]
    pub(crate) classes: Vec<SpellsDetailResponseClassesEntry>,
    #[schema(schema_with = described_field_101)]
    pub(crate) cost: Option<serde_json::Value>,
    #[schema(schema_with = described_field_438)]
    pub(crate) damage: Vec<SpellsDetailResponseDamageEntry>,
    #[schema(schema_with = described_field_441)]
    pub(crate) dcs: Vec<SpellsDetailResponseDcsEntry>,
    #[schema(schema_with = described_field_22)]
    pub(crate) description: String,
    #[schema(schema_with = described_field_37)]
    pub(crate) icon: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_422)]
    pub(crate) max_caster_level: i64,
    #[schema(schema_with = described_field_423)]
    pub(crate) metamagics: Vec<String>,
    #[schema(schema_with = described_field_143)]
    pub(crate) modifiers: Vec<serde_json::Value>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_424)]
    pub(crate) schools: Vec<String>,
    #[schema(schema_with = described_field_226)]
    pub(crate) stances: Vec<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct StancesPageResponseStancesEntryRequirementsEntry {
    #[schema(schema_with = described_field_82)]
    pub(crate) group_index: i64,
    #[schema(schema_with = described_field_83)]
    pub(crate) group_kind: String,
    #[schema(schema_with = described_field_84)]
    pub(crate) items: Vec<String>,
    #[schema(schema_with = described_field_85)]
    pub(crate) req_type: String,
    #[schema(schema_with = described_field_86)]
    pub(crate) value: Option<serde_json::Value>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct StancesPageResponseStancesEntry {
    #[schema(schema_with = described_field_270)]
    pub(crate) auto_controlled: bool,
    #[schema(schema_with = described_field_22)]
    pub(crate) description: String,
    #[schema(schema_with = described_field_442)]
    pub(crate) group_name: String,
    #[schema(schema_with = described_field_37)]
    pub(crate) icon: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_443)]
    pub(crate) incompatible: Option<serde_json::Value>,
    #[schema(schema_with = described_field_143)]
    pub(crate) modifiers: Vec<serde_json::Value>,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_444)]
    pub(crate) requirements: Vec<StancesPageResponseStancesEntryRequirementsEntry>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct StancesPageResponse {
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_445)]
    pub(crate) stances: Vec<StancesPageResponseStancesEntry>,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct StatsPageResponseStatsEntry {
    #[schema(schema_with = described_field_161)]
    pub(crate) category: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct StatsPageResponse {
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_446)]
    pub(crate) stats: Vec<StatsPageResponseStatsEntry>,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct StatsDetailResponseAugmentsAugmentsEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = schema_carrier_value)]
    pub(crate) value: Option<i64>,
    #[schema(schema_with = schema_carrier_value2)]
    pub(crate) value2: Option<i64>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct StatsDetailResponseAugments {
    #[schema(schema_with = described_field_447)]
    pub(crate) augments: Vec<StatsDetailResponseAugmentsAugmentsEntry>,
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct StatsDetailResponseItemsItemsEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = schema_carrier_value)]
    pub(crate) value: Option<i64>,
    #[schema(schema_with = schema_carrier_value2)]
    pub(crate) value2: Option<i64>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct StatsDetailResponseItems {
    #[schema(schema_with = described_field_449)]
    pub(crate) items: Vec<StatsDetailResponseItemsItemsEntry>,
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct StatsDetailResponseSetTiersSetTiersEntry {
    #[schema(schema_with = described_field_419)]
    pub(crate) equipped_count: i64,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_413)]
    pub(crate) set_id: i64,
    #[schema(schema_with = described_field_414)]
    pub(crate) set_name: String,
    #[schema(schema_with = schema_carrier_value)]
    pub(crate) value: Option<i64>,
    #[schema(schema_with = schema_carrier_value2)]
    pub(crate) value2: Option<i64>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct StatsDetailResponseSetTiers {
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_451)]
    pub(crate) set_tiers: Vec<StatsDetailResponseSetTiersSetTiersEntry>,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct StatsDetailResponse {
    #[schema(schema_with = described_field_448)]
    pub(crate) augments: StatsDetailResponseAugments,
    #[schema(schema_with = described_field_161)]
    pub(crate) category: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_450)]
    pub(crate) items: StatsDetailResponseItems,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_452)]
    pub(crate) set_tiers: StatsDetailResponseSetTiers,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct VendorsPageResponseVendorsEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_164)]
    pub(crate) item_count: i64,
    #[schema(schema_with = described_field_344)]
    pub(crate) location: String,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_149)]
    pub(crate) pack: Option<String>,
    #[schema(schema_with = described_field_49)]
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct VendorsPageResponse {
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
    #[schema(schema_with = described_field_453)]
    pub(crate) vendors: Vec<VendorsPageResponseVendorsEntry>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct VendorsDetailResponseItemsEntry {
    #[schema(schema_with = described_field_454)]
    pub(crate) cost: String,
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_9)]
    pub(crate) is_rare: bool,
    #[schema(schema_with = described_field_11)]
    pub(crate) minimum_level: i64,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_12)]
    pub(crate) slot: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct VendorsDetailResponse {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_164)]
    pub(crate) item_count: i64,
    #[schema(schema_with = described_field_455)]
    pub(crate) items: Vec<VendorsDetailResponseItemsEntry>,
    #[schema(schema_with = described_field_344)]
    pub(crate) location: String,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_96)]
    pub(crate) pack: String,
    #[schema(schema_with = described_field_49)]
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct WeaponTypesPageResponseWeaponTypesEntry {
    #[schema(schema_with = described_field_0)]
    pub(crate) id: i64,
    #[schema(schema_with = described_field_456)]
    pub(crate) is_shield: bool,
    #[schema(schema_with = described_field_2)]
    pub(crate) name: String,
    #[schema(schema_with = described_field_358)]
    pub(crate) proficiency: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct WeaponTypesPageResponse {
    #[schema(schema_with = described_field_4)]
    pub(crate) limit: i64,
    #[schema(schema_with = described_field_5)]
    pub(crate) offset: i64,
    #[schema(schema_with = described_field_6)]
    pub(crate) total: i64,
    #[schema(schema_with = described_field_457)]
    pub(crate) weapon_types: Vec<WeaponTypesPageResponseWeaponTypesEntry>,
}
