use ddo_etl::tree::schemas::{kind_templates, Placement, FILE_KINDS};
use std::path::{Path, PathBuf};

const TEMPLATE_NOTE: &str = "Template: the page read and what it confirms.";

fn copy_tree(source: &Path, destination: &Path) {
    for entry in walkdir::WalkDir::new(source).into_iter().map(Result::unwrap) {
        if entry.file_type().is_file() {
            let target = destination.join(entry.path().strip_prefix(source).unwrap());
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn every_template_builds_under_its_placement_and_exports_byte_equal() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let scratch = tempfile::tempdir().unwrap();
    let data = scratch.path().join("data");
    copy_tree(&workspace.join("data"), &data);
    let templates = kind_templates().unwrap();
    let mut entity_files: Vec<(&str, PathBuf, String)> = Vec::new();
    let mut row_templates: Vec<(&str, PathBuf, toml::Value)> = Vec::new();
    for (kind, placement) in FILE_KINDS {
        let template = templates.iter().find(|template| template.kind == *kind);
        match (placement, template) {
            (Placement::Dataset, None) => {}
            (Placement::Dataset, Some(_)) => panic!("{kind} is a dataset file and has no template"),
            (_, None) => panic!("{kind} has no template"),
            (Placement::Entity(pattern), Some(template)) => {
                let path = PathBuf::from(pattern.replace("{slug}", &template.slug));
                std::fs::create_dir_all(data.join(&path).parent().unwrap()).unwrap();
                std::fs::write(data.join(&path), &template.text).unwrap();
                entity_files.push((kind, path, template.text.clone()));
            }
            (Placement::Rows(path), Some(template)) => {
                row_templates.push((kind, PathBuf::from(path), toml::from_str(&template.text).unwrap()));
            }
        }
    }
    for (kind, path, template) in &row_templates {
        let mut file: toml::Value = match std::fs::read_to_string(data.join(path)) {
            Ok(text) => toml::from_str(&text).unwrap(),
            Err(_) => toml::from_str("row = []").unwrap(),
        };
        let rows = file["row"].as_array_mut().unwrap_or_else(|| panic!("{kind}: {} has no rows", path.display()));
        rows.extend(template["row"].as_array().unwrap().iter().cloned());
        std::fs::create_dir_all(data.join(path).parent().unwrap()).unwrap();
        std::fs::write(data.join(path), toml::to_string_pretty(&file).unwrap()).unwrap();
    }
    ddo_etl::icons::place_enhancement_icons_beside_trees(&data).unwrap();
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(&data, &mut db).unwrap_or_else(|error| panic!("{error:#}"));
    let landed: i64 =
        db.query_row("SELECT COUNT(*) FROM citations WHERE note = ?1", [TEMPLATE_NOTE], |row| row.get(0)).unwrap();
    assert_eq!(landed as usize, templates.len(), "every template's citation lands");
    let exported = scratch.path().join("exported");
    ddo_etl::tree::export_database(&db, &exported).unwrap();
    let mut failures = Vec::new();
    for (kind, path, template) in &row_templates {
        let rebuilt: toml::Value = toml::from_str(&std::fs::read_to_string(exported.join(path)).unwrap()).unwrap();
        for row in template["row"].as_array().unwrap() {
            let name = row.get("name").or_else(|| row.get("label")).and_then(toml::Value::as_str).unwrap();
            let present = rebuilt["row"].as_array().unwrap().iter().any(|candidate| {
                candidate.get("name").or_else(|| candidate.get("label")).and_then(toml::Value::as_str) == Some(name)
            });
            if !present {
                failures.push(format!("{kind}: row {name:?} is not in the exported {}", path.display()));
            }
        }
    }
    for (kind, path, text) in entity_files {
        match std::fs::read_to_string(exported.join(&path)) {
            Err(error) => failures.push(format!("{kind}: {} was not exported: {error}", path.display())),
            Ok(rebuilt) if rebuilt != text => {
                failures.push(format!("{kind}: {} changed through the build and export:\n{rebuilt}", path.display()))
            }
            Ok(_) => {}
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
