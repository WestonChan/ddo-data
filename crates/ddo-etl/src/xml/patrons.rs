//! `Patrons.xml`: the favor patrons. Only the name is used for now.

use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

pub fn parse(path: &Path) -> Result<Vec<Patron>> {
    let file: PatronFile = super::read_xml(path)?;
    Ok(file.patrons)
}

#[derive(Debug, Deserialize)]
#[serde(rename = "Patrons")]
struct PatronFile {
    #[serde(rename = "Patron", default)]
    patrons: Vec<Patron>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Patron {
    #[serde(rename = "Name")]
    pub name: String,
}
