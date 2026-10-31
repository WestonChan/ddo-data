#![allow(dead_code)]

use rusqlite::Connection;
use std::path::Path;

pub(crate) fn copy_files(source: &Path, destination: &Path) {
    std::fs::create_dir_all(destination).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_files(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

pub(crate) fn copy_fixture_data(fixtures: &Path, root: &Path) -> Connection {
    copy_files(&fixtures.join("data"), root);
    let mut db = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(root, &mut db).unwrap();
    db
}

pub(crate) fn add_note_fixtures(root: &Path) {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"));
    copy_files(&workspace.join("../ddo-etl/tests/fixtures/verification-notes/data"), root);
    let captured = workspace.join("tests/fixtures/notes");
    copy_files(&captured.join("data"), root);
    for name in ["clickies", "augment-slot-types"] {
        let path = root.join(format!("effects/{name}.toml"));
        let original = std::fs::read_to_string(&path).unwrap();
        let additional = std::fs::read_to_string(captured.join(format!("append/effects/{name}.toml"))).unwrap();
        std::fs::write(path, format!("{original}\n{additional}")).unwrap();
    }
}
