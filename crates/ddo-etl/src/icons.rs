use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::Path;

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
    pub copied: BTreeMap<String, usize>,
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
