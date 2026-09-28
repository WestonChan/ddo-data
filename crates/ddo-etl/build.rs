use std::path::Path;

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/wiki");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .map(|entries| entries.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    files.retain(|p| p.extension().is_some_and(|x| x == "toml"));
    files.sort();
    let entries: String = files
        .iter()
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            format!("    ({name:?}, include_str!({:?})),\n", p.display().to_string())
        })
        .collect();
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("wiki_files.rs");
    std::fs::write(out, format!("&[\n{entries}]\n")).unwrap();
}
