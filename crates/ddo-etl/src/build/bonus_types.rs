use crate::corrections::{Correction, CorrectionValue, Corrections, FieldShape};
use anyhow::{bail, Context, Result};
use ddo_model::enums::{BonusType, CorrectionKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BonusOwnerKind {
    Item,
    ItemAugmentSlotOption,
    Augment,
    Feat,
    SetBonusTier,
}

impl BonusOwnerKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Item => "item",
            Self::ItemAugmentSlotOption => "augment slot option of item",
            Self::Augment => "augment",
            Self::Feat => "feat",
            Self::SetBonusTier => "set bonus tier of",
        }
    }

    const fn correction_kind(self) -> Option<CorrectionKind> {
        match self {
            Self::Item | Self::ItemAugmentSlotOption => Some(CorrectionKind::ItemBonus),
            Self::Augment => Some(CorrectionKind::AugmentBonus),
            Self::Feat | Self::SetBonusTier => None,
        }
    }

    const fn bonus_source_word(self) -> &'static str {
        match self {
            Self::Item => "buff",
            Self::ItemAugmentSlotOption | Self::Augment | Self::Feat | Self::SetBonusTier => "effect",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct BonusOwner<'a> {
    pub(super) kind: BonusOwnerKind,
    pub(super) name: &'a str,
    pub(super) family: Option<&'a str>,
}

pub(super) struct BonusOrigin<'a> {
    pub(super) owner: &'a BonusOwner<'a>,
    pub(super) source_name: &'a str,
    pub(super) stat_name: &'a str,
    pub(super) value: Option<i64>,
}

struct TypingCorrection<'a> {
    correction: &'a Correction,
    bonus_type: BonusType,
    is_applied: bool,
}

pub(super) struct UntypedBonusCorrections<'a> {
    typing_corrections: Vec<TypingCorrection<'a>>,
}

impl<'a> UntypedBonusCorrections<'a> {
    pub(super) fn from_corrections(corrections: &'a Corrections) -> Result<Self> {
        let mut typing_corrections = Vec::new();
        for correction in &corrections.entries {
            let types_an_untyped_bonus = correction.kind.corrects_a_bonus()
                && correction.from == CorrectionValue::Null
                && correction.correctable_field().is_ok_and(|field| field.shape == FieldShape::BonusTypeName);
            if !types_an_untyped_bonus {
                continue;
            }
            let bonus_type_name = correction.to.as_text().context("a bonus type cannot be null")?;
            let Some(bonus_type) = BonusType::parse(bonus_type_name) else {
                bail!(
                    "correction file {}: {}: bonus type {bonus_type_name:?} is not in the bonus_types table; use its exact name",
                    correction.file_name,
                    correction.label()
                );
            };
            typing_corrections.push(TypingCorrection { correction, bonus_type, is_applied: false });
        }
        Ok(Self { typing_corrections })
    }

    pub(super) fn bonus_type_for(&mut self, bonus_origin: &BonusOrigin) -> Result<BonusType> {
        let BonusOrigin { owner, source_name, stat_name, value } = bonus_origin;
        let correction_kind = owner.kind.correction_kind();
        let typing_correction = self.typing_corrections.iter_mut().find(|typing_correction| {
            let correction = typing_correction.correction;
            Some(correction.kind) == correction_kind
                && correction.name == owner.name
                && correction.family.as_deref().is_none_or(|family| Some(family) == owner.family)
                && correction.stat.as_deref() == Some(*stat_name)
                && correction.bonus_value.is_none_or(|bonus_value| Some(bonus_value) == *value)
        });
        if let Some(typing_correction) = typing_correction {
            typing_correction.is_applied = true;
            return Ok(typing_correction.bonus_type);
        }
        let remedy = match correction_kind {
            Some(correction_kind) => format!(
                "type it with a {} bonus_type correction from \"null\" (data/corrections/README.md) or in {}",
                correction_kind.as_str(),
                "data/effect_map.toml"
            ),
            None => "type it in data/effect_map.toml".to_string(),
        };
        bail!(
            "{} {:?}: {} {source_name:?} gives stat {stat_name:?} a bonus with no bonus type; every bonus carries one, so {remedy}",
            owner.kind.as_str(),
            owner.name,
            owner.kind.bonus_source_word()
        )
    }

    pub(super) fn applied_corrections(&self) -> Vec<&'a Correction> {
        self.typing_corrections
            .iter()
            .filter(|typing_correction| typing_correction.is_applied)
            .map(|typing_correction| typing_correction.correction)
            .collect()
    }
}
