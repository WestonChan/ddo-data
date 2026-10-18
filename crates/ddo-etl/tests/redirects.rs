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

fn removed(kind: &str, slug: &str, since: &str) -> String {
    format!("[[removed]]\nkind = {kind:?}\nslug = {slug:?}\nsince = {since:?}\nreason = \"No longer exists\"\n\n")
}

#[test]
fn derived_redirects_follow_foreign_keys_with_overlapping_owner_slugs() {
    for true_owner in ["foo", "foo-bar"] {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("data");
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        common::export_fixture_data(&fixtures, &root);
        overlapping_owners::write_overlapping_owners(&root, true_owner);
        for renamed_owner in ["foo", "foo-bar"] {
            std::fs::write(
                root.join("redirects.toml"),
                redirect("crafting-systems", "former-system", renamed_owner, "2026-10-01")
                    + &redirect("enhancements", "former-enhancement", renamed_owner, "2026-10-01"),
            )
            .unwrap();
            let mut built = Connection::open_in_memory().unwrap();
            ddo_etl::tree::build_from_data(&root, &mut built).unwrap();
            for (kind, child, prefix) in [
                ("crafting-ingredients", "foo-bar-baz-heroic", "former-system"),
                ("enhancement-selections", "foo-bar-baz", "former-enhancement"),
            ] {
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
fn numeric_redirects_and_removals_fail_the_data_build() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("data");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let original = common::export_fixture_data(&fixtures, &root);
    let target: String =
        original.query_row("SELECT slug FROM items ORDER BY slug LIMIT 1", [], |row| row.get(0)).unwrap();
    for slug in ["0", "12345"] {
        for (row, row_kind) in [
            (redirect("items", slug, &target, "2026-10-01"), "redirect"),
            (removed("items", slug, "2026-10-01"), "removed"),
        ] {
            std::fs::write(root.join("redirects.toml"), row).unwrap();
            let result = ddo_etl::tree::build_from_data(&root, &mut Connection::open_in_memory().unwrap());
            let error = format!("{:#}", result.map(|_| ()).expect_err("numeric slug must fail the build"));
            for expected in [row_kind, "items", slug, "all digits"] {
                assert!(error.contains(expected), "{error}");
            }
        }
    }
    std::fs::write(
        root.join("redirects.toml"),
        redirect("items", "123-item", &target, "2026-10-01") + &removed("items", "123-gone", "2026-10-01"),
    )
    .unwrap();
    ddo_etl::tree::build_from_data(&root, &mut Connection::open_in_memory().unwrap()).unwrap();
}

#[test]
fn redirect_dates_and_kinds_reject_malformed_values() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("data");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let original = common::export_fixture_data(&fixtures, &root);
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
fn expiry_reports_redirects_and_removals_at_exact_boundaries() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("data");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let original = common::export_fixture_data(&fixtures, &root);
    let target: String =
        original.query_row("SELECT slug FROM items ORDER BY slug LIMIT 1", [], |row| row.get(0)).unwrap();
    std::fs::write(
        root.join("redirects.toml"),
        redirect("items", "former", &target, "2026-10-01") + &removed("items", "gone", "2026-10-01"),
    )
    .unwrap();
    let mut built = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(&root, &mut built).unwrap();
    for (today, reason) in
        [("2026-10-31", None), ("2026-11-01", Some("older than 30 days")), ("2026-09-30", Some("future"))]
    {
        let violations = ddo_etl::tree::redirects::redirect_expiry_violations(&built, Some(today)).unwrap();
        if let Some(reason) = reason {
            assert_eq!(violations.len(), 2);
            for (violation, slug) in violations.iter().zip(["former", "gone"]) {
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
    let original = common::export_fixture_data(&fixtures, &root);
    let (owner, child): (String, String) = original.query_row("SELECT s.slug, i.slug FROM crafting_ingredients i JOIN crafting_systems s ON s.id = i.system_id ORDER BY i.slug LIMIT 1", [], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
    let old_child = format!("former-system{}", &child[owner.len()..]);
    std::fs::write(
        root.join("redirects.toml"),
        redirect("crafting-systems", "former-system", &owner, "2026-10-01")
            + &redirect("crafting-ingredients", &old_child, &child, "2026-10-01"),
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
    let original = common::export_fixture_data(&fixtures, &root);
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
        (removed("items", &target, "2026-10-01"), vec![target.as_str(), "current", ".toml"]),
        (
            redirect("items", "former", &target, "2026-10-01") + &removed("items", "former", "2026-10-01"),
            vec!["former", target.as_str(), "both"],
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
fn pruning_keeps_day_thirty_and_future_rows_and_uses_export_bytes() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("data");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let original = common::export_fixture_data(&fixtures, &root);
    let target: String =
        original.query_row("SELECT slug FROM items ORDER BY slug LIMIT 1", [], |row| row.get(0)).unwrap();
    let contents = redirect("items", "day-thirty", &target, "2026-10-02")
        + &redirect("items", "day-thirty-one", &target, "2026-10-01")
        + &redirect("items", "future", &target, "2026-11-02")
        + &removed("items", "day-thirty", "2026-10-02")
        + &removed("items", "day-thirty-one", "2026-10-01");
    let contents = contents.replace("slug = \"day-thirty", "slug = \"removed-day-thirty");
    std::fs::write(root.join("redirects.toml"), contents).unwrap();
    let before = ddo_etl::tree::data_file_bytes(&root).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ddo-etl"))
        .args(["prune-redirects", "--data"])
        .arg(&root)
        .args(["--today", "2026-11-01"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("pruned 2"));
    let after = ddo_etl::tree::data_file_bytes(&root).unwrap();
    for (path, bytes) in &before {
        if path != Path::new("redirects.toml") {
            assert_eq!(after[path], *bytes, "{} changed", path.display());
        }
    }
    let contents = std::fs::read_to_string(root.join("redirects.toml")).unwrap();
    assert!(contents.contains("day-thirty") && contents.contains("future"));
    assert!(!contents.contains("day-thirty-one"));
    let mut built = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(&root, &mut built).unwrap();
    let exported = folder.path().join("exported");
    ddo_etl::tree::export_database(&built, &exported).unwrap();
    assert_eq!(std::fs::read_to_string(exported.join("redirects.toml")).unwrap(), contents);
    assert!(ddo_etl::tree::redirects::prune_redirects(&root, Some("2026-02-30")).is_err());
}

#[test]
fn redirect_file_round_trips_authored_rows_and_derives_owned_ingredients() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("data");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let original = common::export_fixture_data(&fixtures, &root);
    let target: String = original.query_row("SELECT slug FROM crafting_systems WHERE id IN (SELECT system_id FROM crafting_ingredients) ORDER BY slug LIMIT 1", [], |row| row.get(0)).unwrap();
    let contents = format!("[[redirect]]\nkind = \"crafting-systems\"\nfrom = \"former-system\"\nto = {target:?}\nsince = \"2026-10-01\"\n\n[[removed]]\nkind = \"items\"\nslug = \"removed-item\"\nsince = \"2026-10-01\"\nreason = \"Duplicate item\"\n");
    std::fs::write(root.join("redirects.toml"), &contents).unwrap();
    let mut built = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(&root, &mut built).unwrap();
    let expected: Vec<String> = original.prepare("SELECT i.slug FROM crafting_ingredients i JOIN crafting_systems s ON s.id = i.system_id WHERE s.slug = ?1 ORDER BY i.slug").unwrap().query_map([&target], |row| row.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    let actual: Vec<(String, String, String)> = built.prepare("SELECT from_slug, to_slug, since FROM served_redirects WHERE kind = 'crafting-ingredients' ORDER BY to_slug").unwrap().query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap();
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
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut db = Connection::open_in_memory().unwrap();
    ddo_etl::build::build_database(
        &fixtures.join("DataFiles"),
        &ddo_etl::wiki::WikiOverrides::from_dir(&fixtures.join("wiki")).unwrap(),
        &ddo_etl::corrections::Corrections::default(),
        &mut db,
        &ddo_model::DatasetVersion {
            upstream_sha: "fixture".to_string(),
            built_at: "2026-10-17T00:00:00Z".to_string(),
        },
    )
    .unwrap();
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
