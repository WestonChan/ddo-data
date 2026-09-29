use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const ICON_FAMILY_SOURCE_FOLDERS: &[(&str, &str)] = &[
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
pub struct IconExportReport {
    pub copied_count_by_family: BTreeMap<String, usize>,
    pub skipped_duplicate_count: usize,
}

pub fn export_icons(data_files_dir: &Path, icons_dir: &Path) -> Result<IconExportReport> {
    let mut report = IconExportReport::default();
    for (family, source_folder) in ICON_FAMILY_SOURCE_FOLDERS {
        let source_dir = data_files_dir.join(source_folder);
        if !source_dir.is_dir() {
            continue;
        }
        let family_dir = icons_dir.join(family);
        std::fs::create_dir_all(&family_dir).with_context(|| format!("creating {}", family_dir.display()))?;
        let mut copied_count = 0;
        for source_path in png_files_under(&source_dir)? {
            let Some(file_name) = source_path.file_name() else { continue };
            let target_path = family_dir.join(file_name);
            if target_path.exists() {
                report.skipped_duplicate_count += 1;
                continue;
            }
            std::fs::copy(&source_path, &target_path).with_context(|| format!("copying {}", source_path.display()))?;
            copied_count += 1;
        }
        report.copied_count_by_family.insert((*family).to_string(), copied_count);
    }
    Ok(report)
}

fn png_files_under(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut png_paths = Vec::new();
    for entry in walkdir::WalkDir::new(dir).sort_by_file_name() {
        let entry = entry?;
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
            png_paths.push(entry.into_path());
        }
    }
    Ok(png_paths)
}
