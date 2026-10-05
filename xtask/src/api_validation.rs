use anyhow::{ensure, Context, Result};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use ddo_api::schema_validation::response_matches_schema;
use ddo_api::{app, AppState};
use http_body_util::BodyExt;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use tower::ServiceExt;

const PAGE_SIZE: usize = 1000;
const DETAIL_SAMPLE_COUNT: usize = 32;
const PARALLEL_DETAIL_REQUESTS: usize = 8;

async fn json_response(router: &axum::Router, path: &str) -> Result<Value> {
    let response = router.clone().oneshot(Request::get(path).body(Body::empty())?).await?;
    let status = response.status();
    let body = response.into_body().collect().await?.to_bytes();
    ensure!(status == StatusCode::OK, "GET {path}: {status}: {}", String::from_utf8_lossy(&body));
    serde_json::from_slice(&body).with_context(|| format!("GET {path} returned invalid JSON"))
}

pub fn validate_api(db_path: &Path, all_details: bool) -> Result<(usize, usize)> {
    let state = AppState::open(db_path)?;
    let router = app(state);
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let spec = runtime.block_on(json_response(&router, "/v1/openapi.json"))?;
    let schemas = spec["components"]["schemas"].as_object().context("OpenAPI components.schemas is absent")?;
    let paths = spec["paths"].as_object().context("OpenAPI paths is absent")?;
    let detail_paths: BTreeSet<&str> = paths.keys().filter_map(|path| path.strip_suffix("/{id}")).collect();
    let mut ids_by_list_path: BTreeMap<&str, BTreeSet<i64>> = BTreeMap::new();
    let mut checked_routes = 0;
    let mut checked_responses = 0;
    let mut violations = Vec::new();
    for (path, route) in paths {
        let Some(schema) = route["get"]["responses"]["200"]["content"]["application/json"].get("schema") else {
            continue;
        };
        if path.ends_with("/{id}") {
            continue;
        }
        checked_routes += 1;
        let mut offset = 0;
        loop {
            let request_path = if path == "/v1/version" {
                path.to_string()
            } else {
                format!("{path}?limit={PAGE_SIZE}&offset={offset}")
            };
            let response = runtime.block_on(json_response(&router, &request_path))?;
            if let Err(error) = response_matches_schema(&response, schema, schemas, path) {
                if violations.len() < 100 {
                    violations.push(error);
                }
            }
            checked_responses += 1;
            let Some(total) = response["total"].as_u64() else {
                break;
            };
            let Some(rows) = response.as_object().and_then(|fields| fields.values().find_map(Value::as_array)) else {
                break;
            };
            if detail_paths.contains(path.as_str()) {
                let ids = ids_by_list_path.entry(path).or_default();
                for row in rows {
                    ids.insert(row["id"].as_i64().with_context(|| format!("{path} row has no numeric id"))?);
                }
            }
            offset += rows.len();
            if rows.is_empty() || offset as u64 >= total {
                break;
            }
        }
    }
    for (path, route) in paths {
        let Some(list_path) = path.strip_suffix("/{id}") else {
            continue;
        };
        let Some(schema) = route["get"]["responses"]["200"]["content"]["application/json"].get("schema") else {
            continue;
        };
        let ids = ids_by_list_path.get(list_path).with_context(|| format!("{path} has no list route"))?;
        ensure!(!ids.is_empty(), "{path} has no rows to validate");
        checked_routes += 1;
        let indexed_ids: Vec<i64> = ids.iter().copied().collect();
        let selected_ids: Vec<i64> = if all_details {
            indexed_ids
        } else {
            let sample_count = indexed_ids.len().min(DETAIL_SAMPLE_COUNT);
            (0..sample_count)
                .map(|index| indexed_ids[index * (indexed_ids.len() - 1) / (sample_count - 1).max(1)])
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect()
        };
        for id_group in selected_ids.chunks(PARALLEL_DETAIL_REQUESTS) {
            let responses: Vec<(String, Value)> = runtime.block_on(async {
                let mut requests = tokio::task::JoinSet::new();
                for id in id_group {
                    let request_path = format!("{list_path}/{id}");
                    let request_router = router.clone();
                    requests.spawn(async move {
                        let response = json_response(&request_router, &request_path).await?;
                        Ok::<_, anyhow::Error>((request_path, response))
                    });
                }
                let mut responses = Vec::new();
                while let Some(response) = requests.join_next().await {
                    responses.push(response??);
                }
                Ok::<_, anyhow::Error>(responses)
            })?;
            for (request_path, response) in responses {
                if let Err(error) = response_matches_schema(&response, schema, schemas, &request_path) {
                    if violations.len() < 100 {
                        violations.push(error);
                    }
                }
                checked_responses += 1;
            }
        }
        println!("checked {path}: {} detail responses", selected_ids.len());
    }
    ensure!(
        violations.is_empty(),
        "schema validation failed for {checked_responses} responses: {}",
        violations.join("\n")
    );
    Ok((checked_routes, checked_responses))
}
