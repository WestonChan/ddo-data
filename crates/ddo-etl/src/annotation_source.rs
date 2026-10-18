use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "SourceFields", into = "SourceFields")]
pub enum AnnotationSource {
    Page(String),
    InGame,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceFields {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    in_game: Option<bool>,
}

impl AnnotationSource {
    pub fn page_url(&self) -> Option<&str> {
        match self {
            Self::Page(url) => Some(url),
            Self::InGame => None,
        }
    }

    pub(crate) fn is_in_game(&self) -> bool {
        matches!(self, Self::InGame)
    }

    pub(crate) fn from_fields(url: Option<String>, in_game: Option<bool>) -> anyhow::Result<Self> {
        match (url, in_game) {
            (Some(url), None) if !url.is_empty() => Ok(Self::Page(url)),
            (None, Some(true)) => Ok(Self::InGame),
            _ => anyhow::bail!("exactly one source is required: url or in_game = true"),
        }
    }

    pub(crate) fn from_columns(url: Option<String>, in_game: bool) -> rusqlite::Result<Self> {
        match (url, in_game) {
            (Some(url), false) => Ok(Self::Page(url)),
            (None, true) => Ok(Self::InGame),
            _ => Err(rusqlite::Error::InvalidQuery),
        }
    }
}

impl TryFrom<SourceFields> for AnnotationSource {
    type Error = anyhow::Error;

    fn try_from(fields: SourceFields) -> Result<Self, Self::Error> {
        Self::from_fields(fields.url, fields.in_game)
    }
}

impl From<AnnotationSource> for SourceFields {
    fn from(source: AnnotationSource) -> Self {
        match source {
            AnnotationSource::Page(url) => Self { url: Some(url), in_game: None },
            AnnotationSource::InGame => Self { url: None, in_game: Some(true) },
        }
    }
}

impl From<String> for AnnotationSource {
    fn from(url: String) -> Self {
        Self::Page(url)
    }
}

impl From<&str> for AnnotationSource {
    fn from(url: &str) -> Self {
        Self::Page(url.to_string())
    }
}

impl std::fmt::Display for AnnotationSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Page(url) => formatter.write_str(url),
            Self::InGame => formatter.write_str("in-game"),
        }
    }
}

impl schemars::JsonSchema for AnnotationSource {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "AnnotationSource".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "minLength": 1 },
                "in_game": { "type": "boolean", "const": true }
            },
            "oneOf": [
                { "required": ["url"], "not": { "required": ["in_game"] } },
                { "required": ["in_game"], "not": { "required": ["url"] } }
            ]
        })
    }
}
