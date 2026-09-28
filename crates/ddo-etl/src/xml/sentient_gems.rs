use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

pub fn parse(path: &Path) -> Result<Vec<SentientGem>> {
    let file: SentientGemFile = super::read_xml(path)?;
    Ok(file.gems.into_iter().map(|g| SentientGem { name: g.name.trim().to_string(), ..g }).collect())
}

#[derive(Deserialize)]
struct SentientGemFile {
    #[serde(rename = "Gem", default)]
    gems: Vec<SentientGem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SentientGem {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Icon")]
    pub icon: Option<String>,
    #[serde(rename = "Description")]
    pub description: Option<String>,
}
