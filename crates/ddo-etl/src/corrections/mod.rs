mod fields;
pub(crate) use fields::CorrectionFieldType;

pub use fields::{correctable_field, correctable_fields, CorrectableField, FieldShape};

use crate::wiki::is_iso_date;
use anyhow::{bail, ensure, Context, Result};
use ddo_model::enums::CorrectionKind;
use rusqlite::types::Value as SqlValue;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

const CORRECTION_TABLE_NAME: &str = "correction";
static EMBEDDED_CORRECTIONS: OnceLock<Result<Corrections, String>> = OnceLock::new();
pub const NULL_SPELLING: &str = "null";

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(from = "TomlCorrectionValue")]
pub enum CorrectionValue {
    Integer(i64),
    Integers(Vec<i64>),
    Texts(Vec<String>),
    Float(f64),
    Text(String),
    Bonus(BonusAddition),
    Tier(TierAddition),
    Effect(EffectAddition),
    Damage(DamageRow),
    Null,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BonusAddition {
    pub stat: String,
    pub bonus_type: String,
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TierAddition {
    pub equipped_count: i64,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectAddition {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub value: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageRow {
    pub trigger: String,
    pub damage_type: String,
    pub dice_number: i64,
    pub dice_sides: i64,
    pub dice_bonus: i64,
    pub amount_from: i64,
    pub scale: f64,
    pub sort_order: i64,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum TomlCorrectionValue {
    Integer(i64),
    Integers(Vec<i64>),
    Texts(Vec<String>),
    Float(f64),
    Text(String),
    Bonus(BonusAddition),
    Tier(TierAddition),
    Effect(EffectAddition),
    Damage(DamageRow),
}

impl From<TomlCorrectionValue> for CorrectionValue {
    fn from(toml_value: TomlCorrectionValue) -> Self {
        match toml_value {
            TomlCorrectionValue::Integer(number) => Self::Integer(number),
            TomlCorrectionValue::Integers(numbers) => Self::Integers(numbers),
            TomlCorrectionValue::Texts(strings) => Self::Texts(strings),
            TomlCorrectionValue::Float(number) => Self::Float(number),
            TomlCorrectionValue::Text(text) if text == NULL_SPELLING => Self::Null,
            TomlCorrectionValue::Text(text) => Self::Text(text),
            TomlCorrectionValue::Bonus(bonus) => Self::Bonus(bonus),
            TomlCorrectionValue::Tier(tier) => Self::Tier(tier),
            TomlCorrectionValue::Effect(effect) => Self::Effect(effect),
            TomlCorrectionValue::Damage(damage) => Self::Damage(damage),
        }
    }
}

impl CorrectionValue {
    pub fn from_json(encoded: &str) -> Result<Self> {
        let value: serde_json::Value = serde_json::from_str(encoded)?;
        Ok(match value {
            serde_json::Value::Null => CorrectionValue::Null,
            serde_json::Value::Bool(flag) => CorrectionValue::Integer(i64::from(flag)),
            serde_json::Value::Number(number) => number
                .as_i64()
                .map_or_else(|| CorrectionValue::Float(number.as_f64().unwrap_or_default()), CorrectionValue::Integer),
            serde_json::Value::String(text) => CorrectionValue::Text(text),
            serde_json::Value::Object(fields) => {
                let object = serde_json::Value::Object(fields);
                if object.get("stat").is_some() {
                    CorrectionValue::Bonus(serde_json::from_value(object)?)
                } else if object.get("equipped_count").is_some() {
                    CorrectionValue::Tier(serde_json::from_value(object)?)
                } else if object.get("trigger").is_some() {
                    CorrectionValue::Damage(serde_json::from_value(object)?)
                } else {
                    CorrectionValue::Effect(serde_json::from_value(object)?)
                }
            }
            serde_json::Value::Array(values) => {
                if values.iter().all(serde_json::Value::is_string) && !values.is_empty() {
                    Self::Texts(serde_json::from_value(serde_json::Value::Array(values))?)
                } else {
                    Self::Integers(
                        serde_json::from_value(serde_json::Value::Array(values))
                            .context("correction list must contain only integers or only text")?,
                    )
                }
            }
        })
    }

    pub fn to_json(&self) -> String {
        match self {
            Self::Integer(number) => number.to_string(),
            Self::Integers(numbers) => serde_json::to_string(numbers).expect("integer list serializes"),
            Self::Texts(strings) => serde_json::to_string(strings).expect("text list serializes"),
            Self::Float(number) => serde_json::Value::from(*number).to_string(),
            Self::Text(text) => serde_json::Value::from(text.as_str()).to_string(),
            Self::Bonus(bonus) => format!(
                "{{\"bonus_type\":{},\"stat\":{},\"value\":{}}}",
                serde_json::Value::from(bonus.bonus_type.as_str()),
                serde_json::Value::from(bonus.stat.as_str()),
                bonus.value
            ),
            Self::Tier(tier) => format!(
                "{{\"description\":{},\"equipped_count\":{}}}",
                serde_json::Value::from(tier.description.as_str()),
                tier.equipped_count
            ),
            Self::Effect(effect) => {
                let mut fields = serde_json::json!({"description": effect.description, "name": effect.name});
                if let Some(amount) = effect.value {
                    fields["value"] = serde_json::json!(amount);
                }
                fields.to_string()
            }
            Self::Damage(damage) => serde_json::to_value(damage).expect("damage row serializes").to_string(),
            Self::Null => NULL_SPELLING.to_string(),
        }
    }

    pub fn to_sql(&self) -> SqlValue {
        match self {
            Self::Integer(number) => SqlValue::Integer(*number),
            Self::Float(number) => SqlValue::Real(*number),
            Self::Text(text) => SqlValue::Text(text.clone()),
            Self::Bonus(_) | Self::Tier(_) | Self::Effect(_) | Self::Damage(_) | Self::Integers(_) | Self::Texts(_) => {
                SqlValue::Text(self.to_json())
            }
            Self::Null => SqlValue::Null,
        }
    }

    pub fn from_sql(sql_value: SqlValue) -> Self {
        match sql_value {
            SqlValue::Integer(number) => Self::Integer(number),
            SqlValue::Real(number) => Self::Float(number),
            SqlValue::Text(text) => Self::Text(text),
            SqlValue::Blob(bytes) => Self::Text(String::from_utf8_lossy(&bytes).into_owned()),
            SqlValue::Null => Self::Null,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_bonus_type_name(&self) -> Option<&str> {
        match self {
            Self::Text(bonus_type_name) => Some(bonus_type_name),
            Self::Null => Some(NULL_SPELLING),
            _ => None,
        }
    }

    pub fn added_effect_name(&self) -> Option<&str> {
        match self {
            Self::Text(effect_name) => Some(effect_name),
            Self::Effect(effect) => Some(&effect.name),
            _ => None,
        }
    }

    pub fn added_effect_description(&self) -> Option<&str> {
        match self {
            Self::Effect(effect) => Some(&effect.description),
            _ => None,
        }
    }

    pub fn added_effect_value(&self) -> Option<i64> {
        match self {
            Self::Effect(effect) => effect.value,
            _ => None,
        }
    }

    fn fits(&self, field: &CorrectableField) -> bool {
        match (self, field.shape) {
            (Self::Null, _) => field.is_nullable,
            (Self::Integers(_), FieldShape::IntegerList) | (Self::Texts(_), FieldShape::TextList) => true,
            (Self::Integers(numbers), FieldShape::TextList) => numbers.is_empty(),
            (Self::Integer(_), FieldShape::Integer | FieldShape::EffectBonusConstant | FieldShape::BonusDedupe) => true,
            (Self::Float(number), FieldShape::StatScale) => number.is_finite() && *number > 0.0,
            (Self::Text(rounding), FieldShape::StatRounding) => matches!(rounding.as_str(), "down" | "up" | "nearest"),
            (Self::Damage(damage), FieldShape::DamageAddition | FieldShape::DamageRemoval) => {
                !damage.trigger.trim().is_empty()
                    && !damage.damage_type.trim().is_empty()
                    && damage.dice_number > 0
                    && damage.dice_sides > 0
                    && (0..=2).contains(&damage.amount_from)
                    && damage.scale.is_finite()
                    && damage.scale > 0.0
                    && damage.sort_order >= 0
            }
            (Self::Integer(flag), FieldShape::Flag) => matches!(flag, 0 | 1),
            (
                Self::Text(_),
                FieldShape::Text
                | FieldShape::NamedReference { .. }
                | FieldShape::SetName
                | FieldShape::RowName
                | FieldShape::BonusTypeName,
            ) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Correction {
    pub kind: CorrectionKind,
    pub name: String,
    pub field: String,
    pub from: CorrectionValue,
    pub to: CorrectionValue,
    pub family: Option<String>,
    pub selection: Option<String>,
    pub stance: Option<String>,
    pub dc: Option<String>,
    pub effect: Option<String>,
    pub stat: Option<String>,
    pub bonus_type: Option<String>,
    pub bonus_value: Option<i64>,
    pub equipped_count: Option<i64>,
    pub reason: String,
    pub source: crate::AnnotationSource,
    pub read: String,
    pub file_name: String,
}

impl Correction {
    pub fn correctable_field(&self) -> Result<&'static CorrectableField> {
        let fields = if self.selection.is_some() && self.stance.is_none() && self.dc.is_none() {
            crate::tree::selection_correctable_fields()
        } else {
            correctable_fields(self.kind)
        };
        fields.iter().find(|field| field.name == self.field).with_context(|| {
            let allowed_fields: Vec<&str> = fields.iter().map(|field| field.name).collect();
            if self.kind == CorrectionKind::Quest && fields::WIKI_QUEST_FIELDS.contains(&self.field.as_str()) {
                format!(
                    "field {:?} comes from the wiki's quests.toml, not Maetrim's files; correct it there. A quest correction takes {}",
                    self.field,
                    allowed_fields.join(", ")
                )
            } else {
                format!(
                    "field {:?} cannot be corrected on a {}; the allowed fields are {}",
                    self.field,
                    self.kind.as_str(),
                    allowed_fields.join(", ")
                )
            }
        })
    }

    pub fn label(&self) -> String {
        let qualifier = self.qualifier();
        if qualifier.is_empty() {
            format!("{} {:?}.{}", self.kind.as_str(), self.name, self.field)
        } else {
            format!("{} {:?} [{qualifier}].{}", self.kind.as_str(), self.name, self.field)
        }
    }

    pub fn qualifier(&self) -> String {
        let mut qualifier_parts: Vec<String> = Vec::new();
        if let Some(selection) = &self.selection {
            qualifier_parts.push(format!("selection {selection:?}"));
        }
        if let Some(stance) = &self.stance {
            qualifier_parts.push(format!("stance {stance:?}"));
        }
        if let Some(dc) = &self.dc {
            qualifier_parts.push(format!("dc {dc:?}"));
        }
        if let Some(family) = &self.family {
            qualifier_parts.push(format!("family {family:?}"));
        }
        if let Some(effect) = &self.effect {
            qualifier_parts.push(format!("effect {effect:?}"));
        }
        if let Some(equipped_count) = self.equipped_count {
            qualifier_parts.push(format!("equipped_count {equipped_count}"));
        }
        if self.kind == CorrectionKind::Effect && self.field == "constant" {
            if let Some(stat) = &self.stat {
                qualifier_parts.push(format!("stat {stat:?}"));
            }
        } else if let (Some(stat), Some(bonus_type)) = (&self.stat, &self.bonus_type) {
            match self.bonus_value {
                Some(bonus_value) => qualifier_parts.push(format!("{stat} / {bonus_type} / {bonus_value}")),
                None => qualifier_parts.push(format!("{stat} / {bonus_type}")),
            }
        }
        match &self.to {
            CorrectionValue::Bonus(bonus) => qualifier_parts.push(format!("{} / {}", bonus.stat, bonus.bonus_type)),
            CorrectionValue::Text(added_socket_label) if self.kind == CorrectionKind::ItemSocket => {
                qualifier_parts.push(added_socket_label.clone())
            }
            added_effect if self.kind == CorrectionKind::ItemEffect => {
                if let Some(added_effect_name) = added_effect.added_effect_name() {
                    qualifier_parts.push(added_effect_name.to_string())
                }
            }
            _ => {}
        }
        qualifier_parts.join(", ")
    }

    pub fn bonus_key(&self) -> Option<(&str, &str)> {
        match (&self.stat, &self.bonus_type, &self.to) {
            (Some(stat), Some(bonus_type), _) => Some((stat, bonus_type)),
            (_, _, CorrectionValue::Bonus(bonus)) => Some((&bonus.stat, &bonus.bonus_type)),
            _ => None,
        }
    }

    pub(crate) fn target_table_name(&self) -> &'static str {
        if self.dc.is_some() {
            "dcs"
        } else if self.stance.is_some() {
            "stances"
        } else if self.selection.is_some() {
            "enhancement_selections"
        } else {
            self.kind.table_name()
        }
    }

    fn validate_qualifier_keys(&self, field: &CorrectableField) -> Result<()> {
        ensure!(
            self.selection.is_none() || self.kind == CorrectionKind::Enhancement,
            "selection names an enhancement selection row"
        );
        ensure!(
            self.stance.is_none()
                || (matches!(self.kind, CorrectionKind::Spell | CorrectionKind::Feat | CorrectionKind::Enhancement)
                    && self.field == "icon"),
            "stance names an ability's icon row"
        );
        ensure!(
            self.selection.as_ref().is_none_or(|name| !name.trim().is_empty())
                && self.stance.as_ref().is_none_or(|name| !name.trim().is_empty())
                && self.dc.as_ref().is_none_or(|name| !name.trim().is_empty()),
            "nested icon corrections need a nonblank row name"
        );
        ensure!(
            self.dc.is_none()
                || (self.kind == CorrectionKind::Enhancement && self.field == "icon" && self.stance.is_none()),
            "dc names an enhancement or selection icon row"
        );
        if self.effect.is_some()
            != (self.kind == CorrectionKind::ItemEffect && matches!(field.name, "value" | "bonus_type" | "remove"))
        {
            bail!("effect names only an item_effect value, bonus_type or remove correction, which requires effect");
        }
        if self.family.is_some() && !matches!(self.kind, CorrectionKind::Augment | CorrectionKind::AugmentBonus) {
            bail!("family narrows only an augment or augment_bonus correction, not a {}", self.kind.as_str());
        }
        let names_a_bonus = self.stat.is_some() || self.bonus_type.is_some() || self.bonus_value.is_some();
        let needs_a_bonus = matches!(
            self.kind,
            CorrectionKind::AugmentBonus | CorrectionKind::ItemBonus | CorrectionKind::SetTierBonus
        ) && field.shape != FieldShape::BonusAddition;
        let needs_a_tier = matches!(self.kind, CorrectionKind::SetTier | CorrectionKind::SetTierBonus);
        if needs_a_tier != self.equipped_count.is_some() {
            bail!("a set_tier or set_tier_bonus correction requires equipped_count, and other kinds forbid it");
        }
        if self.equipped_count.is_some_and(|count| count < 1) {
            bail!("equipped_count must be positive");
        }
        if needs_a_bonus && (self.stat.is_none() || self.bonus_type.is_none()) {
            bail!("a {} {} correction names the bonus with stat and bonus_type", self.kind.as_str(), field.name);
        }
        if field.shape == FieldShape::BonusDedupe && self.bonus_value.is_none() {
            bail!("an augment_bonus dedupe correction needs bonus_value to identify the duplicate amount");
        }
        if self.kind == CorrectionKind::Effect && field.name == "constant" {
            if self.stat.is_none() || self.bonus_type.is_some() || self.bonus_value.is_some() {
                bail!("an effect constant correction names exactly one stat");
            }
        } else if names_a_bonus && !needs_a_bonus {
            bail!(
                "stat, bonus_type and bonus_value name the bonus of an augment_bonus, item_bonus or set_tier_bonus value, bonus_type, scale, rounding or remove correction; an add names them in to"
            );
        }
        if field.shape == FieldShape::BonusTypeName
            && self.kind != CorrectionKind::ItemEffect
            && self.bonus_type.as_deref() != self.from.as_bonus_type_name()
        {
            bail!("from must be the bonus_type the correction names, his current type or \"null\"");
        }
        Ok(())
    }

    pub(crate) fn validate_values(&self, field: &CorrectableField) -> Result<()> {
        match field.shape {
            FieldShape::Removal => {
                if (&self.from, &self.to) != (&CorrectionValue::Integer(0), &CorrectionValue::Integer(1)) {
                    bail!("a remove takes from = 0 (his files still carry the row) and to = 1");
                }
            }
            FieldShape::BonusAddition => {
                let (CorrectionValue::Null, CorrectionValue::Bonus(bonus)) = (&self.from, &self.to) else {
                    bail!(
                        "an add takes from = \"null\" and to = {{ stat = \"...\", bonus_type = \"...\", value = N }}"
                    );
                };
                if bonus.bonus_type == NULL_SPELLING || bonus.bonus_type.trim().is_empty() {
                    bail!("an added bonus must have a named bonus type; bonus_type cannot be null or empty");
                }
            }
            FieldShape::BonusRemoval => {
                if !matches!(
                    (&self.from, &self.to),
                    (CorrectionValue::Integer(_), CorrectionValue::Null)
                        | (CorrectionValue::Null, CorrectionValue::Integer(1))
                ) {
                    bail!("a bonus remove takes its current integer value to \"null\", or a missing value from \"null\" to 1");
                }
            }
            FieldShape::BonusDedupe => {
                if (&self.from, &self.to) != (&CorrectionValue::Integer(2), &CorrectionValue::Integer(1)) {
                    bail!("a duplicate bonus correction reduces exactly two matching links to one");
                }
            }
            FieldShape::TierAddition => {
                let (CorrectionValue::Null, CorrectionValue::Tier(tier)) = (&self.from, &self.to) else {
                    bail!("a tier add takes from = \"null\" and to = {{ equipped_count = N, description = \"...\" }}");
                };
                if Some(tier.equipped_count) != self.equipped_count || tier.description.trim().is_empty() {
                    bail!("a tier add must use its equipped_count qualifier and a nonempty description");
                }
            }
            FieldShape::BonusTypeName => {
                if !matches!(
                    (&self.from, &self.to),
                    (CorrectionValue::Null | CorrectionValue::Text(_), CorrectionValue::Text(_))
                ) {
                    bail!("a bonus_type takes from = his current type or \"null\", and to = a bonus type's name");
                }
            }
            FieldShape::SocketAddition => {
                if !matches!((&self.from, &self.to), (CorrectionValue::Null, CorrectionValue::Text(_))) {
                    bail!("an add takes from = \"null\" and to = the socket label to add");
                }
            }
            FieldShape::EffectAddition => {
                if !matches!(
                    (&self.from, &self.to),
                    (CorrectionValue::Null, CorrectionValue::Text(_) | CorrectionValue::Effect(_))
                ) {
                    bail!(
                        "an add takes from = \"null\" and to = the name of the effect to add, or {{ name = \"...\", description = \"...\" }}"
                    );
                }
            }
            FieldShape::DamageAddition => {
                if !matches!((&self.from, &self.to), (CorrectionValue::Null, CorrectionValue::Damage(_))) {
                    bail!("damage add takes from = \"null\" and to = a damage row");
                }
                if !self.to.fits(field) {
                    bail!("damage row needs positive dice and scale, an amount slot 0–2, and a nonnegative order");
                }
            }
            FieldShape::DamageRemoval => {
                if !matches!((&self.from, &self.to), (CorrectionValue::Damage(_), CorrectionValue::Null)) {
                    bail!("damage remove takes from = the existing damage row and to = \"null\"");
                }
                if !self.from.fits(field) {
                    bail!("damage row needs positive dice and scale, an amount slot 0–2, and a nonnegative order");
                }
            }
            _ => {
                for (value_role, value) in [("from", &self.from), ("to", &self.to)] {
                    if !(value.fits(field) || value_role == "from" && matches!(value, CorrectionValue::Null)) {
                        bail!(
                            "{value_role} = {} does not fit {}: {}",
                            value.to_json(),
                            field.name,
                            expected_value_text(field)
                        );
                    }
                }
            }
        }
        Ok(())
    }

    fn validate(&self) -> Result<()> {
        let field = self.correctable_field()?;
        self.validate_qualifier_keys(field)?;
        self.validate_values(field)?;
        if self.from == self.to {
            bail!("to equals from ({}); a correction must change the value", self.to.to_json());
        }
        if self.reason.trim().is_empty() {
            bail!("reason is empty; say why his value is wrong");
        }
        if !self.source.is_in_game()
            && self.kind == CorrectionKind::Effect
            && self.field == "name"
            && !self.source.page_url().is_some_and(|url| url.starts_with("https://"))
        {
            bail!(
                "an effect rename must cite the wiki page that shows the new name; source {:?} is not one",
                self.source
            );
        }
        let internal_template_source = self.source.page_url() == Some("internal:effect-name-and-description")
            && self.kind == CorrectionKind::Effect
            && matches!(self.field.as_str(), "verbose_name_template" | "description_template");
        let upstream_icon_folder =
            self.field == "icon" && self.source.page_url().is_some_and(|url| url.starts_with("Output/DataFiles/"));
        if !self.source.is_in_game()
            && !internal_template_source
            && !upstream_icon_folder
            && (!self.source.page_url().is_some_and(|url| url.starts_with("https://"))
                || self.source.page_url() == Some("https://"))
        {
            bail!(
                "source {:?} must be a supporting https:// URL or an internal effect-template consistency check",
                self.source
            );
        }
        if !is_iso_date(&self.read) {
            bail!("read {:?} must be the ISO date the source was read, YYYY-MM-DD", self.read);
        }
        Ok(())
    }
}

fn expected_value_text(field: &CorrectableField) -> String {
    let non_null_text = match field.shape {
        FieldShape::IntegerList => "an integer list",
        FieldShape::TextList => "a text list",
        FieldShape::Integer | FieldShape::EffectBonusConstant => "an integer",
        FieldShape::StatScale => "a positive finite number",
        FieldShape::StatRounding => "down, up or nearest",
        FieldShape::Flag => "0 or 1",
        FieldShape::Text => "a string",
        FieldShape::NamedReference { .. } | FieldShape::SetName | FieldShape::BonusTypeName => {
            "the name of the row it refers to"
        }
        FieldShape::RowName => "the row's name",
        FieldShape::Removal => "0 for from and 1 for to",
        FieldShape::BonusAddition => "a { stat, bonus_type, value } table",
        FieldShape::BonusRemoval => "the current integer value for from and \"null\" for to",
        FieldShape::BonusDedupe => "2 for from and 1 for to",
        FieldShape::TierAddition => "a { equipped_count, description } table",
        FieldShape::EffectAddition => "an effect name or a { name, description } table",
        FieldShape::DamageAddition | FieldShape::DamageRemoval => "a damage row",
        FieldShape::SocketAddition => "a socket label",
    };
    if field.is_nullable {
        format!("{non_null_text}, or \"null\"")
    } else {
        format!("{non_null_text}; it cannot be null")
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TomlCorrection {
    kind: String,
    name: String,
    field: String,
    family: Option<String>,
    selection: Option<String>,
    stance: Option<String>,
    dc: Option<String>,
    effect: Option<String>,
    stat: Option<String>,
    bonus_type: Option<String>,
    bonus_value: Option<i64>,
    equipped_count: Option<i64>,
    from: CorrectionValue,
    to: CorrectionValue,
    reason: String,
    source: Option<String>,
    in_game: Option<bool>,
    read: String,
}

impl TomlCorrection {
    fn into_correction(self, file_name: &str) -> Result<Correction> {
        let kind = CorrectionKind::parse(&self.kind).with_context(|| {
            let allowed_kinds: Vec<&str> = CorrectionKind::ALL.iter().map(|kind| kind.as_str()).collect();
            format!("kind {:?} is not one of {}", self.kind, allowed_kinds.join(", "))
        })?;
        Ok(Correction {
            kind,
            name: self.name,
            field: self.field,
            from: self.from,
            to: self.to,
            family: self.family,
            selection: self.selection,
            stance: self.stance,
            dc: self.dc,
            effect: self.effect,
            stat: self.stat,
            bonus_type: self.bonus_type,
            bonus_value: self.bonus_value,
            equipped_count: self.equipped_count,
            reason: self.reason,
            source: crate::AnnotationSource::from_fields(self.source, self.in_game)?,
            read: self.read,
            file_name: file_name.to_string(),
        })
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Corrections {
    pub entries: Vec<Correction>,
}

impl Corrections {
    pub fn from_data(root: &Path) -> Result<Self> {
        let mut db = Connection::open_in_memory()?;
        crate::tree::build_from_data(root, &mut db)?;
        Self::from_database(&db)
    }

    pub fn from_database(db: &Connection) -> Result<Self> {
        let mut statement = db.prepare(
            "SELECT kind, name, field, from_value, to_value, qualifier, reason, source, read, in_game
             FROM corrections ORDER BY id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                crate::AnnotationSource::from_columns(row.get(7)?, row.get(9)?)?,
                row.get::<_, String>(8)?,
            ))
        })?;
        let mut entries = Vec::new();
        for row in rows {
            let (kind, name, field, from_value, to_value, qualifier, reason, source, read) = row?;
            let mut correction = Correction {
                kind: CorrectionKind::parse(&kind).with_context(|| format!("unknown correction kind {kind:?}"))?,
                name,
                field,
                from: CorrectionValue::from_json(&from_value)?,
                to: CorrectionValue::from_json(&to_value)?,
                family: None,
                selection: None,
                stance: None,
                dc: None,
                effect: None,
                stat: None,
                bonus_type: None,
                bonus_value: None,
                equipped_count: None,
                reason,
                source,
                read,
                file_name: "data".to_string(),
            };
            set_correction_qualifiers(&mut correction, &qualifier)?;
            ensure!(correction.qualifier() == qualifier, "{} has unparsed qualifier {qualifier:?}", correction.label());
            correction.validate()?;
            entries.push(correction);
        }
        let original_set_names = entries
            .iter()
            .filter(|entry| entry.kind == CorrectionKind::SetBonus && entry.field == "name")
            .filter_map(|entry| Some((entry.to.as_text()?.to_string(), entry.from.as_text()?.to_string())))
            .collect::<HashMap<_, _>>();
        for entry in &mut entries {
            if entry.kind == CorrectionKind::Item && entry.field == "set_bonus" {
                if let Some(current_name) = entry.to.as_text() {
                    if let Some(original_name) = original_set_names.get(current_name) {
                        entry.to = CorrectionValue::Text(original_name.clone());
                    }
                }
            }
        }
        Ok(Self { entries })
    }

    pub fn embedded() -> Result<Self> {
        EMBEDDED_CORRECTIONS
            .get_or_init(|| {
                Self::from_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("data/corrections"))
                    .map_err(|error| format!("{error:#}"))
            })
            .clone()
            .map_err(|error| anyhow::anyhow!("{error}"))
    }

    pub fn from_dir(dir: &Path) -> Result<Self> {
        let mut paths: Vec<_> = std::fs::read_dir(dir)
            .with_context(|| format!("listing {}", dir.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "toml"))
            .collect();
        paths.sort();
        let toml_files = paths
            .iter()
            .map(|path| {
                let toml_text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
                Ok((path.file_name().unwrap_or_default().to_string_lossy().into_owned(), toml_text))
            })
            .collect::<Result<Vec<_>>>()?;
        Self::from_toml_files(
            &toml_files
                .iter()
                .map(|(file_name, toml_text)| (file_name.as_str(), toml_text.as_str()))
                .collect::<Vec<_>>(),
        )
    }

    pub fn from_toml_files(toml_files: &[(&str, &str)]) -> Result<Self> {
        let mut corrections = Self::default();
        let mut first_file_by_label: HashMap<String, &str> = HashMap::new();
        for (file_name, toml_text) in toml_files {
            for correction in parse_correction_file(file_name, toml_text)? {
                if let Some(first_file) = first_file_by_label.insert(correction.label(), file_name) {
                    bail!(
                        "correction file {file_name}: {} is already corrected in {first_file}; a field is corrected once",
                        correction.label()
                    );
                }
                corrections.entries.push(correction);
            }
        }
        Ok(corrections)
    }
}

fn set_correction_qualifiers(correction: &mut Correction, qualifier: &str) -> Result<()> {
    for part in qualifier.split(", ").filter(|part| !part.is_empty()) {
        if let Some(selection) = part.strip_prefix("selection ") {
            correction.selection = Some(serde_json::from_str(selection)?);
        } else if let Some(dc) = part.strip_prefix("dc ") {
            correction.dc = Some(serde_json::from_str(dc)?);
        } else if let Some(stance) = part.strip_prefix("stance ") {
            correction.stance = Some(serde_json::from_str(stance)?);
        } else if let Some(family) = part.strip_prefix("family ") {
            correction.family = Some(serde_json::from_str(family)?);
        } else if let Some(effect) = part.strip_prefix("effect ") {
            correction.effect = Some(serde_json::from_str(effect)?);
        } else if let Some(count) = part.strip_prefix("equipped_count ") {
            correction.equipped_count = Some(count.parse()?);
        } else if let Some(stat) = part.strip_prefix("stat ") {
            correction.stat = Some(serde_json::from_str(stat)?);
        } else if matches!(
            correction.kind,
            CorrectionKind::ItemBonus | CorrectionKind::AugmentBonus | CorrectionKind::SetTierBonus
        ) && !matches!(correction.to, CorrectionValue::Bonus(_))
        {
            let names = part.split(" / ").collect::<Vec<_>>();
            ensure!((2..=3).contains(&names.len()), "{} has invalid bonus qualifier {part:?}", correction.label());
            correction.stat = Some(names[0].to_string());
            correction.bonus_type = Some(names[1].to_string());
            if let Some(amount) = names.get(2) {
                correction.bonus_value = Some(amount.parse()?);
            }
        }
    }
    Ok(())
}

fn parse_correction_file(file_name: &str, toml_text: &str) -> Result<Vec<Correction>> {
    let mut document: toml::Table =
        toml::from_str(toml_text).with_context(|| format!("correction file {file_name}"))?;
    let tables = match document.remove(CORRECTION_TABLE_NAME) {
        None => Vec::new(),
        Some(toml::Value::Array(tables)) => tables,
        Some(_) => bail!(
            "correction file {file_name}: {CORRECTION_TABLE_NAME} must be an array of tables, written [[{CORRECTION_TABLE_NAME}]]"
        ),
    };
    if let Some(unknown_key) = document.keys().next() {
        bail!("correction file {file_name}: unknown top-level key {unknown_key:?}; the file holds only [[{CORRECTION_TABLE_NAME}]] tables");
    }
    tables
        .into_iter()
        .enumerate()
        .map(|(index, table)| {
            let entry_label = match table.get("name").and_then(toml::Value::as_str) {
                Some(name) => format!("{CORRECTION_TABLE_NAME} {name:?}"),
                None => format!("{CORRECTION_TABLE_NAME} #{}", index + 1),
            };
            let correction = table
                .try_into::<TomlCorrection>()
                .map_err(anyhow::Error::from)
                .and_then(|toml_correction| toml_correction.into_correction(file_name))
                .with_context(|| format!("correction file {file_name}: {entry_label}"))?;
            correction.validate().with_context(|| format!("correction file {file_name}: {}", correction.label()))?;
            Ok(correction)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{CorrectionValue, Corrections, DamageRow};

    #[test]
    fn committed_data_restores_all_current_corrections() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let corrections = Corrections::from_data(&root).unwrap();
        let constitution = corrections
            .entries
            .iter()
            .find(|correction| {
                correction.kind == ddo_model::enums::CorrectionKind::Enhancement
                    && correction.name == "Reaper's Constitution I"
                    && correction.field == "cost_per_rank"
            })
            .expect("missing confirmed Reaper cost correction");
        assert_eq!(constitution.from, CorrectionValue::Integers(vec![2]));
        assert_eq!(constitution.to, CorrectionValue::Integers(vec![1]));
        assert_eq!(constitution.source.page_url(), Some("https://ddowiki.com/page/Reaper_enhancements"));
        assert_eq!(constitution.read, "2026-10-08");
        assert_eq!(corrections.entries.len(), Corrections::embedded().unwrap().entries.len() + 1);
        let tree: toml::Value = toml::from_str(
            &std::fs::read_to_string(root.join("characters/enhancements/grim-barricade/tree.toml")).unwrap(),
        )
        .unwrap();
        let luck = tree["enhancement"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["name"].as_str() == Some("Reaper's Luck"))
            .unwrap();
        assert_eq!(luck["cost_per_rank"].as_array().unwrap(), &[toml::Value::Integer(1)]);
        let citation = luck["citation"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["field"].as_str() == Some("cost_per_rank"))
            .unwrap();
        assert_eq!(citation["in_game"].as_bool(), Some(true));
        assert_eq!(citation["read"].as_str(), Some("2026-10-18"));
        assert_eq!(
            citation["note"].as_str(),
            Some("The game shows cost 1; ddowiki's Reaper_enhancements page says 2.")
        );

        for (name, amount) in [("Legendary Sceptre of the Oozing Hunger", 25), ("Sceptre of the Oozing Hunger", 12)] {
            let sceptre = corrections
                .entries
                .iter()
                .find(|entry| {
                    entry.kind == ddo_model::enums::CorrectionKind::ItemBonus
                        && entry.name == name
                        && entry.field == "bonus_type"
                        && entry.stat.as_deref() == Some("Acid Spell Critical Damage")
                        && entry.bonus_value == Some(amount)
                })
                .unwrap();
            assert_eq!(sceptre.to.as_text(), Some("Enhancement"));
        }
    }

    #[test]
    fn damage_correction_json_uses_sorted_field_names() {
        let damage = DamageRow {
            trigger: "On Hit".to_string(),
            damage_type: "Acid".to_string(),
            dice_number: 1,
            dice_sides: 7,
            dice_bonus: 1,
            amount_from: 0,
            scale: 1.0,
            sort_order: 0,
        };
        assert!(CorrectionValue::Damage(damage).to_json().starts_with("{\"amount_from\":0,"));
    }
}
