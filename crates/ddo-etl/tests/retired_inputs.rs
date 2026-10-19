use std::path::{Path, PathBuf};

const RETIRED_PATHS: &[&str] = &[
    "crates/ddo-etl/build.rs",
    "crates/ddo-etl/data/wiki",
    "crates/ddo-etl/data/corrections",
    "crates/ddo-etl/tests/fixtures/wiki",
    "crates/ddo-etl/tests/fixtures/corrections",
    "crates/ddo-etl/tests/fixtures/data-corrections",
    "crates/ddo-etl/tests/fixtures/import_ddobuilder/wiki",
    "crates/ddo-etl/tests/fixtures/import_ddobuilder/corrections",
];

const RETIRED_TEXT: &[&str] =
    &["data/wiki", "data/corrections", "wiki-check", "wiki-batch", "wiki_check", "wiki_batch"];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn scanned_files(root: &Path) -> Vec<PathBuf> {
    ["crates", "xtask", ".github"]
        .iter()
        .flat_map(|top| walkdir::WalkDir::new(root.join(top)))
        .map(Result::unwrap)
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| !entry.path().components().any(|part| part.as_os_str() == "target"))
        .filter(|entry| entry.path().file_name().is_some_and(|name| name != "retired_inputs.rs"))
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| ["rs", "md", "toml", "yml"].iter().any(|kept| extension == *kept))
        })
        .map(walkdir::DirEntry::into_path)
        .collect()
}

#[test]
fn retired_inputs_and_tools_are_gone_from_the_workspace() {
    let root = workspace_root();
    let present: Vec<&str> = RETIRED_PATHS.iter().copied().filter(|path| root.join(path).exists()).collect();
    assert!(present.is_empty(), "retired paths still exist: {present:?}");
    let mut mentions = Vec::new();
    for path in scanned_files(&root) {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        for retired in RETIRED_TEXT {
            if text.contains(retired) {
                mentions.push(format!("{} mentions {retired}", path.strip_prefix(&root).unwrap().display()));
            }
        }
    }
    assert!(mentions.is_empty(), "{mentions:#?}");
}
