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
