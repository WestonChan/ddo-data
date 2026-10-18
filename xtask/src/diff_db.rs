use anyhow::{Context, Result};
use axum::body::Body;
use axum::http::Request;
use ddo_api::{app, AppState};
use ddo_etl::database_tables::{table_columns, table_names, table_rows};
use http_body_util::BodyExt;
use rusqlite::Connection;
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Display};
use std::fs;
use std::path::Path;
use std::process::Command;
use tower::ServiceExt;

const PAGE_SIZE: usize = 1000;

mod api_baseline;
mod by_slug;
pub use by_slug::{diff_databases_by_slug, diff_databases_by_slug_against_api};

#[derive(Debug, Default)]
pub struct DiffReport {
    pub compared_tables: usize,
    pub compared_responses: usize,
    pub table_differences: Vec<String>,
    pub response_differences: Vec<String>,
    pub total_response_differences: usize,
    pub paired_details: usize,
    pub removed_details: usize,
    pub added_details: usize,
    pub failed_details: BTreeSet<String>,
    pub pairing_failures: BTreeSet<String>,
}

impl DiffReport {
    pub fn is_equal(&self) -> bool {
        self.table_differences.is_empty() && self.total_response_differences == 0 && self.pairing_failures.is_empty()
    }
}

pub fn write_allowed_differences(report: &DiffReport, path: &Path) -> Result<()> {
    anyhow::ensure!(
        report.pairing_failures.is_empty(),
        "response identity pairing failed: {:?}",
        report.pairing_failures
    );
    let mut contents = String::new();
    for difference in report.table_differences.iter().chain(&report.response_differences) {
        contents.push_str(difference);
        contents.push('\n');
    }
    fs::write(path, contents).with_context(|| format!("writing {}", path.display()))
}

pub fn check_allowed_differences(report: &DiffReport, path: &Path) -> Result<()> {
    anyhow::ensure!(
        report.pairing_failures.is_empty(),
        "response identity pairing failed: {:?}",
        report.pairing_failures
    );
    let expected = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let expected = expected.lines().filter(|line| !line.is_empty()).collect::<Vec<_>>();
    let actual =
        report.table_differences.iter().chain(&report.response_differences).map(String::as_str).collect::<Vec<_>>();
    if let Some((index, (expected, actual))) =
        expected.iter().zip(&actual).enumerate().find(|(_, pair)| pair.0 != pair.1)
    {
        anyhow::bail!("{}: difference {} changed: expected {expected:?}, found {actual:?}", path.display(), index + 1);
    }
    anyhow::ensure!(
        expected.len() == actual.len(),
        "{}: expected {} differences, found {}",
        path.display(),
        expected.len(),
        actual.len()
    );
    Ok(())
}

impl Display for DiffReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            formatter,
            "compared {} tables and {} served JSON responses; {} table differences, {} response differences",
            self.compared_tables,
            self.compared_responses,
            self.table_differences.len(),
            self.total_response_differences
        )?;
        writeln!(
            formatter,
            "detail responses: {} paired, {} removed, {} added, {} failed",
            self.paired_details,
            self.removed_details,
            self.added_details,
            self.failed_details.len()
        )?;
        for failure in &self.pairing_failures {
            writeln!(formatter, "{failure}")?;
        }
        for difference in self.table_differences.iter().chain(&self.response_differences) {
            writeln!(formatter, "{difference}")?;
        }
        Ok(())
    }
}

pub fn compare_tables(first: &Connection, second: &Connection, report: &mut DiffReport) -> Result<()> {
    let first_tables = table_names(first)?;
    let second_tables = table_names(second)?;
    for table in first_tables.union(&second_tables) {
        if !first_tables.contains(table) || !second_tables.contains(table) {
            report.table_differences.push(format!("table {table}: present in only one database"));
            continue;
        }
        report.compared_tables += 1;
        let first_columns = table_columns(first, table)?;
        let second_columns = table_columns(second, table)?;
        if first_columns != second_columns {
            report
                .table_differences
                .push(format!("table {table}: columns differ: {first_columns:?} vs {second_columns:?}"));
            continue;
        }
        let first_rows = table_rows(first, table)?;
        let second_rows = table_rows(second, table)?;
        if first_rows != second_rows {
            let mismatch = first_rows
                .iter()
                .zip(&second_rows)
                .position(|(first_row, second_row)| first_row != second_row)
                .unwrap_or(first_rows.len().min(second_rows.len()));
            let first_row = first_rows.get(mismatch);
            let second_row = second_rows.get(mismatch);
            let difference = match (first_row, second_row) {
                (Some(first_row), Some(second_row)) => {
                    let column_index =
                        first_row.iter().zip(second_row).position(|(first, second)| first != second).unwrap();
                    format!(
                        "table {table}: row {} column {}: {:?} vs {:?}",
                        mismatch + 1,
                        first_columns[column_index],
                        first_row[column_index],
                        second_row[column_index]
                    )
                }
                _ => format!(
                    "table {table}: row {} is missing ({} vs {} rows): {first_row:?} vs {second_row:?}",
                    mismatch + 1,
                    first_rows.len(),
                    second_rows.len()
                ),
            };
            report.table_differences.push(difference);
        }
    }
    Ok(())
}

async fn json_response(router: &axum::Router, path: &str) -> Result<JsonValue> {
    let response = router.clone().oneshot(Request::get(path).body(Body::empty())?).await?;
    let status = response.status();
    let bytes = response.into_body().collect().await?.to_bytes();
    anyhow::ensure!(status.is_success(), "{path} returned {status}: {}", String::from_utf8_lossy(&bytes));
    serde_json::from_slice(&bytes).with_context(|| format!("decoding {path}"))
}

fn baseline_response(
    router: &axum::Router,
    path: &str,
    runtime: &tokio::runtime::Runtime,
    against_api: Option<&str>,
) -> Result<JsonValue> {
    let Some(base_url) = against_api else { return runtime.block_on(json_response(router, path)) };
    api_response(base_url, path)
}

fn api_response(base_url: &str, path: &str) -> Result<JsonValue> {
    serde_json::from_slice(&api_bytes(base_url, path)?).with_context(|| format!("decoding {base_url}{path}"))
}

fn api_bytes(base_url: &str, path: &str) -> Result<Vec<u8>> {
    let url = format!("{}{path}", base_url.trim_end_matches('/'));
    let output = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--fail",
            "--retry",
            "8",
            "--retry-all-errors",
            "--retry-delay",
            "1",
            "--url",
        ])
        .arg(&url)
        .output()
        .with_context(|| format!("requesting {url}"))?;
    anyhow::ensure!(output.status.success(), "{url}: {}", String::from_utf8_lossy(&output.stderr));
    Ok(output.stdout)
}

fn record_response_values(
    path: &str,
    first_response: Result<JsonValue>,
    second_response: Result<JsonValue>,
    report: &mut DiffReport,
) -> (JsonValue, JsonValue) {
    let has_response_error = first_response.is_err() || second_response.is_err();
    let first = first_response.unwrap_or_else(|error| JsonValue::String(format!("response error: {error}")));
    let second = second_response.unwrap_or_else(|error| JsonValue::String(format!("response error: {error}")));
    report.compared_responses += 1;
    if first != second || has_response_error {
        report.total_response_differences += 1;
        report.response_differences.extend(
            json_differences(&first, &second, "$").into_iter().map(|difference| format!("route {path}: {difference}")),
        );
    }
    (first, second)
}

fn json_differences(first: &JsonValue, second: &JsonValue, location: &str) -> Vec<String> {
    match (first, second) {
        (JsonValue::Object(first_fields), JsonValue::Object(second_fields)) => {
            let mut differences = Vec::new();
            for key in first_fields.keys().chain(second_fields.keys()).collect::<BTreeSet<_>>() {
                let next_location = format!("{location}.{key}");
                match (first_fields.get(key), second_fields.get(key)) {
                    (Some(first_value), Some(second_value)) if first_value != second_value => {
                        differences.extend(json_differences(first_value, second_value, &next_location));
                    }
                    (None, Some(_)) | (Some(_), None) => {
                        differences.push(format!("{next_location} exists in only one response"));
                    }
                    _ => {}
                }
            }
            differences
        }
        (JsonValue::Array(first_rows), JsonValue::Array(second_rows)) => {
            let mut differences = Vec::new();
            for (index, (first_row, second_row)) in first_rows.iter().zip(second_rows).enumerate() {
                if first_row != second_row {
                    differences.extend(json_differences(first_row, second_row, &format!("{location}[{index}]")));
                }
            }
            if first_rows.len() != second_rows.len() {
                differences.push(format!("{location} length {} vs {}", first_rows.len(), second_rows.len()));
            }
            differences
        }
        _ => vec![format!("{location}: {first} vs {second}")],
    }
}

pub fn compare_responses(
    first_path: &Path,
    second_path: &Path,
    against_api: Option<&str>,
    report: &mut DiffReport,
) -> Result<()> {
    let first_router = app(AppState::open(first_path)?);
    let second_router = app(AppState::open(second_path)?);
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let specification = baseline_response(&first_router, "/v1/openapi.json", &runtime, against_api)?;
    let paths = specification["paths"].as_object().context("OpenAPI paths are absent")?;
    let detail_paths: BTreeSet<String> = paths
        .keys()
        .filter_map(|path| path.strip_suffix("/{slug}").or_else(|| path.strip_suffix("/{id}")).map(str::to_string))
        .collect();
    let mut keys_by_list_path: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (path, route) in paths {
        if path.ends_with("/{id}")
            || path.ends_with("/{slug}")
            || route["get"]["responses"]["200"]["content"]["application/json"].get("schema").is_none()
        {
            continue;
        }
        let mut offset = 0;
        loop {
            let request_path = if path == "/v1/version" {
                path.to_string()
            } else {
                format!("{path}?limit={PAGE_SIZE}&offset={offset}")
            };
            let first = baseline_response(&first_router, &request_path, &runtime, against_api);
            let second = runtime.block_on(json_response(&second_router, &request_path));
            let (first, second) = record_response_values(&request_path, first, second, report);
            let first_rows = first.as_object().and_then(|fields| fields.values().find_map(JsonValue::as_array));
            let second_rows = second.as_object().and_then(|fields| fields.values().find_map(JsonValue::as_array));
            if detail_paths.contains(path) {
                let keys = keys_by_list_path.entry(path.clone()).or_default();
                for row in first_rows.into_iter().chain(second_rows).flat_map(|rows| rows.iter()) {
                    let key = if paths.contains_key(&format!("{path}/{{slug}}")) {
                        row["slug"].as_str().map(str::to_string)
                    } else {
                        row["id"].as_i64().map(|id| id.to_string())
                    };
                    if let Some(key) = key {
                        keys.insert(key);
                    }
                }
            }
            let Some(total) = first["total"].as_u64().max(second["total"].as_u64()) else {
                break;
            };
            let page_length = first_rows.map_or(0, Vec::len).max(second_rows.map_or(0, Vec::len));
            offset += page_length;
            if page_length == 0 || offset as u64 >= total {
                break;
            }
        }
    }
    for list_path in detail_paths {
        let keys =
            keys_by_list_path.get(&list_path).map(|keys| keys.iter().cloned().collect::<Vec<_>>()).unwrap_or_default();
        for key_group in keys.chunks(16) {
            let mut responses = runtime.block_on(async {
                let mut requests = tokio::task::JoinSet::new();
                for key in key_group {
                    let request_path = format!("{list_path}/{key}");
                    let first_router = first_router.clone();
                    let second_router = second_router.clone();
                    requests.spawn(async move {
                        let first = json_response(&first_router, &request_path).await;
                        let second = json_response(&second_router, &request_path).await;
                        (request_path, first, second)
                    });
                }
                let mut responses = Vec::new();
                while let Some(response) = requests.join_next().await {
                    responses.push(response?);
                }
                Ok::<_, anyhow::Error>(responses)
            })?;
            responses.sort_by(|first, second| first.0.cmp(&second.0));
            for (request_path, first, second) in responses {
                let first = if against_api.is_some() {
                    baseline_response(&first_router, &request_path, &runtime, against_api)
                } else {
                    first
                };
                let _ = record_response_values(&request_path, first, second, report);
            }
        }
    }
    Ok(())
}

pub fn diff_databases(first_path: &Path, second_path: &Path) -> Result<DiffReport> {
    diff_databases_against_api(first_path, second_path, None)
}

pub fn diff_databases_against_api(
    first_path: &Path,
    second_path: &Path,
    against_api: Option<&str>,
) -> Result<DiffReport> {
    let first = Connection::open_with_flags(first_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let second = Connection::open_with_flags(second_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut report = DiffReport::default();
    compare_tables(&first, &second, &mut report)?;
    compare_responses(first_path, second_path, against_api, &mut report)?;
    Ok(report)
}
