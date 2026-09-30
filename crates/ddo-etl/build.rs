use std::path::Path;

fn main() {
    embed_toml_files("data/wiki", "wiki_files.rs");
    embed_toml_files("data/corrections", "correction_files.rs");
}

fn embed_toml_files(data_dir: &str, generated_file_name: &str) {
    let toml_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(data_dir);
    println!("cargo:rerun-if-changed={}", toml_dir.display());
    let mut toml_paths: Vec<_> = std::fs::read_dir(&toml_dir)
        .map(|entries| entries.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    toml_paths.retain(|p| p.extension().is_some_and(|x| x == "toml"));
    toml_paths.sort();
    let embedded_file_entries: String = toml_paths
        .iter()
        .map(|path| {
            let file_name = path.file_name().unwrap().to_string_lossy();
            format!("    ({file_name:?}, include_str!({:?})),\n", path.display().to_string())
        })
        .collect();
    let generated_path = Path::new(&std::env::var("OUT_DIR").unwrap()).join(generated_file_name);
    std::fs::write(generated_path, format!("&[\n{embedded_file_entries}]\n")).unwrap();
}
