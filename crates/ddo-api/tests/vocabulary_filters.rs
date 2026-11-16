use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use ddo_api::{app, AppState};
use http_body_util::BodyExt;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::PathBuf;
use tower::ServiceExt;

async fn response(router: &Router, path: &str, parameters: &[(&str, &str)]) -> (StatusCode, Value) {
    let query = serde_urlencoded::to_string(parameters).unwrap();
    let request = Request::get(format!("{path}?{query}")).body(Body::empty()).unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

fn referenced<'document>(document: &'document Value, mut node: &'document Value) -> &'document Value {
    let mut visited = BTreeSet::new();
    while let Some(reference) = node["$ref"].as_str() {
        assert!(visited.insert(reference), "cyclic reference: {reference}");
        node = document.pointer(reference.strip_prefix('#').unwrap()).unwrap();
    }
    node
}

fn has_schema_type(schema: &Value, name: &str) -> bool {
    schema["type"] == name || schema["type"].as_array().is_some_and(|types| types.iter().any(|kind| kind == name))
}

fn string_filter_repeatability(document: &Value, schema: &Value) -> Option<bool> {
    let schema = referenced(document, schema);
    if has_schema_type(schema, "string") {
        return Some(false);
    }
    if has_schema_type(schema, "array") && string_filter_repeatability(document, &schema["items"]) == Some(false) {
        return Some(true);
    }
    ["allOf", "anyOf", "oneOf"]
        .iter()
        .find_map(|key| schema[key].as_array()?.iter().find_map(|branch| string_filter_repeatability(document, branch)))
}

async fn served_values(router: &Router, vocabulary: &Value) -> BTreeSet<String> {
    let path = vocabulary["route"].as_str().unwrap();
    let rows_key = vocabulary["rows_key"].as_str().unwrap();
    let value_key = vocabulary["value_key"].as_str().unwrap();
    let mut accepted = BTreeSet::new();
    let mut offset = 0;
    loop {
        let (status, page) = response(router, path, &[("limit", "10000"), ("offset", &offset.to_string())]).await;
        assert_eq!(status, StatusCode::OK, "{path}: {page}");
        let rows = page[rows_key].as_array().unwrap();
        let total = page["total"].as_u64().unwrap() as usize;
        assert!(!rows.is_empty() || offset == total, "{path}: empty page before total {total}");
        for row in rows {
            accepted.insert(
                row[value_key].as_str().unwrap_or_else(|| panic!("{path}: missing {value_key}: {row}")).to_string(),
            );
        }
        offset += rows.len();
        if offset == total {
            return accepted;
        }
        assert!(offset < total, "{path}: received more rows than total {total}");
    }
}

fn lists_every_value(listed: &str, accepted: &BTreeSet<String>) -> bool {
    if accepted.is_empty() {
        return listed.is_empty();
    }
    accepted.iter().any(|name| {
        let Some(tail) = listed.strip_prefix(name) else {
            return false;
        };
        if accepted.len() == 1 {
            return tail.is_empty();
        }
        let Some(tail) = tail.strip_prefix(", ") else {
            return false;
        };
        let mut remaining = accepted.clone();
        remaining.remove(name);
        lists_every_value(tail, &remaining)
    })
}

async fn known_value(router: &Router, path: &str, parameter: &str, listed: &str) -> Option<String> {
    for end in listed.match_indices(", ").map(|(offset, _)| offset).chain(std::iter::once(listed.len())) {
        let candidate = &listed[..end];
        let (status, _) = response(router, path, &[(parameter, candidate)]).await;
        if status == StatusCode::OK {
            return Some(candidate.to_string());
        }
    }
    None
}

#[tokio::test]
async fn every_vocabulary_filter_rejects_an_unknown_value_naming_its_accepted_values() {
    let database_path = std::env::temp_dir().join(format!("ddo-vocabulary-filters-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&database_path);
    let mut db = Connection::open(&database_path).unwrap();
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ddo-etl/tests/fixtures/data");
    ddo_etl::tree::build_from_data(&fixtures, &mut db).unwrap();
    drop(db);
    let router = app(AppState::open(&database_path).unwrap());
    let (status, document) = response(&router, "/v1/openapi.json", &[]).await;
    assert_eq!(status, StatusCode::OK);
    let unknown = "unknown-vocabulary-contract-value";
    let mut findings = Vec::new();
    let mut list_count = 0;
    let mut filter_count = 0;
    for (path, methods) in document["paths"].as_object().unwrap() {
        let Some(parameters) = methods["get"]["parameters"].as_array() else {
            continue;
        };
        let parameters = parameters.iter().map(|parameter| referenced(&document, parameter)).collect::<Vec<_>>();
        let query_parameters =
            parameters.iter().filter(|parameter| parameter["in"] == "query").copied().collect::<Vec<_>>();
        let documented_names =
            query_parameters.iter().map(|parameter| parameter["name"].as_str().unwrap()).collect::<BTreeSet<_>>();
        if !documented_names.contains("limit") || !documented_names.contains("offset") {
            continue;
        }
        list_count += 1;
        let (status, rejected_key) = response(&router, path, &[(unknown, "1")]).await;
        let accepted_keys = rejected_key["error"]
            .as_str()
            .and_then(|message| message.split_once("; allowed parameters: "))
            .map(|(_, keys)| keys.split(", ").collect::<BTreeSet<_>>());
        if status != StatusCode::BAD_REQUEST || accepted_keys.as_ref() != Some(&documented_names) {
            findings.push(format!("{path}: runtime query keys and OpenAPI parameters disagree: {rejected_key}"));
        }
        for parameter in query_parameters {
            let name = parameter["name"].as_str().unwrap();
            let Some(is_repeatable) = string_filter_repeatability(&document, &parameter["schema"]) else {
                continue;
            };
            if matches!(name, "q" | "sort")
                || parameter["x-match-key"]
                    .as_str()
                    .is_some_and(|key| name == format!("{key}_match") && documented_names.contains(key))
            {
                continue;
            }
            filter_count += 1;
            let (status, rejected) = response(&router, path, &[(name, unknown)]).await;
            let message = rejected["error"].as_str().unwrap_or_default();
            let prefix = format!("unknown {name} {unknown:?}");
            if parameter["x-effect-name"] == true {
                if status != StatusCode::BAD_REQUEST || message != prefix {
                    findings.push(format!("{path}?{name}: name-resolution exemption returned {status}: {message:?}"));
                }
                continue;
            }
            let Some(vocabulary) = parameter.get("x-vocabulary") else {
                findings
                    .push(format!("{path}?{name}: missing x-vocabulary; unknown value returned {status}: {message:?}"));
                continue;
            };
            assert!(vocabulary["name"].as_str().is_some_and(|name| !name.is_empty()), "{path}?{name}: {vocabulary}");
            let full_prefix = format!("{prefix}; accepted values: ");
            let Some(listed) = message.strip_prefix(&full_prefix).filter(|_| status == StatusCode::BAD_REQUEST) else {
                findings.push(format!("{path}?{name}: expected 400 {full_prefix:?}, received {status}: {message:?}"));
                continue;
            };
            let known = if vocabulary.get("route").is_some() {
                let accepted = served_values(&router, vocabulary).await;
                assert!(!accepted.contains(unknown), "{path}?{name}: the unknown probe became a vocabulary value");
                if !lists_every_value(listed, &accepted) {
                    findings.push(format!(
                        "{path}?{name}: accepted values differ from {}: {listed:?}; served: {accepted:?}",
                        vocabulary["route"]
                    ));
                }
                for accepted_value in &accepted {
                    let (status, page) = response(&router, path, &[(name, accepted_value)]).await;
                    if status != StatusCode::OK {
                        findings.push(format!(
                            "{path}?{name}={accepted_value:?}: served value rejected with {status}: {page}"
                        ));
                    }
                }
                accepted.first().cloned()
            } else {
                let known = known_value(&router, path, name, listed).await;
                if !listed.is_empty() && known.is_none() {
                    findings.push(format!("{path}?{name}: no listed value is accepted: {listed:?}"));
                }
                known
            };
            if let Some(known) = known.filter(|_| is_repeatable) {
                let (status, rejected) = response(
                    &router,
                    path,
                    &[(name, &known), (name, unknown), (name, "another-unknown-vocabulary-value")],
                )
                .await;
                if status != StatusCode::BAD_REQUEST || rejected["error"] != message {
                    findings.push(format!("{path}?{name}: mixed known and unknown values must reject the first unknown with the same accepted values: {status}: {rejected}"));
                }
            }
        }
    }
    assert!(list_count > 0 && filter_count > 0, "no list filters discovered in OpenAPI");
    drop(router);
    std::fs::remove_file(database_path).unwrap();
    assert!(findings.is_empty(), "{}", findings.join("\n"));
}
