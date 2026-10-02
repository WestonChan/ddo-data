use super::drop_location::quest_name_spans;
use crate::wiki::is_iso_date;
use anyhow::{bail, Context, Result};
use serde::Deserialize;

const EMBEDDED_FILE_NAME: &str = "data/legacy_drop_sources.toml";
const EMBEDDED_FILE: &str = include_str!("../../data/legacy_drop_sources.toml");
const HTTPS_PREFIX: &str = "https://";
const MATCHED_TEXT_MASK: char = '\0';

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyDropSource {
    pub text: String,
    pub reason: String,
    #[serde(rename = "source")]
    pub source_url: String,
    pub read: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LegacyDropSources {
    sources_longest_text_first: Vec<LegacyDropSource>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyDropSourceFile {
    #[serde(default)]
    legacy: Vec<LegacyDropSource>,
}

impl LegacyDropSources {
    pub fn embedded() -> Result<Self> {
        Self::from_toml_str(EMBEDDED_FILE).context(EMBEDDED_FILE_NAME)
    }

    pub fn from_toml_str(text: &str) -> Result<Self> {
        let source_file: LegacyDropSourceFile = toml::from_str(text)?;
        let mut sources: Vec<LegacyDropSource> = Vec::with_capacity(source_file.legacy.len());
        for source in source_file.legacy {
            source.validate().with_context(|| format!("legacy source {:?}", source.text))?;
            if sources.iter().any(|known_source| known_source.text.eq_ignore_ascii_case(&source.text)) {
                bail!("legacy source {:?} appears twice", source.text);
            }
            sources.push(source);
        }
        sources.sort_by(|a, b| b.text.len().cmp(&a.text.len()).then_with(|| a.text.cmp(&b.text)));
        Ok(Self { sources_longest_text_first: sources })
    }

    pub fn texts_naming_every_segment(&self, segments: &[&str]) -> Option<Vec<&str>> {
        let mut named_texts: Vec<(usize, &str)> = Vec::new();
        for segment in segments {
            let texts_in_segment = self.texts_named_in(segment);
            if texts_in_segment.is_empty() {
                return None;
            }
            for (start, text) in texts_in_segment {
                if !named_texts.iter().any(|(_, known_text)| *known_text == text) {
                    named_texts.push((start, text));
                }
            }
        }
        (!named_texts.is_empty()).then(|| named_texts.into_iter().map(|(_, text)| text).collect())
    }

    pub fn names_legacy_source(&self, segment: &str) -> bool {
        !self.texts_named_in(segment).is_empty()
    }

    fn texts_named_in(&self, segment: &str) -> Vec<(usize, &str)> {
        let mut unmatched_segment = segment.to_string();
        let mut texts_by_first_position = Vec::new();
        for source in &self.sources_longest_text_first {
            let text_spans = quest_name_spans(&unmatched_segment, &source.text);
            let Some(first_span) = text_spans.first() else {
                continue;
            };
            texts_by_first_position.push((first_span.start, source.text.as_str()));
            for span in text_spans {
                let mask: String = std::iter::repeat_n(MATCHED_TEXT_MASK, span.len()).collect();
                unmatched_segment.replace_range(span, &mask);
            }
        }
        texts_by_first_position.sort();
        texts_by_first_position
    }
}

impl LegacyDropSource {
    fn validate(&self) -> Result<()> {
        if self.text.trim().is_empty() {
            bail!("text is empty");
        }
        if self.reason.trim().is_empty() {
            bail!("reason is empty");
        }
        if !self.source_url.starts_with(HTTPS_PREFIX) || self.source_url.len() == HTTPS_PREFIX.len() {
            bail!("source {:?} must be the https:// URL that shows the source is gone", self.source_url);
        }
        if !is_iso_date(&self.read) {
            bail!("read {:?} must be the ISO date the source was read, YYYY-MM-DD", self.read);
        }
        Ok(())
    }
}
