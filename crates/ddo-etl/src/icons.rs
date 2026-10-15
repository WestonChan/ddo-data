use anyhow::{ensure, Context, Result};
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub const ICON_KIND_SOURCE_FOLDERS: &[(&str, &str)] = &[
    ("items", "ItemImages"),
    ("augments", "AugmentImages"),
    ("feats", "FeatImages"),
    ("enhancements", "EnhancementImages"),
    ("spells", "SpellImages"),
    ("classes", "ClassImages"),
    ("filigrees", "FiligreeImages"),
    ("sets", "SetBonusImages"),
    ("sentient-gems", "SentientGemImages"),
    ("stances", "StanceImages"),
    ("ui", "UIImages"),
];

pub const ICON_TABLE_KINDS: &[(&str, &str)] = &[
    ("items", "items"),
    ("item_augment_slot_options", "augments"),
    ("augments", "augments"),
    ("feats", "feats"),
    ("enhancements", "enhancements"),
    ("spells", "spells"),
    ("filigrees", "filigrees"),
    ("set_bonuses", "sets"),
    ("sentient_gems", "sentient-gems"),
    ("enhancement_trees", "enhancements"),
    ("stances", "stances"),
    ("clickies", "spells"),
];

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct IconExportReport {
    pub copied_count_by_kind: BTreeMap<String, usize>,
    pub skipped_duplicate_count: usize,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct IconCompletionReport {
    pub copied_from_other_kind: usize,
    pub missing_icons: Vec<String>,
    pub unresolved_case_aliases: Vec<String>,
}

pub fn icon_kind_for_data_path(path: &Path) -> Option<(&'static str, &'static str)> {
    match crate::tree::file_kind(Path::new(""), path).ok()? {
        "Augment" => Some(("augments", "augments")),
        "Item" => Some(("items", "items")),
        "Set" => Some(("set_bonuses", "sets")),
        "OptionalBuffs" => Some(("optional_buffs", "*")),
        "Class" => Some(("classes", "classes")),
        "FeatCategory" => Some(("feats", "feats")),
        "EnhancementTree" => Some(("enhancements", "enhancements")),
        "SpellList" => Some(("spells", "spells")),
        "Clickies" => Some(("clickies", "spells")),
        "FiligreeSet" => Some(("filigrees", "filigrees")),
        "Stances" => Some(("stances", "stances")),
        "SentientGem" => Some(("sentient_gems", "sentient-gems")),
        _ => None,
    }
}

#[derive(Default)]
pub(crate) struct IconIndex {
    global_paths: BTreeMap<(String, String), PathBuf>,
    enhancement_paths: BTreeMap<String, (String, PathBuf)>,
}

impl IconIndex {
    fn from_global_icons(icons_dir: &Path) -> Result<Self> {
        let mut index = Self::default();
        if icons_dir.is_dir() {
            for entry in std::fs::read_dir(icons_dir)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    let kind = entry.file_name().to_string_lossy().into_owned();
                    ensure!(
                        ICON_KIND_SOURCE_FOLDERS.iter().any(|(known, _)| *known == kind),
                        "unknown icon kind folder {}",
                        entry.path().display()
                    );
                }
            }
        }
        for &(kind, _) in ICON_KIND_SOURCE_FOLDERS {
            let folder = icons_dir.join(kind);
            if !folder.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(&folder)? {
                let entry = entry?;
                if !entry.file_type()?.is_file()
                    || !entry.path().extension().is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
                {
                    continue;
                }
                index.insert_global(kind, entry.path())?;
            }
        }
        Ok(index)
    }

    pub(crate) fn from_data(root: &Path) -> Result<Self> {
        let mut index = Self::from_global_icons(&root.join("icons"))?;
        let issues = enhancement_icon_actions(root)?;
        ensure!(issues.is_empty(), "HARD enhancement icon layout:\n{}", issues.join("\n"));
        let enhancement_root = root.join("characters/enhancements");
        if !enhancement_root.is_dir() {
            return Ok(index);
        }
        for entry in std::fs::read_dir(enhancement_root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let tree_name = entry.file_name().to_string_lossy().into_owned();
            let icons_dir = entry.path().join("icons");
            if !icons_dir.is_dir() {
                continue;
            }
            let tree_file = entry.path().join("tree.toml");
            if !tree_file.is_file() {
                continue;
            }
            let document: toml::Value = toml::from_str(&std::fs::read_to_string(&tree_file)?)
                .with_context(|| format!("reading {}", tree_file.display()))?;
            let mut referenced_icons = BTreeSet::new();
            collect_icon_names(&document, &mut referenced_icons);
            for icon in std::fs::read_dir(icons_dir)? {
                let icon = icon?;
                if !icon.file_type()?.is_file()
                    || !icon.path().extension().is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
                {
                    continue;
                }
                let path = icon.path();
                let file_name =
                    path.file_name().and_then(OsStr::to_str).context("enhancement icon has no UTF-8 name")?;
                let key = file_name.to_lowercase();
                ensure!(referenced_icons.contains(&key), "unreferenced enhancement icon {}", path.display());
                ensure!(
                    !index.global_paths.contains_key(&("enhancements".to_string(), key.clone())),
                    "enhancement icon {file_name:?} exists in both {tree_name} and icons/enhancements"
                );
                if let Some((other_tree, other_path)) =
                    index.enhancement_paths.insert(key, (tree_name.clone(), path.clone()))
                {
                    anyhow::bail!(
                        "enhancement icon {file_name:?} exists in both {other_tree} ({}) and {tree_name} ({})",
                        other_path.display(),
                        path.display()
                    );
                }
            }
        }
        Ok(index)
    }

    fn insert_global(&mut self, kind: &str, path: PathBuf) -> Result<()> {
        let file_name = path.file_name().and_then(OsStr::to_str).context("icon has no UTF-8 name")?;
        let key = (kind.to_string(), file_name.to_lowercase());
        if let Some(previous) = self.global_paths.insert(key, path.clone()) {
            ensure!(previous == path, "icon names differ only by case: {} and {}", previous.display(), path.display());
        }
        Ok(())
    }

    fn global(&self, kind: &str, file_name: &str) -> Option<&Path> {
        self.global_paths.get(&(kind.to_string(), file_name.to_lowercase())).map(PathBuf::as_path)
    }

    fn global_exact(&self, kind: &str, file_name: &str) -> Option<&Path> {
        self.global(kind, file_name).filter(|path| path.file_name() == Some(OsStr::new(file_name)))
    }

    pub(crate) fn has_database_icon(&self, kind: &str, file_name: &str) -> bool {
        self.global_exact(kind, file_name).is_some()
            || (kind == "enhancements"
                && self
                    .enhancement_paths
                    .get(&file_name.to_lowercase())
                    .is_some_and(|(_, path)| path.file_name() == Some(OsStr::new(file_name))))
    }

    pub(crate) fn validate_reference(
        &self,
        kind: &str,
        tree: Option<&str>,
        reference: &str,
    ) -> Result<(String, String)> {
        ensure!(!reference.contains('\\'), "image reference contains a backslash");
        let segments = reference.split('/').collect::<Vec<_>>();
        ensure!(
            segments.iter().all(|segment| !matches!(*segment, "" | "." | "..")),
            "image reference has an empty or relative segment"
        );
        if kind != "*" {
            ensure!(segments.len() == 1, "single-kind image must use a bare name");
            let file_name = format!("{reference}.png");
            self.validate_file_icon(kind, tree, &file_name)?;
            return Ok((kind.to_string(), file_name.to_lowercase()));
        }
        let (kind, file_name) = match segments.as_slice() {
            [kind, name] => {
                let file_name = format!("{name}.png");
                ensure!(self.global_exact(kind, &file_name).is_some(), "missing image {reference}.png");
                (*kind, file_name)
            }
            ["enhancements", tree, name] => {
                let file_name = format!("{name}.png");
                ensure!(
                    self.enhancement_paths.contains_key(&file_name.to_lowercase()),
                    "missing tree-local image {reference}.png"
                );
                self.validate_file_icon("enhancements", Some(tree), &file_name)?;
                ("enhancements", file_name)
            }
            _ => anyhow::bail!("cross-kind image must use kind/name or enhancements/tree/name"),
        };
        Ok((kind.to_string(), file_name.to_lowercase()))
    }

    pub(crate) fn validate_file_icon(&self, kind: &str, tree_name: Option<&str>, file_name: &str) -> Result<()> {
        if kind == "enhancements" {
            if let Some((owner_tree, path)) = self.enhancement_paths.get(&file_name.to_lowercase()) {
                ensure!(
                    tree_name == Some(owner_tree.as_str()),
                    "enhancement icon {file_name:?} belongs to tree {owner_tree}, referenced by tree {}",
                    tree_name.unwrap_or("<unknown>")
                );
                ensure!(
                    path.file_name() == Some(OsStr::new(file_name)),
                    "enhancement icon {file_name:?} differs in case from {}",
                    path.display()
                );
                return Ok(());
            }
        }
        ensure!(self.global_exact(kind, file_name).is_some(), "missing icon {kind}/{file_name}");
        Ok(())
    }

    pub(crate) fn unreferenced_global_counts(
        &self,
        referenced: &BTreeSet<(String, String)>,
    ) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for kind_and_name in self.global_paths.keys() {
            if !referenced.contains(kind_and_name) {
                *counts.entry(kind_and_name.0.clone()).or_insert(0) += 1;
            }
        }
        counts
    }

    fn enhancement_icons(&self) -> impl Iterator<Item = (&str, &Path)> {
        self.enhancement_paths.values().map(|(tree, path)| (tree.as_str(), path.as_path()))
    }
}

pub fn complete_referenced_icons(db: &Connection, data_dir: &Path) -> Result<IconCompletionReport> {
    let icons_dir = data_dir.join("icons");
    let mut icon_index = IconIndex::from_global_icons(&icons_dir)?;
    let mut report = IconCompletionReport::default();
    for &(table, kind) in ICON_TABLE_KINDS {
        let mut statement =
            db.prepare(&format!("SELECT DISTINCT icon FROM {table} WHERE icon IS NOT NULL AND icon <> ''"))?;
        let icon_names: BTreeSet<String> =
            statement.query_map([], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<_>>()?;
        for icon_name in &icon_names {
            ensure!(
                !icon_name.contains('/') && !icon_name.contains('\\'),
                "{table} icon {icon_name:?} contains a path separator"
            );
            let destination = icons_dir.join(kind).join(format!("{icon_name}.png"));
            let file_name = format!("{icon_name}.png");
            if icon_index.global_exact(kind, &file_name).is_some() {
                continue;
            }
            if icon_index.global(kind, &file_name).is_some() {
                report.unresolved_case_aliases.push(format!("{kind}/{file_name}"));
                continue;
            }
            let Some(source) =
                ICON_KIND_SOURCE_FOLDERS.iter().find_map(|(source_kind, _)| icon_index.global(source_kind, &file_name))
            else {
                report.missing_icons.push(format!("{kind}/{file_name}"));
                continue;
            };
            if source.file_name() != Some(OsStr::new(&file_name)) {
                report.unresolved_case_aliases.push(format!("{kind}/{file_name}"));
                continue;
            }
            report.copied_from_other_kind += 1;
            std::fs::create_dir_all(destination.parent().context("icon destination has no folder")?)?;
            std::fs::copy(source, &destination)
                .with_context(|| format!("copying {} to {}", source.display(), destination.display()))?;
            icon_index.insert_global(kind, destination)?;
        }
    }
    Ok(report)
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct EnhancementIconLayoutReport {
    pub moved_to_trees: usize,
    pub shared_icons: usize,
}

fn collect_icon_names(value: &toml::Value, names: &mut BTreeSet<String>) {
    match value {
        toml::Value::Table(fields) => {
            if let Some(icon) = fields.get("icon").and_then(toml::Value::as_str) {
                names.insert(format!("{icon}.png").to_lowercase());
            }
            for child in fields.values() {
                collect_icon_names(child, names);
            }
        }
        toml::Value::Array(rows) => {
            for row in rows {
                collect_icon_names(row, names);
            }
        }
        _ => {}
    }
}

fn enhancement_trees_by_icon(data_dir: &Path) -> Result<BTreeMap<String, BTreeSet<String>>> {
    let enhancement_root = data_dir.join("characters/enhancements");
    if !enhancement_root.is_dir() {
        return Ok(BTreeMap::new());
    }
    let mut trees_by_icon = BTreeMap::<String, BTreeSet<String>>::new();
    for entry in std::fs::read_dir(&enhancement_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let tree_name = entry.file_name().to_string_lossy().into_owned();
        let tree_file = entry.path().join("tree.toml");
        if !tree_file.is_file() {
            continue;
        }
        let document: toml::Value = toml::from_str(&std::fs::read_to_string(&tree_file)?)
            .with_context(|| format!("reading {}", tree_file.display()))?;
        let mut names = BTreeSet::new();
        collect_icon_names(&document, &mut names);
        for name in names {
            trees_by_icon.entry(name).or_default().insert(tree_name.clone());
        }
    }
    Ok(trees_by_icon)
}

pub(crate) fn enhancement_icon_actions(data_dir: &Path) -> Result<Vec<String>> {
    let references = enhancement_trees_by_icon(data_dir)?;
    let enhancement_root = data_dir.join("characters/enhancements");
    let shared_root = data_dir.join("icons/enhancements");
    let mut actions = Vec::new();
    for root in [&enhancement_root, &shared_root] {
        if !root.is_dir() {
            continue;
        }
        for path in png_files_under(root)? {
            let filename = path.file_name().and_then(OsStr::to_str).context("enhancement PNG has no UTF-8 filename")?;
            let trees = references.get(&filename.to_lowercase());
            let Some(trees) = trees.filter(|trees| !trees.is_empty()) else {
                if root == &enhancement_root {
                    actions.push(format!("{}: delete; no exported tree.toml references this PNG", path.display()));
                }
                continue;
            };
            let destination = if trees.len() == 1 {
                enhancement_root.join(trees.first().unwrap()).join("icons").join(filename)
            } else {
                shared_root.join(filename)
            };
            if path == destination {
                continue;
            }
            let action = if destination.is_file() && std::fs::read(&path)? == std::fs::read(&destination)? {
                format!("delete duplicate; referenced PNG is {}", destination.display())
            } else {
                format!("move to {}", destination.display())
            };
            actions.push(format!("{}: {action}", path.display()));
        }
    }
    actions.sort();
    Ok(actions)
}

pub fn place_enhancement_icons_beside_trees(data_dir: &Path) -> Result<EnhancementIconLayoutReport> {
    let icon_index = IconIndex::from_global_icons(&data_dir.join("icons"))?;
    let enhancement_root = data_dir.join("characters/enhancements");
    let trees_by_icon = enhancement_trees_by_icon(data_dir)?;
    let mut report = EnhancementIconLayoutReport::default();
    for (file_name, trees) in trees_by_icon {
        let Some(source) = icon_index.global("enhancements", &file_name) else { continue };
        if trees.len() != 1 {
            report.shared_icons += 1;
            continue;
        }
        let tree_name = trees.iter().next().context("single enhancement tree has no name")?;
        let destination =
            enhancement_root.join(tree_name).join("icons").join(source.file_name().context("icon has no name")?);
        std::fs::create_dir_all(destination.parent().context("tree icon has no folder")?)?;
        if destination.exists() {
            ensure!(
                std::fs::read(source)? == std::fs::read(&destination)?,
                "enhancement icon {} differs from {}",
                source.display(),
                destination.display()
            );
            std::fs::remove_file(source)?;
        } else {
            std::fs::rename(source, &destination)?;
        }
        report.moved_to_trees += 1;
    }
    Ok(report)
}

pub fn export_icons(data_files_dir: &Path, icons_dir: &Path) -> Result<IconExportReport> {
    let mut report = IconExportReport::default();
    for (kind, source_folder) in ICON_KIND_SOURCE_FOLDERS {
        let source_dir = data_files_dir.join(source_folder);
        if !source_dir.is_dir() {
            continue;
        }
        let kind_dir = icons_dir.join(kind);
        std::fs::create_dir_all(&kind_dir).with_context(|| format!("creating {}", kind_dir.display()))?;
        let mut copied_count = 0;
        for source_path in png_files_under(&source_dir)? {
            let Some(file_name) = source_path.file_name() else { continue };
            let target_path = kind_dir.join(file_name);
            if target_path.exists() {
                report.skipped_duplicate_count += 1;
                continue;
            }
            std::fs::copy(&source_path, &target_path).with_context(|| format!("copying {}", source_path.display()))?;
            copied_count += 1;
        }
        report.copied_count_by_kind.insert((*kind).to_string(), copied_count);
    }
    Ok(report)
}

pub fn link_corrected_icons(corrections: &crate::corrections::Corrections, icons_dir: &Path) -> Result<()> {
    for correction in &corrections.entries {
        if correction.field != "icon" {
            continue;
        }
        let Some(reference) = correction.to.as_text() else { continue };
        if reference.split('/').count() == 3 {
            link_qualified_icon(icons_dir, reference)?;
        }
    }
    Ok(())
}

fn link_qualified_icon(icons_dir: &Path, reference: &str) -> Result<()> {
    let segments = reference.split('/').collect::<Vec<_>>();
    ensure!(
        segments.iter().all(|segment| !matches!(*segment, "" | "." | "..") && !segment.contains('\\')),
        "invalid qualified image {reference:?}"
    );
    let ["enhancements", tree, name] = segments.as_slice() else {
        anyhow::bail!("invalid tree-local image {reference:?}");
    };
    let source = icons_dir.join("enhancements").join(format!("{name}.png"));
    if !source.is_file() {
        return Ok(());
    }
    let destination = icons_dir.join("enhancements").join(tree).join(format!("{name}.png"));
    if destination.exists() {
        ensure!(
            std::fs::read(&source)? == std::fs::read(&destination)?,
            "image {} differs from {}",
            destination.display(),
            source.display()
        );
    } else {
        std::fs::create_dir_all(destination.parent().context("qualified image has no parent")?)?;
        std::fs::hard_link(&source, &destination)
            .with_context(|| format!("linking {} to {}", destination.display(), source.display()))?;
    }
    Ok(())
}

pub fn copy_data_icons(data_dir: &Path, icons_dir: &Path) -> Result<IconExportReport> {
    let source_icons = data_dir.join("icons");
    anyhow::ensure!(source_icons.is_dir(), "tree icons directory {} does not exist", source_icons.display());
    let icon_index = IconIndex::from_data(data_dir)?;
    let mut report = IconExportReport::default();
    for (kind, _) in ICON_KIND_SOURCE_FOLDERS {
        let source_kind = source_icons.join(kind);
        if !source_kind.is_dir() {
            continue;
        }
        let target_kind = icons_dir.join(kind);
        std::fs::create_dir_all(&target_kind)?;
        let mut copied_count = 0;
        for source_path in png_files_under(&source_kind)? {
            let destination = target_kind.join(source_path.file_name().context("icon has no file name")?);
            if destination.exists() {
                report.skipped_duplicate_count += 1;
                continue;
            }
            std::fs::copy(&source_path, &destination).with_context(|| format!("copying {}", source_path.display()))?;
            copied_count += 1;
        }
        report.copied_count_by_kind.insert((*kind).to_string(), copied_count);
    }
    let enhancement_dir = icons_dir.join("enhancements");
    std::fs::create_dir_all(&enhancement_dir)?;
    for (tree, source_path) in icon_index.enhancement_icons() {
        let destination = enhancement_dir.join(source_path.file_name().context("enhancement icon has no file name")?);
        if destination.exists() {
            report.skipped_duplicate_count += 1;
        } else {
            std::fs::copy(source_path, &destination)?;
            *report.copied_count_by_kind.entry("enhancements".to_string()).or_default() += 1;
        }
        let name = source_path.file_stem().and_then(OsStr::to_str).context("enhancement image has no UTF-8 name")?;
        link_qualified_icon(icons_dir, &format!("enhancements/{tree}/{name}"))?;
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

#[cfg(test)]
mod icon_index_tests {
    #[test]
    fn icons_require_their_kind_and_exact_case() {
        let root = std::env::temp_dir().join(format!("ddo-icon-kind-{}", std::process::id()));
        let folder = root.join("icons/feats");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("WarforgedFeat.png"), b"icon").unwrap();
        let index = super::IconIndex::from_data(&root).unwrap();
        index.validate_file_icon("feats", None, "WarforgedFeat.png").unwrap();
        assert!(index.validate_file_icon("items", None, "WarforgedFeat.png").is_err());
        assert!(index.validate_file_icon("feats", None, "Warforgedfeat.png").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn orphaned_tree_art_and_unknown_kinds_are_rejected() {
        let root = std::env::temp_dir().join(format!("ddo-icon-orphan-{}", std::process::id()));
        let icons = root.join("characters/enhancements/example/icons");
        std::fs::create_dir_all(&icons).unwrap();
        std::fs::write(icons.join("Unused.png"), b"icon").unwrap();
        let error = super::IconIndex::from_data(&root).err().unwrap().to_string();
        assert!(error.contains("tree.toml"), "{error}");
        std::fs::write(root.join("characters/enhancements/example/tree.toml"), "name = 'Example'").unwrap();
        let error = super::IconIndex::from_data(&root).err().unwrap().to_string();
        assert!(error.contains("Unused.png"), "{error}");
        std::fs::create_dir_all(root.join("icons/unlisted-kind")).unwrap();
        let error = super::IconIndex::from_data(&root).err().unwrap().to_string();
        assert!(error.contains("unlisted-kind"), "{error}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn icon_index_matches_case_without_relisting_the_folder() {
        let root = std::env::temp_dir().join(format!("ddo-icon-case-{}", std::process::id()));
        std::fs::create_dir_all(root.join("icons/feats")).unwrap();
        std::fs::write(root.join("icons/feats/WarforgedFeat.png"), b"icon").unwrap();
        let index = super::IconIndex::from_data(&root).unwrap();
        assert!(index.global_exact("feats", "WarforgedFeat.png").is_some());
        assert!(index.global_exact("feats", "Warforgedfeat.png").is_none());
        assert!(index.global("feats", "Warforgedfeat.png").is_some());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn copied_icons_preserve_upstream_case_and_report_mismatched_references() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        db.execute(
            "INSERT INTO feats (name, source_kind, icon, slug) VALUES ('Test Feat', 'standard', 'MobileSpellCasting', 'test-feat')",
            [],
        )
        .unwrap();
        let root = std::env::temp_dir().join(format!("ddo-icon-normalize-{}", std::process::id()));
        let icons = root.join("icons/feats");
        std::fs::create_dir_all(&icons).unwrap();
        std::fs::write(icons.join("MobileSpellcasting.png"), b"icon").unwrap();
        let report = super::complete_referenced_icons(&db, &root).unwrap();
        assert_eq!(report.unresolved_case_aliases, ["feats/MobileSpellCasting.png"]);
        let index = super::IconIndex::from_data(&root).unwrap();
        assert!(index.global_exact("feats", "MobileSpellcasting.png").is_some());
        assert!(index.global_exact("feats", "MobileSpellCasting.png").is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_upstream_art_is_reported_without_changing_the_baseline() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        for &(table, _) in super::ICON_TABLE_KINDS {
            db.execute_batch(&format!("CREATE TABLE {table} (name TEXT, icon TEXT)")).unwrap();
        }
        db.execute("INSERT INTO items (name, icon) VALUES ('Null Blade', 'Shortsword_18')", []).unwrap();
        let root = std::env::temp_dir().join(format!("ddo-icon-missing-{}", std::process::id()));
        for &(_, folder) in super::ICON_KIND_SOURCE_FOLDERS {
            std::fs::create_dir_all(root.join(folder)).unwrap();
        }
        std::fs::write(root.join("ItemImages/Present.png"), b"icon").unwrap();
        let report = super::complete_referenced_icons(&db, &root).unwrap();
        assert_eq!(report.missing_icons, vec!["items/Shortsword_18.png"]);
        assert_eq!(
            db.query_row("SELECT icon FROM items", [], |row| row.get::<_, Option<String>>(0)).unwrap(),
            Some("Shortsword_18".to_string())
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn enhancement_icons_have_one_owner_and_flatten_for_serving() {
        let root = std::env::temp_dir().join(format!("ddo-enhancement-icon-owner-{}", std::process::id()));
        let output = std::env::temp_dir().join(format!("ddo-enhancement-icon-flat-{}", std::process::id()));
        std::fs::create_dir_all(root.join("icons/enhancements")).unwrap();
        std::fs::create_dir_all(root.join("characters/enhancements/first")).unwrap();
        std::fs::create_dir_all(root.join("characters/enhancements/second")).unwrap();
        std::fs::write(root.join("icons/enhancements/First.png"), b"first").unwrap();
        std::fs::write(root.join("icons/enhancements/Shared.png"), b"shared").unwrap();
        std::fs::write(
            root.join("characters/enhancements/first/tree.toml"),
            "name = 'First'\nicon = 'First'\n[[enhancement]]\nicon = 'Shared'\n",
        )
        .unwrap();
        std::fs::write(
            root.join("characters/enhancements/second/tree.toml"),
            "name = 'Second'\n[[enhancement]]\nicon = 'Shared'\n",
        )
        .unwrap();
        let misplaced =
            super::IconIndex::from_data(&root).err().expect("single-tree artwork must be local").to_string();
        assert!(misplaced.contains("icons/enhancements/First.png"), "{misplaced}");
        assert!(misplaced.contains("characters/enhancements/first"), "{misplaced}");
        let layout = super::place_enhancement_icons_beside_trees(&root).unwrap();
        assert_eq!(layout.moved_to_trees, 1);
        assert_eq!(layout.shared_icons, 1);
        let index = super::IconIndex::from_data(&root).unwrap();
        index.validate_file_icon("enhancements", Some("first"), "First.png").unwrap();
        let error = index.validate_file_icon("enhancements", Some("second"), "First.png").unwrap_err();
        assert!(error.to_string().contains("first") && error.to_string().contains("second"));
        super::copy_data_icons(&root, &output).unwrap();
        assert_eq!(std::fs::read(output.join("enhancements/First.png")).unwrap(), b"first");
        assert_eq!(std::fs::read(output.join("enhancements/first/First.png")).unwrap(), b"first");
        assert_eq!(std::fs::read(output.join("enhancements/Shared.png")).unwrap(), b"shared");
        assert_eq!(super::copy_data_icons(&root, &output).unwrap().skipped_duplicate_count, 2);
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(output).unwrap();
    }

    #[test]
    fn duplicate_enhancement_icon_location_is_rejected() {
        let root = std::env::temp_dir().join(format!("ddo-enhancement-icon-duplicate-{}", std::process::id()));
        std::fs::create_dir_all(root.join("icons/enhancements")).unwrap();
        std::fs::create_dir_all(root.join("characters/enhancements/first/icons")).unwrap();
        std::fs::write(root.join("icons/enhancements/Shared.png"), b"shared").unwrap();
        std::fs::write(root.join("characters/enhancements/first/tree.toml"), "icon = 'Shared'\n").unwrap();
        std::fs::write(root.join("characters/enhancements/first/icons/Shared.png"), b"shared").unwrap();
        let error = super::IconIndex::from_data(&root).err().unwrap();
        assert!(error.to_string().contains("first") && error.to_string().contains("icons/enhancements"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
