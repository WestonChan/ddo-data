mod fields;

pub use fields::{correctable_field, correctable_fields, CorrectableField, FieldShape};

use crate::wiki::is_iso_date;
use anyhow::{bail, Context, Result};
use ddo_model::enums::CorrectionKind;
use rusqlite::types::Value as SqlValue;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

const EMBEDDED_CORRECTION_FILES: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/correction_files.rs"));
const CORRECTION_TABLE_NAME: &str = "correction";
const NULL_SPELLING: &str = "null";

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(from = "TomlCorrectionValue")]
pub enum CorrectionValue {
    Integer(i64),
    Float(f64),
    Text(String),
    Bonus(BonusAddition),
    Null,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BonusAddition {
    pub stat: String,
    pub bonus_type: String,
    pub value: i64,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum TomlCorrectionValue {
    Integer(i64),
    Float(f64),
    Text(String),
    Bonus(BonusAddition),
}

impl From<TomlCorrectionValue> for CorrectionValue {
    fn from(toml_value: TomlCorrectionValue) -> Self {
        match toml_value {
            TomlCorrectionValue::Integer(number) => Self::Integer(number),
            TomlCorrectionValue::Float(number) => Self::Float(number),
            TomlCorrectionValue::Text(text) if text == NULL_SPELLING => Self::Null,
            TomlCorrectionValue::Text(text) => Self::Text(text),
            TomlCorrectionValue::Bonus(bonus) => Self::Bonus(bonus),
        }
    }
}

impl CorrectionValue {
    pub fn to_json(&self) -> String {
        match self {
            Self::Integer(number) => number.to_string(),
            Self::Float(number) => serde_json::Value::from(*number).to_string(),
            Self::Text(text) => serde_json::Value::from(text.as_str()).to_string(),
            Self::Bonus(bonus) => format!(
                "{{\"bonus_type\":{},\"stat\":{},\"value\":{}}}",
                serde_json::Value::from(bonus.bonus_type.as_str()),
                serde_json::Value::from(bonus.stat.as_str()),
                bonus.value
            ),
            Self::Null => NULL_SPELLING.to_string(),
        }
    }

    pub fn to_sql(&self) -> SqlValue {
        match self {
            Self::Integer(number) => SqlValue::Integer(*number),
            Self::Float(number) => SqlValue::Real(*number),
            Self::Text(text) => SqlValue::Text(text.clone()),
            Self::Bonus(_) => SqlValue::Text(self.to_json()),
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

    fn fits(&self, field: &CorrectableField) -> bool {
        match (self, field.shape) {
            (Self::Null, _) => field.is_nullable,
            (Self::Integer(_), FieldShape::Integer) => true,
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
    pub stat: Option<String>,
    pub bonus_type: Option<String>,
    pub reason: String,
    pub source: String,
    pub read: String,
    pub file_name: String,
}

impl Correction {
    pub fn correctable_field(&self) -> Result<&'static CorrectableField> {
        correctable_field(self.kind, &self.field).with_context(|| {
            let allowed_fields: Vec<&str> = correctable_fields(self.kind).iter().map(|field| field.name).collect();
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
        if let Some(family) = &self.family {
            qualifier_parts.push(format!("family {family:?}"));
        }
        if let (Some(stat), Some(bonus_type)) = (&self.stat, &self.bonus_type) {
            qualifier_parts.push(format!("{stat} / {bonus_type}"));
        }
        match &self.to {
            CorrectionValue::Bonus(bonus) => qualifier_parts.push(format!("{} / {}", bonus.stat, bonus.bonus_type)),
            CorrectionValue::Text(added_name)
                if matches!(self.kind, CorrectionKind::ItemSocket | CorrectionKind::ItemEffect) =>
            {
                qualifier_parts.push(added_name.clone())
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

    fn validate_qualifier_keys(&self, field: &CorrectableField) -> Result<()> {
        if self.family.is_some() && !matches!(self.kind, CorrectionKind::Augment | CorrectionKind::AugmentBonus) {
            bail!("family narrows only an augment or augment_bonus correction, not a {}", self.kind.as_str());
        }
        let names_a_bonus = self.stat.is_some() || self.bonus_type.is_some();
        let needs_a_bonus = self.kind == CorrectionKind::AugmentBonus && field.shape != FieldShape::BonusAddition;
        if needs_a_bonus && (self.stat.is_none() || self.bonus_type.is_none()) {
            bail!("an augment_bonus {} correction names the bonus with stat and bonus_type", field.name);
        }
        if names_a_bonus && !needs_a_bonus {
            bail!(
                "stat and bonus_type name the bonus of an augment_bonus value or bonus_type correction; an add names them in to"
            );
        }
        if field.shape == FieldShape::BonusTypeName
            && self.bonus_type.as_ref() != self.from.as_text().map(str::to_string).as_ref()
        {
            bail!("from must be the bonus_type the correction names, his current type");
        }
        Ok(())
    }

    fn validate_values(&self, field: &CorrectableField) -> Result<()> {
        match field.shape {
            FieldShape::Removal => {
                if (&self.from, &self.to) != (&CorrectionValue::Integer(0), &CorrectionValue::Integer(1)) {
                    bail!("a remove takes from = 0 (his files still carry the row) and to = 1");
                }
            }
            FieldShape::BonusAddition => {
                if !matches!((&self.from, &self.to), (CorrectionValue::Null, CorrectionValue::Bonus(_))) {
                    bail!(
                        "an add takes from = \"null\" and to = {{ stat = \"...\", bonus_type = \"...\", value = N }}"
                    );
                }
            }
            FieldShape::SocketAddition => {
                if !matches!((&self.from, &self.to), (CorrectionValue::Null, CorrectionValue::Text(_))) {
                    bail!("an add takes from = \"null\" and to = the socket label to add");
                }
            }
            FieldShape::EffectAddition => {
                if !matches!((&self.from, &self.to), (CorrectionValue::Null, CorrectionValue::Text(_))) {
                    bail!("an add takes from = \"null\" and to = the name of the effect to add");
                }
            }
            _ => {
                for (value_role, value) in [("from", &self.from), ("to", &self.to)] {
                    if !value.fits(field) {
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
        if !self.source.starts_with("https://") || self.source.len() == "https://".len() {
            bail!("source {:?} must be the https:// URL that shows the right value", self.source);
        }
        if !is_iso_date(&self.read) {
            bail!("read {:?} must be the ISO date the source was read, YYYY-MM-DD", self.read);
        }
        Ok(())
    }
}

fn expected_value_text(field: &CorrectableField) -> String {
    let non_null_text = match field.shape {
        FieldShape::Integer => "an integer",
        FieldShape::Flag => "0 or 1",
        FieldShape::Text => "a string",
        FieldShape::NamedReference { .. } | FieldShape::SetName | FieldShape::BonusTypeName => {
            "the name of the row it refers to"
        }
        FieldShape::RowName => "the row's name",
        FieldShape::Removal => "0 for from and 1 for to",
        FieldShape::BonusAddition => "a { stat, bonus_type, value } table",
        FieldShape::EffectAddition => "an effect name",
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
    stat: Option<String>,
    bonus_type: Option<String>,
    from: CorrectionValue,
    to: CorrectionValue,
    reason: String,
    source: String,
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
            stat: self.stat,
            bonus_type: self.bonus_type,
            reason: self.reason,
            source: self.source,
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
    pub fn embedded() -> Result<Self> {
        Self::from_toml_files(EMBEDDED_CORRECTION_FILES)
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
