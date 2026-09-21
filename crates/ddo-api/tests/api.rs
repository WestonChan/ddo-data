//! The API over a database built from the ETL fixtures.

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

/// One fixture database per test binary, on disk so the dump endpoint has a file to serve.
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
        .oneshot(Request::get("/v1/items?q=sireth").header(header::IF_NONE_MATCH, etag.clone()).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);

    // A HEAD carries the same tag as the GET, so a client can probe cheaply.
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
async fn sets_feats_races_classes_trees_spells() {
    let (_, _, sets) = get("/v1/sets").await;
    let winter = sets.as_array().unwrap().iter().find(|s| s["name"] == "Eminence of Winter").unwrap();
    let (_, _, set) = get(&format!("/v1/sets/{}", winter["id"])).await;
    assert!(set["tiers"].as_array().unwrap().iter().all(|t| t["equipped_count"].as_i64().unwrap() >= 2));
    assert_eq!(set["items"][0]["name"], "Legendary Cloak of Winter");

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

    let (status, _, spec) = get("/openapi.json").await;
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
    let response = app(state()).oneshot(Request::get("/docs").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
