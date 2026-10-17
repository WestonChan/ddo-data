use crate::icons::IconIndex;
use anyhow::{Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

type IconKey = (String, String);

#[derive(Debug, Default)]
pub(super) struct IconMerge {
    pub(super) added: usize,
    pub(super) replaced: usize,
    pub(super) removed: usize,
    pub(super) conflicts: Vec<(IconKey, &'static str)>,
    pub(super) declined: Vec<(IconKey, &'static str)>,
    pub(super) paths: BTreeMap<IconKey, PathBuf>,
}

impl IconMerge {
    pub(super) fn resolve_paths(&mut self, staging: &Path) -> Result<()> {
        self.paths = icon_files(staging)?
            .into_iter()
            .map(|(key, path)| Ok((key, path.strip_prefix(staging)?.to_path_buf())))
            .collect::<Result<_>>()?;
        Ok(())
    }
}

fn icon_files(root: &Path) -> Result<BTreeMap<IconKey, PathBuf>> {
    IconIndex::from_data(root)?
        .icon_files()
        .into_iter()
        .map(|(kind, path)| {
            let name =
                path.file_name().and_then(|name| name.to_str()).context("icon has no UTF-8 filename")?.to_string();
            Ok(((kind, name), path))
        })
        .collect()
}

fn bytes(path: Option<&PathBuf>) -> Result<Option<Vec<u8>>> {
    path.map(std::fs::read).transpose().map_err(Into::into)
}

pub(super) fn merge_icon_files(
    previous_tree: &Path,
    new_tree: &Path,
    ours_data: &Path,
    staging: &Path,
    referenced: &BTreeSet<IconKey>,
) -> Result<IconMerge> {
    let previous = icon_files(previous_tree)?;
    let new = icon_files(new_tree)?;
    let ours = icon_files(ours_data)?;
    let keys: BTreeSet<_> = previous.keys().chain(new.keys()).chain(ours.keys()).cloned().collect();
    let mut report = IconMerge::default();
    for key in keys {
        let old_bytes = bytes(previous.get(&key))?;
        let new_bytes = bytes(new.get(&key))?;
        let ours_bytes = bytes(ours.get(&key))?;
        let mut chosen = ours.get(&key);
        if old_bytes != new_bytes {
            match (&old_bytes, &new_bytes, &ours_bytes) {
                (_, Some(_), _) if ours_bytes == new_bytes => {}
                (_, Some(_), _) if ours_bytes == old_bytes => {
                    chosen = new.get(&key);
                    if old_bytes.is_some() {
                        report.replaced += 1;
                    } else {
                        report.added += 1;
                    }
                }
                (_, Some(_), _) => report.conflicts.push((
                    key.clone(),
                    if ours_bytes.is_none() {
                        "changed upstream, removed in ours; kept absent"
                    } else if old_bytes.is_none() {
                        "added on both sides with different bytes; kept our bytes"
                    } else {
                        "both sides changed the bytes differently; kept our bytes"
                    },
                )),
                (Some(_), None, Some(_)) if ours_bytes != old_bytes || referenced.contains(&key) => {
                    report.declined.push((
                        key.clone(),
                        if ours_bytes != old_bytes {
                            "removed upstream, edited by us; kept our bytes"
                        } else {
                            "removed upstream, still referenced; kept our bytes"
                        },
                    ));
                }
                (Some(_), None, Some(_)) => {
                    chosen = None;
                    report.removed += 1;
                }
                _ => {}
            }
        }
        if let Some(source) = chosen {
            let target = staging.join("icons").join(&key.0).join(&key.1);
            std::fs::create_dir_all(target.parent().context("icon destination has no parent")?)?;
            std::fs::copy(source, target)?;
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn removed_icon_still_referenced_is_declined() {
        let scratch = tempfile::tempdir().unwrap();
        let previous = scratch.path().join("previous");
        let new = scratch.path().join("new");
        let ours = scratch.path().join("ours");
        let staging = scratch.path().join("staging");
        for root in [&previous, &new, &ours] {
            std::fs::create_dir_all(root).unwrap();
        }
        let png = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/import_ddobuilder/ba7e18e/DataFiles/FeatImages/Alertness.png");
        for root in [&previous, &ours] {
            std::fs::create_dir_all(root.join("icons/feats")).unwrap();
            std::fs::copy(&png, root.join("icons/feats/Alertness.png")).unwrap();
        }
        let referenced = BTreeSet::from([("feats".to_string(), "Alertness.png".to_string())]);
        let report = merge_icon_files(&previous, &new, &ours, &staging, &referenced).unwrap();
        assert_eq!(report.declined.len(), 1);
        assert!(referenced.contains(&report.declined[0].0));
        assert_eq!(report.declined[0].1, "removed upstream, still referenced; kept our bytes");
        assert_eq!(std::fs::read(staging.join("icons/feats/Alertness.png")).unwrap(), std::fs::read(png).unwrap());
    }
}
