use rusqlite::Connection;
use std::path::Path;

pub(crate) fn export_fixture_data(fixtures: &Path, root: &Path) -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    let mut corrections = ddo_etl::corrections::Corrections::from_dir(&fixtures.join("corrections")).unwrap();
    corrections
        .entries
        .extend(ddo_etl::corrections::Corrections::from_dir(&fixtures.join("data-corrections")).unwrap().entries);
    ddo_etl::build::build_database(
        &fixtures.join("DataFiles"),
        &ddo_etl::wiki::WikiOverrides::from_dir(&fixtures.join("wiki")).unwrap(),
        &corrections,
        &mut db,
        &ddo_model::DatasetVersion {
            upstream_sha: "fixture".to_string(),
            built_at: "2026-10-17T00:00:00Z".to_string(),
        },
    )
    .unwrap();
    export_database_fixture(&db, fixtures, root);
    db
}

pub(crate) fn export_database_fixture(db: &Connection, fixtures: &Path, root: &Path) {
    ddo_etl::tree::export_database(db, root).unwrap();
    ddo_etl::icons::export_icons(&fixtures.join("DataFiles"), &root.join("icons")).unwrap();
    for missing in ddo_etl::icons::complete_referenced_icons(root).unwrap().missing_icons {
        fill_icon(root, &root.join("icons").join(missing));
    }
    fill_fixture_icons(root, root);
    ddo_etl::icons::place_enhancement_icons_beside_trees(root).unwrap();
}

fn fill_icon(root: &Path, path: &Path) {
    if !path.is_file() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::copy(root.join("icons/feats/PowerAttack.png"), path).unwrap();
    }
}

fn fill_fixture_icons(folder: &Path, root: &Path) {
    for entry in std::fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            fill_fixture_icons(&path, root);
            continue;
        }
        if path.extension().is_none_or(|extension| extension != "toml") {
            continue;
        }
        let Some((_, kind)) = ddo_etl::icons::icon_kind_for_data_path(path.strip_prefix(root).unwrap()) else {
            continue;
        };
        for line in std::fs::read_to_string(&path).unwrap().lines() {
            let Some((field, quoted)) = line.trim().split_once(" = ") else { continue };
            let image_kind = match field {
                "icon" => kind,
                "background" => "ui",
                "small_icon" | "large_icon" => "classes",
                _ => continue,
            };
            let Some(icon) = quoted.strip_prefix('"').and_then(|text| text.strip_suffix('"')) else { continue };
            fill_icon(
                root,
                &root.join("icons").join(if image_kind == "*" { "" } else { image_kind }).join(format!("{icon}.png")),
            );
        }
    }
}
