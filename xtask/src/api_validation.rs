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

fn effect_home_types(db: &rusqlite::Connection) -> Result<BTreeMap<i64, Option<String>>> {
    let mut statement = db.prepare(
        "SELECT e.id, b.name
           FROM effects e LEFT JOIN bonus_types b ON b.id = e.home_bonus_type_id",
    )?;
    let home_types = statement
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(home_types)
}

fn response_effect_lines_show_non_home_types(
    response: &Value,
    path: &str,
    effect_home_types: &BTreeMap<i64, Option<String>>,
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
                if let Some(home_type) = effect_home_types.get(&effect_id) {
                    if !matches!(bonus_type, "Enhancement" | "Equipment") && home_type.as_deref() != Some(bonus_type) {
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

fn effect_vocabulary_counts(db: &rusqlite::Connection) -> Result<BTreeMap<i64, (i64, i64)>> {
    let mut statement = db.prepare("SELECT id, item_count, augment_count FROM effect_vocabulary_counts")?;
    let counts = statement
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, (row.get::<_, i64>(1)?, row.get::<_, i64>(2)?))))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(counts)
}

fn effect_detail_totals_match_vocabulary(
    detail: &Value,
    path: &str,
    vocabulary_counts: &BTreeMap<i64, (i64, i64)>,
) -> Result<()> {
    let effect_id = detail["id"].as_i64().with_context(|| format!("{path} has no id"))?;
    let (item_count, augment_count) =
        vocabulary_counts.get(&effect_id).copied().with_context(|| format!("{path} has no vocabulary row"))?;
    for (page_key, expected_total) in [("items", item_count), ("augments", augment_count)] {
        let total = detail[page_key]["total"].as_i64().with_context(|| format!("{path} has no {page_key} total"))?;
        ensure!(
            total == expected_total,
            "{path}: {page_key} total {total} differs from the vocabulary row's count {expected_total}"
        );
    }
    Ok(())
}

fn effects_that_need_a_description(db: &rusqlite::Connection) -> Result<BTreeSet<i64>> {
    let mut statement = db.prepare("SELECT id FROM effects WHERE description_template IS NOT NULL OR is_group = 1")?;
    let effect_ids = statement.query_map([], |row| row.get::<_, i64>(0))?.collect::<rusqlite::Result<_>>()?;
    Ok(effect_ids)
}

fn response_effect_lines_have_descriptions(
    response: &Value,
    path: &str,
    effects_that_need_a_description: &BTreeSet<i64>,
) -> Result<()> {
    match response {
        Value::Array(rows) => {
            for row in rows {
                response_effect_lines_have_descriptions(row, path, effects_that_need_a_description)?;
            }
        }
        Value::Object(fields) => {
            if let (Some(effect_id), Some(verbose_name), Some(description)) = (
                fields.get("effect_id").and_then(Value::as_i64),
                fields.get("verbose_name").and_then(Value::as_str),
                fields.get("description"),
            ) {
                ensure!(
                    !description.is_null() || !effects_that_need_a_description.contains(&effect_id),
                    "{path}: line {verbose_name:?} of effect {effect_id} has no description"
                );
            }
            for field in fields.values() {
                response_effect_lines_have_descriptions(field, path, effects_that_need_a_description)?;
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
    let effects_that_need_a_description = Arc::new(effects_that_need_a_description(&db)?);
    let vocabulary_counts = Arc::new(effect_vocabulary_counts(&db)?);
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
                    let effects_that_need_a_description = effects_that_need_a_description.clone();
                    let vocabulary_counts = vocabulary_counts.clone();
                    let is_effect_detail = list_path == "/v1/effects";
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
                                    response_effect_lines_have_descriptions(
                                        &response,
                                        &request_path,
                                        &effects_that_need_a_description,
                                    )
                                    .map_err(|error| error.to_string())?;
                                    if is_effect_detail {
                                        effect_detail_totals_match_vocabulary(
                                            &response,
                                            &request_path,
                                            &vocabulary_counts,
                                        )
                                        .map_err(|error| error.to_string())?;
                                    }
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
    use super::{
        effect_detail_totals_match_vocabulary, response_effect_lines_have_descriptions,
        response_effect_lines_show_non_home_types,
    };
    use serde_json::json;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn effect_detail_totals_must_equal_the_vocabulary_counts() {
        let counts = BTreeMap::from([(1_i64, (213_i64, 4_i64))]);
        let detail = |items_total: i64| json!({"id": 1, "name": "Strength", "items": {"total": items_total}, "augments": {"total": 4}});
        effect_detail_totals_match_vocabulary(&detail(213), "/v1/effects/1", &counts).unwrap();
        let error = effect_detail_totals_match_vocabulary(&detail(229), "/v1/effects/1", &counts).unwrap_err();
        assert!(error.to_string().contains("229") && error.to_string().contains("213"), "{error}");
    }

    #[test]
    fn a_line_of_an_effect_with_a_description_template_or_a_group_needs_a_description() {
        let described_effects = BTreeSet::from([314_i64]);
        let missing =
            json!({"items": [{"line": {"effect_id": 314, "verbose_name": "Spell Powers +49", "description": null}}]});
        let error = response_effect_lines_have_descriptions(&missing, "/v1/items/1", &described_effects).unwrap_err();
        assert!(error.to_string().contains("Spell Powers +49"), "{error}");
        let present = json!({"items": [{"line": {"effect_id": 314, "verbose_name": "Potency +49", "description": "Each spell power."}}]});
        response_effect_lines_have_descriptions(&present, "/v1/items/1", &described_effects).unwrap();
        let undescribed_effect =
            json!({"effects": [{"effect_id": 99, "verbose_name": "Ethereal", "description": null}]});
        response_effect_lines_have_descriptions(&undescribed_effect, "/v1/items/1", &described_effects).unwrap();
    }

    #[test]
    fn non_home_type_must_remain_visible_on_a_slotless_line() {
        let home_types = BTreeMap::from([(454, Some("Equipment".to_string()))]);
        let line =
            json!({"effects": [{"effect_id": 454, "bonus_type": "Insight", "verbose_name": "Spell Penetration I"}]});
        assert!(response_effect_lines_show_non_home_types(&line, "/v1/items/1", &home_types).is_err());
        let corrected = json!({"effects": [{"effect_id": 454, "bonus_type": "Insight", "verbose_name": "Insightful Spell Penetration I"}]});
        response_effect_lines_show_non_home_types(&corrected, "/v1/items/1", &home_types).unwrap();
    }
}
