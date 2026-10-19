use ddo_model::enums::CorrectionKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldShape {
    Integer,
    IntegerList,
    TextList,
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
    BonusDedupe,
    EffectBonusConstant,
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

pub(crate) trait CorrectionFieldType {
    const SHAPE: FieldShape;
    const IS_NULLABLE: bool = false;
}
impl CorrectionFieldType for i64 {
    const SHAPE: FieldShape = FieldShape::Integer;
}
impl CorrectionFieldType for bool {
    const SHAPE: FieldShape = FieldShape::Flag;
}
impl CorrectionFieldType for String {
    const SHAPE: FieldShape = FieldShape::Text;
}
impl CorrectionFieldType for Vec<i64> {
    const SHAPE: FieldShape = FieldShape::IntegerList;
}
impl CorrectionFieldType for Vec<String> {
    const SHAPE: FieldShape = FieldShape::TextList;
}
impl<T: CorrectionFieldType> CorrectionFieldType for Option<T> {
    const SHAPE: FieldShape = T::SHAPE;
    const IS_NULLABLE: bool = true;
}

const AUGMENT_BONUS_OPERATIONS: &[CorrectableField] = &[
    field("add", FieldShape::BonusAddition, false),
    field("remove", FieldShape::BonusRemoval, false),
    field("dedupe", FieldShape::BonusDedupe, false),
];

const ITEM_BONUS_OPERATIONS: &[CorrectableField] =
    &[field("add", FieldShape::BonusAddition, false), field("remove", FieldShape::BonusRemoval, false)];
const SET_TIER_BONUS_OPERATIONS: &[CorrectableField] = ITEM_BONUS_OPERATIONS;

const SET_TIER_OPERATIONS: &[CorrectableField] =
    &[field("add", FieldShape::TierAddition, false), field("remove", FieldShape::Removal, false)];

const ITEM_EFFECT_OPERATIONS: &[CorrectableField] =
    &[field("add", FieldShape::EffectAddition, false), field("remove", FieldShape::Removal, false)];

const EFFECT_DAMAGE_OPERATIONS: &[CorrectableField] =
    &[field("add", FieldShape::DamageAddition, false), field("remove", FieldShape::DamageRemoval, false)];

const ITEM_SOCKET_OPERATIONS: &[CorrectableField] = &[field("add", FieldShape::SocketAddition, false)];

pub fn correctable_fields(kind: CorrectionKind) -> &'static [CorrectableField] {
    static FIELDS: std::sync::LazyLock<std::collections::HashMap<CorrectionKind, Vec<CorrectableField>>> =
        std::sync::LazyLock::new(|| {
            CorrectionKind::ALL
                .iter()
                .map(|&kind| {
                    let mut fields = crate::tree::declared_correctable_fields(kind);
                    fields.extend_from_slice(kind_operations(kind));
                    (kind, fields)
                })
                .collect()
        });
    &FIELDS[&kind]
}

const fn kind_operations(kind: CorrectionKind) -> &'static [CorrectableField] {
    match kind {
        CorrectionKind::Item => const { &[field("remove", FieldShape::Removal, false)] },
        CorrectionKind::Augment | CorrectionKind::Quest => const { &[field("remove", FieldShape::Removal, false)] },
        CorrectionKind::Effect => const { &[field("constant", FieldShape::EffectBonusConstant, false)] },
        CorrectionKind::AugmentBonus => AUGMENT_BONUS_OPERATIONS,
        CorrectionKind::ItemBonus => ITEM_BONUS_OPERATIONS,
        CorrectionKind::SetTier => SET_TIER_OPERATIONS,
        CorrectionKind::SetTierBonus => SET_TIER_BONUS_OPERATIONS,
        CorrectionKind::ItemEffect => ITEM_EFFECT_OPERATIONS,
        CorrectionKind::EffectDamage => EFFECT_DAMAGE_OPERATIONS,
        CorrectionKind::ItemSocket => ITEM_SOCKET_OPERATIONS,
        _ => &[],
    }
}
