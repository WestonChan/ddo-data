use anyhow::{Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemBuffDefinition {
    pub display_text: String,
    pub bonus_type_name: Option<String>,
}

#[derive(Clone, Copy)]
enum CapturedElement {
    BuffKind,
    DisplayText,
    FirstEffectBonus,
}

pub fn parse(path: &Path) -> Result<HashMap<String, ItemBuffDefinition>> {
    let file_text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let file_text = file_text.strip_prefix('\u{feff}').unwrap_or(&file_text);
    let mut reader = Reader::from_str(file_text);
    reader.config_mut().trim_text(true);

    let mut definitions_by_buff_kind = HashMap::new();
    let mut depth = 0usize;
    let mut buff_kind: Option<String> = None;
    let mut paragraphs: Vec<String> = Vec::new();
    let mut effect_count = 0usize;
    let mut bonus_type_name: Option<String> = None;
    let mut captured_element: Option<CapturedElement> = None;

    loop {
        match reader.read_event().with_context(|| format!("parsing {}", path.display()))? {
            Event::Start(start) => {
                depth += 1;
                captured_element = match (depth, start.name().as_ref()) {
                    (3, "Type") => Some(CapturedElement::BuffKind),
                    (3, "DisplayText") => Some(CapturedElement::DisplayText),
                    (3, "Effect") => {
                        effect_count += 1;
                        None
                    }
                    (4, "Bonus") if effect_count == 1 => Some(CapturedElement::FirstEffectBonus),
                    _ => None,
                };
            }
            Event::Text(text) => {
                let content = text.xml10_content().trim().to_string();
                match captured_element {
                    Some(CapturedElement::BuffKind) => buff_kind = Some(content),
                    Some(CapturedElement::DisplayText) if !content.is_empty() => paragraphs.push(content),
                    Some(CapturedElement::FirstEffectBonus) if !content.is_empty() => {
                        bonus_type_name = Some(content);
                    }
                    _ => {}
                }
            }
            Event::End(end) => {
                captured_element = None;
                if depth == 2 && end.name().as_ref() == "Buff" {
                    let display_text = std::mem::take(&mut paragraphs).join("\n");
                    let first_effect_bonus_type_name = bonus_type_name.take();
                    effect_count = 0;
                    if let Some(kind) = buff_kind.take() {
                        definitions_by_buff_kind.insert(
                            kind,
                            ItemBuffDefinition { display_text, bonus_type_name: first_effect_bonus_type_name },
                        );
                    }
                }
                depth = depth.saturating_sub(1);
            }
            Event::Empty(_)
            | Event::Comment(_)
            | Event::CData(_)
            | Event::Decl(_)
            | Event::PI(_)
            | Event::DocType(_) => {}
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(definitions_by_buff_kind)
}
