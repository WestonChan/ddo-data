//! Copy upstream's image folders into one flat `icons/<family>/<Name>.png` tree that the API
//! serves as static files. Item images are nested by item kind upstream (`ItemImages/Weapon_Maul/
//! Maul_1i.png`) while the database key is just the stem, so those are flattened; every other
//! folder is already flat.

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::Path;

/// `(family served at /icons/<family>, upstream folder)`.
pub const FAMILIES: &[(&str, &str)] = &[
    ("items", "ItemImages"),
    ("augments", "AugmentImages"),
    ("feats", "FeatImages"),
    ("enhancements", "EnhancementImages"),
    ("spells", "SpellImages"),
    ("classes", "ClassImages"),
    ("filigrees", "FiligreeImages"),
    ("sets", "SetBonusImages"),
    ("sentient-gems", "SentientGemImages"),
    ("ui", "UIImages"),
];

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct IconReport {
    /// Files copied per family.
    pub copied: BTreeMap<String, usize>,
    /// Stems that appeared more than once inside one family; the first copy wins.
    pub duplicates: usize,
}

pub fn export_icons(source: &Path, out: &Path) -> Result<IconReport> {
    let mut report = IconReport::default();
    for (family, folder) in FAMILIES {
        let from = source.join(folder);
        if !from.is_dir() {
            continue;
        }
        let to = out.join(family);
        std::fs::create_dir_all(&to).with_context(|| format!("creating {}", to.display()))?;
        let mut count = 0;
        for path in png_files(&from)? {
            let Some(name) = path.file_name() else { continue };
            let target = to.join(name);
            if target.exists() {
                report.duplicates += 1;
                continue;
            }
            std::fs::copy(&path, &target).with_context(|| format!("copying {}", path.display()))?;
            count += 1;
        }
        report.copied.insert((*family).to_string(), count);
    }
    Ok(report)
}

/// Every `.png` under `dir`, recursively, in a stable order.
fn png_files(dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(dir).sort_by_file_name() {
        let entry = entry?;
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
            out.push(entry.into_path());
        }
    }
    Ok(out)
}
