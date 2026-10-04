use ddo_model::enums::CorrectionKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldShape {
    Integer,
    Flag,
    Text,
    NamedReference { referenced_table: &'static str },
    SetName,
    RowName,
    Removal,
    BonusTypeName,
    StatScale,
    StatRounding,
    BonusAddition,
    BonusRemoval,
    TierAddition,
    EffectAddition,
    DamageAddition,
    DamageRemoval,
    SocketAddition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CorrectableField {
    pub name: &'static str,
    pub column: &'static str,
    pub shape: FieldShape,
    pub is_nullable: bool,
}

const fn field(name: &'static str, shape: FieldShape, is_nullable: bool) -> CorrectableField {
    CorrectableField { name, column: name, shape, is_nullable }
}

const fn named_reference(
    name: &'static str,
    column: &'static str,
    referenced_table: &'static str,
    is_nullable: bool,
) -> CorrectableField {
    CorrectableField { name, column, shape: FieldShape::NamedReference { referenced_table }, is_nullable }
}

const ITEM_FIELDS: &[CorrectableField] = &[
    field("minimum_level", FieldShape::Integer, true),
    field("enhancement_bonus", FieldShape::Integer, true),
    field("description", FieldShape::Text, true),
    field("drop_location", FieldShape::Text, true),
    field("race_required", FieldShape::Text, true),
    field("accepts_sentience", FieldShape::Flag, false),
    field("is_minor_artifact", FieldShape::Flag, false),
    field("is_legacy", FieldShape::Flag, false),
    named_reference("slot", "slot_id", "equipment_slots", false),
    field("item_category", FieldShape::Text, false),
    field("item_type", FieldShape::Text, true),
    named_reference("material", "material_id", "item_materials", true),
    field("set_bonus", FieldShape::SetName, true),
    field("name", FieldShape::RowName, false),
    field("remove", FieldShape::Removal, false),
];

const AUGMENT_FIELDS: &[CorrectableField] = &[
    field("min_level", FieldShape::Integer, true),
    field("description", FieldShape::Text, true),
    field("effect_description", FieldShape::Text, true),
    field("family", FieldShape::Text, false),
    field("name", FieldShape::RowName, false),
    field("remove", FieldShape::Removal, false),
];

const QUEST_FIELDS: &[CorrectableField] = &[
    field("level", FieldShape::Integer, true),
    field("epic_level", FieldShape::Integer, true),
    field("favor", FieldShape::Integer, true),
    field("is_raid", FieldShape::Flag, false),
    named_reference("pack", "pack_id", "adventure_packs", true),
    named_reference("patron", "patron_id", "patrons", true),
    field("name", FieldShape::RowName, false),
];

const AUGMENT_BONUS_FIELDS: &[CorrectableField] = &[
    field("value", FieldShape::Integer, true),
    field("bonus_type", FieldShape::BonusTypeName, false),
    field("add", FieldShape::BonusAddition, false),
    field("remove", FieldShape::BonusRemoval, false),
];

const ITEM_BONUS_FIELDS: &[CorrectableField] = &[
    field("value", FieldShape::Integer, true),
    field("bonus_type", FieldShape::BonusTypeName, false),
    field("scale", FieldShape::StatScale, false),
    field("rounding", FieldShape::StatRounding, false),
    field("add", FieldShape::BonusAddition, false),
    field("remove", FieldShape::BonusRemoval, false),
];
const SET_TIER_BONUS_FIELDS: &[CorrectableField] = ITEM_BONUS_FIELDS;

const SET_TIER_FIELDS: &[CorrectableField] = &[
    field("add", FieldShape::TierAddition, false),
    field("description", FieldShape::Text, true),
    field("remove", FieldShape::Removal, false),
];

const ITEM_EFFECT_FIELDS: &[CorrectableField] = &[field("add", FieldShape::EffectAddition, false)];

const EFFECT_DAMAGE_FIELDS: &[CorrectableField] =
    &[field("add", FieldShape::DamageAddition, false), field("remove", FieldShape::DamageRemoval, false)];

const ITEM_SOCKET_FIELDS: &[CorrectableField] = &[field("add", FieldShape::SocketAddition, false)];

const DESCRIPTION_FIELDS: &[CorrectableField] = &[field("description", FieldShape::Text, true)];

const ROW_NAME_FIELDS: &[CorrectableField] = &[field("name", FieldShape::RowName, false)];

pub(super) const WIKI_QUEST_FIELDS: &[&str] =
    &["is_free_to_play", "legendary_level", "zone", "bestowed_by", "flagging"];

pub fn correctable_fields(kind: CorrectionKind) -> &'static [CorrectableField] {
    match kind {
        CorrectionKind::Item => ITEM_FIELDS,
        CorrectionKind::Augment => AUGMENT_FIELDS,
        CorrectionKind::Quest => QUEST_FIELDS,
        CorrectionKind::Feat
        | CorrectionKind::Enhancement
        | CorrectionKind::Race
        | CorrectionKind::Class
        | CorrectionKind::Spell => DESCRIPTION_FIELDS,
        CorrectionKind::AdventurePack
        | CorrectionKind::Patron
        | CorrectionKind::SetBonus
        | CorrectionKind::SocketLabel => ROW_NAME_FIELDS,
        CorrectionKind::AugmentBonus => AUGMENT_BONUS_FIELDS,
        CorrectionKind::ItemBonus => ITEM_BONUS_FIELDS,
        CorrectionKind::SetTier => SET_TIER_FIELDS,
        CorrectionKind::SetTierBonus => SET_TIER_BONUS_FIELDS,
        CorrectionKind::ItemEffect => ITEM_EFFECT_FIELDS,
        CorrectionKind::EffectDamage => EFFECT_DAMAGE_FIELDS,
        CorrectionKind::ItemSocket => ITEM_SOCKET_FIELDS,
    }
}

pub fn correctable_field(kind: CorrectionKind, field_name: &str) -> Option<&'static CorrectableField> {
    correctable_fields(kind).iter().find(|field| field.name == field_name)
}
