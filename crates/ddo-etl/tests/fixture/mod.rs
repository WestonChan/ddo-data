pub(crate) fn database(edit: impl FnOnce(&std::path::Path)) -> rusqlite::Connection {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data");
    let scratch = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(&source).into_iter().map(Result::unwrap) {
        if entry.file_type().is_file() {
            let destination = scratch.path().join(entry.path().strip_prefix(&source).unwrap());
            std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
            std::fs::copy(entry.path(), destination).unwrap();
        }
    }
    edit(scratch.path());
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(scratch.path(), &mut db).unwrap();
    db
}
