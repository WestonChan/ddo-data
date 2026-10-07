#![allow(dead_code)]

use utoipa::openapi::schema::{AllOfBuilder, AnyOfBuilder, Schema};
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

fn schema_spell_slots() -> RefOr<Schema> {
    utoipa::openapi::schema::ObjectBuilder::new()
        .additional_properties(Some(<Vec<i64> as PartialSchema>::schema()))
        .into()
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EffectStatBonus {
    #[schema(schema_with = schema_stat_name)]
    pub(crate) stat: String,
    #[schema(schema_with = schema_stat_category)]
    pub(crate) stat_category: String,
    #[schema(schema_with = schema_stat_bonus_type)]
    pub(crate) bonus_type: String,
    #[schema(schema_with = schema_stat_value)]
    pub(crate) value: i64,
    pub(crate) amount_source: String,
    pub(crate) scale: f64,
    pub(crate) group: Option<EffectBonusGroup>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EffectBonusGroup {
    pub(crate) id: i64,
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EffectTierPosition {
    pub(crate) group: String,
    pub(crate) rank: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EffectDamage {
    pub(crate) trigger: String,
    pub(crate) damage_type: String,
    pub(crate) dice_number: i64,
    pub(crate) dice_sides: i64,
    pub(crate) dice_bonus: i64,
    pub(crate) amount_from: i64,
    pub(crate) scale: f64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EffectLine {
    #[schema(schema_with = schema_line_id)]
    pub(crate) effect_id: i64,
    #[schema(schema_with = schema_line_name)]
    pub(crate) name: String,
    #[schema(schema_with = schema_line_tier)]
    pub(crate) tier: Option<EffectTierPosition>,
    #[schema(schema_with = schema_line_verbose_name)]
    pub(crate) verbose_name: String,
    #[schema(schema_with = schema_line_description)]
    pub(crate) description: Option<String>,
    #[schema(schema_with = schema_line_value)]
    pub(crate) value: Option<i64>,
    #[schema(schema_with = schema_line_value2)]
    pub(crate) value2: Option<i64>,
    #[schema(schema_with = schema_line_bonus_type)]
    pub(crate) bonus_type: Option<String>,
    #[schema(schema_with = schema_line_bonuses)]
    pub(crate) bonuses: Vec<EffectStatBonus>,
    #[schema(schema_with = schema_line_damage)]
    pub(crate) damage: Vec<EffectDamage>,
}

fn schema_stat_name() -> RefOr<Schema> {
    described_schema::<String>("Stat affected by this effect.")
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
fn schema_line_id() -> RefOr<Schema> {
    described_schema::<i64>("Identifier of the shared effect family.")
}
fn schema_line_name() -> RefOr<Schema> {
    described_schema::<String>("Concise effect label, including any qualifier.")
}
fn schema_line_tier() -> RefOr<Schema> {
    described_schema::<Option<EffectTierPosition>>("Tier group and rank, if any.")
}
fn schema_line_verbose_name() -> RefOr<Schema> {
    described_schema::<String>("Rendered effect label with its displayed type and amount.")
}
fn schema_line_description() -> RefOr<Schema> {
    described_schema::<Option<String>>("Description rendered from the family template, if any.")
}
fn schema_line_value() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Effective first amount from this owner, a default or a fixed bonus.")
}
fn schema_line_value2() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Effective second amount from this owner or the effect default.")
}
fn schema_line_bonus_type() -> RefOr<Schema> {
    described_schema::<Option<String>>("Type supplied by the link or fixed by the effect.")
}
fn schema_line_bonuses() -> RefOr<Schema> {
    described_schema::<Vec<EffectStatBonus>>("Stat bonuses derived from this owner link.")
}
fn schema_line_damage() -> RefOr<Schema> {
    described_schema::<Vec<EffectDamage>>("Triggered damage dice granted by this effect.")
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AdventurePacksPageResponseAdventurePacksEntry {
    pub(crate) id: i64,
    pub(crate) is_free_to_play: bool,
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AdventurePacksPageResponse {
    pub(crate) adventure_packs: Vec<AdventurePacksPageResponseAdventurePacksEntry>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AdventurePacksDetailResponseItemsEntry {
    pub(crate) chest: Option<String>,
    pub(crate) id: i64,
    pub(crate) is_rare: bool,
    pub(crate) loot_type: String,
    pub(crate) minimum_level: i64,
    pub(crate) name: String,
    pub(crate) slot: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AdventurePacksDetailResponse {
    pub(crate) augments: Vec<QuestsDetailResponseAugmentsEntry>,
    pub(crate) id: i64,
    pub(crate) is_free_to_play: bool,
    pub(crate) items: Vec<AdventurePacksDetailResponseItemsEntry>,
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AugmentSlotTypesPageResponseAugmentSlotTypesEntry {
    pub(crate) family: String,
    pub(crate) id: i64,
    pub(crate) label: String,
    pub(crate) qualifier: Option<String>,
    pub(crate) variant: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AugmentSlotTypesPageResponse {
    pub(crate) augment_slot_types: Vec<AugmentSlotTypesPageResponseAugmentSlotTypesEntry>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AugmentsPageResponseAugmentsEntry {
    pub(crate) adds_augment: Option<String>,
    pub(crate) choose_level: bool,
    pub(crate) crafting: Vec<AugmentsDetailResponseCraftingEntry>,
    pub(crate) description: String,
    pub(crate) dual_values: bool,
    pub(crate) effect_description: Option<String>,
    pub(crate) effects: Vec<EffectLine>,
    pub(crate) enter_value: bool,
    pub(crate) family: String,
    pub(crate) grants_augment: Option<String>,
    pub(crate) icon: Option<String>,
    pub(crate) id: i64,
    pub(crate) level_values: Option<Vec<f64>>,
    pub(crate) level_values2: Option<Vec<i64>>,
    pub(crate) levels: Option<Vec<i64>>,
    pub(crate) min_level: Option<i64>,
    pub(crate) name: String,
    pub(crate) set_bonus: Option<String>,
    pub(crate) slots: Vec<String>,
    pub(crate) suppress_set_bonus: bool,
    pub(crate) weapon_class: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AugmentsPageResponse {
    pub(crate) augments: Vec<AugmentsPageResponseAugmentsEntry>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
#[schema(description = "Pack-wide drops only.")]
pub(crate) struct AugmentsDetailResponseAdventurePacksEntry {
    pub(crate) chest: Option<String>,
    pub(crate) id: i64,
    pub(crate) is_rare: bool,
    pub(crate) loot_type: String,
    pub(crate) name: String,
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AugmentsDetailResponseCraftingEntryCostEntry {
    pub(crate) ingredient: String,
    pub(crate) quantity: i64,
    pub(crate) tier: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AugmentsDetailResponseCraftingEntry {
    pub(crate) cost: Vec<AugmentsDetailResponseCraftingEntryCostEntry>,
    pub(crate) option: String,
    pub(crate) system: String,
    pub(crate) tier: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AugmentsDetailResponseModifiersEntryRequirementsEntry {
    pub(crate) group_index: i64,
    pub(crate) group_kind: String,
    pub(crate) items: Option<Vec<String>>,
    pub(crate) req_type: String,
    pub(crate) value: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AugmentsDetailResponseModifiersEntry {
    pub(crate) amount_type: Option<String>,
    pub(crate) amounts: Option<Vec<f64>>,
    pub(crate) apply_as_item_effect: bool,
    pub(crate) bonus: Option<String>,
    pub(crate) bonus_type: Option<String>,
    pub(crate) cap: Option<String>,
    pub(crate) damage: Option<String>,
    pub(crate) dice_bonus: Option<Vec<f64>>,
    pub(crate) dice_damage: Option<String>,
    pub(crate) dice_number: Option<Vec<f64>>,
    pub(crate) dice_sides: Option<Vec<f64>>,
    pub(crate) display_name: Option<String>,
    pub(crate) effect_type: String,
    pub(crate) extra_types: Option<Vec<String>>,
    pub(crate) id: i64,
    pub(crate) is_item_specific: bool,
    pub(crate) is_rare: bool,
    pub(crate) percent: bool,
    pub(crate) rank: Option<i64>,
    pub(crate) requirements: Vec<AugmentsDetailResponseModifiersEntryRequirementsEntry>,
    pub(crate) sort_order: i64,
    pub(crate) stack_source: Option<String>,
    pub(crate) targets: Option<Vec<String>>,
    pub(crate) value: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AugmentsDetailResponseQuestsEntry {
    pub(crate) chest: Option<String>,
    pub(crate) difficulties: Vec<String>,
    pub(crate) epic_level: Option<i64>,
    pub(crate) id: i64,
    pub(crate) is_free_to_play: bool,
    pub(crate) is_raid: bool,
    pub(crate) is_rare: bool,
    pub(crate) level: i64,
    pub(crate) loot_type: String,
    pub(crate) name: String,
    pub(crate) pack: String,
    pub(crate) patron: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AugmentsDetailResponseSourcesEntry {
    pub(crate) character_level: Option<i64>,
    pub(crate) chest: Option<String>,
    pub(crate) cost: Option<String>,
    pub(crate) id: i64,
    pub(crate) is_rare: bool,
    pub(crate) kind: String,
    pub(crate) loot_type: Option<String>,
    pub(crate) name: String,
    pub(crate) tier: Option<String>,
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct AugmentsDetailResponse {
    pub(crate) adds_augment: Option<String>,
    pub(crate) adventure_packs: Vec<AugmentsDetailResponseAdventurePacksEntry>,
    pub(crate) choose_level: bool,
    pub(crate) crafting: Vec<AugmentsDetailResponseCraftingEntry>,
    pub(crate) description: String,
    pub(crate) dual_values: bool,
    pub(crate) effect_description: Option<String>,
    pub(crate) effects: Vec<EffectLine>,
    pub(crate) enter_value: bool,
    pub(crate) family: String,
    pub(crate) grants_augment: Option<String>,
    pub(crate) icon: Option<String>,
    pub(crate) id: i64,
    pub(crate) level_values: Option<Vec<f64>>,
    pub(crate) level_values2: Option<Vec<i64>>,
    pub(crate) levels: Option<Vec<i64>>,
    pub(crate) min_level: Option<i64>,
    pub(crate) modifiers: Vec<AugmentsDetailResponseModifiersEntry>,
    pub(crate) name: String,
    pub(crate) quests: Vec<AugmentsDetailResponseQuestsEntry>,
    pub(crate) set_bonus: Option<String>,
    pub(crate) slots: Vec<String>,
    pub(crate) sources: Vec<AugmentsDetailResponseSourcesEntry>,
    pub(crate) suppress_set_bonus: bool,
    pub(crate) weapon_class: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct BonusTypesPageResponseBonusTypesEntry {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) stacks_with_self: bool,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct BonusTypesPageResponse {
    pub(crate) bonus_types: Vec<BonusTypesPageResponseBonusTypesEntry>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ClassesPageResponseClassesEntry {
    pub(crate) alignments: Vec<String>,
    pub(crate) bab: Vec<f64>,
    pub(crate) base_class: Option<String>,
    pub(crate) base_class_id: Option<i64>,
    pub(crate) casting_stats: Option<Vec<String>>,
    pub(crate) class_specific_feat_types: Option<Vec<String>>,
    pub(crate) description: String,
    pub(crate) fortitude: String,
    pub(crate) hit_points: i64,
    pub(crate) id: i64,
    pub(crate) large_icon: String,
    pub(crate) name: String,
    pub(crate) not_heroic: bool,
    pub(crate) reflex: String,
    pub(crate) skill_points: i64,
    pub(crate) small_icon: String,
    pub(crate) spell_points_per_level: Vec<i64>,
    pub(crate) will: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ClassesPageResponse {
    pub(crate) classes: Vec<ClassesPageResponseClassesEntry>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ClassesDetailResponseAutomaticFeatsEntry {
    pub(crate) feat_id: Option<i64>,
    pub(crate) level: i64,
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ClassesDetailResponseFeatSlotsEntry {
    pub(crate) auto_populate: bool,
    pub(crate) feat_type: String,
    pub(crate) level: i64,
    pub(crate) singular: bool,
    pub(crate) update_list: Option<Vec<String>>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ClassesDetailResponseFeatsEntry {
    pub(crate) acquire: String,
    pub(crate) description: Option<String>,
    pub(crate) icon: String,
    pub(crate) id: i64,
    pub(crate) max_times_acquire: Option<i64>,
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ClassesDetailResponseSpellsEntry {
    pub(crate) cost: Option<i64>,
    pub(crate) max_caster_level: Option<i64>,
    pub(crate) name: String,
    pub(crate) spell_id: Option<i64>,
    pub(crate) spell_level: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ClassesDetailResponse {
    pub(crate) alignments: Vec<String>,
    pub(crate) auto_buy_skills: Vec<String>,
    pub(crate) automatic_feats: Vec<ClassesDetailResponseAutomaticFeatsEntry>,
    pub(crate) bab: Vec<f64>,
    pub(crate) base_class: Option<String>,
    pub(crate) base_class_id: Option<i64>,
    pub(crate) casting_stats: Option<Vec<String>>,
    pub(crate) class_skills: Vec<String>,
    pub(crate) class_specific_feat_types: Option<Vec<String>>,
    pub(crate) description: String,
    pub(crate) feat_slots: Vec<ClassesDetailResponseFeatSlotsEntry>,
    pub(crate) feats: Vec<ClassesDetailResponseFeatsEntry>,
    pub(crate) fortitude: String,
    pub(crate) hit_points: i64,
    pub(crate) id: i64,
    pub(crate) large_icon: String,
    pub(crate) name: String,
    pub(crate) not_heroic: bool,
    pub(crate) reflex: String,
    pub(crate) skill_points: i64,
    pub(crate) small_icon: String,
    pub(crate) spell_points_per_level: Vec<i64>,
    #[schema(schema_with = schema_spell_slots)]
    pub(crate) spell_slots: std::collections::BTreeMap<String, Vec<i64>>,
    pub(crate) spells: Vec<ClassesDetailResponseSpellsEntry>,
    pub(crate) will: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ClickiesPageResponseClickiesEntry {
    pub(crate) description: String,
    pub(crate) icon: String,
    pub(crate) id: i64,
    pub(crate) modifiers: Vec<AugmentsDetailResponseModifiersEntry>,
    pub(crate) name: String,
    pub(crate) school: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ClickiesPageResponse {
    pub(crate) clickies: Vec<ClickiesPageResponseClickiesEntry>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct CraftingSystemsPageResponseCraftingSystemsEntry {
    pub(crate) families: Vec<String>,
    pub(crate) id: i64,
    pub(crate) ingredient_count: i64,
    pub(crate) name: String,
    pub(crate) npc: Option<String>,
    pub(crate) pack: Option<String>,
    pub(crate) page: String,
    pub(crate) recipe_count: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct CraftingSystemsPageResponse {
    pub(crate) crafting_systems: Vec<CraftingSystemsPageResponseCraftingSystemsEntry>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct CraftingSystemsDetailResponseIngredientsEntry {
    pub(crate) bind: Option<String>,
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) source: Option<String>,
    pub(crate) tier: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct CraftingSystemsDetailResponseRecipesEntryAugmentsEntry {
    pub(crate) id: i64,
    pub(crate) min_level: i64,
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct CraftingSystemsDetailResponseRecipesEntryCostEntry {
    pub(crate) ingredient: String,
    pub(crate) quantity: i64,
    pub(crate) tier: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct CraftingSystemsDetailResponseRecipesEntry {
    pub(crate) augments: Vec<CraftingSystemsDetailResponseRecipesEntryAugmentsEntry>,
    pub(crate) cost: Vec<CraftingSystemsDetailResponseRecipesEntryCostEntry>,
    pub(crate) grants_slot: Option<String>,
    pub(crate) id: i64,
    pub(crate) note: Option<String>,
    pub(crate) option: String,
    pub(crate) slot: Option<String>,
    pub(crate) tier: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct CraftingSystemsDetailResponse {
    pub(crate) families: Vec<String>,
    pub(crate) id: i64,
    pub(crate) ingredient_count: i64,
    pub(crate) ingredients: Vec<CraftingSystemsDetailResponseIngredientsEntry>,
    pub(crate) name: String,
    pub(crate) npc: Option<String>,
    pub(crate) pack: Option<String>,
    pub(crate) page: String,
    pub(crate) recipe_count: i64,
    pub(crate) recipes: Vec<CraftingSystemsDetailResponseRecipesEntry>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct DamageTypesPageResponseDamageTypesEntry {
    pub(crate) category: String,
    pub(crate) id: i64,
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct DamageTypesPageResponse {
    pub(crate) damage_types: Vec<DamageTypesPageResponseDamageTypesEntry>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EffectsPageResponseEffectsEntryBonusTypesEntry {
    pub(crate) item_count: i64,
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EffectsPageResponseEffectsEntry {
    pub(crate) augment_count: i64,
    pub(crate) bonus_types: Vec<EffectsPageResponseEffectsEntryBonusTypesEntry>,
    pub(crate) detail_path: String,
    pub(crate) id: i64,
    pub(crate) item_count: i64,
    pub(crate) kind: String,
    pub(crate) name: String,
    pub(crate) set_count: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EffectsPageResponse {
    pub(crate) effects: Vec<EffectsPageResponseEffectsEntry>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) total: i64,
}

fn schema_carrier_value() -> RefOr<Schema> {
    described_schema::<Option<i64>>("First amount stated by this owner link, if any.")
}
fn schema_carrier_value2() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Second amount stated by this owner link, if any.")
}
fn schema_carrier_lines() -> RefOr<Schema> {
    described_schema::<Vec<EffectCarrierLine>>(
        "Every link of this owner to the effect, ordered as the owner lists them; the fields above repeat the first.",
    )
}
fn schema_carrier_line() -> RefOr<Schema> {
    described_schema::<EffectLine>("Rendered effect line carried by this owner link.")
}
fn schema_resolved_bonus_value() -> RefOr<Schema> {
    described_schema::<Option<i64>>("Stat bonus after applying the effect default, scale and rounding.")
}
fn schema_set_tier_carriers() -> RefOr<Schema> {
    described_schema::<Vec<EffectSetTierCarrier>>("Set tiers carrying this family or stat.")
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
fn schema_family_tier() -> RefOr<Schema> {
    described_schema::<Option<EffectTierDetail>>("Tier group and ordered steps for this effect, if any.")
}
fn schema_family_wiki_url() -> RefOr<Schema> {
    described_schema::<Option<String>>("Wiki page for this family, if known.")
}
fn schema_tier_steps() -> RefOr<Schema> {
    described_schema::<Vec<EffectTierStep>>("Effects in this tier group ordered by rank.")
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EffectCarrierBonus {
    pub(crate) stat: String,
    pub(crate) stat_category: String,
    pub(crate) bonus_type: String,
    pub(crate) value: i64,
    pub(crate) amount_source: String,
    pub(crate) scale: f64,
    pub(crate) group: Option<EffectBonusGroup>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EffectCarrierLine {
    #[schema(schema_with = schema_carrier_line)]
    pub(crate) line: EffectLine,
    #[schema(schema_with = schema_carrier_value)]
    pub(crate) value: Option<i64>,
    #[schema(schema_with = schema_carrier_value2)]
    pub(crate) value2: Option<i64>,
    pub(crate) effect_id: Option<i64>,
    pub(crate) effect: Option<String>,
    pub(crate) bonus_type: Option<String>,
    pub(crate) amount_source: Option<String>,
    pub(crate) scale: Option<f64>,
    pub(crate) bonuses: Option<Vec<EffectCarrierBonus>>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EffectsDetailResponseAugmentsAugmentsEntry {
    pub(crate) id: i64,
    pub(crate) name: String,
    #[schema(schema_with = schema_carrier_line)]
    pub(crate) line: EffectLine,
    #[schema(schema_with = schema_carrier_value)]
    pub(crate) value: Option<i64>,
    #[schema(schema_with = schema_carrier_value2)]
    pub(crate) value2: Option<i64>,
    pub(crate) effect_id: Option<i64>,
    pub(crate) effect: Option<String>,
    pub(crate) bonus_type: Option<String>,
    pub(crate) amount_source: Option<String>,
    pub(crate) scale: Option<f64>,
    pub(crate) bonuses: Option<Vec<EffectCarrierBonus>>,
    #[schema(schema_with = schema_carrier_lines)]
    pub(crate) lines: Vec<EffectCarrierLine>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EffectsDetailResponseAugments {
    pub(crate) augments: Vec<EffectsDetailResponseAugmentsAugmentsEntry>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EffectsDetailResponseItemsItemsEntry {
    pub(crate) id: i64,
    pub(crate) name: String,
    #[schema(schema_with = schema_carrier_line)]
    pub(crate) line: EffectLine,
    #[schema(schema_with = schema_carrier_value)]
    pub(crate) value: Option<i64>,
    #[schema(schema_with = schema_carrier_value2)]
    pub(crate) value2: Option<i64>,
    pub(crate) effect_id: Option<i64>,
    pub(crate) effect: Option<String>,
    pub(crate) bonus_type: Option<String>,
    pub(crate) amount_source: Option<String>,
    pub(crate) scale: Option<f64>,
    pub(crate) bonuses: Option<Vec<EffectCarrierBonus>>,
    #[schema(schema_with = schema_carrier_lines)]
    pub(crate) lines: Vec<EffectCarrierLine>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EffectsDetailResponseItems {
    pub(crate) items: Vec<EffectsDetailResponseItemsItemsEntry>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EffectSetTierCarrier {
    pub(crate) id: i64,
    pub(crate) name: String,
    #[schema(schema_with = schema_carrier_line)]
    pub(crate) line: EffectLine,
    pub(crate) set_id: i64,
    pub(crate) set_name: String,
    pub(crate) equipped_count: i64,
    #[schema(schema_with = schema_carrier_value)]
    pub(crate) value: Option<i64>,
    #[schema(schema_with = schema_carrier_value2)]
    pub(crate) value2: Option<i64>,
    pub(crate) effect_id: Option<i64>,
    pub(crate) effect: Option<String>,
    pub(crate) bonus_type: Option<String>,
    pub(crate) amount_source: Option<String>,
    pub(crate) scale: Option<f64>,
    pub(crate) bonuses: Option<Vec<EffectCarrierBonus>>,
    #[schema(schema_with = schema_carrier_lines)]
    pub(crate) lines: Vec<EffectCarrierLine>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EffectsDetailResponseSetTiers {
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    #[schema(schema_with = schema_set_tier_carriers)]
    pub(crate) set_tiers: Vec<EffectSetTierCarrier>,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EffectsDetailResponseBonusesEntry {
    pub(crate) amount_from: i64,
    #[schema(schema_with = schema_stat_rule_bonus_type)]
    pub(crate) bonus_type: Option<String>,
    #[schema(schema_with = schema_stat_rule_constant)]
    pub(crate) constant: Option<i64>,
    pub(crate) rounding: String,
    pub(crate) scale: f64,
    pub(crate) target: String,
    pub(crate) target_kind: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EffectTierStep {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) rank: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EffectTierDetail {
    pub(crate) group: String,
    pub(crate) rank: i64,
    #[schema(schema_with = schema_tier_steps)]
    pub(crate) steps: Vec<EffectTierStep>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EffectsDetailResponse {
    pub(crate) augments: EffectsDetailResponseAugments,
    #[schema(schema_with = schema_family_default_value)]
    pub(crate) default_value: Option<i64>,
    #[schema(schema_with = schema_family_default_value2)]
    pub(crate) default_value2: Option<i64>,
    #[schema(schema_with = schema_family_description_template)]
    pub(crate) description_template: Option<String>,
    pub(crate) id: i64,
    pub(crate) items: EffectsDetailResponseItems,
    pub(crate) kind: String,
    pub(crate) category: Option<String>,
    #[schema(schema_with = schema_family_tier)]
    pub(crate) tier: Option<EffectTierDetail>,
    pub(crate) name: String,
    pub(crate) set_tiers: EffectsDetailResponseSetTiers,
    pub(crate) bonuses: Vec<EffectsDetailResponseBonusesEntry>,
    pub(crate) damage: Vec<EffectDamage>,
    pub(crate) verbose_name_template: Option<String>,
    #[schema(schema_with = schema_family_wiki_url)]
    pub(crate) wiki_url: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnhancementTreesPageResponseEnhancementTreesEntryRequirementsEntry {
    pub(crate) group_index: i64,
    pub(crate) group_kind: String,
    pub(crate) items: Option<Vec<String>>,
    pub(crate) req_type: String,
    pub(crate) value: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnhancementTreesPageResponseEnhancementTreesEntry {
    pub(crate) background: String,
    pub(crate) enhancement_count: i64,
    pub(crate) icon: String,
    pub(crate) id: i64,
    pub(crate) is_legacy: bool,
    pub(crate) kind: String,
    pub(crate) name: String,
    pub(crate) requirements: Vec<EnhancementTreesPageResponseEnhancementTreesEntryRequirementsEntry>,
    pub(crate) version: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnhancementTreesPageResponse {
    pub(crate) enhancement_trees: Vec<EnhancementTreesPageResponseEnhancementTreesEntry>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnhancementTreesDetailResponseEnhancementsEntryDcsEntry {
    pub(crate) amount: Option<Vec<f64>>,
    pub(crate) base_class_level: Option<String>,
    pub(crate) class_level: Option<String>,
    pub(crate) dc_type: String,
    pub(crate) dc_versus: String,
    pub(crate) description: String,
    pub(crate) icon: String,
    pub(crate) mod_ability: Option<Vec<String>>,
    pub(crate) name: String,
    pub(crate) other: Option<String>,
    pub(crate) skill: Option<String>,
    pub(crate) tactical: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnhancementTreesDetailResponseEnhancementsEntryModifiersEntry {
    pub(crate) amount_type: Option<String>,
    pub(crate) amounts: Option<Vec<f64>>,
    pub(crate) apply_as_item_effect: bool,
    pub(crate) bonus: Option<String>,
    pub(crate) bonus_type: Option<String>,
    pub(crate) cap: Option<String>,
    pub(crate) damage: Option<String>,
    pub(crate) dice_bonus: Option<Vec<f64>>,
    pub(crate) dice_damage: Option<String>,
    pub(crate) dice_number: Option<Vec<f64>>,
    pub(crate) dice_sides: Option<Vec<f64>>,
    pub(crate) display_name: Option<String>,
    pub(crate) effect_type: String,
    pub(crate) extra_types: Option<Vec<String>>,
    pub(crate) id: i64,
    pub(crate) is_item_specific: bool,
    pub(crate) is_rare: bool,
    pub(crate) percent: bool,
    pub(crate) rank: Option<i64>,
    pub(crate) requirements: Vec<AugmentsDetailResponseModifiersEntryRequirementsEntry>,
    pub(crate) sort_order: i64,
    pub(crate) stack_source: Option<String>,
    pub(crate) targets: Option<Vec<String>>,
    pub(crate) value: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnhancementTreesDetailResponseEnhancementsEntryRequirementsEntry {
    pub(crate) group_index: i64,
    pub(crate) group_kind: String,
    pub(crate) items: Option<Vec<String>>,
    pub(crate) req_type: String,
    pub(crate) value: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnhancementTreesDetailResponseEnhancementsEntry {
    pub(crate) arrows: Option<Vec<String>>,
    pub(crate) attack: Option<FeatsDetailResponseAttack>,
    pub(crate) cooldown_seconds: Option<i64>,
    pub(crate) cost_per_rank: Vec<i64>,
    pub(crate) dcs: Vec<EnhancementTreesDetailResponseEnhancementsEntryDcsEntry>,
    pub(crate) description: String,
    pub(crate) duration_seconds: Option<i64>,
    pub(crate) exclusions: Vec<String>,
    pub(crate) follow_on_modifiers: Vec<AugmentsDetailResponseModifiersEntry>,
    pub(crate) icon: String,
    pub(crate) id: i64,
    pub(crate) internal_name: String,
    pub(crate) is_clickie: bool,
    pub(crate) is_tier5: bool,
    pub(crate) min_spent: i64,
    pub(crate) modifiers: Vec<EnhancementTreesDetailResponseEnhancementsEntryModifiersEntry>,
    pub(crate) name: String,
    pub(crate) ranks: i64,
    pub(crate) requirements: Vec<EnhancementTreesDetailResponseEnhancementsEntryRequirementsEntry>,
    pub(crate) selections: Vec<EnhancementTreesDetailResponseEnhancementsEntrySelectionsEntry>,
    pub(crate) stances: Vec<FeatsDetailResponseStancesEntry>,
    pub(crate) this_attack_modifiers: Vec<AugmentsDetailResponseModifiersEntry>,
    pub(crate) x: i64,
    pub(crate) y: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnhancementTreesDetailResponseRequirementsEntry {
    pub(crate) group_index: i64,
    pub(crate) group_kind: String,
    pub(crate) items: Option<Vec<String>>,
    pub(crate) req_type: String,
    pub(crate) value: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnhancementTreesDetailResponse {
    pub(crate) background: String,
    pub(crate) enhancement_count: i64,
    pub(crate) enhancements: Vec<EnhancementTreesDetailResponseEnhancementsEntry>,
    pub(crate) icon: String,
    pub(crate) id: i64,
    pub(crate) is_legacy: bool,
    pub(crate) kind: String,
    pub(crate) name: String,
    pub(crate) requirements: Vec<EnhancementTreesDetailResponseRequirementsEntry>,
    pub(crate) version: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EquipmentSlotsPageResponseEquipmentSlotsEntry {
    pub(crate) category: String,
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) sort_order: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EquipmentSlotsPageResponse {
    pub(crate) equipment_slots: Vec<EquipmentSlotsPageResponseEquipmentSlotsEntry>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EventsPageResponseEventsEntry {
    pub(crate) id: i64,
    pub(crate) item_count: i64,
    pub(crate) name: String,
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EventsPageResponse {
    pub(crate) events: Vec<EventsPageResponseEventsEntry>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EventsDetailResponseItemsEntry {
    pub(crate) id: i64,
    pub(crate) is_rare: bool,
    pub(crate) minimum_level: i64,
    pub(crate) name: String,
    pub(crate) slot: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EventsDetailResponse {
    pub(crate) id: i64,
    pub(crate) item_count: i64,
    pub(crate) items: Vec<EventsDetailResponseItemsEntry>,
    pub(crate) name: String,
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsPageResponseFeatsEntry {
    pub(crate) acquire: String,
    pub(crate) auto_acquire_ignores_requirements: bool,
    pub(crate) description: Option<String>,
    pub(crate) groups: Vec<String>,
    pub(crate) icon: String,
    pub(crate) id: i64,
    pub(crate) max_times_acquire: Option<i64>,
    pub(crate) name: String,
    pub(crate) source_id: Option<i64>,
    pub(crate) source_kind: String,
    pub(crate) source_name: Option<String>,
    pub(crate) sphere: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsPageResponse {
    pub(crate) feats: Vec<FeatsPageResponseFeatsEntry>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponseAttack {
    pub(crate) cooldown_seconds: Option<i64>,
    pub(crate) description: Option<String>,
    pub(crate) duration_seconds: Option<i64>,
    pub(crate) icon: Option<String>,
    pub(crate) name: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponseAutoAcquireRequirementsEntry {
    pub(crate) group_index: i64,
    pub(crate) group_kind: String,
    pub(crate) items: Option<Vec<String>>,
    pub(crate) req_type: String,
    pub(crate) value: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponseBonusesEntry {
    pub(crate) bonus_type: String,
    pub(crate) description: Option<String>,
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) stat: String,
    pub(crate) stat_category: String,
    pub(crate) value: i64,
    pub(crate) value2: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponseConditionalGroupsEntryRequirementsEntry {
    pub(crate) group_index: i64,
    pub(crate) group_kind: String,
    pub(crate) items: Option<Vec<String>>,
    pub(crate) req_type: String,
    pub(crate) value: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponseConditionalGroupsEntry {
    pub(crate) groups: Vec<String>,
    pub(crate) id: i64,
    pub(crate) requirements: Vec<FeatsDetailResponseConditionalGroupsEntryRequirementsEntry>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponseDcsEntry {
    pub(crate) amount: Option<Vec<f64>>,
    pub(crate) base_class_level: Option<String>,
    pub(crate) class_level: Option<String>,
    pub(crate) dc_type: String,
    pub(crate) dc_versus: String,
    pub(crate) description: String,
    pub(crate) icon: String,
    pub(crate) mod_ability: Option<Vec<String>>,
    pub(crate) name: String,
    pub(crate) other: Option<String>,
    pub(crate) skill: Option<String>,
    pub(crate) tactical: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponseFollowOnModifiersEntry {
    pub(crate) amount_type: Option<String>,
    pub(crate) amounts: Option<Vec<f64>>,
    pub(crate) apply_as_item_effect: bool,
    pub(crate) bonus: Option<String>,
    pub(crate) bonus_type: Option<String>,
    pub(crate) cap: Option<String>,
    pub(crate) damage: Option<String>,
    pub(crate) dice_bonus: Option<Vec<f64>>,
    pub(crate) dice_damage: Option<String>,
    pub(crate) dice_number: Option<Vec<f64>>,
    pub(crate) dice_sides: Option<Vec<f64>>,
    pub(crate) display_name: Option<String>,
    pub(crate) effect_type: String,
    pub(crate) extra_types: Option<Vec<String>>,
    pub(crate) id: i64,
    pub(crate) is_item_specific: bool,
    pub(crate) is_rare: bool,
    pub(crate) percent: bool,
    pub(crate) rank: Option<i64>,
    pub(crate) requirements: Vec<AugmentsDetailResponseModifiersEntryRequirementsEntry>,
    pub(crate) sort_order: i64,
    pub(crate) stack_source: Option<String>,
    pub(crate) targets: Option<Vec<String>>,
    pub(crate) value: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponseModifiersEntryRequirementsEntry {
    pub(crate) group_index: i64,
    pub(crate) group_kind: String,
    pub(crate) items: Option<Vec<String>>,
    pub(crate) req_type: String,
    pub(crate) value: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponseModifiersEntry {
    pub(crate) amount_type: Option<String>,
    pub(crate) amounts: Option<Vec<f64>>,
    pub(crate) apply_as_item_effect: bool,
    pub(crate) bonus: Option<String>,
    pub(crate) bonus_type: Option<String>,
    pub(crate) cap: Option<String>,
    pub(crate) damage: Option<String>,
    pub(crate) dice_bonus: Option<Vec<f64>>,
    pub(crate) dice_damage: Option<String>,
    pub(crate) dice_number: Option<Vec<f64>>,
    pub(crate) dice_sides: Option<Vec<f64>>,
    pub(crate) display_name: Option<String>,
    pub(crate) effect_type: String,
    pub(crate) extra_types: Option<Vec<String>>,
    pub(crate) id: i64,
    pub(crate) is_item_specific: bool,
    pub(crate) is_rare: bool,
    pub(crate) percent: bool,
    pub(crate) rank: Option<i64>,
    pub(crate) requirements: Vec<AugmentsDetailResponseModifiersEntryRequirementsEntry>,
    pub(crate) sort_order: i64,
    pub(crate) stack_source: Option<String>,
    pub(crate) targets: Option<Vec<String>>,
    pub(crate) value: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponseRequirementsEntry {
    pub(crate) group_index: i64,
    pub(crate) group_kind: String,
    pub(crate) items: Option<Vec<String>>,
    pub(crate) req_type: String,
    pub(crate) value: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponseStancesEntry {
    pub(crate) auto_controlled: bool,
    pub(crate) description: String,
    pub(crate) group_name: Option<String>,
    pub(crate) icon: String,
    pub(crate) id: i64,
    pub(crate) incompatible: Option<Vec<String>>,
    pub(crate) modifiers: Vec<AugmentsDetailResponseModifiersEntry>,
    pub(crate) name: String,
    pub(crate) requirements: Vec<AugmentsDetailResponseModifiersEntryRequirementsEntry>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponseSubItemsEntry {
    pub(crate) description: String,
    pub(crate) icon: String,
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponseThisAttackModifiersEntry {
    pub(crate) amount_type: Option<String>,
    pub(crate) amounts: Option<Vec<f64>>,
    pub(crate) apply_as_item_effect: bool,
    pub(crate) bonus: Option<String>,
    pub(crate) bonus_type: Option<String>,
    pub(crate) cap: Option<String>,
    pub(crate) damage: Option<String>,
    pub(crate) dice_bonus: Option<Vec<f64>>,
    pub(crate) dice_damage: Option<String>,
    pub(crate) dice_number: Option<Vec<f64>>,
    pub(crate) dice_sides: Option<Vec<f64>>,
    pub(crate) display_name: Option<String>,
    pub(crate) effect_type: String,
    pub(crate) extra_types: Option<Vec<String>>,
    pub(crate) id: i64,
    pub(crate) is_item_specific: bool,
    pub(crate) is_rare: bool,
    pub(crate) percent: bool,
    pub(crate) rank: Option<i64>,
    pub(crate) requirements: Vec<AugmentsDetailResponseModifiersEntryRequirementsEntry>,
    pub(crate) sort_order: i64,
    pub(crate) stack_source: Option<String>,
    pub(crate) targets: Option<Vec<String>>,
    pub(crate) value: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FeatsDetailResponse {
    pub(crate) acquire: String,
    pub(crate) attack: Option<FeatsDetailResponseAttack>,
    pub(crate) auto_acquire_ignores_requirements: bool,
    pub(crate) auto_acquire_requirements: Vec<FeatsDetailResponseAutoAcquireRequirementsEntry>,
    pub(crate) bonuses: Vec<FeatsDetailResponseBonusesEntry>,
    pub(crate) conditional_groups: Vec<FeatsDetailResponseConditionalGroupsEntry>,
    pub(crate) dcs: Vec<FeatsDetailResponseDcsEntry>,
    pub(crate) description: Option<String>,
    pub(crate) follow_on_modifiers: Vec<FeatsDetailResponseFollowOnModifiersEntry>,
    pub(crate) groups: Vec<String>,
    pub(crate) icon: String,
    pub(crate) id: i64,
    pub(crate) max_times_acquire: Option<i64>,
    pub(crate) modifiers: Vec<FeatsDetailResponseModifiersEntry>,
    pub(crate) name: String,
    pub(crate) requirements: Vec<FeatsDetailResponseRequirementsEntry>,
    pub(crate) source_id: Option<i64>,
    pub(crate) source_kind: String,
    pub(crate) source_name: Option<String>,
    pub(crate) sphere: Option<String>,
    pub(crate) stances: Vec<FeatsDetailResponseStancesEntry>,
    pub(crate) sub_items: Vec<FeatsDetailResponseSubItemsEntry>,
    pub(crate) this_attack_modifiers: Vec<FeatsDetailResponseThisAttackModifiersEntry>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FiligreesPageResponseFiligreesEntryModifiersEntry {
    pub(crate) amount_type: Option<String>,
    pub(crate) amounts: Option<Vec<f64>>,
    pub(crate) apply_as_item_effect: bool,
    pub(crate) bonus: Option<String>,
    pub(crate) bonus_type: Option<String>,
    pub(crate) cap: Option<String>,
    pub(crate) damage: Option<String>,
    pub(crate) dice_bonus: Option<Vec<f64>>,
    pub(crate) dice_damage: Option<String>,
    pub(crate) dice_number: Option<Vec<f64>>,
    pub(crate) dice_sides: Option<Vec<f64>>,
    pub(crate) display_name: Option<String>,
    pub(crate) effect_type: String,
    pub(crate) extra_types: Option<Vec<String>>,
    pub(crate) id: i64,
    pub(crate) is_item_specific: bool,
    pub(crate) is_rare: bool,
    pub(crate) percent: bool,
    pub(crate) rank: Option<i64>,
    pub(crate) requirements: Vec<AugmentsDetailResponseModifiersEntryRequirementsEntry>,
    pub(crate) sort_order: i64,
    pub(crate) stack_source: Option<String>,
    pub(crate) targets: Option<Vec<String>>,
    pub(crate) value: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FiligreesPageResponseFiligreesEntry {
    pub(crate) description: String,
    pub(crate) icon: String,
    pub(crate) id: i64,
    pub(crate) menu: String,
    pub(crate) modifiers: Vec<FiligreesPageResponseFiligreesEntryModifiersEntry>,
    pub(crate) name: String,
    pub(crate) set_id: Option<i64>,
    pub(crate) set_name: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct FiligreesPageResponse {
    pub(crate) filigrees: Vec<FiligreesPageResponseFiligreesEntry>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct GuildBuffsPageResponseGuildBuffsEntryModifiersEntry {
    pub(crate) amount_type: Option<String>,
    pub(crate) amounts: Option<Vec<f64>>,
    pub(crate) apply_as_item_effect: bool,
    pub(crate) bonus: Option<String>,
    pub(crate) bonus_type: Option<String>,
    pub(crate) cap: Option<String>,
    pub(crate) damage: Option<String>,
    pub(crate) dice_bonus: Option<Vec<f64>>,
    pub(crate) dice_damage: Option<String>,
    pub(crate) dice_number: Option<Vec<f64>>,
    pub(crate) dice_sides: Option<Vec<f64>>,
    pub(crate) display_name: Option<String>,
    pub(crate) effect_type: String,
    pub(crate) extra_types: Option<Vec<String>>,
    pub(crate) id: i64,
    pub(crate) is_item_specific: bool,
    pub(crate) is_rare: bool,
    pub(crate) percent: bool,
    pub(crate) rank: Option<i64>,
    pub(crate) requirements: Vec<AugmentsDetailResponseModifiersEntryRequirementsEntry>,
    pub(crate) sort_order: i64,
    pub(crate) stack_source: Option<String>,
    pub(crate) targets: Option<Vec<String>>,
    pub(crate) value: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct GuildBuffsPageResponseGuildBuffsEntry {
    pub(crate) description: String,
    pub(crate) guild_level: i64,
    pub(crate) id: i64,
    pub(crate) modifiers: Vec<GuildBuffsPageResponseGuildBuffsEntryModifiersEntry>,
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct GuildBuffsPageResponse {
    pub(crate) guild_buffs: Vec<GuildBuffsPageResponseGuildBuffsEntry>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsPageResponseItemsEntry {
    pub(crate) category: String,
    pub(crate) enhancement_bonus: Option<i64>,
    pub(crate) icon: Option<String>,
    pub(crate) id: i64,
    pub(crate) is_legacy: bool,
    pub(crate) is_raid: bool,
    pub(crate) is_rare: bool,
    pub(crate) item_type: Option<String>,
    pub(crate) minimum_level: i64,
    pub(crate) name: String,
    pub(crate) pack: Option<String>,
    #[schema(schema_with = schema_item_first_set)]
    pub(crate) set: Option<String>,
    pub(crate) slot: String,
}

fn schema_item_first_set() -> RefOr<Schema> {
    described_schema::<Option<String>>("First set name on the item, or null when it belongs to none.")
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsPageResponse {
    #[schema(schema_with = schema_item_page_rows)]
    pub(crate) items: Vec<ItemsPageResponseItemsEntry>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) total: i64,
}

fn schema_item_page_rows() -> RefOr<Schema> {
    AnyOfBuilder::new()
        .item(<Vec<ItemsPageResponseItemsEntry> as PartialSchema>::schema())
        .item(<Vec<ItemsDetailResponse> as PartialSchema>::schema())
        .into()
}

#[derive(utoipa::ToSchema)]
#[schema(description = "Pack-wide drops only.")]
pub(crate) struct ItemsDetailResponseAdventurePacksEntry {
    pub(crate) chest: Option<String>,
    pub(crate) id: i64,
    pub(crate) is_rare: bool,
    pub(crate) loot_type: Option<String>,
    pub(crate) name: String,
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseArmor {
    pub(crate) adamantine_body: Option<i64>,
    pub(crate) arcane_spell_failure: Option<i64>,
    pub(crate) armor_bonus: Option<i64>,
    pub(crate) armor_check_penalty: Option<i64>,
    pub(crate) armor_type: String,
    pub(crate) damage_reduction: Option<i64>,
    pub(crate) max_dex_bonus: Option<i64>,
    pub(crate) mithral_body: Option<i64>,
    pub(crate) shield_bonus: Option<i64>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseAugmentSlotsEntryOptionsEntryModifiersEntry {
    pub(crate) amount_type: Option<String>,
    pub(crate) amounts: Option<Vec<f64>>,
    pub(crate) apply_as_item_effect: bool,
    pub(crate) bonus: Option<String>,
    pub(crate) bonus_type: Option<String>,
    pub(crate) cap: Option<String>,
    pub(crate) damage: Option<String>,
    pub(crate) dice_bonus: Option<Vec<f64>>,
    pub(crate) dice_damage: Option<String>,
    pub(crate) dice_number: Option<Vec<f64>>,
    pub(crate) dice_sides: Option<Vec<f64>>,
    pub(crate) display_name: Option<String>,
    pub(crate) effect_type: String,
    pub(crate) extra_types: Option<Vec<String>>,
    pub(crate) id: i64,
    pub(crate) is_item_specific: bool,
    pub(crate) is_rare: bool,
    pub(crate) percent: bool,
    pub(crate) rank: Option<i64>,
    pub(crate) requirements: Vec<AugmentsDetailResponseModifiersEntryRequirementsEntry>,
    pub(crate) sort_order: i64,
    pub(crate) stack_source: Option<String>,
    pub(crate) targets: Option<Vec<String>>,
    pub(crate) value: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseAugmentSlotsEntryOptionsEntrySetsEntry {
    pub(crate) id: i64,
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseAugmentSlotsEntryOptionsEntry {
    pub(crate) description: Option<String>,
    pub(crate) effects: Vec<EffectLine>,
    pub(crate) grants_slot: Option<String>,
    pub(crate) icon: Option<String>,
    pub(crate) id: i64,
    pub(crate) min_level: Option<i64>,
    pub(crate) modifiers: Vec<ItemsDetailResponseAugmentSlotsEntryOptionsEntryModifiersEntry>,
    pub(crate) name: String,
    pub(crate) sets: Vec<ItemsDetailResponseAugmentSlotsEntryOptionsEntrySetsEntry>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseAugmentSlotsEntry {
    pub(crate) family: String,
    pub(crate) label: String,
    pub(crate) options: Vec<ItemsDetailResponseAugmentSlotsEntryOptionsEntry>,
    pub(crate) qualifier: Option<String>,
    pub(crate) slot_type_id: i64,
    pub(crate) sort_order: i64,
    pub(crate) variant: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseChallengePacksEntry {
    pub(crate) id: i64,
    pub(crate) is_rare: bool,
    pub(crate) name: String,
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseClickiesEntry {
    pub(crate) clickie_id: Option<i64>,
    pub(crate) description: Option<String>,
    pub(crate) icon: Option<String>,
    pub(crate) name: String,
    pub(crate) spell_id: Option<i64>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseCraftingSystemsEntry {
    pub(crate) id: i64,
    pub(crate) is_rare: bool,
    pub(crate) name: String,
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseEventsEntry {
    pub(crate) id: i64,
    pub(crate) is_rare: bool,
    pub(crate) name: String,
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseModifiersEntry {
    pub(crate) amount_type: Option<String>,
    pub(crate) amounts: Option<Vec<f64>>,
    pub(crate) apply_as_item_effect: bool,
    pub(crate) bonus: Option<String>,
    pub(crate) bonus_type: Option<String>,
    pub(crate) cap: Option<String>,
    pub(crate) damage: Option<String>,
    pub(crate) dice_bonus: Option<Vec<f64>>,
    pub(crate) dice_damage: Option<String>,
    pub(crate) dice_number: Option<Vec<f64>>,
    pub(crate) dice_sides: Option<Vec<f64>>,
    pub(crate) display_name: Option<String>,
    pub(crate) effect_type: String,
    pub(crate) extra_types: Option<Vec<String>>,
    pub(crate) id: i64,
    pub(crate) is_item_specific: bool,
    pub(crate) is_rare: bool,
    pub(crate) percent: bool,
    pub(crate) rank: Option<i64>,
    pub(crate) requirements: Vec<AugmentsDetailResponseModifiersEntryRequirementsEntry>,
    pub(crate) sort_order: i64,
    pub(crate) stack_source: Option<String>,
    pub(crate) targets: Option<Vec<String>>,
    pub(crate) value: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseQuestChainsEntry {
    pub(crate) id: i64,
    pub(crate) is_rare: bool,
    pub(crate) name: String,
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseQuestsEntry {
    pub(crate) chest: Option<String>,
    pub(crate) difficulties: Vec<String>,
    pub(crate) epic_level: Option<i64>,
    pub(crate) id: i64,
    pub(crate) is_free_to_play: bool,
    pub(crate) is_raid: bool,
    pub(crate) is_rare: bool,
    pub(crate) level: i64,
    pub(crate) loot_type: String,
    pub(crate) name: String,
    pub(crate) pack: String,
    pub(crate) patron: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseSagasEntry {
    pub(crate) id: i64,
    pub(crate) is_rare: bool,
    pub(crate) name: String,
    pub(crate) tier: Option<String>,
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseSet {
    pub(crate) icon: String,
    pub(crate) id: i64,
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseSourcesEntry {
    pub(crate) character_level: Option<i64>,
    pub(crate) chest: Option<String>,
    pub(crate) cost: Option<String>,
    pub(crate) id: Option<i64>,
    pub(crate) is_rare: bool,
    pub(crate) kind: String,
    pub(crate) loot_type: Option<String>,
    pub(crate) name: String,
    pub(crate) tier: Option<String>,
    pub(crate) wiki_url: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseStarterRewardsEntry {
    pub(crate) character_level: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseVendorsEntry {
    pub(crate) cost: Option<String>,
    pub(crate) id: i64,
    pub(crate) is_rare: bool,
    pub(crate) location: String,
    pub(crate) name: String,
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponseWeapon {
    pub(crate) attack_modifier: Option<String>,
    pub(crate) base_dice_bonus: Option<i64>,
    pub(crate) base_dice_count: Option<i64>,
    pub(crate) base_dice_sides: Option<i64>,
    pub(crate) critical: Option<String>,
    pub(crate) critical_multiplier: Option<i64>,
    pub(crate) critical_threat_range: Option<i64>,
    pub(crate) damage: Option<String>,
    pub(crate) damage_modifier: Option<String>,
    pub(crate) damage_multiplier: Option<f64>,
    pub(crate) dr_bypass: Vec<String>,
    pub(crate) handedness: String,
    pub(crate) proficiency: Option<String>,
    pub(crate) weapon_type: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct ItemsDetailResponse {
    pub(crate) accepts_sentience: bool,
    pub(crate) adventure_packs: Vec<ItemsDetailResponseAdventurePacksEntry>,
    pub(crate) armor: Option<ItemsDetailResponseArmor>,
    pub(crate) augment_slots: Vec<ItemsDetailResponseAugmentSlotsEntry>,
    pub(crate) category: String,
    pub(crate) challenge_packs: Vec<ItemsDetailResponseChallengePacksEntry>,
    pub(crate) clickies: Vec<ItemsDetailResponseClickiesEntry>,
    pub(crate) crafting_systems: Vec<ItemsDetailResponseCraftingSystemsEntry>,
    pub(crate) description: Option<String>,
    pub(crate) drop_location: Option<String>,
    pub(crate) effects: Vec<EffectLine>,
    pub(crate) enhancement_bonus: Option<i64>,
    pub(crate) events: Vec<ItemsDetailResponseEventsEntry>,
    pub(crate) icon: Option<String>,
    pub(crate) id: i64,
    pub(crate) is_legacy: bool,
    pub(crate) is_minor_artifact: bool,
    pub(crate) item_type: Option<String>,
    pub(crate) material: Option<String>,
    pub(crate) minimum_level: i64,
    pub(crate) modifiers: Vec<ItemsDetailResponseModifiersEntry>,
    pub(crate) name: String,
    pub(crate) quest_chains: Vec<ItemsDetailResponseQuestChainsEntry>,
    pub(crate) quests: Vec<ItemsDetailResponseQuestsEntry>,
    pub(crate) race_required: Option<String>,
    pub(crate) sagas: Vec<ItemsDetailResponseSagasEntry>,
    pub(crate) set: Option<ItemsDetailResponseSet>,
    pub(crate) set_name: Option<String>,
    pub(crate) slot: String,
    pub(crate) sources: Vec<ItemsDetailResponseSourcesEntry>,
    pub(crate) starter_rewards: Vec<ItemsDetailResponseStarterRewardsEntry>,
    pub(crate) vendors: Vec<ItemsDetailResponseVendorsEntry>,
    pub(crate) weapon: Option<ItemsDetailResponseWeapon>,
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct OptionalBuffsPageResponseOptionalBuffsEntryModifiersEntryRequirementsEntry {
    pub(crate) group_index: i64,
    pub(crate) group_kind: String,
    pub(crate) items: Option<Vec<String>>,
    pub(crate) req_type: String,
    pub(crate) value: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct OptionalBuffsPageResponseOptionalBuffsEntryModifiersEntry {
    pub(crate) amount_type: Option<String>,
    pub(crate) amounts: Option<Vec<f64>>,
    pub(crate) apply_as_item_effect: bool,
    pub(crate) bonus: Option<String>,
    pub(crate) bonus_type: Option<String>,
    pub(crate) cap: Option<String>,
    pub(crate) damage: Option<String>,
    pub(crate) dice_bonus: Option<Vec<f64>>,
    pub(crate) dice_damage: Option<String>,
    pub(crate) dice_number: Option<Vec<f64>>,
    pub(crate) dice_sides: Option<Vec<f64>>,
    pub(crate) display_name: Option<String>,
    pub(crate) effect_type: String,
    pub(crate) extra_types: Option<Vec<String>>,
    pub(crate) id: i64,
    pub(crate) is_item_specific: bool,
    pub(crate) is_rare: bool,
    pub(crate) percent: bool,
    pub(crate) rank: Option<i64>,
    pub(crate) requirements: Vec<AugmentsDetailResponseModifiersEntryRequirementsEntry>,
    pub(crate) sort_order: i64,
    pub(crate) stack_source: Option<String>,
    pub(crate) targets: Option<Vec<String>>,
    pub(crate) value: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct OptionalBuffsPageResponseOptionalBuffsEntry {
    pub(crate) description: String,
    pub(crate) icon: String,
    pub(crate) id: i64,
    pub(crate) modifiers: Vec<OptionalBuffsPageResponseOptionalBuffsEntryModifiersEntry>,
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct OptionalBuffsPageResponse {
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) optional_buffs: Vec<OptionalBuffsPageResponseOptionalBuffsEntry>,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct PatronsPageResponsePatronsEntry {
    pub(crate) id: i64,
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct PatronsPageResponse {
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) patrons: Vec<PatronsPageResponsePatronsEntry>,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct QuestChainsPageResponseQuestChainsEntry {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) pack: Option<String>,
    pub(crate) quest_count: i64,
    pub(crate) reward_count: i64,
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct QuestChainsPageResponse {
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) quest_chains: Vec<QuestChainsPageResponseQuestChainsEntry>,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct QuestChainsDetailResponseQuestsEntry {
    pub(crate) id: i64,
    pub(crate) level: i64,
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct QuestChainsDetailResponseRewardsEntry {
    pub(crate) id: i64,
    pub(crate) is_rare: bool,
    pub(crate) minimum_level: i64,
    pub(crate) name: String,
    pub(crate) slot: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct QuestChainsDetailResponse {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) pack: Option<String>,
    pub(crate) quest_count: i64,
    pub(crate) quests: Vec<QuestChainsDetailResponseQuestsEntry>,
    pub(crate) reward_count: i64,
    pub(crate) rewards: Vec<QuestChainsDetailResponseRewardsEntry>,
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct QuestsPageResponseQuestsEntry {
    pub(crate) bestowed_by: Option<String>,
    pub(crate) difficulties: Vec<String>,
    pub(crate) epic_level: Option<i64>,
    pub(crate) epic_name: Option<String>,
    pub(crate) favor: Option<i64>,
    pub(crate) flagging: Option<String>,
    pub(crate) id: i64,
    pub(crate) is_challenge: bool,
    pub(crate) is_free_to_play: bool,
    pub(crate) is_raid: bool,
    pub(crate) legendary_level: Option<i64>,
    pub(crate) level: i64,
    pub(crate) max_level: Option<i64>,
    pub(crate) name: String,
    pub(crate) pack: String,
    pub(crate) patron: Option<String>,
    pub(crate) zone: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct QuestsPageResponse {
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) quests: Vec<QuestsPageResponseQuestsEntry>,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct QuestsDetailResponseQuestChainsEntry {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct QuestsDetailResponse {
    pub(crate) augments: Vec<QuestsDetailResponseAugmentsEntry>,
    pub(crate) bestowed_by: Option<String>,
    pub(crate) difficulties: Vec<String>,
    pub(crate) epic_level: Option<i64>,
    pub(crate) epic_name: Option<String>,
    pub(crate) favor: Option<i64>,
    pub(crate) flagging: Option<String>,
    pub(crate) id: i64,
    pub(crate) is_challenge: bool,
    pub(crate) is_free_to_play: bool,
    pub(crate) is_raid: bool,
    pub(crate) items: Vec<AdventurePacksDetailResponseItemsEntry>,
    pub(crate) legendary_level: Option<i64>,
    pub(crate) level: i64,
    pub(crate) max_level: Option<i64>,
    pub(crate) name: String,
    pub(crate) pack: String,
    pub(crate) patron: Option<String>,
    pub(crate) quest_chains: Vec<QuestsDetailResponseQuestChainsEntry>,
    pub(crate) sagas: Vec<QuestsDetailResponseQuestChainsEntry>,
    pub(crate) zone: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct RacesPageResponseRacesEntry {
    pub(crate) build_points: Vec<i64>,
    pub(crate) description: String,
    pub(crate) iconic_class: Option<String>,
    pub(crate) id: i64,
    pub(crate) is_construct: bool,
    pub(crate) name: String,
    pub(crate) no_past_life: bool,
    pub(crate) short_name: String,
    pub(crate) skill_points: Option<i64>,
    pub(crate) starting_world: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct RacesPageResponse {
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) races: Vec<RacesPageResponseRacesEntry>,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct RacesDetailResponseAbilityModifiersEntry {
    pub(crate) modifier: i64,
    pub(crate) stat: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct RacesDetailResponseFeatSlotsEntry {
    pub(crate) feat_type: String,
    pub(crate) level: i64,
    pub(crate) update_list: Option<Vec<String>>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct RacesDetailResponseFeatsEntry {
    pub(crate) acquire: String,
    pub(crate) description: Option<String>,
    pub(crate) icon: String,
    pub(crate) id: i64,
    pub(crate) max_times_acquire: Option<i64>,
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct RacesDetailResponseGrantedFeatsEntry {
    pub(crate) feat_id: Option<i64>,
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct RacesDetailResponse {
    pub(crate) ability_modifiers: Vec<RacesDetailResponseAbilityModifiersEntry>,
    pub(crate) auto_buy_skills: Vec<String>,
    pub(crate) build_points: Vec<i64>,
    pub(crate) description: String,
    pub(crate) feat_slots: Vec<RacesDetailResponseFeatSlotsEntry>,
    pub(crate) feats: Vec<RacesDetailResponseFeatsEntry>,
    pub(crate) granted_feats: Vec<RacesDetailResponseGrantedFeatsEntry>,
    pub(crate) iconic_class: Option<String>,
    pub(crate) id: i64,
    pub(crate) is_construct: bool,
    pub(crate) name: String,
    pub(crate) no_past_life: bool,
    pub(crate) short_name: String,
    pub(crate) skill_points: Option<i64>,
    pub(crate) starting_world: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SagasPageResponseSagasEntry {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) pack: Option<String>,
    pub(crate) quest_count: i64,
    pub(crate) reward_count: i64,
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SagasPageResponse {
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) sagas: Vec<SagasPageResponseSagasEntry>,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SagasDetailResponseQuestsEntry {
    pub(crate) id: i64,
    pub(crate) level: i64,
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SagasDetailResponseRewardsEntry {
    pub(crate) id: i64,
    pub(crate) is_rare: bool,
    pub(crate) minimum_level: i64,
    pub(crate) name: String,
    pub(crate) slot: String,
    pub(crate) tier: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SagasDetailResponse {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) pack: Option<String>,
    pub(crate) quest_count: i64,
    pub(crate) quests: Vec<SagasDetailResponseQuestsEntry>,
    pub(crate) reward_count: i64,
    pub(crate) rewards: Vec<SagasDetailResponseRewardsEntry>,
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SentientGemsPageResponseSentientGemsEntry {
    pub(crate) description: String,
    pub(crate) icon: String,
    pub(crate) id: i64,
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SentientGemsPageResponse {
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) sentient_gems: Vec<SentientGemsPageResponseSentientGemsEntry>,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SetsPageResponseSetsEntry {
    pub(crate) augment_count: i64,
    pub(crate) icon: String,
    pub(crate) id: i64,
    pub(crate) is_filigree_set: bool,
    pub(crate) item_count: i64,
    pub(crate) name: String,
    pub(crate) tier_count: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SetsPageResponse {
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) sets: Vec<SetsPageResponseSetsEntry>,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SetsDetailResponseAugmentsEntry {
    pub(crate) id: i64,
    pub(crate) min_level: i64,
    pub(crate) name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SetsDetailResponseFiligreesEntryModifiersEntry {
    pub(crate) amount_type: Option<String>,
    pub(crate) amounts: Option<Vec<f64>>,
    pub(crate) apply_as_item_effect: bool,
    pub(crate) bonus: Option<String>,
    pub(crate) bonus_type: Option<String>,
    pub(crate) cap: Option<String>,
    pub(crate) damage: Option<String>,
    pub(crate) dice_bonus: Option<Vec<f64>>,
    pub(crate) dice_damage: Option<String>,
    pub(crate) dice_number: Option<Vec<f64>>,
    pub(crate) dice_sides: Option<Vec<f64>>,
    pub(crate) display_name: Option<String>,
    pub(crate) effect_type: String,
    pub(crate) extra_types: Option<Vec<String>>,
    pub(crate) id: i64,
    pub(crate) is_item_specific: bool,
    pub(crate) is_rare: bool,
    pub(crate) percent: bool,
    pub(crate) rank: Option<i64>,
    pub(crate) requirements: Vec<AugmentsDetailResponseModifiersEntryRequirementsEntry>,
    pub(crate) sort_order: i64,
    pub(crate) stack_source: Option<String>,
    pub(crate) targets: Option<Vec<String>>,
    pub(crate) value: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SetsDetailResponseFiligreesEntry {
    pub(crate) description: String,
    pub(crate) icon: String,
    pub(crate) id: i64,
    pub(crate) menu: String,
    pub(crate) modifiers: Vec<SetsDetailResponseFiligreesEntryModifiersEntry>,
    pub(crate) name: String,
    pub(crate) set_id: i64,
    pub(crate) set_name: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SetsDetailResponseItemsEntry {
    pub(crate) id: i64,
    pub(crate) minimum_level: i64,
    pub(crate) name: String,
    pub(crate) slot: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SetsDetailResponseTiersEntryModifiersEntry {
    pub(crate) amount_type: Option<String>,
    pub(crate) amounts: Option<Vec<f64>>,
    pub(crate) apply_as_item_effect: bool,
    pub(crate) bonus: Option<String>,
    pub(crate) bonus_type: Option<String>,
    pub(crate) cap: Option<String>,
    pub(crate) damage: Option<String>,
    pub(crate) dice_bonus: Option<Vec<f64>>,
    pub(crate) dice_damage: Option<String>,
    pub(crate) dice_number: Option<Vec<f64>>,
    pub(crate) dice_sides: Option<Vec<f64>>,
    pub(crate) display_name: Option<String>,
    pub(crate) effect_type: String,
    pub(crate) extra_types: Option<Vec<String>>,
    pub(crate) id: i64,
    pub(crate) is_item_specific: bool,
    pub(crate) is_rare: bool,
    pub(crate) percent: bool,
    pub(crate) rank: Option<i64>,
    pub(crate) requirements: Vec<AugmentsDetailResponseModifiersEntryRequirementsEntry>,
    pub(crate) sort_order: i64,
    pub(crate) stack_source: Option<String>,
    pub(crate) targets: Option<Vec<String>>,
    pub(crate) value: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SetsDetailResponseTiersEntry {
    pub(crate) effects: Vec<EffectLine>,
    pub(crate) equipped_count: i64,
    pub(crate) id: i64,
    pub(crate) modifiers: Vec<SetsDetailResponseTiersEntryModifiersEntry>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SetsDetailResponse {
    pub(crate) augments: Vec<SetsDetailResponseAugmentsEntry>,
    pub(crate) filigrees: Vec<SetsDetailResponseFiligreesEntry>,
    pub(crate) icon: String,
    pub(crate) id: i64,
    pub(crate) is_filigree_set: bool,
    pub(crate) items: Vec<SetsDetailResponseItemsEntry>,
    pub(crate) name: String,
    pub(crate) tiers: Vec<SetsDetailResponseTiersEntry>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SpellsPageResponseSpellsEntry {
    pub(crate) cost: Option<i64>,
    pub(crate) description: String,
    pub(crate) icon: String,
    pub(crate) id: i64,
    pub(crate) max_caster_level: Option<i64>,
    pub(crate) metamagics: Option<Vec<String>>,
    pub(crate) name: String,
    pub(crate) schools: Option<Vec<String>>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SpellsPageResponse {
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) spells: Vec<SpellsPageResponseSpellsEntry>,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SpellsDetailResponseClassesEntry {
    pub(crate) class: String,
    pub(crate) class_id: i64,
    pub(crate) cost: Option<i64>,
    pub(crate) max_caster_level: Option<i64>,
    pub(crate) spell_level: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SpellsDetailResponseDamageEntry {
    pub(crate) base_dice_bonus: Option<i64>,
    pub(crate) base_dice_number: Option<i64>,
    pub(crate) base_dice_sides: Option<i64>,
    pub(crate) bonus_dice_bonus: Option<i64>,
    pub(crate) bonus_dice_number: Option<i64>,
    pub(crate) bonus_dice_sides: Option<i64>,
    pub(crate) damage: Option<String>,
    pub(crate) per_caster_levels: Option<i64>,
    pub(crate) spell_power: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SpellsDetailResponseDcsEntry {
    pub(crate) amount: Option<Vec<f64>>,
    pub(crate) casting_stat_mod: bool,
    pub(crate) dc_type: String,
    pub(crate) dc_versus: String,
    pub(crate) mod_abilities: Option<Vec<String>>,
    pub(crate) schools: Option<Vec<String>>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct SpellsDetailResponse {
    pub(crate) classes: Vec<SpellsDetailResponseClassesEntry>,
    pub(crate) cost: Option<i64>,
    pub(crate) damage: Vec<SpellsDetailResponseDamageEntry>,
    pub(crate) dcs: Vec<SpellsDetailResponseDcsEntry>,
    pub(crate) description: String,
    pub(crate) icon: String,
    pub(crate) id: i64,
    pub(crate) max_caster_level: Option<i64>,
    pub(crate) metamagics: Option<Vec<String>>,
    pub(crate) modifiers: Vec<AugmentsDetailResponseModifiersEntry>,
    pub(crate) name: String,
    pub(crate) schools: Option<Vec<String>>,
    pub(crate) stances: Vec<StancesPageResponseStancesEntry>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct StancesPageResponseStancesEntryRequirementsEntry {
    pub(crate) group_index: i64,
    pub(crate) group_kind: String,
    pub(crate) items: Option<Vec<String>>,
    pub(crate) req_type: String,
    pub(crate) value: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct StancesPageResponseStancesEntry {
    pub(crate) auto_controlled: bool,
    pub(crate) description: String,
    pub(crate) group_name: String,
    pub(crate) icon: String,
    pub(crate) id: i64,
    pub(crate) incompatible: Option<Vec<String>>,
    pub(crate) modifiers: Vec<AugmentsDetailResponseModifiersEntry>,
    pub(crate) name: String,
    pub(crate) requirements: Vec<StancesPageResponseStancesEntryRequirementsEntry>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct StancesPageResponse {
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) stances: Vec<StancesPageResponseStancesEntry>,
    pub(crate) total: i64,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct VendorsPageResponseVendorsEntry {
    pub(crate) id: i64,
    pub(crate) item_count: i64,
    pub(crate) location: String,
    pub(crate) name: String,
    pub(crate) pack: Option<String>,
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct VendorsPageResponse {
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) total: i64,
    pub(crate) vendors: Vec<VendorsPageResponseVendorsEntry>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct VendorsDetailResponseItemsEntry {
    pub(crate) cost: Option<String>,
    pub(crate) id: i64,
    pub(crate) is_rare: bool,
    pub(crate) minimum_level: i64,
    pub(crate) name: String,
    pub(crate) slot: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct VendorsDetailResponse {
    pub(crate) id: i64,
    pub(crate) item_count: i64,
    pub(crate) items: Vec<VendorsDetailResponseItemsEntry>,
    pub(crate) location: String,
    pub(crate) name: String,
    pub(crate) pack: Option<String>,
    pub(crate) wiki_url: String,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct WeaponTypesPageResponseWeaponTypesEntry {
    pub(crate) id: i64,
    pub(crate) is_shield: bool,
    pub(crate) name: String,
    pub(crate) proficiency: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct WeaponTypesPageResponse {
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) total: i64,
    pub(crate) weapon_types: Vec<WeaponTypesPageResponseWeaponTypesEntry>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct EnhancementTreesDetailResponseEnhancementsEntrySelectionsEntry {
    pub(crate) attack: Option<FeatsDetailResponseAttack>,
    pub(crate) cooldown_seconds: Option<i64>,
    pub(crate) cost_per_rank: Vec<i64>,
    pub(crate) dcs: Vec<EnhancementTreesDetailResponseEnhancementsEntryDcsEntry>,
    pub(crate) description: Option<String>,
    pub(crate) duration_seconds: Option<i64>,
    pub(crate) follow_on_modifiers: Vec<AugmentsDetailResponseModifiersEntry>,
    pub(crate) icon: String,
    pub(crate) id: i64,
    pub(crate) is_clickie: bool,
    pub(crate) min_spent: Option<i64>,
    pub(crate) modifiers: Vec<AugmentsDetailResponseModifiersEntry>,
    pub(crate) name: String,
    pub(crate) ranks: Option<i64>,
    pub(crate) requirements: Vec<AugmentsDetailResponseModifiersEntryRequirementsEntry>,
    pub(crate) stances: Vec<FeatsDetailResponseStancesEntry>,
    pub(crate) this_attack_modifiers: Vec<AugmentsDetailResponseModifiersEntry>,
}

#[derive(utoipa::ToSchema)]
pub(crate) struct QuestsDetailResponseAugmentsEntry {
    pub(crate) chest: Option<String>,
    pub(crate) family: String,
    pub(crate) id: i64,
    pub(crate) is_rare: bool,
    pub(crate) loot_type: String,
    pub(crate) min_level: i64,
    pub(crate) name: String,
}
