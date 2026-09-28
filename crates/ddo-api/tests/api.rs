use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use ddo_api::{app, AppState};
use ddo_model::DatasetVersion;
use http_body_util::BodyExt;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::OnceLock;
use tower::ServiceExt;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ddo-etl/tests/fixtures/DataFiles")
}

fn db_path() -> &'static PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let path = std::env::temp_dir().join(format!("ddo-api-test-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut conn = rusqlite::Connection::open(&path).unwrap();
        let version = DatasetVersion { upstream_sha: "fixture-sha".into(), built_at: "2026-09-21T00:00:00Z".into() };
        ddo_etl::build::build(&fixtures(), &mut conn, &version).expect("fixture build");
        path
    })
}

fn state() -> AppState {
    AppState::open(db_path()).expect("state opens")
}

async fn get(path: &str) -> (StatusCode, axum::http::HeaderMap, Value) {
    let response = app(state()).oneshot(Request::get(path).body(Body::empty()).unwrap()).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into()))
    };
    (status, headers, json)
}

#[tokio::test]
async fn version_reports_dataset_and_schema() {
    let (status, headers, json) = get("/v1/version").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["dataset"]["upstream_sha"], "fixture-sha");
    assert_eq!(json["schema_version"], ddo_model::SCHEMA_VERSION);
    assert!(json.get("api_commit").is_some(), "version must report the API build commit, null when unknown");
    assert_eq!(json["counts"]["items"], 13);
    assert_eq!(headers.get("x-dataset-version").unwrap(), "fixture-sha");
    assert!(headers.get(header::ETAG).is_some());
    assert!(headers.get(header::CACHE_CONTROL).unwrap().to_str().unwrap().contains("max-age"));
}

#[tokio::test]
async fn items_list_filters_and_pages() {
    let (status, _, json) = get("/v1/items?q=sireth").await;
    assert_eq!(status, StatusCode::OK);
    let rows = json["items"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["name"], "Sireth, Spear of the Sky");
    assert_eq!(rows[0]["slot"], "Main Hand");
    assert_eq!(rows[0]["category"], "Weapon");
    assert_eq!(rows[0]["minimum_level"], 23);
    assert_eq!(rows[0]["is_raid"], true, "Caught in the Web is a raid");
    assert_eq!(json["total"], 1);

    let (_, _, all) = get("/v1/items?limit=5&offset=0").await;
    assert_eq!(all["items"].as_array().unwrap().len(), 5);
    assert_eq!(all["total"], 13);
    let (_, _, armor) = get("/v1/items?category=Armor").await;
    assert!(armor["items"].as_array().unwrap().iter().all(|i| i["category"] == "Armor"));
    let (_, _, ml) = get("/v1/items?min_level=20&max_level=25").await;
    assert!(ml["items"].as_array().unwrap().iter().all(|i| (20..=25).contains(&i["minimum_level"].as_i64().unwrap())));
    let (status, _, _) = get("/v1/items?category=Hat").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "unknown category is a client error");
}

#[tokio::test]
async fn item_detail_joins_every_satellite() {
    let (_, _, list) = get("/v1/items?q=sireth").await;
    let id = list["items"][0]["id"].as_i64().unwrap();
    let (status, _, json) = get(&format!("/v1/items/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["name"], "Sireth, Spear of the Sky");
    assert_eq!(json["enhancement_bonus"], 7);
    assert_eq!(json["material"], "Steel");
    assert_eq!(json["weapon"]["damage"], "3.6[1d10] + 7 Good, Magic, Pierce, Slash");
    assert_eq!(json["weapon"]["weapon_type"], "Quarterstaff");
    assert_eq!(json["weapon"]["dr_bypass"].as_array().unwrap().len(), 4);
    assert!(json["armor"].is_null());
    assert!(json["effects"].as_array().unwrap().iter().any(|e| e["name"] == "Supreme Good"));
    let slots = json["augment_slots"].as_array().unwrap();
    assert_eq!(slots.len(), 4);
    assert_eq!(slots[0]["options"][0]["name"], "Planar Conflux");
    assert_eq!(json["quests"][0]["name"], "Caught in the Web");
    assert_eq!(json["quests"][0]["loot_type"], "raid");
    assert_eq!(json["quests"][0]["difficulties"], serde_json::json!(["normal", "hard", "elite", "reaper"]));

    let (status, _, _) = get("/v1/items/999999").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (_, _, list) = get("/v1/items?q=cloak+of+winter").await;
    let id = list["items"][0]["id"].as_i64().unwrap();
    let (_, _, cloak) = get(&format!("/v1/items/{id}")).await;
    assert_eq!(cloak["bonuses"][0]["stat"], "Cold Absorption");
    assert_eq!(cloak["bonuses"][0]["value"], 34);
    assert_eq!(cloak["bonuses"][0]["bonus_type"], "Enhancement");
    assert_eq!(cloak["set"]["name"], "Eminence of Winter");
}

#[tokio::test]
async fn etag_roundtrip_returns_not_modified() {
    let (_, headers, _) = get("/v1/items?q=sireth").await;
    let etag = headers.get(header::ETAG).unwrap().clone();
    let response = app(state())
        .oneshot(
            Request::get("/v1/items?q=sireth").header(header::IF_NONE_MATCH, etag.clone()).body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);

    let head = app(state()).oneshot(Request::head("/v1/items?q=sireth").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(head.headers().get(header::ETAG).unwrap(), &etag);
}

#[tokio::test]
async fn lookups_and_augments() {
    let (_, _, stats) = get("/v1/stats").await;
    assert_eq!(stats.as_array().unwrap().len(), ddo_model::stats::STATS.len());
    let (_, _, slots) = get("/v1/augment-slot-types").await;
    assert!(slots.as_array().unwrap().iter().any(|s| s["label"] == "red"));
    let (_, _, augs) = get("/v1/augments?slot=red").await;
    let names: Vec<&str> = augs["augments"].as_array().unwrap().iter().map(|a| a["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"Ruby of Acid Damage"), "{names:?}");
    let silver = augs["augments"].as_array().unwrap().iter().find(|a| a["name"] == "Silverscale");
    assert!(silver.is_none(), "Silverscale is an Isle of Dread scale slot, not a red gem");
    let (_, _, all) = get("/v1/augments").await;
    let silver = all["augments"].as_array().unwrap().iter().find(|a| a["name"] == "Silverscale").unwrap();
    assert_eq!(silver["bonuses"][0]["stat"], "Healing Amplification");
    assert_eq!(silver["slots"][0], "isle of dread: scale (armor)");
    let id = silver["id"].as_i64().unwrap();
    let (_, _, detail) = get(&format!("/v1/augments/{id}")).await;
    assert_eq!(detail["modifiers"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn quests_carry_difficulties_and_epic_name() {
    let (_, _, quests) = get("/v1/quests").await;
    let quests = quests.as_array().unwrap();
    let find = |name: &str| quests.iter().find(|q| q["name"] == name).unwrap_or_else(|| panic!("{name}"));
    let madstone = find("Madstone Crater");
    assert_eq!(madstone["epic_name"], "Return to Madstone Crater");
    assert_eq!(madstone["difficulties"], serde_json::json!(["normal", "hard", "elite", "reaper"]));
    assert_eq!(find("The Grotto")["difficulties"], serde_json::json!(["solo"]));
    assert_eq!(find("Land of Lamordia")["difficulties"], serde_json::json!([]));
    assert!(find("The Grotto")["epic_name"].is_null());
}

#[tokio::test]
async fn sets_feats_races_classes_trees_spells() {
    let (_, _, sets) = get("/v1/sets").await;
    let winter = sets.as_array().unwrap().iter().find(|s| s["name"] == "Eminence of Winter").unwrap();
    let (_, _, set) = get(&format!("/v1/sets/{}", winter["id"])).await;
    assert!(set["tiers"].as_array().unwrap().iter().all(|t| t["equipped_count"].as_i64().unwrap() >= 2));
    assert_eq!(set["items"][0]["name"], "Legendary Cloak of Winter");
    assert_eq!(set["augments"], serde_json::json!([]));
    assert_eq!(winter["augment_count"], 0);
    let silence = sets.as_array().unwrap().iter().find(|s| s["name"] == "Perfect Silence").unwrap();
    assert_eq!((&silence["item_count"], &silence["augment_count"]), (&serde_json::json!(0), &serde_json::json!(1)));
    let (_, _, set) = get(&format!("/v1/sets/{}", silence["id"])).await;
    let augment = &set["augments"][0];
    assert_eq!(
        (&augment["name"], &augment["min_level"]),
        (&serde_json::json!("Perfect Silence"), &serde_json::json!(30))
    );
    assert!(augment["id"].is_number());

    let (_, _, feats) = get("/v1/feats?source=race").await;
    assert_eq!(feats["total"], 7);
    let (_, _, feats) = get("/v1/feats?q=power+attack").await;
    let id = feats["feats"][0]["id"].as_i64().unwrap();
    let (_, _, feat) = get(&format!("/v1/feats/{id}")).await;
    assert_eq!(feat["groups"], serde_json::json!(["Standard", "Epic Feat"]));
    assert_eq!(feat["requirements"][0]["req_type"], "Ability");
    assert_eq!(feat["stances"][0]["name"], "Power Attack");
    assert_eq!(feat["modifiers"].as_array().unwrap().len(), 3);
    assert!(feat["modifiers"][0]["requirements"].as_array().unwrap().iter().any(|r| r["req_type"] == "Stance"));

    let (_, _, races) = get("/v1/races").await;
    let dwarf = races.as_array().unwrap().iter().find(|r| r["name"] == "Dwarf").unwrap();
    let (_, _, race) = get(&format!("/v1/races/{}", dwarf["id"])).await;
    assert_eq!(race["ability_modifiers"][0]["stat"], "Constitution");
    assert_eq!(race["granted_feats"].as_array().unwrap().len(), 5);

    let (_, _, classes) = get("/v1/classes").await;
    let pal = classes.as_array().unwrap().iter().find(|c| c["name"] == "Paladin").unwrap();
    let (_, _, class) = get(&format!("/v1/classes/{}", pal["id"])).await;
    assert_eq!(class["fortitude"], "good");
    assert_eq!(class["spell_slots"]["20"], serde_json::json!([4, 4, 4, 4]));
    assert!(class["spells"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["name"] == "Cure Light Wounds" && s["spell_id"].is_number()));

    let (_, _, trees) = get("/v1/enhancement-trees").await;
    let aasimar = trees.as_array().unwrap().iter().find(|t| t["name"] == "Aasimar").unwrap();
    assert_eq!(aasimar["kind"], "racial");
    let (_, _, tree) = get(&format!("/v1/enhancement-trees/{}", aasimar["id"])).await;
    let enh = tree["enhancements"].as_array().unwrap();
    assert_eq!(enh.len(), 21);
    let chooser = enh.iter().find(|e| e["internal_name"] == "AasimarCore2").unwrap();
    assert_eq!(chooser["selections"].as_array().unwrap().len(), 3);
    assert_eq!(chooser["requirements"].as_array().unwrap().len(), 2);

    let kensei = trees.as_array().unwrap().iter().find(|t| t["name"] == "Kensei").unwrap();
    let (_, _, tree) = get(&format!("/v1/enhancement-trees/{}", kensei["id"])).await;
    let enh = tree["enhancements"].as_array().unwrap();
    let surge = enh.iter().find(|e| e["internal_name"] == "KenseiCore4").unwrap();
    assert_eq!(
        (&surge["cooldown_seconds"], &surge["duration_seconds"]),
        (&serde_json::json!(60), &serde_json::json!(60))
    );
    assert_eq!(surge["follow_on_modifiers"][1]["effect_type"], "BonusDamage");
    assert_eq!(surge["modifiers"].as_array().unwrap().len(), 3);
    let boost = enh.iter().find(|e| e["internal_name"] == "KenseiActionBoostI").unwrap();
    assert!(boost["cooldown_seconds"].is_null());
    assert_eq!(boost["follow_on_modifiers"], serde_json::json!([]));
    let haste = &boost["selections"][1];
    assert_eq!(
        (&haste["cooldown_seconds"], &haste["duration_seconds"]),
        (&serde_json::json!(30), &serde_json::json!(20))
    );
    assert_eq!(haste["follow_on_modifiers"][0]["amounts"], serde_json::json!([10, 20, 30]));

    let (_, _, spells) = get("/v1/spells?q=static").await;
    let id = spells["spells"][0]["id"].as_i64().unwrap();
    let (_, _, spell) = get(&format!("/v1/spells/{id}")).await;
    assert_eq!(spell["damage"][0]["base_dice_sides"], 6);
    assert_eq!(spell["dcs"][0]["dc_versus"], "Reflex");
    assert!(spell["metamagics"].as_array().unwrap().len() >= 7);
}

#[tokio::test]
async fn dump_and_openapi() {
    let response = app(state()).oneshot(Request::get("/v1/dump.sqlite").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "application/vnd.sqlite3");
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert!(bytes.starts_with(b"SQLite format 3\0"));

    let (status, _, spec) = get("/v1/openapi.json").await;
    assert_eq!(status, StatusCode::OK);
    let paths = spec["paths"].as_object().unwrap();
    for p in [
        "/v1/version",
        "/v1/items",
        "/v1/items/{id}",
        "/v1/feats/{id}",
        "/v1/enhancement-trees/{id}",
        "/v1/dump.sqlite",
    ] {
        assert!(paths.contains_key(p), "missing {p}");
    }
    let response = app(state()).oneshot(Request::get("/v1/docs").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn icons_are_served_when_configured() {
    let icons = std::env::temp_dir().join(format!("ddo-api-icons-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&icons);
    ddo_etl::icons::export_icons(&fixtures(), &icons).unwrap();
    let with_icons = || state().with_icons(&icons);
    let response = app(with_icons())
        .oneshot(Request::get("/icons/items/Quarterstaff_6a.png").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "image/png");
    assert!(response.headers().get(header::ETAG).is_some());
    let response =
        app(with_icons()).oneshot(Request::get("/icons/items/Nope.png").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let response = app(state())
        .oneshot(Request::get("/icons/items/Quarterstaff_6a.png").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND, "no icons directory, no route");
    let _ = std::fs::remove_dir_all(&icons);
}

#[tokio::test]
async fn openapi_describes_every_operation_parameter_and_tag() {
    let (_, _, spec) = get("/v1/openapi.json").await;
    let info = &spec["info"];
    assert!(info["description"].as_str().is_some_and(|d| d.len() > 200), "info.description is thin");

    for tag in spec["tags"].as_array().expect("tags") {
        let name = tag["name"].as_str().unwrap();
        assert!(tag["description"].as_str().is_some_and(|d| d.len() > 20), "tag {name} needs a real description");
    }

    let mut checked = 0;
    for (path, item) in spec["paths"].as_object().expect("paths") {
        for (method, op) in item.as_object().unwrap() {
            let at = format!("{} {path}", method.to_uppercase());
            let summary = op["summary"].as_str().unwrap_or("");
            assert!((8..=60).contains(&summary.len()), "{at}: summary {summary:?} must be a short title");
            assert!(!summary.ends_with('.'), "{at}: summary is a title, not a sentence");
            let description = op["description"].as_str().unwrap_or("");
            assert!(description.len() >= 60, "{at}: description {description:?} must explain the response");
            for param in op["parameters"].as_array().into_iter().flatten() {
                let name = param["name"].as_str().unwrap();
                assert!(
                    param["description"].as_str().is_some_and(|d| d.len() >= 15),
                    "{at}: parameter {name} needs a description"
                );
            }
            for (status, response) in op["responses"].as_object().unwrap() {
                assert!(
                    response["description"].as_str().is_some_and(|d| d.len() >= 10),
                    "{at}: response {status} needs a description"
                );
            }
            checked += 1;
        }
    }
    assert!(checked >= 25, "only {checked} operations in the spec");
}

fn shape(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(map.iter().map(|(k, v)| (k.clone(), shape(v))).collect()),
        Value::Array(items) => Value::Array(items.iter().take(1).map(shape).collect()),
        Value::Null => Value::Null,
        other => Value::String(
            match other {
                Value::Bool(_) => "bool",
                Value::Number(_) => "number",
                _ => "string",
            }
            .into(),
        ),
    }
}

fn assert_same_shape(at: &str, example: &Value, real: &Value) {
    match (example, real) {
        (Value::Object(e), Value::Object(r)) => {
            let ek: Vec<_> = e.keys().collect();
            let rk: Vec<_> = r.keys().collect();
            assert_eq!(ek, rk, "{at}: example keys differ from a real response");
            for (k, ev) in e {
                assert_same_shape(&format!("{at}.{k}"), ev, &r[k]);
            }
        }
        (Value::Array(e), Value::Array(r)) => {
            if let (Some(ev), Some(rv)) = (e.first(), r.first()) {
                assert_same_shape(&format!("{at}[0]"), ev, rv);
            }
        }
        (Value::Null, _) | (_, Value::Null) => {}
        (e, r) => assert_eq!(shape(e), shape(r), "{at}: example value type differs from a real response"),
    }
}

async fn sample_for(path: &str) -> Value {
    if let Some(list_path) = path.strip_suffix("/{id}") {
        let (_, _, list) = get(list_path).await;
        let rows = list
            .as_array()
            .cloned()
            .or_else(|| list.as_object().and_then(|o| o.values().find_map(|v| v.as_array().cloned())));
        let id = rows.and_then(|r| r.first().and_then(|row| row["id"].as_i64())).expect("a row to sample");
        get(&format!("{list_path}/{id}")).await.2
    } else {
        get(path).await.2
    }
}

#[tokio::test]
async fn openapi_carries_a_real_example_for_every_json_response() {
    let (_, _, spec) = get("/v1/openapi.json").await;
    let mut checked = 0;
    for (path, item) in spec["paths"].as_object().expect("paths") {
        let Some(content) = item["get"]["responses"]["200"]["content"]["application/json"].as_object() else {
            continue;
        };
        let example = content.get("example").unwrap_or(&Value::Null);
        assert!(!example.is_null(), "GET {path}: 200 response has no example");
        let real = sample_for(path).await;
        assert_same_shape(&format!("GET {path}"), example, &real);
        checked += 1;
    }
    assert!(checked >= 25, "only {checked} JSON responses carry examples");
}

#[tokio::test]
async fn each_api_version_has_its_own_docs_and_the_bare_paths_point_at_the_latest() {
    let (status, _, spec) = get("/v1/openapi.json").await;
    assert_eq!(status, StatusCode::OK);
    assert!(spec["paths"].as_object().unwrap().keys().all(|p| p.starts_with("/v1/")), "v1 spec lists other versions");
    let response = app(state()).oneshot(Request::get("/v1/docs").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    for (from, to) in [("/docs", "/v1/docs"), ("/openapi.json", "/v1/openapi.json"), ("/", "/v1/docs")] {
        let response = app(state()).oneshot(Request::get(from).body(Body::empty()).unwrap()).await.unwrap();
        assert!(response.status().is_redirection(), "{from} should redirect");
        assert_eq!(response.headers()[header::LOCATION], to, "{from} should point at the latest version");
    }
}

const LIST_ENDPOINTS: [&str; 4] = ["/v1/items", "/v1/augments", "/v1/feats", "/v1/spells"];

fn schema_type(param: &Value) -> &str {
    let ty = &param["schema"]["type"];
    ty.as_str()
        .or_else(|| ty.as_array().into_iter().flatten().filter_map(Value::as_str).find(|t| *t != "null"))
        .unwrap_or("string")
}

fn accepted_sample(name: &str, ty: &str) -> &'static str {
    match (name, ty) {
        ("category", _) => "Armor",
        ("source", _) => "standard",
        (_, "boolean") => "true",
        (_, "integer" | "number") => "1",
        _ => "x",
    }
}

#[tokio::test]
async fn list_endpoints_reject_unknown_query_parameters_as_json() {
    for path in LIST_ENDPOINTS {
        let (status, headers, json) = get(&format!("{path}?bogus=1")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}?bogus=1 must be rejected");
        assert!(
            headers.get(header::CONTENT_TYPE).is_some_and(|v| v.to_str().unwrap().starts_with("application/json")),
            "{path}?bogus=1 must answer with JSON"
        );
        let error = json["error"].as_str().unwrap_or_else(|| panic!("{path}?bogus=1 body {json} has no error string"));
        assert!(error.contains("bogus"), "{path}?bogus=1 error {error:?} must name the parameter");
    }

    let (status, _, json) = get("/v1/items?raid=maybe").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(json["error"].as_str().is_some_and(|e| e.contains("raid")), "malformed raid body {json} must be JSON");
}

#[tokio::test]
async fn list_endpoints_accept_every_documented_query_parameter() {
    let (_, _, spec) = get("/v1/openapi.json").await;
    for path in LIST_ENDPOINTS {
        let op = &spec["paths"][path]["get"];
        assert!(op["responses"]["400"]["description"].is_string(), "{path} must document its 400 response");
        let params = op["parameters"].as_array().unwrap_or_else(|| panic!("{path} documents no parameters"));
        let query_params: Vec<&Value> = params.iter().filter(|p| p["in"] == "query").collect();
        assert!(query_params.len() >= 5, "{path} documents only {} query parameters", query_params.len());
        for param in query_params {
            let name = param["name"].as_str().unwrap();
            let value = accepted_sample(name, schema_type(param));
            let (status, _, json) = get(&format!("{path}?{name}={value}")).await;
            assert_eq!(status, StatusCode::OK, "{path}?{name}={value} is documented but rejected: {json}");
        }
    }
}
