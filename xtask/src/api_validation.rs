use anyhow::{ensure, Context, Result};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use ddo_api::drop_validation::{detail_has_unique_drop_locations, source_pack_names};
use ddo_api::schema_validation::response_matches_schema;
use ddo_api::{app, AppState};
use http_body_util::BodyExt;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;
use tower::ServiceExt;

const PAGE_SIZE: usize = 1000;
const DETAIL_SAMPLE_COUNT: usize = 32;
const PARALLEL_DETAIL_REQUESTS: usize = 16;

async fn json_response(router: &axum::Router, path: &str) -> Result<Value> {
    let response = router.clone().oneshot(Request::get(path).body(Body::empty())?).await?;
    let status = response.status();
    let body = response.into_body().collect().await?.to_bytes();
    ensure!(status == StatusCode::OK, "GET {path}: {status}: {}", String::from_utf8_lossy(&body));
    serde_json::from_slice(&body).with_context(|| format!("GET {path} returned invalid JSON"))
}

fn effect_home_types(db: &rusqlite::Connection) -> Result<BTreeMap<i64, (Option<String>, bool)>> {
    let mut statement = db.prepare(
        "SELECT e.id, b.name, INSTR(COALESCE(e.verbose_name_template, ''), '%b1') > 0
           FROM effects e LEFT JOIN bonus_types b ON b.id = e.home_bonus_type_id",
    )?;
    let home_types = statement
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, (row.get::<_, Option<String>>(1)?, row.get::<_, bool>(2)?))))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(home_types)
}

fn response_effect_lines_show_non_home_types(
    response: &Value,
    path: &str,
    effect_home_types: &BTreeMap<i64, (Option<String>, bool)>,
) -> Result<()> {
    match response {
        Value::Array(rows) => {
            for row in rows {
                response_effect_lines_show_non_home_types(row, path, effect_home_types)?;
            }
        }
        Value::Object(fields) => {
            if let (Some(effect_id), Some(bonus_type), Some(verbose_name)) = (
                fields.get("effect_id").and_then(Value::as_i64),
                fields.get("bonus_type").and_then(Value::as_str),
                fields.get("verbose_name").and_then(Value::as_str),
            ) {
                if let Some((home_type, has_type_slot)) = effect_home_types.get(&effect_id) {
                    if *has_type_slot
                        && !matches!(bonus_type, "Enhancement" | "Equipment")
                        && home_type.as_deref() != Some(bonus_type)
                    {
                        let visible_type = if bonus_type == "Insight" { "Insightful" } else { bonus_type };
                        ensure!(
                            verbose_name.contains(visible_type)
                                || verbose_name.contains(&format!("{bonus_type} Bonus")),
                            "{path}: effect {effect_id} has non-home type {bonus_type:?} but verbose_name {verbose_name:?} omits it"
                        );
                    }
                }
            }
            for field in fields.values() {
                response_effect_lines_show_non_home_types(field, path, effect_home_types)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn validate_api(db_path: &Path, all_details: bool) -> Result<(usize, usize)> {
    let state = AppState::open(db_path)?;
    let db = rusqlite::Connection::open(db_path)?;
    let source_packs = Arc::new(source_pack_names(&db)?);
    let effect_home_types = Arc::new(effect_home_types(&db)?);
    let router = app(state);
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let spec = runtime.block_on(json_response(&router, "/v1/openapi.json"))?;
    let schemas = spec["components"]["schemas"].as_object().context("OpenAPI components.schemas is absent")?;
    let shared_schemas = Arc::new(schemas.clone());
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
        let detail_schema = Arc::new(schema.clone());
        for id_group in selected_ids.chunks(PARALLEL_DETAIL_REQUESTS) {
            let validated_responses: Vec<Option<String>> = runtime.block_on(async {
                let mut requests = tokio::task::JoinSet::new();
                for id in id_group {
                    let request_path = format!("{list_path}/{id}");
                    let check_drop_locations = list_path == "/v1/items" || list_path == "/v1/augments";
                    let request_router = router.clone();
                    let detail_schema = detail_schema.clone();
                    let shared_schemas = shared_schemas.clone();
                    let source_packs = source_packs.clone();
                    let effect_home_types = effect_home_types.clone();
                    requests.spawn(async move {
                        let response = json_response(&request_router, &request_path).await?;
                        let violation = tokio::task::spawn_blocking(move || {
                            response_matches_schema(&response, &detail_schema, &shared_schemas, &request_path)
                                .and_then(|()| {
                                    if check_drop_locations {
                                        detail_has_unique_drop_locations(&response, &request_path, &source_packs)?;
                                    }
                                    response_effect_lines_show_non_home_types(
                                        &response,
                                        &request_path,
                                        &effect_home_types,
                                    )
                                    .map_err(|error| error.to_string())?;
                                    Ok(())
                                })
                                .err()
                        })
                        .await?;
                        Ok::<_, anyhow::Error>(violation)
                    });
                }
                let mut validated_responses = Vec::new();
                while let Some(response) = requests.join_next().await {
                    validated_responses.push(response??);
                }
                Ok::<_, anyhow::Error>(validated_responses)
            })?;
            for violation in validated_responses {
                if let Some(error) = violation {
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

#[cfg(test)]
mod tests {
    use super::response_effect_lines_show_non_home_types;
    use serde_json::json;
    use std::collections::BTreeMap;

    #[test]
    fn non_home_type_must_remain_visible_on_a_slotless_line() {
        let home_types = BTreeMap::from([(454, (Some("Equipment".to_string()), true))]);
        let line =
            json!({"effects": [{"effect_id": 454, "bonus_type": "Insight", "verbose_name": "Spell Penetration I"}]});
        assert!(response_effect_lines_show_non_home_types(&line, "/v1/items/1", &home_types).is_err());
        let corrected = json!({"effects": [{"effect_id": 454, "bonus_type": "Insight", "verbose_name": "Insightful Spell Penetration I"}]});
        response_effect_lines_show_non_home_types(&corrected, "/v1/items/1", &home_types).unwrap();
    }
}
