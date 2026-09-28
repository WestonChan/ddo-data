use anyhow::{Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;
use std::path::Path;

pub fn parse(path: &Path) -> Result<HashMap<String, String>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);

    let mut out = HashMap::new();
    let mut depth = 0usize;
    let mut kind: Option<String> = None;
    let mut paragraphs: Vec<String> = Vec::new();
    let mut capturing: Option<&'static str> = None;

    loop {
        match reader.read_event().with_context(|| format!("parsing {}", path.display()))? {
            Event::Start(e) => {
                depth += 1;
                if depth == 3 {
                    capturing = match e.name().as_ref() {
                        "Type" => Some("Type"),
                        "DisplayText" => Some("DisplayText"),
                        _ => None,
                    };
                }
            }
            Event::Text(t) => {
                if let Some(tag) = capturing {
                    let value = t.xml10_content().trim().to_string();
                    match tag {
                        "Type" => kind = Some(value),
                        _ => {
                            if !value.is_empty() {
                                paragraphs.push(value);
                            }
                        }
                    }
                }
            }
            Event::End(e) => {
                if depth == 3 {
                    capturing = None;
                }
                if depth == 2 && e.name().as_ref() == "Buff" {
                    if let Some(k) = kind.take() {
                        out.insert(k, std::mem::take(&mut paragraphs).join("\n"));
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
    Ok(out)
}
