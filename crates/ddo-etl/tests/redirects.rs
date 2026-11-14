use ddo_model::enums::EntityKind;
use rusqlite::Connection;
use std::collections::BTreeSet;
use std::path::Path;

#[path = "../../ddo-api/tests/common/mod.rs"]
mod common;

#[path = "../../ddo-api/tests/common/overlapping_owners.rs"]
mod overlapping_owners;

fn redirect(kind: &str, from: &str, to: &str, since: &str) -> String {
    format!("[[redirect]]\nkind = {kind:?}\nfrom = {from:?}\nto = {to:?}\nsince = {since:?}\n\n")
}

#[test]
fn derived_redirects_follow_foreign_keys_with_overlapping_owner_slugs() {
    for true_owner in ["foo", "foo-bar"] {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("data");
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        common::copy_fixture_data(&fixtures, &root);
        overlapping_owners::write_overlapping_enhancements(&root, true_owner);
        for renamed_owner in ["foo", "foo-bar"] {
            std::fs::write(
                root.join("redirects.toml"),
                redirect("enhancements", "former-enhancement", renamed_owner, "2026-10-01"),
            )
            .unwrap();
            let mut built = Connection::open_in_memory().unwrap();
            ddo_etl::tree::build_from_data(&root, &mut built).unwrap();
            {
                let (kind, child, prefix) = ("enhancement-selections", "foo-bar-baz", "former-enhancement");
                let actual: Vec<String> = built
                    .prepare("SELECT from_slug FROM served_redirects WHERE kind = ?1 AND to_slug = ?2")
                    .unwrap()
                    .query_map([kind, child], |row| row.get(0))
                    .unwrap()
                    .collect::<rusqlite::Result<_>>()
                    .unwrap();
                let expected = if renamed_owner == true_owner {
                    vec![format!("{prefix}{}", &child[true_owner.len()..])]
                } else {
                    vec![]
                };
                assert_eq!(actual, expected, "{kind}: {true_owner} owns {child}, renamed {renamed_owner}");
            }
        }
    }
}

#[test]
fn numeric_redirects_fail_the_data_build() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("data");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let original = common::copy_fixture_data(&fixtures, &root);
    let target: String =
        original.query_row("SELECT slug FROM items ORDER BY slug LIMIT 1", [], |row| row.get(0)).unwrap();
    for slug in ["0", "12345"] {
        {
            let row = redirect("items", slug, &target, "2026-10-01");
            std::fs::write(root.join("redirects.toml"), row).unwrap();
            let result = ddo_etl::tree::build_from_data(&root, &mut Connection::open_in_memory().unwrap());
            let error = format!("{:#}", result.map(|_| ()).expect_err("numeric slug must fail the build"));
            for expected in ["redirect", "items", slug, "all digits"] {
                assert!(error.contains(expected), "{error}");
            }
        }
    }
    std::fs::write(root.join("redirects.toml"), redirect("items", "123-item", &target, "2026-10-01")).unwrap();
    ddo_etl::tree::build_from_data(&root, &mut Connection::open_in_memory().unwrap()).unwrap();
}

#[test]
fn redirect_dates_and_kinds_reject_malformed_values() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("data");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let original = common::copy_fixture_data(&fixtures, &root);
    let target: String =
        original.query_row("SELECT slug FROM items ORDER BY slug LIMIT 1", [], |row| row.get(0)).unwrap();
    for (kind, since) in
        [("items", "2026-02-30"), ("items", "2026-1-01"), ("items", "tomorrow"), ("set_bonuses", "2026-10-01")]
    {
        std::fs::write(root.join("redirects.toml"), redirect(kind, "former", &target, since)).unwrap();
        let error = ddo_etl::tree::build_from_data(&root, &mut Connection::open_in_memory().unwrap()).unwrap_err();
        assert!(format!("{error:#}").contains("redirects.toml"));
    }
}

#[test]
fn expiry_reports_redirects_at_exact_boundaries() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("data");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let original = common::copy_fixture_data(&fixtures, &root);
    let target: String =
        original.query_row("SELECT slug FROM items ORDER BY slug LIMIT 1", [], |row| row.get(0)).unwrap();
    std::fs::write(root.join("redirects.toml"), redirect("items", "former", &target, "2026-10-01")).unwrap();
    let mut built = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(&root, &mut built).unwrap();
    for (today, reason) in [
        ("2026-11-01", None),
        ("2027-10-01", None),
        ("2027-10-02", Some("older than one year")),
        ("2026-09-30", Some("future")),
    ] {
        let violations = ddo_etl::tree::redirects::redirect_expiry_violations(&built, Some(today)).unwrap();
        if let Some(reason) = reason {
            assert_eq!(violations.len(), 1);
            for (violation, slug) in violations.iter().zip(["former"]) {
                assert_eq!(violation.contains("prune-redirects"), reason != "future", "{violation}");
                assert!(
                    violation.contains(slug)
                        && violation.contains(reason)
                        && violation.contains(if reason == "future" {
                            "correct the date"
                        } else {
                            "prune-redirects --data data"
                        }),
                    "{violation}"
                );
            }
        } else {
            assert!(violations.is_empty(), "{violations:?}");
        }
    }
}

#[test]
fn authored_child_redirects_cannot_duplicate_derived_redirects() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("data");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    common::copy_fixture_data(&fixtures, &root);
    overlapping_owners::write_overlapping_enhancements(&root, "foo");
    let mut original = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(&root, &mut original).unwrap();
    let (owner, child): (String, String) = original.query_row("SELECT s.slug, i.slug FROM enhancement_selections i JOIN enhancements s ON s.id = i.enhancement_id ORDER BY i.slug LIMIT 1", [], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
    let old_child = format!("former-system{}", &child[owner.len()..]);
    std::fs::write(
        root.join("redirects.toml"),
        redirect("enhancements", "former-system", &owner, "2026-10-01")
            + &redirect("enhancement-selections", &old_child, &child, "2026-10-01"),
    )
    .unwrap();
    let error = ddo_etl::tree::build_from_data(&root, &mut Connection::open_in_memory().unwrap()).unwrap_err();
    let message = format!("{error:#}");
    assert!(
        message.contains("duplicate served redirect") && message.contains(&old_child) && message.contains(&child),
        "{message}"
    );
}

#[test]
fn invalid_redirects_name_both_sides_and_collect_independent_violations() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("data");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let original = common::copy_fixture_data(&fixtures, &root);
    let target: String =
        original.query_row("SELECT slug FROM items ORDER BY slug LIMIT 1", [], |row| row.get(0)).unwrap();
    let current = redirect("items", &target, "absent-target", "2026-10-01");
    let cases = [
        (current, vec![target.as_str(), "absent-target", ".toml"]),
        (
            redirect("items", "former", "intermediate", "2026-10-01")
                + &redirect("items", "intermediate", &target, "2026-10-01"),
            vec!["former", "intermediate", target.as_str(), "retarget"],
        ),
        (
            redirect("items", "former", &target, "2026-10-01")
                + &redirect("items", "former", "other-target", "2026-10-01"),
            vec!["duplicate", "former", target.as_str(), "other-target"],
        ),
        (
            redirect("items", "first", "second", "2026-10-01") + &redirect("items", "second", "first", "2026-10-01"),
            vec!["first", "second", "target is not current"],
        ),
        (redirect("items", &target, &target, "2026-10-01"), vec![target.as_str(), "CHECK"]),
        (
            redirect("items", "first", "absent-first", "2026-10-01")
                + &redirect("items", "second", "absent-second", "2026-10-01"),
            vec!["first", "absent-first", "second", "absent-second"],
        ),
    ];
    for (contents, expected) in cases {
        std::fs::write(root.join("redirects.toml"), contents).unwrap();
        let error = ddo_etl::tree::build_from_data(&root, &mut Connection::open_in_memory().unwrap()).unwrap_err();
        let message = format!("{error:#}");
        for text in expected {
            assert!(message.contains(text), "missing {text:?}: {message}");
        }
    }
}

#[test]
fn pruning_keeps_one_year_and_future_rows_and_uses_export_bytes() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("data");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let original = common::copy_fixture_data(&fixtures, &root);
    let target: String =
        original.query_row("SELECT slug FROM items ORDER BY slug LIMIT 1", [], |row| row.get(0)).unwrap();
    let contents = redirect("items", "one-year", &target, "2025-11-01")
        + &redirect("items", "older-than-one-year", &target, "2025-10-31")
        + &redirect("items", "future", &target, "2026-11-02");
    std::fs::write(root.join("redirects.toml"), contents).unwrap();
    let before = ddo_etl::tree::data_file_bytes(&root).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ddo-etl"))
        .args(["prune-redirects", "--data"])
        .arg(&root)
        .args(["--today", "2026-11-01"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("pruned 1"));
    let after = ddo_etl::tree::data_file_bytes(&root).unwrap();
    for (path, bytes) in &before {
        if path != Path::new("redirects.toml") {
            assert_eq!(after[path], *bytes, "{} changed", path.display());
        }
    }
    let contents = std::fs::read_to_string(root.join("redirects.toml")).unwrap();
    assert!(contents.contains("one-year") && contents.contains("future"));
    assert!(!contents.contains("older-than-one-year"));
    let mut built = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(&root, &mut built).unwrap();
    let exported = folder.path().join("exported");
    ddo_etl::tree::export_database(&built, &exported).unwrap();
    assert_eq!(std::fs::read_to_string(exported.join("redirects.toml")).unwrap(), contents);
    assert!(ddo_etl::tree::redirects::prune_redirects(&root, Some("2026-02-30")).is_err());
}

#[test]
fn redirect_file_round_trips_authored_rows_and_derives_enhancement_selections() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("data");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    common::copy_fixture_data(&fixtures, &root);
    overlapping_owners::write_overlapping_enhancements(&root, "foo");
    let mut original = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(&root, &mut original).unwrap();
    let target: String = original.query_row("SELECT slug FROM enhancements WHERE id IN (SELECT enhancement_id FROM enhancement_selections) ORDER BY slug LIMIT 1", [], |row| row.get(0)).unwrap();
    let contents = format!(
        "[[redirect]]\nkind = \"enhancements\"\nfrom = \"former-system\"\nto = {target:?}\nsince = \"2026-10-01\"\n"
    );
    std::fs::write(root.join("redirects.toml"), &contents).unwrap();
    let mut built = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(&root, &mut built).unwrap();
    let expected: Vec<String> = original.prepare("SELECT i.slug FROM enhancement_selections i JOIN enhancements s ON s.id = i.enhancement_id WHERE s.slug = ?1 ORDER BY i.slug").unwrap().query_map([&target], |row| row.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    let actual: Vec<(String, String, String)> = built.prepare("SELECT from_slug, to_slug, since FROM served_redirects WHERE kind = 'enhancement-selections' ORDER BY to_slug").unwrap().query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    assert!(!expected.is_empty());
    assert_eq!(
        actual,
        expected
            .into_iter()
            .map(|slug| (format!("former-system{}", &slug[target.len()..]), slug, "2026-10-01".to_string()))
            .collect::<Vec<_>>()
    );
    let exported = folder.path().join("exported");
    ddo_etl::tree::export_database(&built, &exported).unwrap();
    assert_eq!(std::fs::read_to_string(exported.join("redirects.toml")).unwrap(), contents);
}

#[test]
fn fixture_identity_kinds_match_the_entity_kind_declaration() {
    let folder = tempfile::tempdir().unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let db = common::copy_fixture_data(&fixtures, &folder.path().join("data"));
    let actual: BTreeSet<String> = db
        .prepare("SELECT DISTINCT kind FROM entity_identity")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    let expected = EntityKind::ALL.iter().map(|kind| kind.table_name().to_string()).collect();
    assert_eq!(actual, expected);
}
