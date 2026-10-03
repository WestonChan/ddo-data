use super::effect::Effect;
use anyhow::{Context, Result};
use serde::de::IgnoredAny;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct ItemBuffDefinition {
    pub display_text: String,
    pub bonus_type_name: Option<String>,
    pub fixed_amount: Option<i64>,
    pub effects: Vec<Effect>,
    pub has_activation_condition: bool,
}

#[derive(Deserialize)]
#[serde(rename = "Buffs")]
struct RawItemBuffs {
    #[serde(rename = "Buff", default)]
    buffs: Vec<RawItemBuff>,
}

#[derive(Deserialize)]
struct RawItemBuff {
    #[serde(rename = "$value", default)]
    children: Vec<ItemBuffChild>,
}

#[derive(Deserialize)]
enum ItemBuffChild {
    Type(String),
    DisplayText(String),
    Effect(Box<Effect>),
    Ignore(IgnoredAny),
    ApplyToWeaponOnly(IgnoredAny),
    Requirements(IgnoredAny),
    NegativeValues(IgnoredAny),
    Stance(IgnoredAny),
}

pub fn parse(path: &Path) -> Result<HashMap<String, ItemBuffDefinition>> {
    let file_text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let file_text = file_text.strip_prefix('\u{feff}').unwrap_or(&file_text);
    let raw_item_buffs: RawItemBuffs =
        quick_xml::de::from_str(file_text).with_context(|| format!("parsing {}", path.display()))?;
    let mut definitions_by_buff_kind = HashMap::new();
    for raw_buff in raw_item_buffs.buffs {
        let mut buff_kind = None;
        let mut paragraphs = Vec::new();
        let mut effects = Vec::new();
        let mut has_activation_condition = false;
        for child in raw_buff.children {
            match child {
                ItemBuffChild::Type(kind) => buff_kind = Some(kind.trim().to_string()),
                ItemBuffChild::DisplayText(paragraph) if !paragraph.trim().is_empty() => {
                    paragraphs.push(paragraph.trim().to_string());
                }
                ItemBuffChild::Effect(effect) => effects.push(*effect),
                ItemBuffChild::Requirements(_) | ItemBuffChild::Stance(_) => has_activation_condition = true,
                ItemBuffChild::DisplayText(_)
                | ItemBuffChild::Ignore(_)
                | ItemBuffChild::ApplyToWeaponOnly(_)
                | ItemBuffChild::NegativeValues(_) => {}
            }
        }
        if let Some(buff_kind) = buff_kind {
            let fixed_amount = match effects.as_slice() {
                [effect] => effect.simple_integer_amount().filter(|amount| *amount != 0),
                _ => None,
            };
            let bonus_type_name = effects.first().and_then(|effect| effect.bonus.clone());
            definitions_by_buff_kind.insert(
                buff_kind,
                ItemBuffDefinition {
                    display_text: paragraphs.join("\n"),
                    bonus_type_name,
                    fixed_amount,
                    effects,
                    has_activation_condition,
                },
            );
        }
    }
    Ok(definitions_by_buff_kind)
}
