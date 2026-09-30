use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use ddo_api::{app, AppState};
use ddo_model::DatasetVersion;
use http_body_util::BodyExt;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::OnceLock;
use tower::ServiceExt;

fn fixture_data_files_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ddo-etl/tests/fixtures/DataFiles")
}

fn fixture_db_path() -> &'static PathBuf {
    static FIXTURE_DB_PATH: OnceLock<PathBuf> = OnceLock::new();
    FIXTURE_DB_PATH.get_or_init(|| {
        let path = std::env::temp_dir().join(format!("ddo-api-test-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut db = rusqlite::Connection::open(&path).unwrap();
        let dataset_version =
            DatasetVersion { upstream_sha: "fixture-sha".into(), built_at: "2026-09-21T00:00:00Z".into() };
        let wiki_overrides =
            ddo_etl::wiki::WikiOverrides::from_dir(&fixture_data_files_dir().parent().unwrap().join("wiki")).unwrap();
        ddo_etl::build::build_database(
            &fixture_data_files_dir(),
            &wiki_overrides,
            &ddo_etl::corrections::Corrections::from_dir(
                &fixture_data_files_dir().parent().unwrap().join("corrections"),
            )
            .unwrap(),
            &mut db,
            &dataset_version,
        )
        .expect("fixture build");
        path
    })
}

fn fixture_state() -> AppState {
    AppState::open(fixture_db_path()).expect("state opens")
}

async fn get(path: &str) -> (StatusCode, axum::http::HeaderMap, Value) {
    let response = app(fixture_state()).oneshot(Request::get(path).body(Body::empty()).unwrap()).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body_json = if body_bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body_bytes).unwrap_or(Value::String(String::from_utf8_lossy(&body_bytes).into()))
    };
    (status, headers, body_json)
}

#[tokio::test]
async fn version_reports_dataset_and_schema() {
    let (status, headers, json) = get("/v1/version").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["dataset"]["upstream_sha"], "fixture-sha");
    assert_eq!(json["schema_version"], ddo_model::SCHEMA_VERSION);
    assert!(json.get("api_commit").is_some(), "version must report the API build commit, null when unknown");
    assert_eq!(json["counts"]["items"], 16, "15 of Maetrim's and the wiki fixture's axe");
    assert_eq!(
        (
            &json["counts"]["crafting_systems"],
            &json["counts"]["crafting_recipes"],
            &json["counts"]["crafting_ingredients"]
        ),
        (&serde_json::json!(2), &serde_json::json!(5), &serde_json::json!(5))
    );
    assert_eq!(headers.get("x-dataset-version").unwrap(), "fixture-sha");
    assert!(headers.get(header::ETAG).is_some());
    assert!(headers.get(header::CACHE_CONTROL).unwrap().to_str().unwrap().contains("max-age"));
}

#[tokio::test]
async fn items_list_filters_and_pages() {
    let (status, _, json) = get("/v1/items?q=sireth").await;
    assert_eq!(status, StatusCode::OK);
    let sireth_matches = json["items"].as_array().unwrap();
    assert_eq!(sireth_matches.len(), 1);
    assert_eq!(sireth_matches[0]["name"], "Sireth, Spear of the Sky");
    assert_eq!(sireth_matches[0]["slot"], "Main Hand");
    assert_eq!(sireth_matches[0]["category"], "Weapon");
    assert_eq!(sireth_matches[0]["minimum_level"], 23);
    assert_eq!(sireth_matches[0]["is_raid"], true, "Caught in the Web is a raid");
    assert_eq!(json["total"], 1);

    let (_, _, first_page) = get("/v1/items?limit=5&offset=0").await;
    assert_eq!(first_page["items"].as_array().unwrap().len(), 5);
    assert_eq!(first_page["total"], 16);
    let (_, _, armor) = get("/v1/items?category=Armor").await;
    assert!(armor["items"].as_array().unwrap().iter().all(|i| i["category"] == "Armor"));
    let (_, _, level_range) = get("/v1/items?min_level=20&max_level=25").await;
    assert!(level_range["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|i| (20..=25).contains(&i["minimum_level"].as_i64().unwrap())));
    assert_eq!(sireth_matches[0]["is_rare"], false);
    let (_, _, rare) = get("/v1/items?rare=true").await;
    let rare = rare["items"].as_array().unwrap();
    assert_eq!(rare.len(), 1);
    assert_eq!(rare[0]["name"], "Buckler of the Golden Age");
    assert_eq!(rare[0]["is_rare"], true, "its drop text and the wiki both mark it a rare Book Burning drop");
    let (_, _, unfiltered) = get("/v1/items?rare=false").await;
    assert_eq!(unfiltered["total"], 16);
    let (status, _, _) = get("/v1/items?category=Hat").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "unknown category is a client error");
}

#[tokio::test]
async fn items_say_whether_maetrim_or_the_wiki_supplied_them() {
    let (_, _, axe_matches) = get("/v1/items?q=oozing").await;
    let axe_row = &axe_matches["items"][0];
    assert_eq!(axe_row["name"], "Battle Axe of the Oozing Hunger");
    assert_eq!(axe_row["source"], "wiki");
    let (_, _, sireth_matches) = get("/v1/items?q=sireth").await;
    assert_eq!(sireth_matches["items"][0]["source"], "maetrim");

    let (_, _, axe) = get(&format!("/v1/items/{}", axe_row["id"])).await;
    assert_eq!(axe["source"], "wiki");
    assert_eq!(axe["weapon"]["weapon_type"], "Battle Axe");
    let (_, _, sireth) = get(&format!("/v1/items/{}", sireth_matches["items"][0]["id"])).await;
    assert_eq!(sireth["source"], "maetrim");

    let (_, _, version) = get("/v1/version").await;
    assert_eq!(version["counts"]["wiki_items"], 1);
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
    let augment_slots = json["augment_slots"].as_array().unwrap();
    assert_eq!(augment_slots.len(), 4);
    assert_eq!(augment_slots[0]["options"][0]["name"], "Planar Conflux");
    assert_eq!(json["quests"][0]["name"], "Caught in the Web");
    assert_eq!(json["quests"][0]["loot_type"], "raid");
    assert_eq!(json["quests"][0]["difficulties"], serde_json::json!(["normal", "hard", "elite", "reaper"]));
    assert_eq!(json["quests"][0]["is_rare"], false);

    let (status, _, _) = get("/v1/items/999999").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (_, _, list) = get("/v1/items?q=cloak+of+winter").await;
    let id = list["items"][0]["id"].as_i64().unwrap();
    let (_, _, cloak) = get(&format!("/v1/items/{id}")).await;
    assert_eq!(cloak["bonuses"][0]["stat"], "Cold Absorption");
    assert_eq!(cloak["bonuses"][0]["value"], 34);
    assert_eq!(cloak["bonuses"][0]["bonus_type"], "Enhancement");
    assert_eq!(cloak["set"]["name"], "Eminence of Winter");

    let (_, _, list) = get("/v1/items?q=buckler+of+the+golden").await;
    let id = list["items"][0]["id"].as_i64().unwrap();
    let (_, _, buckler) = get(&format!("/v1/items/{id}")).await;
    assert_eq!(buckler["quests"][0]["name"], "Book Burning");
    assert_eq!(buckler["quests"][0]["loot_type"], "chest");
    assert_eq!(buckler["quests"][0]["is_rare"], true);
}

#[tokio::test]
async fn etag_roundtrip_returns_not_modified() {
    let (_, headers, _) = get("/v1/items?q=sireth").await;
    let etag = headers.get(header::ETAG).unwrap().clone();
    let response = app(fixture_state())
        .oneshot(
            Request::get("/v1/items?q=sireth").header(header::IF_NONE_MATCH, etag.clone()).body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);

    let head =
        app(fixture_state()).oneshot(Request::head("/v1/items?q=sireth").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(head.headers().get(header::ETAG).unwrap(), &etag);
}

#[tokio::test]
async fn lookups_and_augments() {
    let (_, _, stats) = get("/v1/stats").await;
    assert_eq!(stats.as_array().unwrap().len(), ddo_model::stats::STATS.len());
    let (_, _, augment_slot_types) = get("/v1/augment-slot-types").await;
    assert!(augment_slot_types.as_array().unwrap().iter().any(|s| s["label"] == "red"));
    let (_, _, red_augments) = get("/v1/augments?slot=red").await;
    let names: Vec<&str> =
        red_augments["augments"].as_array().unwrap().iter().map(|a| a["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"Ruby of Acid Damage"), "{names:?}");
    let silverscale = red_augments["augments"].as_array().unwrap().iter().find(|a| a["name"] == "Silverscale");
    assert!(silverscale.is_none(), "Silverscale is an Isle of Dread scale slot, not a red gem");
    let (_, _, all_augments) = get("/v1/augments").await;
    let silverscale = all_augments["augments"].as_array().unwrap().iter().find(|a| a["name"] == "Silverscale").unwrap();
    assert_eq!(silverscale["bonuses"][0]["stat"], "Healing Amplification");
    assert_eq!(silverscale["slots"][0], "isle of dread: scale (armor)");
    let id = silverscale["id"].as_i64().unwrap();
    let (_, _, silverscale_detail) = get(&format!("/v1/augments/{id}")).await;
    assert_eq!(silverscale_detail["modifiers"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn quests_carry_difficulties_and_epic_name() {
    let (_, _, quests) = get("/v1/quests").await;
    let quests = quests.as_array().unwrap();
    let quest_named = |name: &str| quests.iter().find(|q| q["name"] == name).unwrap_or_else(|| panic!("{name}"));
    let madstone = quest_named("Madstone Crater");
    assert_eq!(madstone["epic_name"], "Return to Madstone Crater");
    assert_eq!(madstone["difficulties"], serde_json::json!(["normal", "hard", "elite", "reaper"]));
    assert_eq!(quest_named("The Grotto")["difficulties"], serde_json::json!(["solo"]));
    assert_eq!(quest_named("Land of Lamordia")["difficulties"], serde_json::json!([]));
    assert!(quest_named("The Grotto")["epic_name"].is_null());
    assert_eq!(quest_named("The Grotto")["is_challenge"], false);
    assert!(quest_named("The Grotto")["max_level"].is_null());
    let moving = quest_named("Dr. Rushmore's Mansion - Moving Targets - EPIC");
    assert_eq!(moving["is_challenge"], true);
    assert_eq!((&moving["level"], &moving["max_level"]), (&serde_json::json!(21), &serde_json::json!(25)));
    assert_eq!(moving["patron"], "House Cannith");
    assert_eq!(moving["pack"], "Secrets of the Artificers");
}

#[tokio::test]
async fn quests_carry_the_wiki_facts() {
    let (_, _, quests) = get("/v1/quests").await;
    let quests = quests.as_array().unwrap();
    let quest_named = |name: &str| quests.iter().find(|q| q["name"] == name).unwrap_or_else(|| panic!("{name}"));
    let chronoscope = quest_named("The Chronoscope");
    assert_eq!(chronoscope["is_free_to_play"], false);
    assert_eq!(chronoscope["legendary_level"], 34);
    assert_eq!(chronoscope["zone"], "The Harbor");
    assert_eq!(chronoscope["bestowed_by"], "A harbor quest giver");
    assert_eq!(chronoscope["flagging"], "None; open to all.");
    let grotto = quest_named("The Grotto");
    assert_eq!(grotto["is_free_to_play"], true);
    assert!(grotto["legendary_level"].is_null() && grotto["zone"].is_null());
    assert_eq!(quest_named("Land of Lamordia")["is_free_to_play"], false);
    for quest in quests {
        assert!(quest.get("duration").is_none() && quest.get("xp").is_none(), "{quest}");
    }
}

#[tokio::test]
async fn item_quests_carry_free_to_play() {
    let (_, _, list) = get("/v1/items?q=buckler+of+the+golden").await;
    let id = list["items"][0]["id"].as_i64().unwrap();
    let (_, _, buckler) = get(&format!("/v1/items/{id}")).await;
    let quest = &buckler["quests"][0];
    assert_eq!(quest["name"], "Book Burning");
    assert_eq!(quest["is_free_to_play"], false);
    assert!(quest.get("duration").is_none() && quest.get("xp").is_none() && quest.get("zone").is_none(), "{quest}");
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
    let paladin = classes.as_array().unwrap().iter().find(|c| c["name"] == "Paladin").unwrap();
    let (_, _, class) = get(&format!("/v1/classes/{}", paladin["id"])).await;
    assert_eq!(class["fortitude"], "good");
    assert_eq!(class["spell_slots"]["20"], serde_json::json!([4, 4, 4, 4]));
    assert!(class["spells"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["name"] == "Cure Light Wounds" && s["spell_id"].is_number()));

    let (_, _, enhancement_trees) = get("/v1/enhancement-trees").await;
    let aasimar = enhancement_trees.as_array().unwrap().iter().find(|t| t["name"] == "Aasimar").unwrap();
    assert_eq!(aasimar["kind"], "racial");
    let (_, _, enhancement_tree) = get(&format!("/v1/enhancement-trees/{}", aasimar["id"])).await;
    let enhancements = enhancement_tree["enhancements"].as_array().unwrap();
    assert_eq!(enhancements.len(), 21);
    let selector = enhancements.iter().find(|e| e["internal_name"] == "AasimarCore2").unwrap();
    assert_eq!(selector["selections"].as_array().unwrap().len(), 3);
    assert_eq!(selector["requirements"].as_array().unwrap().len(), 2);

    let kensei = enhancement_trees.as_array().unwrap().iter().find(|t| t["name"] == "Kensei").unwrap();
    let (_, _, enhancement_tree) = get(&format!("/v1/enhancement-trees/{}", kensei["id"])).await;
    let enhancements = enhancement_tree["enhancements"].as_array().unwrap();
    let surge = enhancements.iter().find(|e| e["internal_name"] == "KenseiCore4").unwrap();
    assert_eq!(
        (&surge["cooldown_seconds"], &surge["duration_seconds"]),
        (&serde_json::json!(60), &serde_json::json!(60))
    );
    assert_eq!(surge["follow_on_modifiers"][1]["effect_type"], "BonusDamage");
    assert_eq!(surge["modifiers"].as_array().unwrap().len(), 3);
    let boost = enhancements.iter().find(|e| e["internal_name"] == "KenseiActionBoostI").unwrap();
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
    let response =
        app(fixture_state()).oneshot(Request::get("/v1/dump.sqlite").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "application/vnd.sqlite3");
    let dump_bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert!(dump_bytes.starts_with(b"SQLite format 3\0"));

    let (status, _, spec) = get("/v1/openapi.json").await;
    assert_eq!(status, StatusCode::OK);
    let paths = spec["paths"].as_object().unwrap();
    for path in [
        "/v1/version",
        "/v1/items",
        "/v1/items/{id}",
        "/v1/feats/{id}",
        "/v1/enhancement-trees/{id}",
        "/v1/dump.sqlite",
    ] {
        assert!(paths.contains_key(path), "missing {path}");
    }
    let response = app(fixture_state()).oneshot(Request::get("/v1/docs").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn icons_are_served_when_configured() {
    let icons_dir = std::env::temp_dir().join(format!("ddo-api-icons-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&icons_dir);
    ddo_etl::icons::export_icons(&fixture_data_files_dir(), &icons_dir).unwrap();
    let fixture_state_with_icons = || fixture_state().with_icons_dir(&icons_dir);
    let response = app(fixture_state_with_icons())
        .oneshot(Request::get("/icons/items/Quarterstaff_6a.png").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "image/png");
    assert!(response.headers().get(header::ETAG).is_some());
    let response = app(fixture_state_with_icons())
        .oneshot(Request::get("/icons/items/Nope.png").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let response = app(fixture_state())
        .oneshot(Request::get("/icons/items/Quarterstaff_6a.png").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND, "no icons directory, no route");
    let _ = std::fs::remove_dir_all(&icons_dir);
}

#[tokio::test]
async fn openapi_describes_every_operation_parameter_and_tag() {
    let (_, _, spec) = get("/v1/openapi.json").await;
    let spec_info = &spec["info"];
    assert!(spec_info["description"].as_str().is_some_and(|d| d.len() > 200), "info.description is thin");

    for tag in spec["tags"].as_array().expect("tags") {
        let name = tag["name"].as_str().unwrap();
        assert!(tag["description"].as_str().is_some_and(|d| d.len() > 20), "tag {name} needs a real description");
    }

    let mut checked_operation_count = 0;
    for (path, path_item) in spec["paths"].as_object().expect("paths") {
        for (method, operation) in path_item.as_object().unwrap() {
            let operation_label = format!("{} {path}", method.to_uppercase());
            let summary = operation["summary"].as_str().unwrap_or("");
            assert!((8..=60).contains(&summary.len()), "{operation_label}: summary {summary:?} must be a short title");
            assert!(!summary.ends_with('.'), "{operation_label}: summary is a title, not a sentence");
            let description = operation["description"].as_str().unwrap_or("");
            assert!(
                description.len() >= 60,
                "{operation_label}: description {description:?} must explain the response"
            );
            for param in operation["parameters"].as_array().into_iter().flatten() {
                let name = param["name"].as_str().unwrap();
                assert!(
                    param["description"].as_str().is_some_and(|d| d.len() >= 15),
                    "{operation_label}: parameter {name} needs a description"
                );
            }
            for (status, response) in operation["responses"].as_object().unwrap() {
                assert!(
                    response["description"].as_str().is_some_and(|d| d.len() >= 10),
                    "{operation_label}: response {status} needs a description"
                );
            }
            checked_operation_count += 1;
        }
    }
    assert!(checked_operation_count >= 25, "only {checked_operation_count} operations in the spec");
}

fn shape_of(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(map.iter().map(|(k, v)| (k.clone(), shape_of(v))).collect()),
        Value::Array(items) => Value::Array(items.iter().take(1).map(shape_of).collect()),
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

fn assert_same_shape(location: &str, example: &Value, real: &Value) {
    match (example, real) {
        (Value::Object(example_object), Value::Object(real_object)) => {
            let example_keys: Vec<_> = example_object.keys().collect();
            let real_keys: Vec<_> = real_object.keys().collect();
            assert_eq!(example_keys, real_keys, "{location}: example keys differ from a real response");
            for (key, example_value) in example_object {
                assert_same_shape(&format!("{location}.{key}"), example_value, &real_object[key]);
            }
        }
        (Value::Array(example_items), Value::Array(real_items)) => {
            if let (Some(example_value), Some(real_value)) = (example_items.first(), real_items.first()) {
                assert_same_shape(&format!("{location}[0]"), example_value, real_value);
            }
        }
        (Value::Null, _) | (_, Value::Null) => {}
        (example, real) => {
            assert_eq!(shape_of(example), shape_of(real), "{location}: example value type differs from a real response")
        }
    }
}

async fn sample_response_for(path: &str) -> Value {
    if let Some(list_path) = path.strip_suffix("/{id}") {
        let (_, _, list_response) = get(list_path).await;
        let list_rows = list_response
            .as_array()
            .cloned()
            .or_else(|| list_response.as_object().and_then(|o| o.values().find_map(|v| v.as_array().cloned())));
        let id = list_rows.and_then(|r| r.first().and_then(|row| row["id"].as_i64())).expect("a row to sample");
        get(&format!("{list_path}/{id}")).await.2
    } else {
        get(path).await.2
    }
}

#[tokio::test]
async fn openapi_carries_a_real_example_for_every_json_response() {
    let (_, _, spec) = get("/v1/openapi.json").await;
    let mut checked_response_count = 0;
    for (path, path_item) in spec["paths"].as_object().expect("paths") {
        let Some(content) = path_item["get"]["responses"]["200"]["content"]["application/json"].as_object() else {
            continue;
        };
        let example = content.get("example").unwrap_or(&Value::Null);
        assert!(!example.is_null(), "GET {path}: 200 response has no example");
        let real_response = sample_response_for(path).await;
        assert_same_shape(&format!("GET {path}"), example, &real_response);
        checked_response_count += 1;
    }
    assert!(checked_response_count >= 25, "only {checked_response_count} JSON responses carry examples");
}

#[tokio::test]
async fn each_api_version_has_its_own_docs_and_the_bare_paths_point_at_the_latest() {
    let (status, _, spec) = get("/v1/openapi.json").await;
    assert_eq!(status, StatusCode::OK);
    assert!(spec["paths"].as_object().unwrap().keys().all(|p| p.starts_with("/v1/")), "v1 spec lists other versions");
    let response = app(fixture_state()).oneshot(Request::get("/v1/docs").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    for (from, to) in [("/docs", "/v1/docs"), ("/openapi.json", "/v1/openapi.json"), ("/", "/v1/docs")] {
        let response = app(fixture_state()).oneshot(Request::get(from).body(Body::empty()).unwrap()).await.unwrap();
        assert!(response.status().is_redirection(), "{from} should redirect");
        assert_eq!(response.headers()[header::LOCATION], to, "{from} should point at the latest version");
    }
}

const FILTERED_LIST_PATHS: [&str; 4] = ["/v1/items", "/v1/augments", "/v1/feats", "/v1/spells"];

fn param_schema_type(param: &Value) -> &str {
    let schema_type = &param["schema"]["type"];
    schema_type
        .as_str()
        .or_else(|| schema_type.as_array().into_iter().flatten().filter_map(Value::as_str).find(|t| *t != "null"))
        .unwrap_or("string")
}

fn accepted_sample_value(param_name: &str, schema_type: &str) -> &'static str {
    match (param_name, schema_type) {
        ("category", _) => "Armor",
        ("source", _) => "standard",
        (_, "boolean") => "true",
        (_, "integer" | "number") => "1",
        _ => "x",
    }
}

#[tokio::test]
async fn list_endpoints_reject_unknown_query_parameters_as_json() {
    for path in FILTERED_LIST_PATHS {
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
    for path in FILTERED_LIST_PATHS {
        let operation = &spec["paths"][path]["get"];
        assert!(operation["responses"]["400"]["description"].is_string(), "{path} must document its 400 response");
        let documented_params =
            operation["parameters"].as_array().unwrap_or_else(|| panic!("{path} documents no parameters"));
        let query_params: Vec<&Value> = documented_params.iter().filter(|p| p["in"] == "query").collect();
        assert!(query_params.len() >= 5, "{path} documents only {} query parameters", query_params.len());
        for param in query_params {
            let name = param["name"].as_str().unwrap();
            let sample_value = accepted_sample_value(name, param_schema_type(param));
            let (status, _, json) = get(&format!("{path}?{name}={sample_value}")).await;
            assert_eq!(status, StatusCode::OK, "{path}?{name}={sample_value} is documented but rejected: {json}");
        }
    }
}

#[tokio::test]
async fn stances_lists_the_standalone_stances_in_file_order() {
    let (status, _, json) = get("/v1/stances").await;
    assert_eq!(status, StatusCode::OK);
    let stances = json.as_array().unwrap();
    let names: Vec<&str> = stances.iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Two Weapon Fighting", "Two Handed Fighting", "Aura of Good"]);
    assert_eq!(stances[0]["auto_controlled"], true);
    assert_eq!(stances[0]["group_name"], "Auto");
    assert_eq!(stances[0]["requirements"].as_array().unwrap().len(), 10);
    assert_eq!(stances[2]["requirements"][0]["value"], "1");
    assert_eq!(stances[0]["modifiers"], serde_json::json!([]));
}

#[tokio::test]
async fn guild_buffs_carry_their_unlock_level_and_per_level_modifiers() {
    let (status, _, json) = get("/v1/guild-buffs").await;
    assert_eq!(status, StatusCode::OK);
    let guild_buffs = json.as_array().unwrap();
    let guild_levels: Vec<i64> = guild_buffs.iter().map(|b| b["guild_level"].as_i64().unwrap()).collect();
    assert_eq!(guild_levels, [10, 17, 21]);
    let flame = &guild_buffs[0];
    assert_eq!(flame["name"], "Sign of the Silver Flame I");
    assert_eq!(flame["modifiers"][0]["amount_type"], "TotalLevel");
    assert_eq!(flame["modifiers"][0]["bonus_type"], "Guild");
    assert_eq!(flame["modifiers"][0]["amounts"].as_array().unwrap().len(), 40);
}

#[tokio::test]
async fn optional_buffs_list_by_name_with_modifiers() {
    let (status, _, json) = get("/v1/optional-buffs").await;
    assert_eq!(status, StatusCode::OK);
    let optional_buffs = json.as_array().unwrap();
    let names: Vec<&str> = optional_buffs.iter().map(|b| b["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Barkskin", "Bless", "Deadly Weapons", "Stone of Change: Alchemical Shield Eldritch Ritual"]);
    assert_eq!(optional_buffs[0]["icon"], "Barkskin");
    assert_eq!(optional_buffs[0]["modifiers"][0]["effect_type"], "NaturalArmor");
    assert_eq!(optional_buffs[3]["modifiers"][0]["requirements"][0]["req_type"], "Stance");
}

#[tokio::test]
async fn sentient_gems_list_by_name() {
    let (status, _, json) = get("/v1/sentient-gems").await;
    assert_eq!(status, StatusCode::OK);
    let sentient_gems = json.as_array().unwrap();
    let names: Vec<&str> = sentient_gems.iter().map(|g| g["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        ["Sentient Jewel of the Hopeful", "Sentient Jewel of the Inquisitive", "Sentient Jewel of the Resolute"]
    );
    assert_eq!(sentient_gems[2]["icon"], "SentientJewel_Blue");
    assert_eq!(sentient_gems[2]["description"], "Voiced by: Ally Murphy");
}

#[tokio::test]
async fn feat_attacks_carry_cooldown_and_their_bonuses() {
    let (_, _, feats) = get("/v1/feats?q=improved+feint").await;
    let id = feats["feats"][0]["id"].as_i64().unwrap();
    let (_, _, feat) = get(&format!("/v1/feats/{id}")).await;
    assert_eq!(feat["attack"]["cooldown_seconds"], 6);
    assert_eq!(feat["attack"]["duration_seconds"], 4);
    assert_eq!(feat["follow_on_modifiers"][0]["effect_type"], "AllowSneakAttack");
    assert_eq!(feat["this_attack_modifiers"][0]["effect_type"], "BonusDamagePercent");
    assert_eq!(feat["this_attack_modifiers"][0]["amounts"], serde_json::json!([20]));

    let (_, _, enhancement_trees) = get("/v1/enhancement-trees").await;
    let kensei = enhancement_trees.as_array().unwrap().iter().find(|t| t["name"] == "Kensei").unwrap();
    let (_, _, enhancement_tree) = get(&format!("/v1/enhancement-trees/{}", kensei["id"])).await;
    let enhancements = enhancement_tree["enhancements"].as_array().unwrap();
    let reed = enhancements.iter().find(|e| e["internal_name"] == "KenseiReedInTheWind").unwrap();
    assert_eq!(reed["this_attack_modifiers"][0]["amounts"], serde_json::json!([20, 40, 60]));
    assert_eq!(reed["attack"]["cooldown_seconds"], 8);
    let shattering = enhancements.iter().find(|e| e["internal_name"] == "KenseiShatteringStrike").unwrap();
    assert_eq!(shattering["selections"][1]["this_attack_modifiers"][0]["effect_type"], "BonusDamagePercent");
}

#[tokio::test]
async fn crafting_systems_list_their_families_and_counts() {
    let (status, _, json) = get("/v1/crafting-systems").await;
    assert_eq!(status, StatusCode::OK);
    let crafting_systems = json.as_array().unwrap();
    assert_eq!(crafting_systems.len(), 2);
    let crafting_system = &crafting_systems[0];
    assert!(crafting_system["id"].is_number());
    assert_eq!(crafting_system["name"], "Heroic Green Steel");
    assert_eq!(crafting_system["page"], "https://ddowiki.com/page/Green_Steel_items");
    assert!(crafting_system["pack"].is_null());
    assert_eq!(crafting_system["npc"], "Altar of Invasion");
    assert_eq!(crafting_system["families"], serde_json::json!(["Greensteel_Heroic"]));
    assert_eq!(
        (&crafting_system["ingredient_count"], &crafting_system["recipe_count"]),
        (&serde_json::json!(4), &serde_json::json!(3))
    );
    let upgrade_system = &crafting_systems[1];
    assert_eq!(upgrade_system["name"], "Test Upgrade Altar");
    assert_eq!(upgrade_system["families"], serde_json::json!([]));
}

#[tokio::test]
async fn crafting_system_detail_carries_ingredients_and_recipes_with_augments_and_cost() {
    let (_, _, list) = get("/v1/crafting-systems").await;
    let id = list[0]["id"].as_i64().unwrap();
    let (status, _, crafting_system) = get(&format!("/v1/crafting-systems/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(crafting_system["name"], "Heroic Green Steel");
    assert_eq!(crafting_system["families"], serde_json::json!(["Greensteel_Heroic"]));
    let ingredients = crafting_system["ingredients"].as_array().unwrap();
    assert_eq!(ingredients.len(), 4);
    assert_eq!(ingredients[0]["name"], "Small Shard of Power");
    assert_eq!(ingredients[0]["tier"], "heroic");
    assert_eq!(ingredients[0]["bind"], "Bound to Account");
    assert_eq!(ingredients[0]["source"], "The Shroud");
    let recipes = crafting_system["recipes"].as_array().unwrap();
    let recipe_options: Vec<&str> = recipes.iter().map(|r| r["option"].as_str().unwrap()).collect();
    assert_eq!(recipe_options, ["+5 Fortitude Save", "Minor Fire Guard", "Cleanse an item"]);
    let fortitude = &recipes[0];
    assert_eq!(fortitude["tier"], "heroic");
    assert_eq!(fortitude["slot"], "crafting: accessory invasion");
    assert!(fortitude["note"].is_null());
    assert!(recipes.iter().all(|r| r.get("grants_slot").is_some_and(Value::is_null)), "{recipes:?}");
    let augments = fortitude["augments"].as_array().unwrap();
    assert_eq!(augments.len(), 2, "both +5 Fortitude Save augments in the family");
    assert!(augments.iter().all(|a| a["name"] == "+5 Fortitude Save" && a["min_level"] == 11 && a["id"].is_number()));
    assert_eq!(
        fortitude["cost"],
        serde_json::json!([
            { "ingredient": "Small Shard of Power", "tier": "heroic", "quantity": 1 },
            { "ingredient": "Small Focus of Earth", "tier": "heroic", "quantity": 1 }
        ])
    );
    let cleanse = &recipes[2];
    assert!(cleanse["slot"].is_null());
    assert_eq!(cleanse["augments"], serde_json::json!([]));
    assert_eq!(cleanse["note"], "Returns a crafted item to its blank state; no augment counterpart.");
    assert_eq!(cleanse["cost"][0]["tier"], "any");

    let upgrade_id = list[1]["id"].as_i64().unwrap();
    let (_, _, upgrade_system) = get(&format!("/v1/crafting-systems/{upgrade_id}")).await;
    let upgrade_recipes = upgrade_system["recipes"].as_array().unwrap();
    let granted_slots: Vec<&Value> = upgrade_recipes.iter().map(|r| &r["grants_slot"]).collect();
    assert_eq!(granted_slots, [&serde_json::json!("upgrade: tier 2"), &serde_json::json!("colorless")]);
    assert!(upgrade_recipes.iter().all(|r| r["augments"] == serde_json::json!([]) && r["slot"].is_null()));

    let (status, _, _) = get("/v1/crafting-systems/9999").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn augment_detail_lists_the_crafting_recipes_that_yield_it() {
    let (_, _, found) = get("/v1/augments?q=minor+fire+guard").await;
    let id = found["augments"][0]["id"].as_i64().unwrap();
    let (_, _, augment) = get(&format!("/v1/augments/{id}")).await;
    assert_eq!(
        augment["crafting"],
        serde_json::json!([{
            "system": "Heroic Green Steel",
            "tier": "heroic",
            "option": "Minor Fire Guard",
            "cost": [
                { "ingredient": "Small Shard of Power", "tier": "heroic", "quantity": 1 },
                { "ingredient": "Small Focus of Fire", "tier": "heroic", "quantity": 1 }
            ]
        }])
    );
    let (_, _, ruby) = get("/v1/augments?q=ruby+of+acid").await;
    let id = ruby["augments"][0]["id"].as_i64().unwrap();
    let (_, _, augment) = get(&format!("/v1/augments/{id}")).await;
    assert_eq!(augment["crafting"], serde_json::json!([]));
}

#[tokio::test]
async fn augment_list_rows_carry_the_crafting_recipes_that_yield_them() {
    let (status, _, found) = get("/v1/augments?q=fortitude+save&family=Greensteel_Heroic").await;
    assert_eq!(status, StatusCode::OK);
    let augments = found["augments"].as_array().unwrap();
    assert_eq!(augments.len(), 2, "{augments:?}");
    for augment in augments {
        assert_eq!(
            augment["crafting"],
            serde_json::json!([{
                "system": "Heroic Green Steel",
                "tier": "heroic",
                "option": "+5 Fortitude Save",
                "cost": [
                    { "ingredient": "Small Shard of Power", "tier": "heroic", "quantity": 1 },
                    { "ingredient": "Small Focus of Earth", "tier": "heroic", "quantity": 1 }
                ]
            }])
        );
    }
    let (_, _, ruby) = get("/v1/augments?q=ruby+of+acid").await;
    assert_eq!(ruby["augments"][0]["crafting"], serde_json::json!([]));
}

async fn id_of_item_named(item_name: &str) -> i64 {
    let (_, _, items) = get(&format!("/v1/items?q={}", item_name.replace(' ', "+"))).await;
    items["items"].as_array().unwrap().iter().find(|item| item["name"] == item_name).unwrap()["id"].as_i64().unwrap()
}

#[tokio::test]
async fn items_augments_and_quests_report_the_corrections_applied_to_them() {
    let (_, _, docent) = get(&format!("/v1/items/{}", id_of_item_named("Docent of Defiance").await)).await;
    assert_eq!(docent["minimum_level"], 11);
    assert_eq!(
        docent["corrections"],
        serde_json::json!([{
            "field": "minimum_level",
            "from": 10,
            "to": 11,
            "reason": "Test correction of an item's level.",
            "source": "https://ddowiki.com/page/Item:Docent_of_Defiance"
        }])
    );
    let (_, _, crossbow) =
        get(&format!("/v1/items/{}", id_of_item_named("+1 Ember Repeating Light Crossbow").await)).await;
    assert_eq!(crossbow["corrections"], serde_json::json!([]));

    let (_, _, augments) = get("/v1/augments?q=voidscale").await;
    let voidscale_id = augments["augments"][0]["id"].as_i64().unwrap();
    let (_, _, voidscale) = get(&format!("/v1/augments/{voidscale_id}")).await;
    assert_eq!(voidscale["min_level"], 30);
    assert_eq!(voidscale["corrections"][0]["field"], "min_level");
    assert_eq!(
        (&voidscale["corrections"][0]["from"], &voidscale["corrections"][0]["to"]),
        (&serde_json::json!(31), &serde_json::json!(30))
    );

    let (_, _, quests) = get("/v1/quests").await;
    let quests = quests.as_array().unwrap();
    let plane_of_night = quests.iter().find(|quest| quest["name"] == "Plane of Night").unwrap();
    assert_eq!(plane_of_night["favor"], 6);
    assert_eq!(plane_of_night["corrections"][0]["field"], "favor");
    assert_eq!(plane_of_night["corrections"][0]["reason"], "Test correction of a quest's favor.");
    assert!(quests
        .iter()
        .filter(|quest| quest["name"] != "Plane of Night")
        .all(|quest| quest["corrections"] == serde_json::json!([])));

    let (_, _, version) = get("/v1/version").await;
    assert_eq!(version["counts"]["corrections"], 3);
}
