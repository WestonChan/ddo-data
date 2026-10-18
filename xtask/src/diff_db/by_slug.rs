use super::{baseline_response, json_differences, json_response, table_columns, table_names, DiffReport, PAGE_SIZE};
use anyhow::{ensure, Context, Result};
use ddo_api::{app, AppState};
use ddo_etl::delta::{polymorphic_table, sql_text, IdentityIndex};
use rusqlite::Connection;
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

type DetailPairs = BTreeMap<String, BTreeMap<String, (Option<i64>, Option<i64>)>>;

fn compare_tables_by_slug(
    first: &Connection,
    second: &Connection,
    first_index: &mut IdentityIndex,
    second_index: &mut IdentityIndex,
    report: &mut DiffReport,
) -> Result<()> {
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
        let first_rows = first_index.normalized_rows(first, table)?;
        let second_rows = second_index.normalized_rows(second, table)?;
        if first_rows != second_rows {
            let key_index = first_columns.iter().position(|column| column == "id");
            let rows_by_key = |rows: Vec<Vec<String>>| {
                let mut counts = BTreeMap::<String, usize>::new();
                rows.into_iter()
                    .map(|row| {
                        let identity = if table == "effect_vocabulary_bonus_types" {
                            ["kind", "id", "bonus_type_id"]
                                .iter()
                                .map(|column| {
                                    row[first_columns.iter().position(|name| name == column).unwrap()].clone()
                                })
                                .collect::<Vec<_>>()
                                .join("/")
                        } else {
                            key_index.map_or_else(|| row.join("/"), |index| row[index].clone())
                        };
                        let count = counts.entry(identity.clone()).or_default();
                        let key = format!("{identity} occurrence {count}");
                        *count += 1;
                        (key, row)
                    })
                    .collect::<BTreeMap<_, _>>()
            };
            let first_by_key = rows_by_key(first_rows);
            let second_by_key = rows_by_key(second_rows);
            for key in first_by_key.keys().chain(second_by_key.keys()).collect::<BTreeSet<_>>() {
                match (first_by_key.get(key), second_by_key.get(key)) {
                    (Some(left), Some(right)) => {
                        for (column, (before, after)) in first_columns.iter().zip(left.iter().zip(right)) {
                            if before != after {
                                report
                                    .table_differences
                                    .push(format!("table {table}: {key} column {column}: {before} vs {after}"));
                            }
                        }
                    }
                    (Some(_), None) => {
                        report.table_differences.push(format!("table {table}: {key} missing from second"))
                    }
                    (None, Some(_)) => {
                        report.table_differences.push(format!("table {table}: {key} missing from first"))
                    }
                    (None, None) => {}
                }
            }
        }
    }
    Ok(())
}

fn route_table(path: &str) -> Option<&'static str> {
    ddo_model::enums::EntityKind::parse(path.strip_prefix("/v1/")?).map(|kind| kind.table_name())
}

fn response_reference_table(column: &str) -> Option<&'static str> {
    Some(match column {
        "base_class_id" | "class_id" => "classes",
        "clickie_id" => "clickies",
        "effect_id" => "effects",
        "feat_id" => "feats",
        "set_id" => "set_bonuses",
        "slot_type_id" => "augment_slot_types",
        "spell_id" => "spells",
        _ => return None,
    })
}

fn response_collection_table(field: &str) -> Option<&'static str> {
    Some(match field {
        "adventure_packs" => "adventure_packs",
        "augments" => "augments",
        "classes" => "classes",
        "enhancements" => "enhancements",
        "selections" => "enhancement_selections",
        "events" => "events",
        "feats" => "feats",
        "filigrees" => "filigrees",
        "ingredients" => "crafting_ingredients",
        "items" => "items",
        "patrons" => "patrons",
        "quest_chains" => "quest_chains",
        "quests" => "quests",
        "races" => "races",
        "sagas" => "sagas",
        "sets" => "set_bonuses",
        "spells" => "spells",
        "vendors" => "vendors",
        "group" => "effects",
        _ => return None,
    })
}

fn normalize_response(value: &mut JsonValue, index: &mut IdentityIndex, root_table: Option<&str>) -> Result<()> {
    match value {
        JsonValue::Array(rows) => {
            for row in rows {
                normalize_response(row, index, root_table)?;
            }
        }
        JsonValue::Object(fields) => {
            let named_identity = match (fields.get("id").and_then(JsonValue::as_i64), root_table) {
                (Some(id), Some(table)) => Some(index.identity(table, id)?),
                _ => None,
            };
            let child_table = if fields.contains_key("id") { None } else { root_table };
            for (field, child) in fields.iter_mut() {
                if field != "id" && !field.ends_with("_id") {
                    normalize_response(child, index, response_collection_table(field).or(child_table))?;
                }
            }
            let source_kind = fields
                .get("kind")
                .or_else(|| fields.get("source_kind"))
                .and_then(JsonValue::as_str)
                .map(str::to_string);
            let keys = fields.keys().cloned().collect::<Vec<_>>();
            for field in keys {
                if field.ends_with("_id") {
                    let Some(id) = fields.get(&field).and_then(JsonValue::as_i64) else { continue };
                    let target = if field == "source_id" {
                        source_kind.as_deref().and_then(polymorphic_table)
                    } else {
                        response_reference_table(&field)
                    };
                    if let Some(target) = target {
                        fields.insert(field, JsonValue::String(index.identity(target, id)?));
                    }
                }
            }
            if fields.get("id").and_then(JsonValue::as_i64).is_some() {
                let identity = named_identity.or_else(|| {
                    fields.get("name").or_else(|| fields.get("label")).and_then(JsonValue::as_str).map(|name| {
                        let family = fields.get("family").and_then(JsonValue::as_str).unwrap_or("");
                        let level = fields
                            .get("min_level")
                            .and_then(JsonValue::as_i64)
                            .map_or_else(String::new, |n| n.to_string());
                        format!("{name}/{family}/{level}")
                    })
                });
                let identity = identity.unwrap_or_else(|| {
                    let mut contents = fields.clone();
                    contents.remove("id");
                    format!("row:{}", JsonValue::Object(contents))
                });
                fields.insert("id".to_string(), JsonValue::String(identity.clone()));
                if let Some(JsonValue::String(detail_path)) = fields.get_mut("detail_path") {
                    if let Some((prefix, tail)) = detail_path.rsplit_once('/') {
                        if tail.parse::<i64>().is_ok() {
                            *detail_path = format!("{prefix}/{identity}");
                        }
                    }
                }
            }
            if fields.contains_key("built_at") {
                fields.insert("built_at".to_string(), JsonValue::String("build-time".to_string()));
            }
        }
        _ => {}
    }
    Ok(())
}

struct ResponseComparison<'a> {
    first_index: &'a mut IdentityIndex,
    second_index: &'a mut IdentityIndex,
    report: &'a mut DiffReport,
}

impl ResponseComparison<'_> {
    fn record_pair(
        &mut self,
        path: &str,
        first: Result<JsonValue>,
        second: Result<JsonValue>,
        root_table: Option<&str>,
    ) -> Result<(JsonValue, JsonValue)> {
        let has_error = first.is_err() || second.is_err();
        let mut first = first.unwrap_or_else(|error| JsonValue::String(format!("response error: {error}")));
        let mut second = second.unwrap_or_else(|error| JsonValue::String(format!("response error: {error}")));
        let first_identity = normalize_response(&mut first, self.first_index, root_table);
        let second_identity = normalize_response(&mut second, self.second_index, root_table);
        let mut pairing_failed = false;
        for (side, result) in [("baseline", first_identity), ("current", second_identity)] {
            if let Err(error) = result {
                self.report.pairing_failures.insert(format!("route {path}: {side} pairing failed: {error:#}"));
                pairing_failed = true;
            }
        }
        self.report.compared_responses += 1;
        if pairing_failed {
            if path.contains("/{") {
                self.report.failed_details.insert(path.to_string());
            }
            return Ok((first, second));
        }
        if first != second || has_error {
            self.report.total_response_differences += 1;
            self.report.response_differences.extend(
                json_differences(&first, &second, "$")
                    .into_iter()
                    .map(|difference| format!("route {path}: {difference}")),
            );
        }
        Ok((first, second))
    }
}

fn collect_detail_ids(
    response: &JsonValue,
    route: &str,
    index: &mut IdentityIndex,
    pairs: &mut BTreeMap<String, (Option<i64>, Option<i64>)>,
    side: usize,
    report: &mut DiffReport,
) -> Result<()> {
    let Some(table) = route_table(route) else { return Ok(()) };
    let rows = response.as_object().and_then(|fields| fields.values().find_map(JsonValue::as_array));
    for row in rows.into_iter().flatten() {
        let Some(id) = row.get("id").and_then(JsonValue::as_i64) else { continue };
        let identity = match row
            .get("slug")
            .and_then(JsonValue::as_str)
            .map(|slug| Ok(format!("{table}/{slug}")))
            .unwrap_or_else(|| index.identity(table, id))
        {
            Ok(identity) => identity,
            Err(error) => {
                let side = if side == 0 { "baseline" } else { "current" };
                let label = format!("route {route}/{id}: {side} table {table} id {id}");
                report.failed_details.insert(label.clone());
                report.pairing_failures.insert(format!("{label}: {error:#}"));
                continue;
            }
        };
        let pair = pairs.entry(identity.clone()).or_default();
        if side == 0 {
            ensure!(pair.0.is_none(), "{route}: duplicate slug {identity:?} for ids {:?} and {id}", pair.0);
            pair.0 = Some(id)
        } else {
            ensure!(pair.1.is_none(), "{route}: duplicate slug {identity:?} for ids {:?} and {id}", pair.1);
            pair.1 = Some(id)
        }
    }
    Ok(())
}

fn compare_responses_by_slug(
    first_path: &Path,
    second_path: &Path,
    against_api: Option<&str>,
    first_index: &mut IdentityIndex,
    second_index: &mut IdentityIndex,
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
    let mut detail_pairs = DetailPairs::new();
    let mut comparison = ResponseComparison { first_index, second_index, report };
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
            if detail_paths.contains(path) {
                let pairs = detail_pairs.entry(path.clone()).or_default();
                if let Ok(response) = &first {
                    collect_detail_ids(response, path, comparison.first_index, pairs, 0, comparison.report)?;
                }
                if let Ok(response) = &second {
                    collect_detail_ids(response, path, comparison.second_index, pairs, 1, comparison.report)?;
                }
            }
            let (first, second) = comparison.record_pair(&request_path, first, second, route_table(path))?;
            let first_rows = first.as_object().and_then(|fields| fields.values().find_map(JsonValue::as_array));
            let second_rows = second.as_object().and_then(|fields| fields.values().find_map(JsonValue::as_array));
            let Some(total) = first["total"].as_u64().max(second["total"].as_u64()) else { break };
            let page_length = first_rows.map_or(0, Vec::len).max(second_rows.map_or(0, Vec::len));
            offset += page_length;
            if page_length == 0 || offset as u64 >= total {
                break;
            }
        }
    }
    for path in detail_paths {
        let pairs = detail_pairs.remove(&path).unwrap_or_default();
        let Some(table) = route_table(&path) else { continue };
        for (identity, (first_id, second_id)) in pairs {
            let label = format!("{path}/{{{identity}}}");
            match (first_id, second_id) {
                (Some(baseline_id), Some(_current_id)) => {
                    let slug =
                        identity.strip_prefix(&format!("{table}/")).context("detail identity lacks its table")?;
                    let baseline_key = if paths.contains_key(&format!("{path}/{{id}}")) {
                        baseline_id.to_string()
                    } else {
                        slug.to_string()
                    };
                    let first =
                        baseline_response(&first_router, &format!("{path}/{baseline_key}"), &runtime, against_api);
                    let second = runtime.block_on(json_response(&second_router, &format!("{path}/{slug}")));
                    comparison.record_pair(&label, first, second, Some(table))?;
                    if !comparison.report.failed_details.contains(&label) {
                        comparison.report.paired_details += 1;
                    }
                }
                (baseline_id, current_id) => {
                    let (index, id, change) = if let Some(id) = baseline_id {
                        comparison.report.removed_details += 1;
                        (&comparison.first_index, id, "removed")
                    } else {
                        comparison.report.added_details += 1;
                        (&comparison.second_index, current_id.context("detail pair has no ids")?, "added")
                    };
                    let fields = index.row(table, id).context("detail has no row")?;
                    let name = fields
                        .get("name")
                        .or_else(|| fields.get("label"))
                        .and_then(sql_text)
                        .context("detail has no name")?;
                    comparison.report.total_response_differences += 1;
                    comparison
                        .report
                        .response_differences
                        .push(format!("route {path}/{id}: table {table} name {name:?} {change}"));
                }
            }
        }
    }
    ensure!(comparison.report.paired_details > 0, "no detail responses were paired by slug");
    Ok(())
}

pub fn diff_databases_by_slug(first_path: &Path, second_path: &Path) -> Result<DiffReport> {
    diff_databases_by_slug_against_api(first_path, second_path, None)
}

pub fn diff_databases_by_slug_against_api(
    first_path: &Path,
    second_path: &Path,
    against_api: Option<&str>,
) -> Result<DiffReport> {
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY;
    let first = Connection::open_with_flags(first_path, flags)?;
    let second = Connection::open_with_flags(second_path, flags)?;
    let mut first_index = IdentityIndex::from_db(&first)?;
    let mut second_index = IdentityIndex::from_db(&second)?;
    let mut report = DiffReport::default();
    compare_tables_by_slug(&first, &second, &mut first_index, &mut second_index, &mut report)?;
    if let Some(base_url) = against_api {
        let dump = super::api_baseline::baseline_dump(base_url)?;
        first_index = IdentityIndex::from_db(&Connection::open_with_flags(dump, flags)?)?;
        first_index.pair_response_names(&mut second_index)?;
    }
    compare_responses_by_slug(first_path, second_path, against_api, &mut first_index, &mut second_index, &mut report)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_database() -> Connection {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/ddo-etl/tests/fixtures");
        let wiki = ddo_etl::wiki::WikiOverrides::from_dir(&fixtures.join("wiki")).unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        ddo_etl::build::build_database(
            &fixtures.join("DataFiles"),
            &wiki,
            &ddo_etl::corrections::Corrections::default(),
            &mut db,
            &ddo_model::DatasetVersion {
                upstream_sha: "fixture".to_string(),
                built_at: "2026-10-08T00:00:00Z".to_string(),
            },
        )
        .unwrap();
        db
    }

    #[test]
    fn legacy_response_ids_use_the_baseline_names_for_roots_and_references() {
        let db = fixture_database();
        let mut baseline = IdentityIndex::from_db_without_columns(&db, &["slug", "variant"]).unwrap();
        let mut current = IdentityIndex::from_db(&db).unwrap();
        let effect_id: i64 =
            db.query_row("SELECT id FROM effects WHERE name = 'Armor Class'", [], |row| row.get(0)).unwrap();
        let expected_slug: String =
            db.query_row("SELECT slug FROM effects WHERE id = ?1", [effect_id], |row| row.get(0)).unwrap();
        baseline.pair_response_names(&mut current).unwrap();
        let mut response =
            serde_json::json!({"id": effect_id, "name": "Armor Class", "bonuses": [{"effect_id": effect_id}]});
        normalize_response(&mut response, &mut baseline, Some("effects")).unwrap();
        assert_eq!(response["id"], format!("effects/{expected_slug}"));
        assert_eq!(response["bonuses"][0]["effect_id"], response["id"]);
        assert!(baseline
            .identity("effects", i64::MAX)
            .unwrap_err()
            .to_string()
            .contains(&format!("effects id {}", i64::MAX)));
    }

    #[test]
    fn legacy_ambiguous_names_fail_with_route_table_and_id() {
        let db = fixture_database();
        let mut baseline = IdentityIndex::from_db_without_columns(&db, &["slug", "variant"]).unwrap();
        let mut current = IdentityIndex::from_db(&db).unwrap();
        baseline.pair_response_names(&mut current).unwrap();
        let mut select = db.prepare("SELECT id, name FROM feats WHERE name = 'Lay on Hands'").unwrap();
        let rows = select
            .query_map([], |row| Ok(serde_json::json!({"id": row.get::<_, i64>(0)?, "name": row.get::<_, String>(1)?})))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(rows.len(), 2);
        let mut report = DiffReport::default();
        let mut pairs = BTreeMap::new();
        collect_detail_ids(&serde_json::json!({"feats": rows}), "/v1/feats", &mut baseline, &mut pairs, 0, &mut report)
            .unwrap();
        assert!(pairs.is_empty());
        assert_eq!(report.failed_details.len(), 2);
        assert_eq!(report.pairing_failures.len(), 2);
        for row in rows {
            let id = row["id"].as_i64().unwrap();
            assert!(report.pairing_failures.iter().any(|error| error.contains(&format!("route /v1/feats/{id}"))
                && error.contains(&format!("table feats id {id}"))
                && error.contains("ambiguous name")));
        }
    }

    #[test]
    fn missing_names_are_removals_and_additions_while_missing_dump_records_fail() {
        let db = fixture_database();
        let mut baseline = IdentityIndex::from_db_without_columns(&db, &["slug", "variant"]).unwrap();
        let mut current = IdentityIndex::from_db(&db).unwrap();
        let mut select = db.prepare("SELECT id, name FROM items ORDER BY name LIMIT 3").unwrap();
        let rows = select
            .query_map([], |row| Ok(serde_json::json!({"id": row.get::<_, i64>(0)?, "name": row.get::<_, String>(1)?})))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        let removed_id = rows[0]["id"].as_i64().unwrap();
        let added_id = rows[1]["id"].as_i64().unwrap();
        current.remove_row("items", removed_id);
        baseline.remove_row("items", added_id);
        baseline.pair_response_names(&mut current).unwrap();
        let mut pairs = BTreeMap::new();
        let mut report = DiffReport::default();
        collect_detail_ids(
            &serde_json::json!({"items": [&rows[0], &rows[2]]}),
            "/v1/items",
            &mut baseline,
            &mut pairs,
            0,
            &mut report,
        )
        .unwrap();
        collect_detail_ids(
            &serde_json::json!({"items": [&rows[1], &rows[2]]}),
            "/v1/items",
            &mut current,
            &mut pairs,
            1,
            &mut report,
        )
        .unwrap();
        assert_eq!(pairs.values().filter(|(first, second)| first.is_some() && second.is_some()).count(), 1);
        assert_eq!(pairs.values().filter(|(first, second)| first.is_some() && second.is_none()).count(), 1);
        assert_eq!(pairs.values().filter(|(first, second)| first.is_none() && second.is_some()).count(), 1);
        assert!(report.pairing_failures.is_empty());
        collect_detail_ids(
            &serde_json::json!({"items": [&rows[1]]}),
            "/v1/items",
            &mut baseline,
            &mut pairs,
            0,
            &mut report,
        )
        .unwrap();
        let error = report.pairing_failures.iter().next().unwrap();
        assert!(error.contains(&format!("route /v1/items/{added_id}")), "{error}");
        assert!(error.contains(&format!("table items id {added_id}")), "{error}");
        assert!(error.contains("has no row"), "{error}");
    }

    #[test]
    fn list_response_order_is_part_of_the_comparison() {
        let db = fixture_database();
        let mut first_index = IdentityIndex::from_db(&db).unwrap();
        let mut second_index = IdentityIndex::from_db(&db).unwrap();
        let mut select = db.prepare("SELECT id, name FROM items ORDER BY id LIMIT 2").unwrap();
        let rows = select
            .query_map([], |row| Ok(serde_json::json!({"id": row.get::<_, i64>(0)?, "name": row.get::<_, String>(1)?})))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(rows.len(), 2);
        let mut reversed = rows.clone();
        reversed.reverse();
        let mut report = DiffReport::default();
        let mut comparison =
            ResponseComparison { first_index: &mut first_index, second_index: &mut second_index, report: &mut report };
        comparison
            .record_pair(
                "/v1/items",
                Ok(serde_json::json!({"items": rows})),
                Ok(serde_json::json!({"items": reversed})),
                None,
            )
            .unwrap();
        assert_eq!(report.total_response_differences, 1, "{report}");
    }
}
