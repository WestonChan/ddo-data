use rusqlite::Connection;
use std::path::Path;
use xtask::slug_checks::redirect_check_violations;

#[path = "../../crates/ddo-api/tests/common/mod.rs"]
mod common;

#[path = "../../crates/ddo-api/tests/common/overlapping_owners.rs"]
mod overlapping_owners;

fn build(root: &Path) -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(root, &mut db).unwrap();
    db
}

fn exported_fixture(root: &Path) {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/ddo-etl/tests/fixtures");
    common::export_fixture_data(&fixtures, root);
}

#[test]
fn owned_stance_changes_need_no_redirect() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("data");
    exported_fixture(&root);
    let deployed = build(&root);
    deployed.execute_batch("DROP VIEW served_redirects; DROP TABLE redirects; DROP TABLE removed_slugs;").unwrap();
    let path = walkdir::WalkDir::new(root.join("characters"))
        .into_iter()
        .map(Result::unwrap)
        .find(|entry| {
            entry.file_type().is_file()
                && entry.path().extension().is_some_and(|extension| extension == "toml")
                && std::fs::read_to_string(entry.path()).unwrap().contains("[[enhancement.stance]]")
        })
        .unwrap()
        .into_path();
    let mut document: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let enhancement = document["enhancement"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|enhancement| enhancement.get("stance").is_some())
        .unwrap();
    enhancement["stance"][0]["name"] = "Renamed owned stance".into();
    std::fs::write(path, toml::to_string_pretty(&document).unwrap()).unwrap();
    let current = build(&root);
    let owned_slugs = |db: &Connection| {
        db.prepare("SELECT slug FROM stances WHERE owner_kind <> 'standalone'")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<std::collections::BTreeSet<_>>>()
            .unwrap()
    };
    assert_eq!(owned_slugs(&deployed).difference(&owned_slugs(&current)).count(), 1);
    assert!(redirect_check_violations(&current, Some(&deployed), Some("2026-10-31")).unwrap().is_empty());
}

#[test]
fn schema_37_child_exemptions_follow_foreign_keys_not_slug_prefixes() {
    for true_owner in ["foo", "foo-bar"] {
        for declared_owner in ["foo", "foo-bar"] {
            for disposition in ["redirect", "removed"] {
                let folder = tempfile::tempdir().unwrap();
                let root = folder.path().join("data");
                exported_fixture(&root);
                let tree_path = overlapping_owners::write_overlapping_owners(&root, true_owner);
                let deployed = build(&root);
                deployed
                    .execute_batch("DROP VIEW served_redirects; DROP TABLE redirects; DROP TABLE removed_slugs;")
                    .unwrap();
                for owner in ["foo", "foo-bar"] {
                    let directory = root.join(format!("packs/shared/crafting/{owner}"));
                    let path = directory.join("crafting.toml");
                    let mut system: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
                    system.as_table_mut().unwrap().remove("ingredient");
                    if owner == declared_owner {
                        if disposition == "removed" {
                            std::fs::remove_dir_all(directory).unwrap();
                            continue;
                        }
                        system["name"] = format!("renamed-{owner}").into();
                        let renamed = directory.with_file_name(format!("renamed-{owner}"));
                        std::fs::rename(directory, &renamed).unwrap();
                        std::fs::write(renamed.join("crafting.toml"), toml::to_string_pretty(&system).unwrap())
                            .unwrap();
                    } else {
                        std::fs::write(path, toml::to_string_pretty(&system).unwrap()).unwrap();
                    }
                }
                let mut tree: toml::Value = toml::from_str(&std::fs::read_to_string(&tree_path).unwrap()).unwrap();
                let enhancements = tree["enhancement"].as_array_mut().unwrap();
                if disposition == "removed" {
                    enhancements.retain(|enhancement| enhancement["name"].as_str() != Some(declared_owner));
                }
                for enhancement in enhancements {
                    if matches!(enhancement["name"].as_str(), Some("foo" | "foo-bar")) {
                        enhancement.as_table_mut().unwrap().remove("selection");
                        if enhancement["name"].as_str() == Some(declared_owner) {
                            enhancement["name"] = format!("renamed-{declared_owner}").into();
                            enhancement["internal_name"] = format!("renamed-{declared_owner}").into();
                        }
                    }
                }
                std::fs::write(tree_path, toml::to_string_pretty(&tree).unwrap()).unwrap();
                let declarations = ["crafting-systems", "enhancements"].map(|kind| {
                    if disposition == "redirect" {
                        format!("[[redirect]]\nkind = {kind:?}\nfrom = {declared_owner:?}\nto = \"renamed-{declared_owner}\"\nsince = '2026-10-01'\n")
                    } else {
                        format!("[[removed]]\nkind = {kind:?}\nslug = {declared_owner:?}\nsince = '2026-10-01'\nreason = 'Removed owner'\n")
                    }
                }).join("\n");
                std::fs::write(root.join("redirects.toml"), declarations).unwrap();
                let current = build(&root);
                let violations = redirect_check_violations(&current, Some(&deployed), Some("2026-10-31")).unwrap();
                if true_owner == declared_owner {
                    assert!(violations.is_empty(), "{true_owner} {disposition}: {violations:?}");
                } else {
                    assert_eq!(violations.len(), 2, "{true_owner} {declared_owner} {disposition}: {violations:?}");
                    for kind in ["enhancement-selections", "crafting-ingredients"] {
                        assert!(
                            violations
                                .iter()
                                .any(|violation| violation.contains(kind) && violation.contains("foo-bar-baz")),
                            "{violations:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn vanished_slug_requires_a_redirect_or_removal_against_schema_37() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("data");
    exported_fixture(&root);
    let gear = root.join("packs/unsourced/gear");
    std::fs::create_dir_all(&gear).unwrap();
    let old_file = gear.join("vanished-fixture.toml");
    let contents = "name = 'Vanished fixture'\nslot = 'Feet'\nitem_category = 'Clothing'\nwiki_url = 'https://ddowiki.com/page/Item:Vanished_fixture'\nminimum_level = 1\n";
    std::fs::write(&old_file, contents).unwrap();
    let deployed = build(&root);
    deployed.execute_batch("DROP VIEW served_redirects; DROP TABLE redirects; DROP TABLE removed_slugs;").unwrap();
    std::fs::remove_file(old_file).unwrap();
    let current_file = gear.join("current-fixture.toml");
    std::fs::write(&current_file, contents.replace("Vanished fixture", "Current fixture")).unwrap();
    let current = build(&root);
    let violations = redirect_check_violations(&current, Some(&deployed), Some("2026-10-31")).unwrap();
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(violations[0].contains("items") && violations[0].contains("vanished-fixture"));
    let current_path = folder.path().join("current.db");
    let deployed_path = folder.path().join("deployed.db");
    current.execute("VACUUM INTO ?1", [current_path.to_str().unwrap()]).unwrap();
    deployed.execute("VACUUM INTO ?1", [deployed_path.to_str().unwrap()]).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args([
            "check-redirects",
            "--db",
            current_path.to_str().unwrap(),
            "--deployed",
            deployed_path.to_str().unwrap(),
            "--today",
            "2026-10-31",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("items") && error.contains("vanished-fixture"), "{error}");
    std::fs::write(
        root.join("redirects.toml"),
        "[[redirect]]\nkind = 'items'\nfrom = 'vanished-fixture'\nto = 'current-fixture'\nsince = '2026-10-01'\n",
    )
    .unwrap();
    let current = build(&root);
    assert!(redirect_check_violations(&current, Some(&deployed), Some("2026-10-31")).unwrap().is_empty());
    for today in ["2026-09-30", "2026-11-01"] {
        let violations = redirect_check_violations(&current, None, Some(today)).unwrap();
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("vanished-fixture"));
        assert!(violations[0].contains("prune-redirects"));
    }
    std::fs::remove_file(current_file).unwrap();
    std::fs::write(
        root.join("redirects.toml"),
        "[[removed]]\nkind = 'items'\nslug = 'vanished-fixture'\nsince = '2026-10-01'\nreason = 'Removed fixture'\n",
    )
    .unwrap();
    let current = build(&root);
    assert!(redirect_check_violations(&current, Some(&deployed), Some("2026-10-31")).unwrap().is_empty());
    for today in ["2026-09-30", "2026-11-01"] {
        let violations = redirect_check_violations(&current, None, Some(today)).unwrap();
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("vanished-fixture"));
        assert!(violations[0].contains("prune-redirects"));
    }
}
