use super::feats::Stance;
use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

pub fn parse(path: &Path) -> Result<Vec<Stance>> {
    let file: StanceFile = super::read_xml(path)?;
    Ok(file.stances)
}

#[derive(Deserialize)]
struct StanceFile {
    #[serde(rename = "Stance", default)]
    stances: Vec<Stance>,
}
