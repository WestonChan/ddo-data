use rusqlite::Connection;
use std::path::Path;
use xtask::redirect_checks::redirect_check_violations;

#[path = "../../crates/ddo-api/tests/common/mod.rs"]
mod common;

fn build(root: &Path) -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(root, &mut db).unwrap();
    db
}

fn exported_fixture(root: &Path) {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/ddo-etl/tests/fixtures");
    common::copy_fixture_data(&fixtures, root);
}

#[test]
fn owned_stance_changes_need_no_redirect() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("data");
    exported_fixture(&root);
    let deployed = build(&root);
    deployed.execute_batch("DROP VIEW served_redirects; DROP TABLE redirects;").unwrap();
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
fn a_previously_served_effect_slug_can_become_a_permanent_alias() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("data");
    exported_fixture(&root);
    let previous_path = root.join("effects/previous-title.toml");
    std::fs::write(&previous_path, "name = 'Previous Title'\nverbose_name_template = 'Previous Title'\n").unwrap();
    let deployed = build(&root);
    std::fs::remove_file(previous_path).unwrap();
    let canonical_path = root.join("effects/canonical-title.toml");
    let canonical = "name = 'Canonical Title'\nverbose_name_template = 'Canonical Title'\n";
    for template in ["", "verbose_name_template = 'Previous Title'\n"] {
        std::fs::write(&canonical_path, format!("{canonical}\n[[alias]]\nname = 'Previous Title'\n{template}"))
            .unwrap();
        let current = build(&root);
        let resolved = ddo_api::entity_slugs::resolve_entity_slug(
            &current,
            ddo_model::enums::EntityKind::Effect,
            "previous-title",
        )
        .unwrap()
        .unwrap();
        assert!(resolved.is_redirect);
        assert_eq!(resolved.slug, "canonical-title");
        let redirects: i64 = current
            .query_row(
                "SELECT COUNT(*) FROM redirects WHERE kind = 'effects' AND from_slug = 'previous-title'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(redirects, 0);
        let violations = redirect_check_violations(&current, Some(&deployed), Some("2026-10-31")).unwrap();
        assert!(violations.is_empty(), "{violations:?}");
    }
    std::fs::write(canonical_path, canonical).unwrap();
    let current = build(&root);
    let violations = redirect_check_violations(&current, Some(&deployed), Some("2026-10-31")).unwrap();
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn deployed_addresses_follow_unique_kept_names() {
    for (previous_variants, current_variants, current_name, redirected, expected) in [
        (vec![None], vec![Some("New")], "Address fixture", false, 1),
        (vec![None], vec![Some("New")], "Address fixture", true, 0),
        (vec![None], vec![Some("New")], "Renamed fixture", false, 0),
        (vec![None], vec![], "Address fixture", false, 0),
        (vec![None, Some("Old")], vec![Some("New")], "Address fixture", false, 0),
        (vec![None], vec![Some("New"), Some("Other")], "Address fixture", false, 0),
    ] {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("data");
        exported_fixture(&root);
        let write_ingredients = |name: &str, variants: &[Option<&str>]| {
            let system = root.join("packs/shared/crafting/address-checks");
            std::fs::create_dir_all(system.join("ingredients")).unwrap();
            let mut recipes =
                String::from("name='Address checks'\npage='https://ddowiki.com/page/Address_checks'\ncitation=[]\n");
            for (position, variant) in variants.iter().enumerate() {
                recipes.push_str(&format!("[[recipe]]\noption='Recipe {position}'\ntier='any'\n[[recipe.ingredient]]\ningredient={name:?}\nquantity=1\n"));
                if let Some(variant) = variant {
                    recipes.push_str(&format!("variant={variant:?}\n"));
                }
            }
            std::fs::write(system.join("crafting.toml"), recipes).unwrap();
            variants
                .iter()
                .map(|variant| {
                    let slug = ddo_model::identity::slug(
                        &variant.map_or_else(|| name.to_string(), |variant| format!("{name} ({variant})")),
                    );
                    let path = system.join(format!("ingredients/{slug}.toml"));
                    let variant = variant.map_or(String::new(), |variant| format!("variant={variant:?}\n"));
                    std::fs::write(&path, format!("name={name:?}\n{variant}tier='any'\ncitation=[]\n")).unwrap();
                    path
                })
                .collect::<Vec<_>>()
        };
        let previous = write_ingredients("Address fixture", &previous_variants);
        let deployed = build(&root);
        for path in previous {
            std::fs::remove_file(path).unwrap();
        }
        write_ingredients(current_name, &current_variants);
        if redirected {
            std::fs::write(root.join("redirects.toml"), "[[redirect]]\nkind='crafting-ingredients'\nfrom='address-fixture'\nto='address-fixture-new'\nsince='2026-10-01'\n").unwrap();
        }
        let current = build(&root);
        let violations = redirect_check_violations(&current, Some(&deployed), Some("2026-10-31")).unwrap();
        assert_eq!(
            violations.len(),
            expected,
            "{previous_variants:?} -> {current_name} {current_variants:?}, redirected={redirected}: {violations:?}"
        );
        if expected != 0 {
            assert!(
                violations[0].contains("crafting-ingredients") && violations[0].contains("address-fixture"),
                "{violations:?}"
            );
        }
    }
}
