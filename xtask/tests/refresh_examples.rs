use ddo_etl::corrections::Corrections;
use ddo_etl::wiki::WikiOverrides;
use serde_json::Value;
use std::path::{Path, PathBuf};
use xtask::dataset::build_database_file;
use xtask::response_examples::{write_response_examples, ExampleRequest, EXAMPLE_REQUESTS};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/ddo-etl/tests/fixtures")
}

fn fixture_db_in(work_dir: &Path) -> PathBuf {
    let db_path = work_dir.join("fixture.db");
    let wiki_overrides = WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap();
    build_database_file(&fixtures_dir().join("DataFiles"), &wiki_overrides, &Corrections::default(), &db_path).unwrap();
    db_path
}

fn with_first_fixture_id(sample_path: &str) -> String {
    match sample_path.rsplit_once('/') {
        Some((collection_path, id)) if id.chars().all(|c| c.is_ascii_digit()) => format!("{collection_path}/1"),
        _ => sample_path.to_string(),
    }
}

fn fixture_sample_paths() -> Vec<(&'static str, String)> {
    EXAMPLE_REQUESTS.iter().map(|request| (request.example_name, with_first_fixture_id(request.path))).collect()
}

fn as_requests<'a>(sample_paths: &'a [(&'static str, String)]) -> Vec<ExampleRequest<'a>> {
    sample_paths.iter().map(|(example_name, path)| ExampleRequest { example_name, path }).collect()
}

fn assert_arrays_trimmed(example: &Value, example_name: &str) {
    match example {
        Value::Array(elements) => {
            assert!(elements.len() <= 3, "{example_name} keeps {} array elements", elements.len());
            elements.iter().for_each(|element| assert_arrays_trimmed(element, example_name));
        }
        Value::Object(fields) => fields.values().for_each(|field| assert_arrays_trimmed(field, example_name)),
        _ => {}
    }
}

#[test]
fn refresh_writes_a_trimmed_example_for_every_documented_route() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_in(work_dir.path());
    let examples_dir = work_dir.path().join("examples");
    std::fs::create_dir(&examples_dir).unwrap();

    write_response_examples(&db_path, &as_requests(&fixture_sample_paths()), &examples_dir).unwrap();

    for documented_example in ddo_api::v1_response_examples() {
        let example_path = examples_dir.join(documented_example.file_name);
        let example_text = std::fs::read_to_string(&example_path).unwrap_or_else(|e| {
            panic!("no example for {} at {}: {e}", documented_example.route, example_path.display())
        });
        let example: Value = serde_json::from_str(&example_text).unwrap();
        assert!(!example.is_null(), "{} is null", documented_example.file_name);
        assert_arrays_trimmed(&example, documented_example.file_name);
        assert!(example_text.ends_with("}\n") || example_text.ends_with("]\n"), "{}", documented_example.file_name);
    }
    let stats_text = std::fs::read_to_string(examples_dir.join("stats.json")).unwrap();
    assert!(stats_text.starts_with("[\n  {\n    \""), "two-space indentation: {stats_text}");
}

#[test]
fn refresh_writes_keys_in_sorted_order() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_in(work_dir.path());

    write_response_examples(&db_path, &as_requests(&fixture_sample_paths()), work_dir.path()).unwrap();

    let version_text = std::fs::read_to_string(work_dir.path().join("version.json")).unwrap();
    let key_positions: Vec<usize> = ["\"api_commit\"", "\"counts\"", "\"dataset\"", "\"schema_version\""]
        .iter()
        .map(|key| version_text.find(key).unwrap_or_else(|| panic!("{key} missing from {version_text}")))
        .collect();
    assert!(key_positions.is_sorted(), "keys out of order: {version_text}");
}

#[test]
fn refresh_keeps_the_api_commit_already_in_version_json() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_in(work_dir.path());
    std::fs::write(work_dir.path().join("version.json"), r#"{ "api_commit": "committed-sha" }"#).unwrap();

    write_response_examples(&db_path, &as_requests(&fixture_sample_paths()), work_dir.path()).unwrap();

    let version: Value =
        serde_json::from_str(&std::fs::read_to_string(work_dir.path().join("version.json")).unwrap()).unwrap();
    assert_eq!(version["api_commit"], "committed-sha");
    assert!(version["counts"]["items"].as_i64().unwrap() > 0);
}

#[test]
fn refresh_fails_naming_the_example_whose_sample_fails_and_writes_nothing() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_in(work_dir.path());
    let examples_dir = work_dir.path().join("examples");
    std::fs::create_dir(&examples_dir).unwrap();
    let mut sample_paths = fixture_sample_paths();
    sample_paths.iter_mut().find(|(example_name, _)| *example_name == "items_id").unwrap().1 =
        "/v1/items/999999".to_string();

    let error = write_response_examples(&db_path, &as_requests(&sample_paths), &examples_dir).unwrap_err();

    assert!(format!("{error:#}").contains("items_id"), "{error:#}");
    assert_eq!(std::fs::read_dir(&examples_dir).unwrap().count(), 0);
}

#[test]
fn refresh_fails_naming_a_documented_example_without_a_sample() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_in(work_dir.path());
    let sample_paths: Vec<_> =
        fixture_sample_paths().into_iter().filter(|(example_name, _)| *example_name != "clickies").collect();

    let error = write_response_examples(&db_path, &as_requests(&sample_paths), work_dir.path()).unwrap_err();

    assert!(format!("{error:#}").contains("clickies"), "{error:#}");
}
