use anyhow::{Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;
use std::path::Path;

pub fn parse(path: &Path) -> Result<HashMap<String, String>> {
    let file_text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let file_text = file_text.strip_prefix('\u{feff}').unwrap_or(&file_text);
    let mut reader = Reader::from_str(file_text);
    reader.config_mut().trim_text(true);

    let mut templates_by_buff_kind = HashMap::new();
    let mut depth = 0usize;
    let mut buff_kind: Option<String> = None;
    let mut paragraphs: Vec<String> = Vec::new();
    let mut captured_element: Option<&'static str> = None;

    loop {
        match reader.read_event().with_context(|| format!("parsing {}", path.display()))? {
            Event::Start(start) => {
                depth += 1;
                if depth == 3 {
                    captured_element = match start.name().as_ref() {
                        "Type" => Some("Type"),
                        "DisplayText" => Some("DisplayText"),
                        _ => None,
                    };
                }
            }
            Event::Text(text) => {
                if let Some(element) = captured_element {
                    let content = text.xml10_content().trim().to_string();
                    match element {
                        "Type" => buff_kind = Some(content),
                        _ => {
                            if !content.is_empty() {
                                paragraphs.push(content);
                            }
                        }
                    }
                }
            }
            Event::End(end) => {
                if depth == 3 {
                    captured_element = None;
                }
                if depth == 2 && end.name().as_ref() == "Buff" {
                    if let Some(kind) = buff_kind.take() {
                        templates_by_buff_kind.insert(kind, std::mem::take(&mut paragraphs).join("\n"));
                    } else {
                        paragraphs.clear();
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
    Ok(templates_by_buff_kind)
}
