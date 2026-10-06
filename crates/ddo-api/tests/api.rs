use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use ddo_api::drop_validation::{detail_has_unique_drop_locations, source_pack_names};
use ddo_api::schema_validation::response_matches_schema;
use ddo_api::{app, AppState};
use ddo_model::DatasetVersion;
use http_body_util::BodyExt;
use rusqlite::trace::{TraceEvent, TraceEventCodes};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;
use tower::ServiceExt;

fn fixture_data_files_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ddo-etl/tests/fixtures/DataFiles")
}

fn fixture_wiki() -> ddo_etl::wiki::WikiOverrides {
    ddo_etl::wiki::WikiOverrides::from_dir(&fixture_data_files_dir().parent().unwrap().join("wiki")).unwrap()
}

fn build_fixture_database(db: &mut rusqlite::Connection, wiki: &ddo_etl::wiki::WikiOverrides) {
    let dataset_version =
        DatasetVersion { upstream_sha: "fixture-sha".into(), built_at: "2026-09-21T00:00:00Z".into() };
    ddo_etl::build::build_database(
        &fixture_data_files_dir(),
        wiki,
        &ddo_etl::corrections::Corrections::from_dir(&fixture_data_files_dir().parent().unwrap().join("corrections"))
            .unwrap(),
        db,
        &dataset_version,
    )
    .expect("fixture build");
}

fn fixture_db_path() -> &'static PathBuf {
    static FIXTURE_DB_PATH: OnceLock<PathBuf> = OnceLock::new();
    FIXTURE_DB_PATH.get_or_init(|| {
        let path = std::env::temp_dir().join(format!("ddo-api-test-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut db = rusqlite::Connection::open(&path).unwrap();
        build_fixture_database(&mut db, &fixture_wiki());
        path
    })
}

fn fixture_state() -> AppState {
    AppState::open(fixture_db_path()).expect("state opens")
}

fn fixture_state_with_wiki(wiki: &ddo_etl::wiki::WikiOverrides, test_name: &str) -> AppState {
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    build_fixture_database(&mut db, wiki);
    let path = std::env::temp_dir().join(format!("ddo-api-{test_name}-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    db.execute("VACUUM INTO ?1", [path.to_str().unwrap()]).unwrap();
    AppState::open(&path).unwrap()
}

async fn get(path: &str) -> (StatusCode, axum::http::HeaderMap, Value) {
    get_from(fixture_state(), path).await
}

async fn get_from(state: AppState, path: &str) -> (StatusCode, axum::http::HeaderMap, Value) {
    let response = app(state).oneshot(Request::get(path).body(Body::empty()).unwrap()).await.unwrap();
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

async fn get_list_rows(path: &str) -> (StatusCode, axum::http::HeaderMap, Value) {
    let separator = if path.contains('?') { "&" } else { "?" };
    let (status, headers, page) = get(&format!("{path}{separator}limit=10000")).await;
    let rows_key = path.split('?').next().unwrap().trim_start_matches("/v1/").replace('-', "_");
    (status, headers, page[&rows_key].clone())
}

#[tokio::test]
async fn detail_effects_render_family_stats_and_text_only_lines() {
    let item = item_detail_named("oozing").await;
    let lines = item["effects"].as_array().expect("item effects");
    assert!(item.get("bonuses").is_none());
    assert!(lines.iter().all(|line| {
        [
            "effect_id",
            "name",
            "tier",
            "verbose_name",
            "description",
            "value",
            "value2",
            "bonus_type",
            "bonuses",
            "damage",
        ]
        .iter()
        .all(|key| line.get(*key).is_some())
    }));
    assert!(lines.iter().any(|line| line["bonuses"].as_array().is_some_and(|rows| !rows.is_empty())));
    assert!(lines.iter().any(|line| line["bonuses"] == serde_json::json!([])));

    let rune_arm = item_detail_named("echoes%20of%20night").await;
    let charge_rate =
        rune_arm["effects"].as_array().unwrap().iter().find(|line| line["name"] == "Rune Arm Charge Rate").unwrap();
    assert_eq!(charge_rate["bonus_type"], "Enhancement");

    let (_, _, augment_list) = get("/v1/augments?q=Silverscale").await;
    let augment_id = augment_list["augments"][0]["id"].as_i64().unwrap();
    let (_, _, augment) = get(&format!("/v1/augments/{augment_id}")).await;
    assert!(augment.get("bonuses").is_none());
    assert!(augment["effects"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|line| line["bonuses"].as_array().unwrap())
        .any(|bonus| bonus["stat"] == "Healing Amplification"));

    let (_, _, set) = get("/v1/sets/6").await;
    for tier in set["tiers"].as_array().unwrap() {
        assert!(tier.get("description").is_none());
        assert!(tier["effects"].is_array());
        assert!(tier.get("bonuses").is_none());
    }
}

#[tokio::test]
async fn command_expands_charisma_skills_and_group_filters_find_its_item() {
    let plate = item_detail_named("grudgebearer").await;
    let command = plate["effects"].as_array().unwrap().iter().find(|line| line["name"] == "Command").unwrap();
    let bonuses = command["bonuses"].as_array().unwrap();
    let grouped: Vec<_> = bonuses.iter().filter(|bonus| bonus["group"]["name"] == "Charisma Skills").collect();
    assert_eq!(grouped.len(), 6);
    assert!(grouped.iter().all(|bonus| bonus["value"] == command["value"] && bonus["bonus_type"] == "Insight"));
    assert!(bonuses.iter().any(|bonus| bonus["stat"] == "Hide" && bonus["bonus_type"] == "Penalty"));
    let (_, _, vocabulary) = get_list_rows("/v1/effects?kind=group").await;
    let group = effect_named(&vocabulary, "Charisma Skills", "group").unwrap();
    let (_, _, detail) = get(group["detail_path"].as_str().unwrap()).await;
    assert_eq!(detail["bonuses"].as_array().unwrap().len(), 6);
    for filter in ["Bluff", "Charisma%20Skills"] {
        let (_, _, page) = get(&format!("/v1/items?bonus={filter}")).await;
        assert!(item_names(&page).contains(&"Grudgebearer's Plate"), "{filter}");
    }
}

#[tokio::test]
async fn direct_skill_group_links_render_the_group_description() {
    let item = item_detail_named("lindal").await;
    let line = item["effects"].as_array().unwrap().iter().find(|line| line["name"] == "Charisma Skills").unwrap();
    assert_eq!(line["description"], "+4 Insight bonus to all Charisma based skills.");
    assert_eq!(line["name"], "Charisma Skills");
    assert_eq!(line["verbose_name"], "Insightful Charisma Skills +4");
    assert!(line.get("text").is_none());
}

#[tokio::test]
async fn school_focus_uses_the_game_name_in_lines_and_filter_vocabulary() {
    let visor = item_detail_named("Visor%20of%20Fraz-Urb%27luu").await;
    assert!(visor["effects"].as_array().unwrap().iter().any(|line| line["name"] == "Illusion Focus"));
    let (_, _, rows) = get("/v1/effects?q=Illusion%20Focus").await;
    assert!(rows["effects"].as_array().unwrap().iter().any(|row| row["name"] == "Illusion Focus"));
    let (_, _, items) = get("/v1/items?bonus=Illusion%20Focus&limit=10000").await;
    assert!(item_names(&items).contains(&"Visor of Fraz-Urb'luu"));
    let (status, _, error) = get("/v1/items?bonus=Illusion%20Spell%20Focus").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
}

#[tokio::test]
async fn effect_lines_separate_labels_from_rendered_names() {
    let bracers = item_detail_named("Epic%20Ethereal%20Bracers").await;
    let lines = bracers["effects"].as_array().unwrap();
    let dexterity = lines.iter().find(|line| line["name"] == "Dexterity").unwrap();
    assert_eq!(dexterity["verbose_name"], "Dexterity +11");
    assert_eq!(dexterity["bonus_type"], "Enhancement");
    let riposte = lines.iter().find(|line| line["name"] == "Riposte").unwrap();
    assert_eq!(riposte["verbose_name"], "Riposte +5");
    assert_eq!(riposte["value"], 5);
    let speed = lines.iter().find(|line| line["name"] == "Speed XIV").unwrap();
    assert_eq!(speed["value"], 30);
    assert_eq!(speed["value2"], 14);
    assert!(lines.iter().all(|line| line.get("text").is_none()));

    let rune_arm = item_detail_named("Echoes%20of%20Night").await;
    let lore = rune_arm["effects"].as_array().unwrap().iter().find(|line| line["name"] == "Sonic Lore").unwrap();
    assert_eq!(lore["verbose_name"], "Sonic Lore +8%");
    assert_eq!(lore["bonus_type"], "Equipment");
}

#[tokio::test]
async fn fixture_tooltip_templates_render_signed_units_types_groups_and_tiers() {
    let cases = [
        ("Epic%20Ethereal%20Bracers", "Dexterity", "Dexterity +11"),
        ("Epic%20Ethereal%20Bracers", "Riposte", "Riposte +5"),
        ("Epic%20Ethereal%20Bracers", "Speed XIV", "Speed XIV"),
        ("Echoes%20of%20Night", "Sonic Lore", "Sonic Lore +8%"),
        ("Alaric%27s%20Grim%20Gauntlets", "Wisdom", "Wisdom +14"),
        ("Alaric%27s%20Grim%20Gauntlets", "Dark Restoration Lore", "Dark Restoration Lore +23%"),
        ("Alaric%27s%20Grim%20Gauntlets", "Parrying", "Parrying +5"),
        ("Kardin%27s%20Eye", "Heightened Awareness", "Heightened Awareness 6"),
        ("Lindal%27s%20Mighty%20Belt", "Charisma Skills", "Insightful Charisma Skills +4"),
        ("Lindal%27s%20Mighty%20Belt", "Unwieldy", "Unwieldy"),
        ("Epic%20Ring%20of%20the%20Stalker", "Deception", "Deception +3"),
        ("Alarphon%27s%20Staff", "Spell Lore", "Spell Lore +6%"),
    ];
    for (item_search, effect_name, expected) in cases {
        let item = item_detail_named(item_search).await;
        let line = item["effects"].as_array().unwrap().iter().find(|line| line["name"] == effect_name).unwrap();
        assert_eq!(line["verbose_name"], expected, "{item_search} {effect_name}");
    }
    let belt = item_detail_named("Lindal%27s%20Mighty%20Belt").await;
    let unwieldy = belt["effects"].as_array().unwrap().iter().find(|line| line["name"] == "Unwieldy").unwrap();
    assert_eq!(unwieldy["description"], "-2 Dexterity");
    assert_eq!(unwieldy["value"], -2);
    let invisibility = belt["effects"].as_array().unwrap().iter().find(|line| line["name"] == "Invisibility").unwrap();
    assert_eq!(invisibility["verbose_name"], "Invisibility");
    assert_eq!(invisibility["value"], 2);
    let bracers = item_detail_named("Epic%20Ethereal%20Bracers").await;
    let speed = bracers["effects"].as_array().unwrap().iter().find(|line| line["name"] == "Speed XIV").unwrap();
    assert_eq!((speed["value"].as_i64(), speed["value2"].as_i64()), (Some(30), Some(14)));
    let (_, _, sets) = get("/v1/sets?q=Cooking%20By%20the%20Book").await;
    let set_id = sets["sets"][0]["id"].as_i64().unwrap();
    let (_, _, set) = get(&format!("/v1/sets/{set_id}")).await;
    let tier_line = &set["tiers"][0]["effects"][0];
    assert_eq!(tier_line["name"], "Universal Spell Power");
    assert_eq!(tier_line["verbose_name"], "+20 Artifact Bonus to Universal Spell Power");
    let (_, _, winter) = get("/v1/sets?q=Eminence%20of%20Winter").await;
    let winter_id = winter["sets"][0]["id"].as_i64().unwrap();
    let (_, _, winter) = get(&format!("/v1/sets/{winter_id}")).await;
    let armor = winter["tiers"].as_array().unwrap().iter().find(|tier| tier["equipped_count"] == 7).unwrap();
    assert_eq!(armor["effects"][0]["verbose_name"], "+15% Artifact Bonus to Armor Class");
}

#[tokio::test]
async fn vocabulary_rows_route_by_kind_even_when_database_ids_overlap() {
    let (_, _, page) = get("/v1/effects?limit=10000").await;
    let rows = page["effects"].as_array().unwrap();
    let family = rows.iter().find(|row| row["kind"] == "effect").unwrap();
    let stat = rows.iter().find(|row| row["kind"] == "stat").unwrap();
    for row in [family, stat] {
        assert_eq!(row["detail_path"], format!("/v1/effects/{}", row["id"]));
        assert!(row["bonus_types"].is_array());
        let (status, _, detail) = get(row["detail_path"].as_str().unwrap()).await;
        assert_eq!(status, StatusCode::OK, "{detail}");
    }
}

#[tokio::test]
async fn repeated_item_vocabulary_filters_support_any_and_all() {
    let (_, _, feet) = get("/v1/items?slot=Feet").await;
    let (_, _, hands) = get("/v1/items?slot=Main%20Hand").await;
    let (_, _, either) = get("/v1/items?slot=Feet&slot=Main%20Hand").await;
    assert_eq!(either["total"].as_i64(), Some(feet["total"].as_i64().unwrap() + hands["total"].as_i64().unwrap()));
    let (status, _, invalid) = get("/v1/items?slot=Feet&slot_match=all").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{invalid}");
    assert!(invalid["error"].as_str().unwrap().contains("slot"));
    let (status, _, invalid) = get("/v1/items?category=Armor&category_match=all").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{invalid}");
    assert!(invalid["error"].as_str().unwrap().contains("category"));

    let (_, _, strength) = get("/v1/items?bonus=Strength").await;
    let (_, _, charisma) = get("/v1/items?bonus=Charisma").await;
    let (_, _, both) = get("/v1/items?bonus=Strength&bonus=Charisma&bonus_match=all").await;
    let strength_ids: std::collections::BTreeSet<_> =
        strength["items"].as_array().unwrap().iter().map(|row| row["id"].as_i64().unwrap()).collect();
    let charisma_ids: std::collections::BTreeSet<_> =
        charisma["items"].as_array().unwrap().iter().map(|row| row["id"].as_i64().unwrap()).collect();
    let expected: std::collections::BTreeSet<_> = strength_ids.intersection(&charisma_ids).copied().collect();
    let actual: std::collections::BTreeSet<_> =
        both["items"].as_array().unwrap().iter().map(|row| row["id"].as_i64().unwrap()).collect();
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn items_filter_by_any_or_all_sets_and_sort_by_first_set_name() {
    let first_set = "Cooking%20By%20the%20Book";
    let second_set = "Fried%20%26%20Frozen%20Frenzy";
    let (_, _, cooking) = get(&format!("/v1/items?set={first_set}&limit=10000")).await;
    let (_, _, fried) = get(&format!("/v1/items?set={second_set}&limit=10000")).await;
    let (_, _, either) = get(&format!("/v1/items?set={first_set}&set={second_set}&limit=10000")).await;
    let (_, _, both) = get(&format!("/v1/items?set={first_set}&set={second_set}&set_match=all&limit=10000")).await;
    let ids = |page: &Value| -> std::collections::BTreeSet<i64> {
        page["items"].as_array().unwrap().iter().map(|row| row["id"].as_i64().unwrap()).collect()
    };
    assert_eq!(ids(&either), ids(&cooking).union(&ids(&fried)).copied().collect());
    assert_eq!(ids(&both), ids(&cooking).intersection(&ids(&fried)).copied().collect());
    assert!(item_names(&both).contains(&"Fried Sword Fish"));

    let (status, _, unknown) = get("/v1/items?set=Unknown%20Set").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(unknown["error"].as_str().unwrap().contains("Unknown Set"));

    let (_, _, sorted) = get("/v1/items?sort=set&limit=10000").await;
    let set_names: Vec<_> = sorted["items"].as_array().unwrap().iter().filter_map(|row| row["set"].as_str()).collect();
    assert!(set_names.windows(2).all(|pair| pair[0] <= pair[1]));
    assert!(sorted["items"].as_array().unwrap().last().unwrap()["set"].is_null());
    let fish = sorted["items"].as_array().unwrap().iter().find(|row| row["name"] == "Fried Sword Fish").unwrap();
    assert_eq!(fish["set"], "Cooking By the Book");

    let (_, _, spec) = get("/v1/openapi.json").await;
    let params = spec["paths"]["/v1/items"]["get"]["parameters"].as_array().unwrap();
    assert!(params.iter().any(|param| param["name"] == "set"));
    assert!(params.iter().any(|param| param["name"] == "set_match"));
}

#[tokio::test]
async fn sets_picker_supports_search_and_paging() {
    let (_, _, page) = get("/v1/sets?q=Cooking%20By%20the%20Book&limit=1&offset=0").await;
    assert_eq!(page["total"], 1);
    assert_eq!(page["limit"], 1);
    assert_eq!(page["sets"][0]["name"], "Cooking By the Book");
    let (_, _, second_page) = get("/v1/sets?q=Cooking%20By%20the%20Book&limit=1&offset=1").await;
    assert_eq!(second_page["total"], 1);
    assert_eq!(second_page["sets"], serde_json::json!([]));
}

#[tokio::test]
async fn family_and_stat_detail_page_their_real_carriers() {
    let (_, _, vocabulary) = get_list_rows("/v1/effects").await;
    let freedom = effect_named(&vocabulary, "Freedom of Movement", "effect").unwrap();
    let family_path = freedom["detail_path"].as_str().unwrap();
    let (status, _, family) = get(&format!("{family_path}?items_limit=1&items_offset=1")).await;
    assert_eq!(status, StatusCode::OK, "{family}");
    assert_eq!(family["name"], "Freedom of Movement");
    assert_eq!(family["items"]["total"], 3);
    assert_eq!(family["items"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(family["items"]["offset"], 1);

    let strength = effect_named(&vocabulary, "Strength", "stat").unwrap();
    let (status, _, stat) = get(strength["detail_path"].as_str().unwrap()).await;
    assert_eq!(status, StatusCode::OK, "{stat}");
    assert_eq!(stat["name"], "Strength");
    assert!(stat["items"]["total"].as_i64().unwrap() >= 3);
    let (_, _, stat_page) =
        get(&format!("{}?items_limit=1&items_offset=1", strength["detail_path"].as_str().unwrap())).await;
    assert_eq!(stat_page["items"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(stat_page["items"]["offset"], 1);

    let attack_speed = effect_named(&vocabulary, "Melee Attack Speed", "stat").unwrap();
    let (_, _, attack_speed_detail) = get(attack_speed["detail_path"].as_str().unwrap()).await;
    let bracers = attack_speed_detail["items"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == "Epic Ethereal Bracers")
        .unwrap();
    assert_eq!(bracers["effect"], "Speed XIV");
    assert_eq!(bracers["bonus_type"], "Enhancement");
    assert_eq!(bracers["value"], 14);
    assert!(bracers["value2"].is_null());
    let (status, _, _) = get("/v1/effects/999999").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let deception = effect_named(&vocabulary, "Deception", "effect").unwrap();
    let (_, _, step) = get(deception["detail_path"].as_str().unwrap()).await;
    assert_eq!(step["tier"]["group"], "Deception");
    assert_eq!(step["tier"]["steps"].as_array().unwrap().len(), 2);
    let carrier_id = step["items"]["items"][0]["id"].as_i64().unwrap();
    let (_, _, carrier) = get(&format!("/v1/items/{carrier_id}")).await;
    assert!(carrier["effects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|line| line["effect_id"] == deception["id"] && line["tier"]["group"] == "Deception"));

    let spell_focus = effect_named(&vocabulary, "Spell Focus Mastery", "effect").unwrap();
    assert_eq!(spell_focus["id"], 100);
    let (_, _, mastery) = get(spell_focus["detail_path"].as_str().unwrap()).await;
    assert!(mastery["items"]["total"].as_i64().unwrap() > 0);
    let carrier = item_detail_named("Band%20of%20Diani%20ir%27Wynarn").await;
    let line =
        carrier["effects"].as_array().unwrap().iter().find(|line| line["effect_id"] == spell_focus["id"]).unwrap();
    assert_eq!(line["bonuses"].as_array().unwrap().len(), 1);
    assert_eq!(line["bonuses"][0]["stat"], "Spell DCs");

    let all_abilities = effect_named(&vocabulary, "All Ability Scores", "group").unwrap();
    assert!(all_abilities["bonus_types"]
        .as_array()
        .unwrap()
        .iter()
        .any(|kind| kind["name"] == "Artifact" && kind["item_count"] == 0));
    let (_, _, family) = get(all_abilities["detail_path"].as_str().unwrap()).await;
    assert_eq!(family["bonuses"].as_array().unwrap().len(), 6);
    let tier = &family["set_tiers"]["set_tiers"][0];
    let (_, _, set) = get(&format!("/v1/sets/{}", tier["set_id"])).await;
    let tier_line = set["tiers"].as_array().unwrap().iter().find(|candidate| candidate["id"] == tier["id"]).unwrap()
        ["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|line| line["effect_id"] == all_abilities["id"])
        .unwrap();
    assert_eq!(tier_line["bonuses"].as_array().unwrap().len(), 6);

    let combustion = effect_named(&vocabulary, "Combustion", "effect").unwrap();
    assert_eq!(combustion["item_count"], 0);
    let (_, _, current) = get(combustion["detail_path"].as_str().unwrap()).await;
    assert_eq!(current["items"]["total"], combustion["item_count"]);
    let (_, _, all) = get(&format!("{}?include_legacy=true", combustion["detail_path"].as_str().unwrap())).await;
    assert_eq!(all["items"]["total"], 1);
}

#[tokio::test]
async fn named_effect_carriers_page_links_and_nest_each_resolved_bonus() {
    let (_, _, vocabulary) = get_list_rows("/v1/effects").await;
    let riposte_id = effect_named(&vocabulary, "Riposte", "effect").unwrap()["id"].as_i64().unwrap();
    let (_, _, riposte) = get(&format!("/v1/effects/{riposte_id}?items_limit=100")).await;
    let bracers: Vec<_> = riposte["items"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["name"] == "Epic Ethereal Bracers")
        .collect();
    assert_eq!(bracers.len(), 1);
    assert_eq!(bracers[0]["value"], 5);
    assert_eq!(bracers[0]["bonus_type"], "Insight");
    assert_eq!(bracers[0]["line"]["name"], "Riposte");
    assert_eq!(bracers[0]["line"]["verbose_name"], "Riposte +5");
    assert!(bracers[0]["line"].get("text").is_none());
    assert_eq!(bracers[0]["bonuses"].as_array().unwrap().len(), 2);
    assert!(bracers[0]["bonuses"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["stat"] == "Armor Class" && row["value"] == 3));
    assert!(bracers[0]["bonuses"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["stat"] == "Saving Throws" && row["value"] == 2));
    assert_eq!(riposte["items"]["total"], riposte["items"]["items"].as_array().unwrap().len());

    let deception_id = effect_named(&vocabulary, "Improved Deception", "effect").unwrap()["id"].as_i64().unwrap();
    let (_, _, deception) = get(&format!("/v1/effects/{deception_id}?items_limit=100")).await;
    assert!(deception["items"]["items"].as_array().unwrap().iter().any(|row| row["bonus_type"] == "Enhancement"));

    let strength_id = effect_named(&vocabulary, "Strength", "stat").unwrap()["id"].as_i64().unwrap();
    let (_, _, strength) = get(&format!("/v1/effects/{strength_id}?augments_limit=100")).await;
    assert!(strength["augments"]["augments"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["name"] == "Solar Gem of Strength (Heroic)" && row["line"]["name"] == "Strength"));
    let all_abilities_id = effect_named(&vocabulary, "All Ability Scores", "group").unwrap()["id"].as_i64().unwrap();
    let (_, _, all_abilities) = get(&format!("/v1/effects/{all_abilities_id}")).await;
    assert!(all_abilities["set_tiers"]["set_tiers"].as_array().unwrap().iter().any(|row| row["bonuses"]
        .as_array()
        .is_some_and(|rows| rows.len() == 6)
        && row["line"]["name"] == "All Ability Scores"
        && row["line"]["verbose_name"].as_str().is_some_and(|line| line.contains("All Ability Scores"))));
}

fn fixture_state_with_sql(injected_sql: &str, test_name: &str) -> AppState {
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    build_fixture_database(&mut db, &fixture_wiki());
    db.execute_batch(injected_sql).unwrap();
    let path = std::env::temp_dir().join(format!("ddo-api-{test_name}-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    db.execute("VACUUM INTO ?1", [path.to_str().unwrap()]).unwrap();
    AppState::open(&path).unwrap()
}

#[tokio::test]
async fn effect_detail_totals_count_distinct_owners_and_match_the_vocabulary_row() {
    let state = fixture_state_with_sql(
        "INSERT INTO item_effects (item_id, effect_id, bonus_type_id, value, sort_order)
         SELECT item_id, effect_id, bonus_type_id, value, 990 FROM item_effects
          WHERE effect_id = (SELECT id FROM effects WHERE name = 'Strength' AND is_stat = 1) AND value IS NOT NULL
          LIMIT 1;",
        "repeated-link",
    );
    let (_, _, vocabulary) = get_from(state.clone(), "/v1/effects?limit=10000").await;
    let mut repeated_owner_rows = 0;
    for row in vocabulary["effects"].as_array().unwrap() {
        let path = format!(
            "{}?items_limit=10000&augments_limit=10000&set_tiers_limit=10000&include_legacy=false",
            row["detail_path"].as_str().unwrap()
        );
        let (_, _, detail) = get_from(state.clone(), &path).await;
        for (page_key, count_key) in [("items", "item_count"), ("augments", "augment_count")] {
            let owners = detail[page_key][page_key].as_array().unwrap();
            let distinct_owner_ids: std::collections::BTreeSet<i64> =
                owners.iter().map(|owner| owner["id"].as_i64().unwrap()).collect();
            assert_eq!(distinct_owner_ids.len(), owners.len(), "{} {page_key} rows are owners", row["name"]);
            assert_eq!(detail[page_key]["total"], row[count_key], "{} {page_key} total", row["name"]);
            repeated_owner_rows +=
                owners.iter().filter(|owner| owner["lines"].as_array().map_or(0, Vec::len) > 1).count();
        }
        let tiers = detail["set_tiers"]["set_tiers"].as_array().unwrap();
        let distinct_tier_ids: std::collections::BTreeSet<i64> =
            tiers.iter().map(|tier| tier["id"].as_i64().unwrap()).collect();
        assert_eq!(distinct_tier_ids.len(), tiers.len(), "{} set tier rows are owners", row["name"]);
        assert_eq!(detail["set_tiers"]["total"], tiers.len(), "{} set tier total", row["name"]);
        let distinct_set_ids: std::collections::BTreeSet<i64> =
            tiers.iter().map(|tier| tier["set_id"].as_i64().unwrap()).collect();
        assert_eq!(distinct_set_ids.len() as i64, row["set_count"].as_i64().unwrap(), "{} sets", row["name"]);
    }
    assert!(repeated_owner_rows > 0, "the injected second Strength link groups under its owner");
}

fn item_ids_in_page(page: &Value) -> std::collections::BTreeSet<i64> {
    page["items"].as_array().unwrap().iter().map(|item| item["id"].as_i64().unwrap()).collect()
}

#[tokio::test]
async fn every_repeated_source_filter_uses_union_or_intersection() {
    let db = rusqlite::Connection::open(fixture_db_path()).unwrap();
    for (filter, sql) in [
        ("pack", "SELECT DISTINCT ap.name FROM loot_adventure_packs lp JOIN adventure_packs ap ON ap.id = lp.pack_id ORDER BY ap.name LIMIT 2"),
        ("quest", "SELECT DISTINCT quest_id FROM sources WHERE kind = 'quest' AND quest_id IS NOT NULL ORDER BY quest_id LIMIT 2"),
        ("quest_chain", "SELECT DISTINCT chain_id FROM sources WHERE kind = 'quest_chain' AND chain_id IS NOT NULL ORDER BY chain_id LIMIT 2"),
        ("saga", "SELECT DISTINCT saga_id FROM sources WHERE kind = 'saga' AND saga_id IS NOT NULL ORDER BY saga_id LIMIT 2"),
    ] {
        let mut statement = db.prepare(sql).unwrap();
        let mut values: Vec<String> = statement.query_map([], |row| row.get::<_, rusqlite::types::Value>(0)).unwrap()
            .map(|result| match result.unwrap() {
                rusqlite::types::Value::Text(text) => text,
                rusqlite::types::Value::Integer(number) => number.to_string(),
                other => panic!("unexpected filter value {other:?}"),
            }).collect();
        if values.len() == 1 { values.push("999999".to_string()); }
        assert_eq!(values.len(), 2, "{filter} fixture needs two values");
        let single_paths: Vec<String> = values.iter().map(|value| format!("/v1/items?{}&limit=10000",
            serde_urlencoded::to_string([(filter, value)]).unwrap())).collect();
        let (_, _, first) = get(&single_paths[0]).await;
        let (_, _, second) = get(&single_paths[1]).await;
        let base_query = serde_urlencoded::to_string([(filter, &values[0]), (filter, &values[1])]).unwrap();
        let (status, _, any) = get(&format!("/v1/items?{base_query}&limit=10000")).await;
        assert_eq!(status, StatusCode::OK, "{filter}: {any}");
        let (status, _, all) = get(&format!("/v1/items?{base_query}&{filter}_match=all&limit=10000")).await;
        assert_eq!(status, StatusCode::OK, "{filter}: {all}");
        assert_eq!(item_ids_in_page(&any), item_ids_in_page(&first).union(&item_ids_in_page(&second)).copied().collect(), "{filter} any");
        assert_eq!(item_ids_in_page(&all), item_ids_in_page(&first).intersection(&item_ids_in_page(&second)).copied().collect(), "{filter} all");
        let (status, _, invalid) = get(&format!("/v1/items?{filter}_match=neither")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{filter}: {invalid}");
    }
}

#[tokio::test]
async fn effect_filter_matches_families_ladders_and_typed_stats() {
    let (_, _, deception) = get("/v1/items?bonus=Deception&limit=10000").await;
    let (_, _, improved) = get("/v1/items?bonus=Improved%20Deception&limit=10000").await;
    assert!(item_ids_in_page(&improved).is_subset(&item_ids_in_page(&deception)));

    let (_, _, typed) = get("/v1/items?bonus=Strength%3AEnhancement&limit=10000").await;
    assert!(typed["total"].as_i64().unwrap() > 0);
    for item in typed["items"].as_array().unwrap() {
        let (_, _, detail) = get(&format!("/v1/items/{}", item["id"])).await;
        assert!(detail["effects"].as_array().unwrap().iter().any(|line| {
            line["bonuses"]
                .as_array()
                .unwrap()
                .iter()
                .any(|bonus| bonus["stat"] == "Strength" && bonus["bonus_type"] == "Enhancement")
        }));
    }
    let (status, _, unknown_type) = get("/v1/items?bonus=Strength%3ANotAType").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(unknown_type["error"].as_str().unwrap().contains("NotAType"));

    let (status, _, canonical) = get("/v1/items?bonus=Constitution%3AInsight").await;
    assert_eq!(status, StatusCode::OK, "{canonical}");
    let (status, _, alias) = get("/v1/items?bonus=Constitution%3AInsightful").await;
    assert_eq!(status, StatusCode::OK, "{alias}");
    assert_eq!(canonical, alias);

    let (status, _, unknown_slot) = get("/v1/items?slot=Bogus").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(unknown_slot["error"].as_str().unwrap().contains("slot"));
}

#[tokio::test]
async fn item_effect_rendering_uses_one_query_at_small_and_large_link_counts() {
    static EFFECT_QUERY_COUNT: AtomicUsize = AtomicUsize::new(0);
    static OPTION_EFFECT_QUERY_COUNT: AtomicUsize = AtomicUsize::new(0);
    let db = rusqlite::Connection::open(fixture_db_path()).unwrap();
    let mut statement =
        db.prepare("SELECT item_id, COUNT(*) FROM item_effects GROUP BY item_id ORDER BY COUNT(*)").unwrap();
    let counts: Vec<(i64, i64)> =
        statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?))).unwrap().map(Result::unwrap).collect();
    assert!(counts.last().unwrap().1 > counts.first().unwrap().1);
    let state = fixture_state();
    state
        .read_db(|db| {
            db.trace_v2(
                TraceEventCodes::SQLITE_TRACE_STMT,
                Some(|event| {
                    if let TraceEvent::Stmt(statement, _) = event {
                        if statement.sql().contains("FROM item_effects j JOIN effects e") {
                            EFFECT_QUERY_COUNT.fetch_add(1, Ordering::SeqCst);
                        }
                        if statement.sql().contains("FROM item_augment_slot_option_effects j JOIN effects e") {
                            OPTION_EFFECT_QUERY_COUNT.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                }),
            );
            Ok(())
        })
        .await
        .unwrap();
    for (item_id, _) in [counts[0], *counts.last().unwrap()] {
        EFFECT_QUERY_COUNT.store(0, Ordering::SeqCst);
        OPTION_EFFECT_QUERY_COUNT.store(0, Ordering::SeqCst);
        let (status, _, item) = get_from(state.clone(), &format!("/v1/items/{item_id}")).await;
        assert_eq!(status, StatusCode::OK, "{item}");
        assert_eq!(EFFECT_QUERY_COUNT.load(Ordering::SeqCst), 1, "item {item_id}");
        assert_eq!(OPTION_EFFECT_QUERY_COUNT.load(Ordering::SeqCst), 1, "item {item_id}");
    }
}

#[tokio::test]
async fn augment_page_effects_use_one_query_for_one_or_many_rows() {
    static EFFECT_QUERY_COUNT: AtomicUsize = AtomicUsize::new(0);
    let state = fixture_state();
    state
        .read_db(|db| {
            db.trace_v2(
                TraceEventCodes::SQLITE_TRACE_STMT,
                Some(|event| {
                    if let TraceEvent::Stmt(statement, _) = event {
                        if statement.sql().contains("FROM augment_effects j JOIN effects e") {
                            EFFECT_QUERY_COUNT.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                }),
            );
            Ok(())
        })
        .await
        .unwrap();
    for limit in [1, 100] {
        EFFECT_QUERY_COUNT.store(0, Ordering::SeqCst);
        let (status, _, page) = get_from(state.clone(), &format!("/v1/augments?limit={limit}")).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(EFFECT_QUERY_COUNT.load(Ordering::SeqCst), 1, "limit {limit}");
    }
}

#[tokio::test]
async fn version_reports_dataset_and_schema() {
    let (status, headers, json) = get("/v1/version").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["dataset"]["upstream_sha"], "fixture-sha");
    assert_eq!(json["schema_version"], ddo_model::SCHEMA_VERSION);
    for count_name in [
        "effects",
        "effect_bonuses",
        "item_effects",
        "augment_effects",
        "set_bonus_tier_effects",
        "feat_effects",
        "item_augment_slot_option_effects",
    ] {
        assert!(json["counts"][count_name].is_number(), "missing {count_name}");
    }
    assert!(json["counts"].get("bonuses").is_none());
    assert!(json.get("api_commit").is_some(), "version must report the API build commit, null when unknown");
    assert_eq!(json["counts"]["items"], 59, "58 of Maetrim's and the wiki fixture's axe");
    assert_eq!(
        json["counts"]["legacy_items"], 3,
        "a legacy and a historic version, and an axe that drops only in a retired Temple of Elemental Evil part"
    );
    assert_eq!(json["counts"]["quest_augment_loot"], 7);
    assert_eq!(
        (
            &json["counts"]["crafting_systems"],
            &json["counts"]["crafting_recipes"],
            &json["counts"]["crafting_ingredients"]
        ),
        (&serde_json::json!(4), &serde_json::json!(5), &serde_json::json!(5))
    );
    assert_eq!(headers.get("x-dataset-version").unwrap(), "fixture-sha");
    assert!(headers.get(header::ETAG).is_some());
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
    assert_eq!(first_page["total"], 56, "the three legacy items are left out by default");
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
    let rare_names: Vec<&str> = rare.iter().map(|item| item["name"].as_str().unwrap()).collect();
    assert_eq!(
        rare_names,
        ["Buckler of the Golden Age", "Light Crossbow of the Golden Age"],
        "a rare Book Burning drop by his text and the wiki, and a rare drop of any Magic of Myth Drannor end chest"
    );
    assert!(rare.iter().all(|item| item["is_rare"] == true));
    let (_, _, unfiltered) = get("/v1/items?rare=false").await;
    assert_eq!(unfiltered["total"], 56);
    let (status, _, _) = get("/v1/items?category=Hat").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "unknown category is a client error");
}

#[tokio::test]
async fn items_list_leaves_out_legacy_items_unless_asked_to_include_them() {
    let legacy_names = ["+3 Combustion Scorched Battle Axe", "Allegiance (historic)", "Ratkiller (legacy) (level 4)"];
    for path in ["/v1/items?limit=10000", "/v1/items?limit=10000&include_legacy=false"] {
        let (status, _, current) = get(path).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(current["total"], 56, "{path}");
        let rows = current["items"].as_array().unwrap();
        assert!(rows.iter().all(|row| row["is_legacy"] == false), "{path}");
        assert!(legacy_names.iter().all(|name| rows.iter().all(|row| row["name"] != *name)), "{path}");
    }
    let (_, _, with_legacy) = get("/v1/items?limit=10000&include_legacy=true").await;
    assert_eq!(with_legacy["total"], 59);
    let legacy_rows: Vec<&Value> =
        with_legacy["items"].as_array().unwrap().iter().filter(|row| row["is_legacy"] == true).collect();
    assert_eq!(legacy_rows.iter().map(|row| row["name"].as_str().unwrap()).collect::<Vec<_>>(), legacy_names);

    let legacy_item_id = legacy_rows[0]["id"].as_i64().unwrap();
    let (status, _, detail) = get(&format!("/v1/items/{legacy_item_id}")).await;
    assert_eq!(status, StatusCode::OK, "a legacy item's detail is still served");
    assert_eq!(detail["is_legacy"], true);
    let (_, _, current_detail) = get(&format!("/v1/items/{}", sireth_id().await)).await;
    assert_eq!(current_detail["is_legacy"], false);
}

async fn sireth_id() -> i64 {
    let (_, _, json) = get("/v1/items?q=Sireth").await;
    json["items"][0]["id"].as_i64().unwrap()
}

fn item_names(list_response: &Value) -> Vec<&str> {
    list_response["items"].as_array().unwrap().iter().map(|item| item["name"].as_str().unwrap()).collect()
}

#[tokio::test]
async fn items_filter_by_any_of_several_stat_effects_given_as_repeated_keys() {
    let (status, _, repeated) = get("/v1/items?bonus=Strength&bonus=Charisma").await;
    assert_eq!(status, StatusCode::OK, "repeated effect keys are rejected: {repeated}");
    assert_eq!(
        item_names(&repeated),
        [
            "Battle Axe of the Oozing Hunger",
            "Fabricator's Gauntlets",
            "Legendary Ring of Unbridled Might",
            "Ring of the Kraken"
        ]
    );
    assert_eq!(repeated["total"], 4);

    let (status, _, unknown) = get("/v1/items?bonus=Strength&bonus=Strenght").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "an unknown bonus is a client error, not an empty page");
    assert_eq!(unknown["error"], r#"unknown bonus "Strenght""#);
    let (status, _, retired) = get("/v1/items?stat=Strength").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "`stat` gave way to `effect`: {retired}");
}

#[tokio::test]
async fn items_filter_by_an_effect_whose_name_contains_a_comma() {
    let (status, _, poisoned) = get("/v1/items?bonus=Constitution%20Poison%2C%20Lesser").await;
    assert_eq!(status, StatusCode::OK, "a comma inside one effect name must not split it: {poisoned}");
    assert_eq!(item_names(&poisoned), ["Ivy Wraps (Level 15)"]);

    let (_, _, poison_or_vorpal) = get("/v1/items?bonus=Constitution%20Poison%2C%20Lesser&bonus=Vorpal").await;
    assert_eq!(item_names(&poison_or_vorpal), ["Ivy Wraps (Level 15)", "Light Crossbow of the Golden Age"]);

    let (status, _, comma_list) = get("/v1/items?bonus=Strength,Charisma").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "a comma no longer separates names: {comma_list}");
    assert_eq!(comma_list["error"], r#"unknown bonus "Strength,Charisma""#);
}

#[tokio::test]
async fn items_filter_by_effect_effects_alone_or_beside_stats() {
    let (status, _, vorpal) = get("/v1/items?bonus=Vorpal").await;
    assert_eq!(status, StatusCode::OK, "{vorpal}");
    assert_eq!(item_names(&vorpal), ["Light Crossbow of the Golden Age"]);

    let (_, _, effect_or_stat) = get("/v1/items?bonus=Freedom%20of%20Movement&bonus=Charisma").await;
    assert_eq!(
        item_names(&effect_or_stat),
        ["Epic Kundarak Delving Boots", "Kundarak Delving Boots", "Ring of the Kraken", "Sireth, Spear of the Sky"],
        "three items carry the Freedom of Movement effect and the ring a Charisma bonus"
    );

    let (_, _, effect_and_search_text) = get("/v1/items?bonus=Freedom%20of%20Movement&q=sireth").await;
    assert_eq!(
        item_names(&effect_and_search_text),
        ["Sireth, Spear of the Sky"],
        "other filters still narrow the match"
    );

    let (status, _, lower_case) = get("/v1/items?bonus=vorpal").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "names match case-sensitively, as `stat` did: {lower_case}");
}

#[tokio::test]
async fn items_match_a_stat_effect_on_their_sets_tiers_only_when_set_bonuses_are_included() {
    let (_, _, own_bonuses_only) = get("/v1/items?bonus=Sneak%20Attack%20Dice").await;
    assert_eq!(own_bonuses_only["total"], 0, "no fixture item has its own Sneak Attack Dice bonus");
    let (status, _, with_set_bonuses) = get("/v1/items?bonus=Sneak%20Attack%20Dice&include_set_bonuses=true").await;
    assert_eq!(status, StatusCode::OK, "{with_set_bonuses}");
    assert_eq!(item_names(&with_set_bonuses), ["Kundarak Delving Boots"], "its set's three-piece tier gives the dice");

    let (_, _, own_or_set) =
        get("/v1/items?bonus=Hide&bonus=Physical%20Resistance%20Rating&include_set_bonuses=true").await;
    assert_eq!(
        item_names(&own_or_set),
        ["Grudgebearer's Plate", "Kundarak Delving Boots", "Legendary Cloak of Winter"],
        "Command's Hide penalty and the boots' Hide bonus match beside the cloak's Eminence of Winter PRR tier"
    );
    let (_, _, explicitly_excluded) = get("/v1/items?bonus=Sneak%20Attack%20Dice&include_set_bonuses=false").await;
    assert_eq!(explicitly_excluded["total"], 0);
}

fn effect_named<'a>(effects: &'a Value, name: &str, kind: &str) -> Option<&'a Value> {
    effects.as_array().unwrap().iter().find(|row| row["name"] == name && row["kind"] == kind)
}

#[tokio::test]
async fn effect_vocabulary_lists_each_family_and_stat_with_carrier_counts() {
    let (status, _, rows) = get_list_rows("/v1/effects").await;
    assert_eq!(status, StatusCode::OK);
    let rows = rows.as_array().unwrap();
    let db = rusqlite::Connection::open(fixture_db_path()).unwrap();
    let expected_count: i64 = db.query_row("SELECT COUNT(*) FROM effects", [], |row| row.get(0)).unwrap();
    assert_eq!(rows.len(), expected_count as usize);
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM effect_vocabulary_counts", [], |row| row.get::<_, i64>(0)).unwrap(),
        expected_count
    );
    let strength = rows.iter().find(|row| row["name"] == "Strength" && row["kind"] == "stat").unwrap();
    assert_eq!(strength["item_count"], 3);
    assert!(strength["augment_count"].as_i64().unwrap() > 0);
    assert!(strength["bonus_types"].as_array().unwrap().iter().any(|kind| kind["name"] == "Enhancement"));
    let freedom = rows.iter().find(|row| row["name"] == "Freedom of Movement" && row["kind"] == "effect").unwrap();
    assert_eq!(freedom["item_count"], 3);
    assert_eq!(freedom["detail_path"], format!("/v1/effects/{}", freedom["id"]));
    assert!(rows.iter().all(|row| row["id"].is_i64() && row["detail_path"].is_string()));
}

#[tokio::test]
async fn effect_vocabulary_searches_both_kinds_and_rejects_unknown_kind() {
    let (_, _, resistances) = get_list_rows("/v1/effects?q=RESISTANCE").await;
    assert!(resistances.as_array().unwrap().iter().any(|row| row["name"] == "Acid Resistance"));
    let (_, _, families) = get_list_rows("/v1/effects?kind=effect").await;
    assert!(families.as_array().unwrap().iter().all(|row| row["kind"] == "effect"));
    let (_, _, stat_rows) = get_list_rows("/v1/effects?kind=stat&q=strength").await;
    assert!(stat_rows.as_array().unwrap().iter().any(|row| row["name"] == "Strength"));
    let (status, _, unknown) = get("/v1/effects?kind=unknown").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(unknown["error"].as_str().unwrap().contains("unknown"));
}

#[tokio::test]
async fn retired_enchantment_routes_and_filters_have_no_aliases() {
    let (status, _, _) = get("/v1/enchantments").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = get("/v1/enchantments/1").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, response) = get("/v1/items?enchantment=Strength").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(response["error"].as_str().unwrap().contains("enchantment"));

    let (_, _, version) = get("/v1/version").await;
    assert!(version["counts"]["effects"].is_number());
    assert!(version["counts"]["effect_bonuses"].is_number());
    assert!(version["counts"].get("enchantments").is_none());
}

#[tokio::test]
async fn effect_search_matches_its_own_name_then_granted_stats_in_one_direction() {
    let (_, _, strength_page) = get("/v1/effects?q=strength&limit=10000").await;
    let strength_rows = strength_page["effects"].as_array().unwrap();
    let own_matches = strength_rows.iter().take_while(|row| row["name"] == "Strength").count();
    assert_eq!(own_matches, 1);
    assert!(strength_rows.iter().skip(own_matches).any(|row| row["name"] == "All Ability Scores"));
    let (_, _, alphabetic_page) = get("/v1/effects?q=strength&sort=name&limit=10000").await;
    assert_eq!(alphabetic_page["effects"][0]["name"], "All Ability Scores");

    let (_, _, family_page) = get("/v1/effects?q=all%20ability%20scores&limit=10000").await;
    let family_rows = family_page["effects"].as_array().unwrap();
    assert!(family_rows.iter().any(|row| row["name"] == "All Ability Scores"));
    assert!(!family_rows.iter().any(|row| row["name"] == "Strength" && row["kind"] == "stat"));

    let (_, _, stats_only) = get("/v1/effects?kind=stat&q=strength&limit=10000").await;
    assert!(stats_only["effects"].as_array().unwrap().iter().all(|row| row["kind"] == "stat"));

    let (_, _, many_stats) = get("/v1/effects?q=n&limit=10000").await;
    assert_eq!(
        many_stats["effects"].as_array().unwrap().iter().filter(|row| row["name"] == "All Ability Scores").count(),
        1
    );
}

#[tokio::test]
async fn effect_search_finds_firestorm_lore_through_electric_spell_lore_in_one_direction() {
    let (_, _, electric_page) = get("/v1/effects?q=electric%20spell%20lore&limit=10000").await;
    let electric_rows = electric_page["effects"].as_array().unwrap();
    let electric_stat = electric_rows
        .iter()
        .position(|row| row["name"] == "Electric Spell Lore" && row["kind"] == "stat")
        .expect("Electric Spell Lore stat");
    let firestorm_family = electric_rows
        .iter()
        .position(|row| row["name"] == "Firestorm Lore" && row["kind"] == "effect")
        .expect("Firestorm Lore family");
    assert_eq!(electric_stat, 0);
    assert!(electric_stat < firestorm_family);

    let (_, _, firestorm_page) = get("/v1/effects?q=firestorm&limit=10000").await;
    let firestorm_rows = firestorm_page["effects"].as_array().unwrap();
    assert!(firestorm_rows.iter().any(|row| row["name"] == "Firestorm Lore" && row["kind"] == "effect"));
    assert!(firestorm_rows.iter().all(|row| row["name"].as_str().unwrap().to_ascii_lowercase().contains("firestorm")));

    let (_, _, lore_page) = get("/v1/effects?q=spell%20lore&limit=10000").await;
    assert_eq!(
        lore_page["effects"].as_array().unwrap().iter().filter(|row| row["name"] == "Firestorm Lore").count(),
        1
    );
}

#[tokio::test]
async fn effect_search_ranks_exact_prefix_substring_then_granted_stat_matches() {
    let (_, _, page) = get("/v1/effects?q=dodge&limit=10000").await;
    let rows = page["effects"].as_array().unwrap();
    let names: Vec<&str> = rows.iter().map(|row| row["name"].as_str().unwrap()).collect();
    assert_eq!(names[0], "Dodge");
    let prefix_end = names.iter().position(|name| !name.to_ascii_lowercase().starts_with("dodge")).unwrap();
    assert!(names[1..prefix_end].contains(&"Dodge Bypass"));
    assert_eq!(names[prefix_end], "Hireling Dodge");
    assert_eq!(names[prefix_end + 1], "Song Dodge");
    assert!(names[prefix_end + 2..].iter().all(|name| !name.to_ascii_lowercase().contains("dodge")));
}

async fn id_in_list_named(list_path: &str, name: &str) -> i64 {
    let (_, _, rows) = get_list_rows(list_path).await;
    let rows = rows.as_array().unwrap_or_else(|| panic!("{list_path} is not a list"));
    rows.iter().find(|row| row["name"] == name).unwrap_or_else(|| panic!("{list_path} has no {name}"))["id"]
        .as_i64()
        .unwrap()
}

#[tokio::test]
async fn items_filter_by_the_quest_quest_chain_or_saga_that_drops_or_rewards_them() {
    let ghosts_of_perdition = id_in_list_named("/v1/quests", "Ghosts of Perdition").await;
    let (status, _, quest_loot) = get(&format!("/v1/items?quest={ghosts_of_perdition}")).await;
    assert_eq!(status, StatusCode::OK, "{quest_loot}");
    assert_eq!(
        item_names(&quest_loot),
        ["Argenti's Armor", "Battle Axe of the Oozing Hunger", "Epic Ethereal Bracers"],
        "a chest drop and an end reward of the quest both count"
    );
    let (status, _, unknown_quest) = get("/v1/items?quest=999999").await;
    assert_eq!((status, &unknown_quest["total"]), (StatusCode::OK, &serde_json::json!(0)), "as an unknown pack is");
    let (status, _, malformed) = get("/v1/items?quest=perdition").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(malformed["error"].as_str().unwrap().contains("quest"), "{malformed}");

    let lost_seekers = id_in_list_named("/v1/quest-chains", "The Lost Seekers").await;
    let (_, _, chain_rewards) = get(&format!("/v1/items?quest_chain={lost_seekers}")).await;
    assert_eq!(item_names(&chain_rewards), ["Acrobat's Ring", "Docent of Defiance", "Kundarak Delving Boots"]);

    let masterminds = id_in_list_named("/v1/sagas", "Masterminds of Sharn").await;
    let (_, _, saga_rewards) = get(&format!("/v1/items?saga={masterminds}")).await;
    assert_eq!(
        item_names(&saga_rewards),
        ["Alabaster of the Twelve", "Band of Diani ir'Wynarn", "Five Rings"],
        "an item offered in two tiers is listed once"
    );
    assert_eq!(saga_rewards["total"], 3);
}

#[tokio::test]
async fn item_search_text_also_matches_a_slot_category_or_pack_name_and_ranks_exact_then_prefix_names_first() {
    let (_, _, ring) = get("/v1/items?q=ring").await;
    assert_eq!(
        item_names(&ring),
        [
            "Ring of the Kraken",
            "A Memento of Mori",
            "Acrobat's Ring",
            "Celestial Emerald Ring",
            "Epic Ring of Master Artifice",
            "Epic Ring of the Stalker",
            "Five Rings",
            "Lantern of the Abyss",
            "Legendary Ring of Unbridled Might"
        ],
        "a name starting with the text comes before names merely containing it"
    );
    assert_eq!(ring["total"], 9);
    let (_, _, boots) = get("/v1/items?q=Kundarak%20Delving%20Boots").await;
    assert_eq!(item_names(&boots), ["Kundarak Delving Boots", "Epic Kundarak Delving Boots"], "the exact name first");

    let (_, _, feet) = get("/v1/items?q=FEET").await;
    assert_eq!(
        item_names(&feet),
        ["Epic Kundarak Delving Boots", "Green Steel Weave Boots", "Kundarak Delving Boots"],
        "the Feet slot"
    );
    let (_, _, jewelry) = get("/v1/items?q=jewelry").await;
    assert!(jewelry["items"].as_array().unwrap().iter().all(|item| item["category"] == "Jewelry"), "{jewelry}");
    assert_eq!(jewelry["total"], 16);
    let (_, _, free_to_play) = get("/v1/items?q=free%20to%20play").await;
    assert_eq!(
        item_names(&free_to_play),
        [
            "Acrobat's Ring",
            "Alabaster of the Twelve",
            "Battle Axe of the Oozing Hunger",
            "Docent of Defiance",
            "Ethereal Great Crossbow",
            "Kundarak Delving Boots"
        ],
        "quest, chain and vendor packs all match, including packs after the first one the row shows"
    );
    let (_, _, slot_text_in_a_name) = get("/v1/items?q=fee").await;
    assert_eq!(slot_text_in_a_name["total"], 0, "slot, category and pack names must equal the text, not contain it");
}

#[tokio::test]
async fn items_hide_whether_maetrim_or_the_wiki_supplied_them() {
    let (_, _, axe_matches) = get("/v1/items?q=oozing").await;
    let axe_row = &axe_matches["items"][0];
    assert_eq!(axe_row["name"], "Battle Axe of the Oozing Hunger");
    assert_no_provenance(axe_row);
    let (_, _, sireth_matches) = get("/v1/items?q=sireth").await;
    assert_no_provenance(&sireth_matches["items"][0]);

    let (_, _, axe) = get(&format!("/v1/items/{}", axe_row["id"])).await;
    assert_no_provenance(&axe);
    assert_eq!(axe["weapon"]["weapon_type"], "Battle Axe");
    let (_, _, sireth) = get(&format!("/v1/items/{}", sireth_matches["items"][0]["id"])).await;
    assert_no_provenance(&sireth);

    let (_, _, version) = get("/v1/version").await;
    assert_eq!(version["counts"]["wiki_items"], 1);
}

#[tokio::test]
async fn wiki_only_effects_render_shared_family_templates() {
    let axe = item_detail_named("Battle%20Axe%20of%20the%20Oozing%20Hunger").await;
    let effect = axe["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["name"] == "Test Oozing Hunger")
        .expect("wiki-only effect");
    assert_eq!(effect["description"], "Test description: on hit, the target oozes.");
    let ethereal = axe["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["name"] == "Ethereal")
        .expect("Maetrim family reused by wiki item");
    assert!(ethereal["description"].as_str().unwrap().starts_with("Equipping this item"));
}

#[tokio::test]
async fn augments_hide_whether_maetrim_or_the_wiki_supplied_them() {
    let (_, _, gem_matches) = get("/v1/augments?q=oozing").await;
    let gem_row = &gem_matches["augments"][0];
    assert_eq!(gem_row["name"], "Test Gem of Oozing Resistance");
    assert_no_provenance(gem_row);
    assert_eq!(gem_row["slots"], serde_json::json!(["colorless", "green"]));
    let (_, _, bulwark_matches) = get("/v1/augments?q=bulwark").await;
    assert_eq!(bulwark_matches["total"], 1, "the superseded wiki entry writes no second row");
    assert_no_provenance(&bulwark_matches["augments"][0]);

    let (_, _, gem) = get(&format!("/v1/augments/{}", gem_row["id"])).await;
    assert_no_provenance(&gem);
    assert_eq!(gem["set_bonus"], "Eminence of Winter");
    let (_, _, bulwark) = get(&format!("/v1/augments/{}", bulwark_matches["augments"][0]["id"])).await;
    assert_no_provenance(&bulwark);

    let (_, _, version) = get("/v1/version").await;
    assert_eq!(version["counts"]["wiki_augments"], 1);
}

async fn item_detail_named(search_text: &str) -> Value {
    let (_, _, list) = get(&format!("/v1/items?include_legacy=true&q={search_text}")).await;
    let id = list["items"][0]["id"].as_i64().unwrap();
    let (status, _, item) = get(&format!("/v1/items/{id}")).await;
    assert_eq!(status, StatusCode::OK, "{item}");
    item
}

#[tokio::test]
async fn item_detail_keeps_what_each_augment_slot_option_gives_on_the_option() {
    let axe = item_detail_named("Combustion%20Scorched").await;
    let first_tier = &axe["augment_slots"][0]["options"][0];
    let bonus_lines: Vec<String> = first_tier["effects"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|line| line["bonuses"].as_array().unwrap())
        .map(|bonus| format!("{} {} {}", bonus["stat"], bonus["bonus_type"], bonus["value"]))
        .collect();
    assert_eq!(bonus_lines, ["\"Spell Penetration\" \"Equipment\" 1", "\"Armor Class\" \"Insight\" 1"]);
    assert_eq!(first_tier["modifiers"][1]["effect_type"], "ACBonus");
    assert!(first_tier["grants_slot"].is_null());
    assert_eq!(first_tier["sets"], serde_json::json!([]));
    assert_eq!(axe["augment_slots"][1]["options"][0]["grants_slot"], "purple");
    assert!(
        !axe["effects"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|line| line["bonuses"].as_array().unwrap())
            .any(|bonus| bonus["stat"] == "Spell Penetration"),
        "an option's bonus is not the item's"
    );

    let sireth = item_detail_named("sireth").await;
    assert_eq!(sireth["augment_slots"][0]["options"][0]["icon"], "Heroism");
    assert_eq!(sireth["augment_slots"][3]["options"][0]["grants_slot"], "red");

    let gauntlets = item_detail_named("Fabricator").await;
    let option_sets = &gauntlets["augment_slots"][0]["options"][0]["sets"];
    assert_eq!(option_sets[0]["name"], "Fabricator's Ingenuity");
    assert!(option_sets[0]["id"].is_i64());
    assert!(gauntlets["set"].is_null(), "the set is the option's until the player applies it");
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
    let cold_absorption = cloak["effects"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|line| line["bonuses"].as_array().unwrap())
        .find(|bonus| bonus["stat"] == "Cold Absorption")
        .unwrap();
    assert_eq!(cold_absorption["value"], 34);
    assert_eq!(cold_absorption["bonus_type"], "Enhancement");
    assert_eq!(cloak["set"]["name"], "Eminence of Winter");

    let (_, _, list) = get("/v1/items?q=buckler+of+the+golden").await;
    let id = list["items"][0]["id"].as_i64().unwrap();
    let (_, _, buckler) = get(&format!("/v1/items/{id}")).await;
    assert_eq!(buckler["quests"][0]["name"], "Book Burning");
    assert_eq!(buckler["quests"][0]["loot_type"], "chest");
    assert_eq!(buckler["quests"][0]["is_rare"], true);
    assert_eq!(buckler["quests"][0]["chest"], "end chest");
}

#[tokio::test]
async fn augment_detail_lists_the_quests_it_drops_in_as_item_detail_does() {
    let (_, _, list) = get("/v1/augments?q=elemental+absorption").await;
    let id = list["augments"][0]["id"].as_i64().unwrap();
    let (_, _, augment) = get(&format!("/v1/augments/{id}")).await;
    let quests = augment["quests"].as_array().unwrap();
    assert_eq!(quests.len(), 1, "{quests:?}");
    assert_eq!(quests[0]["name"], "Land of Lamordia");
    assert_eq!(quests[0]["loot_type"], "chest");
    assert_eq!(quests[0]["is_rare"], true);
    assert_eq!(quests[0]["chest"], "vornir frosthelm's chest");
    assert_no_provenance(&quests[0]);
    assert_eq!(quests[0]["pack"], "Chill of Ravenloft");

    let (_, _, list) = get("/v1/items?q=buckler+of+the+golden").await;
    let item_id = list["items"][0]["id"].as_i64().unwrap();
    let (_, _, buckler) = get(&format!("/v1/items/{item_id}")).await;
    let mut item_quest_fields: Vec<&String> = buckler["quests"][0].as_object().unwrap().keys().collect();
    let mut augment_quest_fields: Vec<&String> = quests[0].as_object().unwrap().keys().collect();
    item_quest_fields.sort();
    augment_quest_fields.sort();
    assert_eq!(augment_quest_fields, item_quest_fields);

    let (_, _, ruby_list) = get("/v1/augments?q=ruby+of+acid+damage").await;
    let ruby_id = ruby_list["augments"][0]["id"].as_i64().unwrap();
    let (_, _, ruby) = get(&format!("/v1/augments/{ruby_id}")).await;
    assert_eq!(ruby["quests"], serde_json::json!([]));
}

#[tokio::test]
async fn quest_detail_is_the_list_row_plus_the_items_and_augments_it_drops() {
    let (_, _, quest_list) = get_list_rows("/v1/quests").await;
    let book_burning_row =
        quest_list.as_array().unwrap().iter().find(|quest| quest["name"] == "Book Burning").unwrap().clone();
    let id = book_burning_row["id"].as_i64().unwrap();
    let (status, _, quest) = get(&format!("/v1/quests/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    for (field_name, value) in book_burning_row.as_object().unwrap() {
        assert_eq!(&quest[field_name], value, "{field_name} as the list returns it");
    }

    let items = quest["items"].as_array().unwrap();
    let buckler = items.iter().find(|item| item["name"] == "Buckler of the Golden Age").expect("{items:?}");
    let mut item_fields: Vec<&String> = buckler.as_object().unwrap().keys().collect();
    item_fields.sort();
    assert_eq!(item_fields, ["chest", "id", "is_rare", "loot_type", "minimum_level", "name", "slot"]);
    assert_eq!(
        (&buckler["loot_type"], &buckler["is_rare"], &buckler["chest"]),
        (&serde_json::json!("chest"), &serde_json::json!(true), &serde_json::json!("end chest"))
    );
    assert!(buckler["slot"].is_string() && buckler["minimum_level"].is_i64(), "{buckler}");

    let augments = quest["augments"].as_array().unwrap();
    let lunar_gem = augments
        .iter()
        .find(|augment| augment["name"] == "Lunar Gem of Magical Protection (Heroic)")
        .expect("{augments:?}");
    let mut augment_fields: Vec<&String> = lunar_gem.as_object().unwrap().keys().collect();
    augment_fields.sort();
    assert_eq!(augment_fields, ["chest", "family", "id", "is_rare", "loot_type", "min_level", "name"]);
    assert_eq!(lunar_gem["family"], "SunAndMoon");
    assert_eq!(lunar_gem["is_rare"], true);

    let (status, _, _) = get("/v1/quests/999999").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
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
    let (_, _, stats) = get_list_rows("/v1/effects?kind=stat").await;
    assert_eq!(stats.as_array().unwrap().len(), ddo_model::stats::STATS.len());
    let (_, _, augment_slot_types) = get_list_rows("/v1/augment-slot-types").await;
    assert!(augment_slot_types.as_array().unwrap().iter().any(|s| s["label"] == "red"));
    let (_, _, red_augments) = get("/v1/augments?slot=red").await;
    let names: Vec<&str> =
        red_augments["augments"].as_array().unwrap().iter().map(|a| a["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"Ruby of Acid Damage"), "{names:?}");
    let silverscale = red_augments["augments"].as_array().unwrap().iter().find(|a| a["name"] == "Silverscale");
    assert!(silverscale.is_none(), "Silverscale is an Isle of Dread scale slot, not a red gem");
    let (_, _, all_augments) = get("/v1/augments").await;
    let silverscale = all_augments["augments"].as_array().unwrap().iter().find(|a| a["name"] == "Silverscale").unwrap();
    assert_eq!(silverscale["effects"][0]["bonuses"][0]["stat"], "Healing Amplification");
    assert_eq!(silverscale["slots"][0], "isle of dread: scale (armor)");
    let id = silverscale["id"].as_i64().unwrap();
    let (_, _, silverscale_detail) = get(&format!("/v1/augments/{id}")).await;
    assert_eq!(silverscale_detail["modifiers"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn quests_carry_difficulties_and_epic_name() {
    let (_, _, quests) = get_list_rows("/v1/quests").await;
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
    let (_, _, quests) = get_list_rows("/v1/quests").await;
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
async fn quests_hide_whether_maetrim_or_the_wiki_supplied_them() {
    let (_, _, quests) = get_list_rows("/v1/quests").await;
    let quests = quests.as_array().unwrap();
    let quest_named = |name: &str| quests.iter().find(|q| q["name"] == name).unwrap_or_else(|| panic!("{name}"));
    let ghosts_of_perdition = quest_named("Ghosts of Perdition");
    assert_no_provenance(ghosts_of_perdition);
    assert_eq!(
        (&ghosts_of_perdition["pack"], &ghosts_of_perdition["level"]),
        (&serde_json::json!("Chill of Ravenloft"), &serde_json::json!(32))
    );
    assert_no_provenance(quest_named("The Grotto"));
    let (_, _, ghosts_of_perdition_detail) = get(&format!("/v1/quests/{}", ghosts_of_perdition["id"])).await;
    assert_eq!(ghosts_of_perdition_detail["name"], "Ghosts of Perdition");
    assert_no_provenance(&ghosts_of_perdition_detail);

    let (_, _, axe_matches) = get("/v1/items?q=oozing").await;
    let (_, _, axe) = get(&format!("/v1/items/{}", axe_matches["items"][0]["id"])).await;
    let axe_quests = axe["quests"].as_array().unwrap();
    assert_eq!(
        axe_quests.iter().map(|quest| quest["name"].as_str().unwrap()).collect::<Vec<_>>(),
        ["Ghosts of Perdition", "The Grotto"]
    );
    axe_quests.iter().for_each(assert_no_provenance);

    let (_, _, version) = get("/v1/version").await;
    assert_eq!(version["counts"]["wiki_quests"], 1);
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
    let (_, _, sets) = get_list_rows("/v1/sets").await;
    let winter = sets.as_array().unwrap().iter().find(|s| s["name"] == "Eminence of Winter").unwrap();
    let (_, _, set) = get(&format!("/v1/sets/{}", winter["id"])).await;
    assert!(set["tiers"].as_array().unwrap().iter().all(|t| t["equipped_count"].as_i64().unwrap() >= 2));
    assert_eq!(set["items"][0]["name"], "Legendary Cloak of Winter");
    let two_piece_tier = set["tiers"].as_array().unwrap().iter().find(|t| t["equipped_count"] == 2).unwrap();
    assert_eq!(
        two_piece_tier["effects"][0]["bonuses"][0]["stat"], "Physical Resistance Rating",
        "tier {two_piece_tier}"
    );
    assert_eq!(two_piece_tier["effects"][0]["bonuses"][0]["value"], 30);
    let winter_augment_names: Vec<&str> =
        set["augments"].as_array().unwrap().iter().map(|augment| augment["name"].as_str().unwrap()).collect();
    assert_eq!(winter_augment_names, ["Test Gem of Oozing Resistance"], "the fixture wiki augment joins its set");
    assert_eq!(winter["augment_count"], 1);
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

    let (_, _, races) = get_list_rows("/v1/races").await;
    let dwarf = races.as_array().unwrap().iter().find(|r| r["name"] == "Dwarf").unwrap();
    let (_, _, race) = get(&format!("/v1/races/{}", dwarf["id"])).await;
    assert_eq!(race["ability_modifiers"][0]["stat"], "Constitution");
    assert_eq!(race["granted_feats"].as_array().unwrap().len(), 5);

    let (_, _, classes) = get_list_rows("/v1/classes").await;
    let paladin = classes.as_array().unwrap().iter().find(|c| c["name"] == "Paladin").unwrap();
    let (_, _, class) = get(&format!("/v1/classes/{}", paladin["id"])).await;
    assert_eq!(class["fortitude"], "good");
    assert_eq!(class["spell_slots"]["20"], serde_json::json!([4, 4, 4, 4]));
    assert!(class["spells"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["name"] == "Cure Light Wounds" && s["spell_id"].is_number()));

    let (_, _, enhancement_trees) = get_list_rows("/v1/enhancement-trees").await;
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
                (35..=180).contains(&description.len()),
                "{operation_label}: description {description:?} must name the response in at most two sentences"
            );
            assert!(
                description.split(". ").count() <= 2,
                "{operation_label}: route details belong in schemas and the Introduction: {description:?}"
            );
            for param in operation["parameters"].as_array().into_iter().flatten() {
                let name = param["name"].as_str().unwrap();
                assert!(
                    param["description"].as_str().is_some_and(|d| d.len() >= 15),
                    "{operation_label}: parameter {name} needs a description"
                );
                if name != "sort" {
                    assert!(
                        param["description"].as_str().unwrap().split_whitespace().count() <= 35,
                        "{operation_label}: parameter {name} should be one concise sentence"
                    );
                }
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

async fn sample_response_for(path: &str, example: &Value) -> Value {
    if let Some(list_path) = path.strip_suffix("/{id}") {
        let (_, _, list_response) = get_list_rows(list_path).await;
        let list_rows = list_response
            .as_array()
            .cloned()
            .or_else(|| list_response.as_object().and_then(|o| o.values().find_map(|v| v.as_array().cloned())));
        let id = list_rows
            .and_then(|rows| {
                rows.into_iter()
                    .find(|row| path != "/v1/effects/{id}" || row["kind"] == example["kind"])
                    .and_then(|row| row["id"].as_i64())
            })
            .expect("a row to sample");
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
        let real_response = sample_response_for(path, example).await;
        assert_same_shape(&format!("GET {path}"), example, &real_response);
        checked_response_count += 1;
    }
    assert!(checked_response_count >= 25, "only {checked_response_count} JSON responses carry examples");
}

#[tokio::test]
async fn typed_response_schemas_resolve_every_reference_and_describe_fields() {
    let (_, _, spec) = get("/v1/openapi.json").await;
    let schemas = spec["components"]["schemas"].as_object().unwrap();
    assert!(schemas.len() > 100, "nested response bodies should expose field trees");
    fn inspect(value: &Value, schemas: &serde_json::Map<String, Value>) {
        match value {
            Value::Object(object) => {
                if object.get("type").and_then(Value::as_str) == Some("object") {
                    assert!(
                        object.contains_key("properties") || object.contains_key("additionalProperties"),
                        "untyped object schema"
                    );
                }
                if object.get("type").and_then(Value::as_str) == Some("array") {
                    assert!(object.contains_key("items"), "untyped array schema");
                }
                if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
                    let name = reference.strip_prefix("#/components/schemas/").expect("local schema reference");
                    assert!(schemas.contains_key(name), "unresolved {reference}");
                }
                for field in object.values() {
                    inspect(field, schemas);
                }
            }
            Value::Array(values) => {
                for field in values {
                    inspect(field, schemas);
                }
            }
            _ => {}
        }
    }
    inspect(&spec["paths"], schemas);
    inspect(&spec["components"], schemas);
    for (name, schema) in schemas {
        if let Some(fields) = schema["properties"].as_object() {
            for (field_name, field) in fields {
                if let Some(description) = field["description"].as_str() {
                    assert!(!description.contains("for this record"), "{name}.{field_name} has a filler description");
                    assert!(!description.trim().is_empty(), "{name}.{field_name} has an empty description");
                }
            }
        }
    }
}

#[tokio::test]
async fn every_json_route_response_matches_its_openapi_schema() {
    let (_, _, spec) = get("/v1/openapi.json").await;
    let schemas = spec["components"]["schemas"].as_object().unwrap();
    let mut checked_routes = 0;
    let mut violations = Vec::new();
    for (path, route) in spec["paths"].as_object().unwrap() {
        let Some(schema) = route["get"]["responses"]["200"]["content"]["application/json"].get("schema") else {
            continue;
        };
        if let Some(list_path) = path.strip_suffix("/{id}") {
            let (_, _, rows) = get_list_rows(list_path).await;
            let rows = rows.as_array().expect("detail list rows");
            let matching_rows: Vec<&Value> =
                rows.iter().filter(|row| list_path != "/v1/effects" || row["kind"] == "effect").collect();
            for index in [0, matching_rows.len() / 2, matching_rows.len().saturating_sub(1)] {
                let id = matching_rows[index]["id"].as_i64().expect("detail id");
                let response = get(&format!("{list_path}/{id}")).await.2;
                if let Err(error) = response_matches_schema(&response, schema, schemas, path) {
                    violations.push(error);
                }
            }
        } else {
            let separator = if path.contains('?') { "&" } else { "?" };
            let response = get(&format!("{path}{separator}limit=10000")).await.2;
            if let Err(error) = response_matches_schema(&response, schema, schemas, path) {
                violations.push(error);
            }
        }
        checked_routes += 1;
    }
    assert!(checked_routes >= 40, "only {checked_routes} JSON routes checked");
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

#[tokio::test]
async fn documentation_tags_follow_reader_tasks() {
    let (_, _, spec) = get("/v1/openapi.json").await;
    let names: Vec<&str> = spec["tags"].as_array().unwrap().iter().map(|tag| tag["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        [
            "meta",
            "items",
            "augments",
            "bonuses",
            "sets",
            "crafting",
            "quests",
            "sources",
            "spells",
            "feats",
            "enhancements",
            "classes",
            "races",
            "stances",
            "buffs",
            "bulk"
        ]
    );
}

#[tokio::test]
async fn owner_response_schemas_share_one_typed_effect_line() {
    let (_, _, spec) = get("/v1/openapi.json").await;
    let schemas = &spec["components"]["schemas"];
    let line = &schemas["EffectLine"]["properties"];
    let expected: std::collections::BTreeSet<&str> = [
        "effect_id",
        "name",
        "tier",
        "verbose_name",
        "description",
        "value",
        "value2",
        "bonus_type",
        "bonuses",
        "damage",
    ]
    .into_iter()
    .collect();
    let actual: std::collections::BTreeSet<&str> = line.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(actual, expected);
    let set_tier = schema_reference(&schemas["SetsDetailResponse"]["properties"]["tiers"]["items"], schemas);
    for owner_line in [
        schema_reference(&schemas["ItemsDetailResponse"]["properties"]["effects"]["items"], schemas),
        schema_reference(&schemas["AugmentsDetailResponse"]["properties"]["effects"]["items"], schemas),
        schema_reference(&set_tier["properties"]["effects"]["items"], schemas),
    ] {
        let owner_line = &owner_line["properties"];
        let owner_keys: std::collections::BTreeSet<&str> =
            owner_line.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(owner_keys, expected);
        assert_eq!(
            schema_reference(&owner_line["bonuses"]["items"], schemas)["properties"],
            schemas["EffectStatBonus"]["properties"]
        );
    }
}

fn schema_reference<'a>(schema: &'a Value, schemas: &'a Value) -> &'a Value {
    match schema["$ref"].as_str() {
        Some(reference) => schema_reference(&schemas[reference.trim_start_matches("#/components/schemas/")], schemas),
        None => schema["allOf"]
            .as_array()
            .and_then(|items| items.first())
            .map_or(schema, |item| schema_reference(item, schemas)),
    }
}

#[tokio::test]
async fn effect_and_stat_detail_schemas_type_every_backlink_and_rule() {
    let (_, _, spec) = get("/v1/openapi.json").await;
    let schemas = &spec["components"]["schemas"];
    let family = &schemas["EffectsDetailResponse"]["properties"];
    for owner in [family] {
        for page in ["items", "augments", "set_tiers"] {
            let page_schema = schema_reference(&owner[page], schemas);
            let entry_schema = schema_reference(&page_schema["properties"][page]["items"], schemas);
            assert!(entry_schema["properties"].is_object(), "{page} backlink entries need typed fields");
        }
    }
    assert!(schema_reference(&family["bonuses"]["items"], schemas)["properties"]["amount_from"]["type"].is_string());
    let tier_schema = schema_reference(&family["tier"]["oneOf"][1], schemas);
    assert!(schema_reference(&tier_schema["properties"]["steps"]["items"], schemas)["properties"].is_object());
}

#[tokio::test]
async fn item_and_quest_examples_show_the_quest_chains_they_belong_to() {
    let (_, _, spec) = get("/v1/openapi.json").await;
    for path in ["/v1/items/{id}", "/v1/quests/{id}"] {
        let example = &spec["paths"][path]["get"]["responses"]["200"]["content"]["application/json"]["example"];
        let chain_count = example["quest_chains"].as_array().map_or(0, Vec::len);
        assert!(chain_count > 0, "GET {path}: example belongs to no quest chain");
    }
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

#[tokio::test]
async fn docs_page_carries_the_api_title_and_the_site_mark_as_its_icon() {
    let response = app(fixture_state()).oneshot(Request::get("/v1/docs").body(Body::empty()).unwrap()).await.unwrap();
    let page_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let page = String::from_utf8(page_bytes.to_vec()).unwrap();
    assert!(page.contains("<title>DDO Tools data API</title>"), "docs page title is not the API title");
    assert!(page.contains(r#"<link rel="icon" type="image/svg+xml""#), "docs page has no SVG icon");
    assert!(page.contains(r#""paths":"#), "docs page no longer embeds the spec");
}

const ALL_LIST_PATHS: &[(&str, &str)] = &[
    ("/v1/items", "items"),
    ("/v1/augments", "augments"),
    ("/v1/feats", "feats"),
    ("/v1/spells", "spells"),
    ("/v1/quests", "quests"),
    ("/v1/quest-chains", "quest_chains"),
    ("/v1/sagas", "sagas"),
    ("/v1/vendors", "vendors"),
    ("/v1/events", "events"),
    ("/v1/sets", "sets"),
    ("/v1/enhancement-trees", "enhancement_trees"),
    ("/v1/classes", "classes"),
    ("/v1/races", "races"),
    ("/v1/stances", "stances"),
    ("/v1/guild-buffs", "guild_buffs"),
    ("/v1/optional-buffs", "optional_buffs"),
    ("/v1/crafting-systems", "crafting_systems"),
    ("/v1/effects", "effects"),
    ("/v1/adventure-packs", "adventure_packs"),
    ("/v1/patrons", "patrons"),
    ("/v1/bonus-types", "bonus_types"),
    ("/v1/equipment-slots", "equipment_slots"),
    ("/v1/augment-slot-types", "augment_slot_types"),
    ("/v1/damage-types", "damage_types"),
    ("/v1/weapon-types", "weapon_types"),
    ("/v1/filigrees", "filigrees"),
    ("/v1/sentient-gems", "sentient_gems"),
    ("/v1/clickies", "clickies"),
];

#[tokio::test]
async fn every_scalar_list_column_and_filter_has_a_sort_key() {
    let (_, _, spec) = get("/v1/openapi.json").await;
    let filter_only_toggles = ["include_set_bonuses", "include_legacy"];
    for &(path, rows_key) in ALL_LIST_PATHS {
        let parameters = spec["paths"][path]["get"]["parameters"].as_array().unwrap();
        let sort_description =
            parameters.iter().find(|parameter| parameter["name"] == "sort").unwrap()["description"].as_str().unwrap();
        let allowed: std::collections::BTreeSet<&str> =
            sort_description.split("Allowed fields: ").nth(1).unwrap().split('.').next().unwrap().split(", ").collect();
        let (_, _, page) = get(&format!("{path}?limit=10000")).await;
        let rows = page[rows_key].as_array().unwrap();
        let column_names: std::collections::BTreeSet<&str> =
            rows.iter().flat_map(|row| row.as_object().unwrap().keys()).map(String::as_str).collect();
        let scalar_columns: std::collections::BTreeSet<&str> = column_names
            .into_iter()
            .filter(|name| rows.iter().all(|row| !row[*name].is_array() && !row[*name].is_object()))
            .collect();
        for column in scalar_columns {
            assert!(allowed.contains(column), "{path}: row column {column} cannot be sorted");
        }
        for parameter in parameters {
            if parameter["in"] != "query" {
                continue;
            }
            let name = parameter["name"].as_str().unwrap();
            if ["q", "limit", "offset", "sort"].contains(&name)
                || name.ends_with("_match")
                || filter_only_toggles.contains(&name)
            {
                continue;
            }
            assert!(allowed.contains(name), "{path}: filter {name} cannot be sorted");
        }
    }
}

#[tokio::test]
async fn boolean_sort_fields_put_false_before_true_and_keep_null_last() {
    for key in ["is_raid", "is_rare", "is_legacy"] {
        let (_, _, ascending) = get(&format!("/v1/items?include_legacy=true&sort={key}&limit=10000")).await;
        let values: Vec<bool> =
            ascending["items"].as_array().unwrap().iter().map(|item| item[key].as_bool().unwrap()).collect();
        assert!(values.windows(2).all(|pair| !pair[0] || pair[1]), "{key} ascending");
        let (_, _, descending) = get(&format!("/v1/items?include_legacy=true&sort=-{key}&limit=10000")).await;
        let values: Vec<bool> =
            descending["items"].as_array().unwrap().iter().map(|item| item[key].as_bool().unwrap()).collect();
        assert!(values.windows(2).all(|pair| pair[0] || !pair[1]), "{key} descending");
    }
}

#[tokio::test]
async fn list_parameter_docs_share_descriptions_and_match_runtime_sort_fields() {
    let (_, _, spec) = get("/v1/openapi.json").await;
    let reference_params = spec["paths"]["/v1/patrons"]["get"]["parameters"].as_array().unwrap();
    for &(path, _) in ALL_LIST_PATHS {
        let params = spec["paths"][path]["get"]["parameters"].as_array().unwrap();
        for name in ["q", "limit", "offset"] {
            let reference = reference_params.iter().find(|param| param["name"] == name).unwrap();
            let parameter = params.iter().find(|param| param["name"] == name).unwrap();
            if name != "q" || !matches!(path, "/v1/items" | "/v1/augment-slot-types" | "/v1/effects") {
                assert_eq!(parameter, reference, "{path}: {name} must use the common parameter contract");
            }
        }
        let search_description =
            params.iter().find(|param| param["name"] == "q").unwrap()["description"].as_str().unwrap();
        let expected_search_description = match path {
            "/v1/items" => {
                "Case-insensitive item-name substring or exact slot, category or pack name; exact and prefix names rank first without sort."
            }
            "/v1/effects" => {
                "Case-insensitive own-name or granted-stat substring; without sort, exact, prefix and substring own names rank before stat-only matches."
            }
            "/v1/augment-slot-types" => {
                "Case-insensitive substring of the socket's display `label`; \
                surrounding whitespace is trimmed and blank applies no search."
            }
            _ => {
                "Case-insensitive name substring after trimming; blank applies no search."
            }
        };
        assert_eq!(search_description, expected_search_description, "{path}");
        let sort = params.iter().find(|param| param["name"] == "sort").unwrap();
        assert_eq!(sort["style"], "form", "{path}");
        assert_eq!(sort["explode"], true, "{path}");
        let documented_fields =
            sort["description"].as_str().unwrap().split("Allowed fields: ").nth(1).unwrap().split('.').next().unwrap();
        let (status, _, response) = get(&format!("{path}?sort=unknown_sort_field")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let runtime_fields = response["error"].as_str().unwrap().split("allowed fields: ").nth(1).unwrap();
        assert_eq!(documented_fields, runtime_fields, "{path}");
    }
}

#[tokio::test]
async fn every_list_route_has_paging_sorting_and_an_envelope() {
    let (_, _, spec) = get("/v1/openapi.json").await;
    for &(path, rows_key) in ALL_LIST_PATHS {
        let operation = &spec["paths"][path]["get"];
        let parameters = operation["parameters"].as_array().unwrap_or_else(|| panic!("{path} has no parameters"));
        for name in ["q", "limit", "offset", "sort"] {
            let parameter = parameters
                .iter()
                .find(|parameter| parameter["name"] == name && parameter["in"] == "query")
                .unwrap_or_else(|| panic!("{path} has no {name} query parameter"));
            assert!(parameter["description"].as_str().is_some_and(|description| !description.is_empty()));
        }
        let sort_description =
            parameters.iter().find(|parameter| parameter["name"] == "sort").unwrap()["description"].as_str().unwrap();
        assert!(sort_description.contains("name"), "{path}: {sort_description}");
        if path != "/v1/effects" {
            assert!(sort_description.contains("id"), "{path}: {sort_description}");
        }
        let (status, _, page) = get(&format!("{path}?limit=1&offset=0&sort=-name")).await;
        assert_eq!(status, StatusCode::OK, "{path}: {page}");
        assert_eq!(page["limit"], 1, "{path}");
        assert_eq!(page["offset"], 0, "{path}");
        assert!(page["total"].is_number(), "{path}");
        assert!(page[rows_key].is_array(), "{path}: {page}");
        assert!(page[rows_key].as_array().unwrap().len() <= 1, "{path}");
    }
}

#[tokio::test]
async fn repeated_sort_keys_and_invalid_keys_follow_route_allow_lists() {
    let (_, _, sorted) = get("/v1/items?sort=-name&sort=minimum_level&limit=10000").await;
    let rows = sorted["items"].as_array().unwrap();
    assert!(rows.windows(2).all(|pair| {
        let first_name = pair[0]["name"].as_str().unwrap();
        let second_name = pair[1]["name"].as_str().unwrap();
        first_name > second_name
            || first_name == second_name && pair[0]["minimum_level"].as_i64() <= pair[1]["minimum_level"].as_i64()
    }));
    let (_, _, grouped) = get("/v1/items?sort=category&sort=-name&limit=10000").await;
    let grouped_rows = grouped["items"].as_array().unwrap();
    assert!(grouped_rows.windows(2).all(|pair| {
        let first_category = pair[0]["category"].as_str().unwrap();
        let second_category = pair[1]["category"].as_str().unwrap();
        first_category < second_category
            || first_category == second_category && pair[0]["name"].as_str() >= pair[1]["name"].as_str()
    }));
    for bad_sort in ["missing", "-", "", "name,minimum_level"] {
        let (status, _, response) = get(&format!("/v1/items?sort={bad_sort}")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let message = response["error"].as_str().unwrap();
        assert!(message.contains(bad_sort) && message.contains("minimum_level"), "{message}");
    }
}

#[tokio::test]
async fn every_documented_sort_key_runs_and_paging_is_clamped() {
    let (_, _, spec) = get("/v1/openapi.json").await;
    for &(path, rows_key) in ALL_LIST_PATHS {
        let params = spec["paths"][path]["get"]["parameters"].as_array().unwrap();
        let sort_description =
            params.iter().find(|param| param["name"] == "sort").unwrap()["description"].as_str().unwrap();
        let fields = sort_description.split("Allowed fields: ").nth(1).unwrap().split('.').next().unwrap();
        for field in fields.split(", ") {
            for prefix in ["", "-"] {
                let (status, _, response) = get(&format!("{path}?sort={prefix}{field}&limit=1")).await;
                assert_eq!(status, StatusCode::OK, "{path} sort={prefix}{field}: {response}");
            }
        }
        let (status, _, page) = get(&format!("{path}?limit=-4&offset=-5")).await;
        assert_eq!(status, StatusCode::OK, "{path}: {page}");
        assert_eq!(page["limit"], 1);
        assert_eq!(page["offset"], 0);
        assert!(page[rows_key].as_array().unwrap().len() <= 1);
        let (status, _, capped_page) = get(&format!("{path}?limit=20000&offset=100000")).await;
        assert_eq!(status, StatusCode::OK, "{path}: {capped_page}");
        assert_eq!(capped_page["limit"], 10000);
        assert_eq!(capped_page["offset"], 100000);
        assert!(capped_page[rows_key].as_array().unwrap().is_empty());
        let (status, _, invalid) = get(&format!("{path}?sort=not_a_field")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}: {invalid}");
        assert!(invalid["error"].as_str().unwrap().contains("not_a_field"));
    }
}

#[tokio::test]
async fn every_list_query_searches_its_display_name() {
    for &(path, rows_key) in ALL_LIST_PATHS {
        let (_, _, first_page) = get(&format!("{path}?limit=1")).await;
        let Some(first_row) = first_page[rows_key].as_array().unwrap().first() else { continue };
        let display_name = if path == "/v1/augment-slot-types" { &first_row["label"] } else { &first_row["name"] };
        let search_letter = display_name.as_str().unwrap().chars().find(char::is_ascii_alphabetic).unwrap();
        let (status, _, matched_page) = get(&format!("{path}?q={}&limit=1", search_letter.to_ascii_uppercase())).await;
        assert_eq!(status, StatusCode::OK, "{path}: {matched_page}");
        assert!(matched_page["total"].as_i64().unwrap() > 0, "{path}: {matched_page}");
        let matched_name = if path == "/v1/augment-slot-types" {
            &matched_page[rows_key][0]["label"]
        } else {
            &matched_page[rows_key][0]["name"]
        };
        assert!(
            matched_name.as_str().unwrap().to_ascii_lowercase().contains(search_letter.to_ascii_lowercase()),
            "{path}: {matched_page}"
        );
    }
}

#[tokio::test]
async fn nullable_quest_sort_fields_put_null_last_in_both_directions() {
    for direction in ["legendary_level", "-legendary_level"] {
        let (status, _, page) = get(&format!("/v1/quests?sort={direction}&limit=10000")).await;
        assert_eq!(status, StatusCode::OK);
        let rows = page["quests"].as_array().unwrap();
        assert!(rows.iter().any(|row| row["legendary_level"].is_null()));
        assert!(rows.iter().any(|row| row["legendary_level"].is_number()));
        let first_null = rows.iter().position(|row| row["legendary_level"].is_null()).unwrap();
        assert!(rows[first_null..].iter().all(|row| row["legendary_level"].is_null()), "{direction}");
    }
}

fn param_schema_type(param: &Value) -> &str {
    let schema_type = &param["schema"]["type"];
    schema_type
        .as_str()
        .or_else(|| schema_type.as_array().into_iter().flatten().filter_map(Value::as_str).find(|t| *t != "null"))
        .unwrap_or("string")
}

fn accepted_sample_value(param_name: &str, schema_type: &str) -> &'static str {
    match (param_name, schema_type) {
        ("sort", _) => "name",
        ("slot", _) => "Wrists",
        ("category", _) => "Armor",
        ("source", _) => "standard",
        ("bonus", _) => "Strength",
        ("set", _) => "Cooking%20By%20the%20Book",
        ("kind", _) => "stat",
        ("bonus_match" | "pack_match" | "quest_match" | "quest_chain_match" | "saga_match" | "set_match", _) => "any",
        ("quest" | "quest_chain" | "saga", _) => "1",
        (_, "boolean") => "true",
        (_, "integer" | "number") => "1",
        _ => "x",
    }
}

#[tokio::test]
async fn list_endpoints_reject_unknown_query_parameters_as_json() {
    let (_, _, spec) = get("/v1/openapi.json").await;
    for &(path, _) in ALL_LIST_PATHS {
        let (status, headers, json) = get(&format!("{path}?bogus=1")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}?bogus=1 must be rejected");
        assert!(
            headers.get(header::CONTENT_TYPE).is_some_and(|v| v.to_str().unwrap().starts_with("application/json")),
            "{path}?bogus=1 must answer with JSON"
        );
        let error = json["error"].as_str().unwrap_or_else(|| panic!("{path}?bogus=1 body {json} has no error string"));
        assert!(error.contains("bogus"), "{path}?bogus=1 error {error:?} must name the parameter");
        let accepted_parameters = error
            .split("; allowed parameters: ")
            .nth(1)
            .unwrap_or_else(|| panic!("{path}: {error} must list all accepted parameters"));
        let accepted_parameters: std::collections::BTreeSet<_> = accepted_parameters.split(", ").collect();
        let documented_parameters: std::collections::BTreeSet<_> = spec["paths"][path]["get"]["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|parameter| parameter["in"] == "query")
            .map(|parameter| parameter["name"].as_str().unwrap())
            .collect();
        assert_eq!(accepted_parameters, documented_parameters, "{path}: {error}");
    }

    let (status, _, json) = get("/v1/items?raid=maybe").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(json["error"].as_str().is_some_and(|e| e.contains("raid")), "malformed raid body {json} must be JSON");
}

#[tokio::test]
async fn list_endpoints_reject_duplicate_or_malformed_common_parameters() {
    for &(path, _) in ALL_LIST_PATHS {
        for (query_string, bad_parameter) in [
            ("q=one&q=two", "q"),
            ("limit=1&limit=2", "limit"),
            ("offset=0&offset=1", "offset"),
            ("limit=all", "limit"),
            ("offset=first", "offset"),
        ] {
            let (status, _, response) = get(&format!("{path}?{query_string}")).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{path}?{query_string}: {response}");
            assert!(response["error"].as_str().unwrap().contains(bad_parameter), "{response}");
        }
    }
}

#[tokio::test]
async fn list_endpoints_accept_every_documented_query_parameter() {
    let (_, _, spec) = get("/v1/openapi.json").await;
    for &(path, _) in ALL_LIST_PATHS {
        let operation = &spec["paths"][path]["get"];
        assert!(operation["responses"]["400"]["description"].is_string(), "{path} must document its 400 response");
        let documented_params =
            operation["parameters"].as_array().unwrap_or_else(|| panic!("{path} documents no parameters"));
        let query_params: Vec<&Value> = documented_params.iter().filter(|p| p["in"] == "query").collect();
        assert!(query_params.len() >= 4, "{path} documents only {} query parameters", query_params.len());
        for param in query_params {
            let name = param["name"].as_str().unwrap();
            if matches!(name, "slot_match" | "category_match") {
                continue;
            }
            let sample_value = accepted_sample_value(name, param_schema_type(param));
            let (status, _, json) = get(&format!("{path}?{name}={sample_value}")).await;
            assert_eq!(status, StatusCode::OK, "{path}?{name}={sample_value} is documented but rejected: {json}");
        }
    }
}

#[tokio::test]
async fn openapi_documents_item_filters_and_shared_query_conventions() {
    let (_, _, spec) = get("/v1/openapi.json").await;
    let operation = &spec["paths"]["/v1/items"]["get"];
    let parameters = operation["parameters"].as_array().unwrap();
    let names: std::collections::BTreeSet<_> =
        parameters.iter().map(|parameter| parameter["name"].as_str().unwrap()).collect();
    for name in [
        "slot",
        "slot_match",
        "category",
        "category_match",
        "pack",
        "pack_match",
        "quest",
        "quest_match",
        "quest_chain",
        "quest_chain_match",
        "saga",
        "saga_match",
        "bonus",
        "bonus_match",
        "include_set_bonuses",
        "include_legacy",
    ] {
        assert!(names.contains(name), "missing {name}");
    }
    for name in ["slot", "category", "pack", "quest", "quest_chain", "saga", "bonus"] {
        let parameter = parameters.iter().find(|parameter| parameter["name"] == name).unwrap();
        assert_eq!(parameter["schema"]["type"], "array", "{name}");
        assert_eq!(parameter["style"], "form", "{name}");
        assert_eq!(parameter["explode"], true, "{name}");
    }
    let introduction = spec["info"]["description"].as_str().unwrap();
    for phrase in [
        "**Query parameters.**",
        "**Effects and bonuses.**",
        "one id space",
        "detail_path",
        "bonus_match",
        "slot_match",
        "stat:bonus type",
    ] {
        assert!(introduction.contains(phrase), "missing {phrase} in Introduction");
    }
}

#[tokio::test]
async fn stances_lists_the_standalone_stances_in_file_order() {
    let (status, _, json) = get_list_rows("/v1/stances").await;
    assert_eq!(status, StatusCode::OK);
    let stances = json.as_array().unwrap();
    let names: Vec<&str> = stances.iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Two Weapon Fighting", "Two Handed Fighting", "Aura of Good"]);
    assert_eq!(stances[0]["auto_controlled"], true);
    assert_eq!(stances[0]["group_name"], "Auto");
    assert_eq!(stances[0]["requirements"].as_array().unwrap().len(), 10);
    assert_eq!(stances[2]["requirements"][0]["value"], "1");
    assert_eq!(stances[0]["modifiers"], serde_json::json!([]));
    assert!(stances.iter().all(|stance| stance.get("sort_order").is_none()));
    let (_, _, page) = get("/v1/stances?sort=name&limit=1&offset=1").await;
    assert_eq!(page["total"], 3);
    assert_eq!(page["stances"], serde_json::json!([stances[1]]));
}

#[tokio::test]
async fn guild_buffs_carry_their_unlock_level_and_per_level_modifiers() {
    let (status, _, json) = get_list_rows("/v1/guild-buffs").await;
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
    let (status, _, json) = get_list_rows("/v1/optional-buffs").await;
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
    let (status, _, json) = get_list_rows("/v1/sentient-gems").await;
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

    let (_, _, enhancement_trees) = get_list_rows("/v1/enhancement-trees").await;
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
    let (status, _, json) = get_list_rows("/v1/crafting-systems").await;
    assert_eq!(status, StatusCode::OK);
    let crafting_systems = json.as_array().unwrap();
    let names: Vec<&str> = crafting_systems.iter().map(|system| system["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Catalyst Crafting", "Heroic Green Steel", "Test Upgrade Altar", "Thunder-Forged"]);
    let crafting_system = &crafting_systems[1];
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
    let upgrade_system = &crafting_systems[2];
    assert_eq!(upgrade_system["name"], "Test Upgrade Altar");
    assert_eq!(upgrade_system["families"], serde_json::json!([]));
}

#[tokio::test]
async fn crafting_system_detail_carries_ingredients_and_recipes_with_augments_and_cost() {
    let (_, _, list) = get_list_rows("/v1/crafting-systems").await;
    let id = list[1]["id"].as_i64().unwrap();
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

    let upgrade_id = list[2]["id"].as_i64().unwrap();
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

#[tokio::test]
async fn an_item_both_dropped_and_given_as_a_quests_end_reward_appears_once_per_loot_type() {
    let crown_id = id_of_item_named("Rusted Crown").await;
    let (_, _, crown) = get(&format!("/v1/items/{crown_id}")).await;
    let crown_links: Vec<(&Value, &Value)> =
        crown["quests"].as_array().unwrap().iter().map(|quest| (&quest["name"], &quest["loot_type"])).collect();
    assert_eq!(
        crown_links,
        [
            (&serde_json::json!("The Tide Turns"), &serde_json::json!("chest")),
            (&serde_json::json!("The Tide Turns"), &serde_json::json!("reward"))
        ]
    );
    let quest_id = crown["quests"][0]["id"].as_i64().unwrap();
    let (_, _, quest) = get(&format!("/v1/quests/{quest_id}")).await;
    let crown_loot_types: Vec<&Value> = quest["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["id"] == crown_id)
        .map(|item| &item["loot_type"])
        .collect();
    assert_eq!(crown_loot_types, [&serde_json::json!("chest"), &serde_json::json!("reward")]);
}

async fn id_of_item_named(item_name: &str) -> i64 {
    let (_, _, items) = get(&format!("/v1/items?q={}", item_name.replace(' ', "+"))).await;
    items["items"].as_array().unwrap().iter().find(|item| item["name"] == item_name).unwrap()["id"].as_i64().unwrap()
}

#[tokio::test]
async fn items_augments_and_quests_carry_corrected_values_without_exposing_the_corrections() {
    let (_, _, docent) = get(&format!("/v1/items/{}", id_of_item_named("Docent of Defiance").await)).await;
    assert_eq!(docent["minimum_level"], 11);
    assert!(docent.get("corrections").is_none(), "item detail exposes corrections: {docent}");

    let (_, _, augments) = get("/v1/augments?q=voidscale").await;
    let voidscale_id = augments["augments"][0]["id"].as_i64().unwrap();
    let (_, _, voidscale) = get(&format!("/v1/augments/{voidscale_id}")).await;
    assert_eq!(voidscale["min_level"], 30);
    assert!(voidscale.get("corrections").is_none(), "augment detail exposes corrections: {voidscale}");

    let (_, _, quests) = get_list_rows("/v1/quests").await;
    let quests = quests.as_array().unwrap();
    let plane_of_night = quests.iter().find(|quest| quest["name"] == "Plane of Night").unwrap();
    assert_eq!(plane_of_night["favor"], 6);
    assert!(
        quests.iter().all(|quest| quest.get("corrections").is_none()),
        "a quest row exposes corrections: {plane_of_night}"
    );

    let (_, _, version) = get("/v1/version").await;
    assert!(version["counts"].get("corrections").is_none(), "version counts expose corrections: {}", version["counts"]);
}

fn assert_no_provenance(response_object: &Value) {
    assert!(response_object.get("provenance").is_none(), "provenance is internal: {response_object}");
}

fn keys_of(value: &Value) -> Vec<&str> {
    let mut keys: Vec<&str> = value.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    keys
}

#[tokio::test]
async fn quest_chains_list_their_pack_page_and_counts() {
    let (status, _, chains) = get_list_rows("/v1/quest-chains").await;
    assert_eq!(status, StatusCode::OK);
    let lost_seekers = &chains[0];
    assert_eq!(keys_of(lost_seekers), ["id", "name", "pack", "quest_count", "reward_count", "wiki_url"]);
    assert_eq!(
        (&lost_seekers["name"], &lost_seekers["pack"], &lost_seekers["wiki_url"]),
        (
            &serde_json::json!("The Lost Seekers"),
            &serde_json::json!("Free to Play"),
            &serde_json::json!("https://ddowiki.com/page/The_Lost_Seekers")
        )
    );
    assert_eq!(
        (&lost_seekers["quest_count"], &lost_seekers["reward_count"]),
        (&serde_json::json!(2), &serde_json::json!(3))
    );
}

#[tokio::test]
async fn quest_chain_detail_lists_its_quests_in_order_and_its_rewards() {
    let (_, _, chains) = get_list_rows("/v1/quest-chains").await;
    let (status, _, chain) = get(&format!("/v1/quest-chains/{}", chains[0]["id"])).await;
    assert_eq!(status, StatusCode::OK);
    assert_no_provenance(&chain);
    for (field_name, value) in chains[0].as_object().unwrap() {
        assert_eq!(&chain[field_name], value, "{field_name} as the list returns it");
    }
    let quests = chain["quests"].as_array().unwrap();
    assert_eq!(keys_of(&quests[0]), ["id", "level", "name"]);
    assert_eq!(quests.iter().map(|q| q["name"].as_str().unwrap()).collect::<Vec<_>>(), ["The Grotto", "Redemption"]);
    let rewards = chain["rewards"].as_array().unwrap();
    assert_eq!(keys_of(&rewards[0]), ["id", "is_rare", "minimum_level", "name", "slot"]);
    assert_eq!(
        rewards.iter().map(|r| (r["name"].as_str().unwrap(), r["is_rare"].as_bool().unwrap())).collect::<Vec<_>>(),
        [("Acrobat's Ring", false), ("Docent of Defiance", false), ("Kundarak Delving Boots", true)]
    );
    let (status, _, body) = get("/v1/quest-chains/999999").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].is_string(), "{body}");
}

#[tokio::test]
async fn saga_detail_lists_its_rewards_by_tier() {
    let (status, _, sagas) = get_list_rows("/v1/sagas").await;
    assert_eq!(status, StatusCode::OK);
    let sharn = sagas.as_array().unwrap().iter().find(|saga| saga["name"] == "Masterminds of Sharn").unwrap();
    assert_eq!(keys_of(sharn), ["id", "name", "pack", "quest_count", "reward_count", "wiki_url"]);
    assert_eq!((&sharn["quest_count"], &sharn["reward_count"]), (&serde_json::json!(2), &serde_json::json!(4)));
    let (_, _, saga) = get(&format!("/v1/sagas/{}", sharn["id"])).await;
    assert_no_provenance(&saga);
    let rewards = saga["rewards"].as_array().unwrap();
    assert_eq!(keys_of(&rewards[0]), ["id", "is_rare", "minimum_level", "name", "slot", "tier"]);
    assert_eq!(
        rewards.iter().map(|r| (r["name"].as_str().unwrap(), r["tier"].clone())).collect::<Vec<_>>(),
        [
            ("Band of Diani ir'Wynarn", serde_json::json!("epic")),
            ("Five Rings", serde_json::json!("epic")),
            ("Five Rings", serde_json::json!("legendary")),
            ("Alabaster of the Twelve", Value::Null)
        ],
        "heroic, epic, legendary, then untiered, each by name"
    );
    assert_eq!(
        saga["quests"].as_array().unwrap().iter().map(|q| q["name"].as_str().unwrap()).collect::<Vec<_>>(),
        ["Project Nemesis", "Ghosts of Perdition"]
    );
    let (status, _, _) = get("/v1/sagas/999999").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn item_and_quest_detail_name_the_quest_chains_and_sagas_they_belong_to() {
    let (_, _, ring) = get(&format!("/v1/items/{}", id_of_item_named("Acrobat's Ring").await)).await;
    assert_eq!(keys_of(&ring["quest_chains"][0]), ["id", "is_rare", "name", "wiki_url"]);
    assert_eq!(ring["quest_chains"][0]["name"], "The Lost Seekers");
    assert_eq!(ring["quest_chains"][0]["wiki_url"], "https://ddowiki.com/page/The_Lost_Seekers");
    assert_eq!(ring["sagas"], serde_json::json!([]));
    let (_, _, band) = get(&format!("/v1/items/{}", id_of_item_named("Band of Diani ir'Wynarn").await)).await;
    assert_eq!(keys_of(&band["sagas"][0]), ["id", "is_rare", "name", "tier", "wiki_url"]);
    assert_eq!(band["sagas"][0]["wiki_url"], "https://ddowiki.com/page/Masterminds_of_Sharn_(saga)");
    assert_eq!(
        (&band["sagas"][0]["name"], &band["sagas"][0]["tier"]),
        (&serde_json::json!("Masterminds of Sharn"), &serde_json::json!("epic"))
    );

    let quest_id = band["quests"][0]["id"].as_i64().unwrap();
    let (_, _, project_nemesis) = get(&format!("/v1/quests/{quest_id}")).await;
    assert_eq!(keys_of(&project_nemesis["sagas"][0]), ["id", "name", "wiki_url"]);
    assert_eq!(project_nemesis["sagas"][0]["wiki_url"], "https://ddowiki.com/page/Masterminds_of_Sharn_(saga)");
    assert_eq!(project_nemesis["sagas"][0]["name"], "Masterminds of Sharn");
    assert_eq!(project_nemesis["quest_chains"], serde_json::json!([]));
}

#[tokio::test]
async fn version_counts_quest_chains_sagas_and_their_rewards() {
    let (_, _, version) = get("/v1/version").await;
    let counts = &version["counts"];
    assert_eq!(
        (&counts["quest_chains"], &counts["quest_chain_rewards"], &counts["sagas"], &counts["saga_rewards"]),
        (&serde_json::json!(1), &serde_json::json!(3), &serde_json::json!(2), &serde_json::json!(5))
    );
}

async fn response_to_cross_origin_get(router: axum::Router, path: &str) -> axum::response::Response {
    let request = Request::get(path)
        .header(header::ORIGIN, "https://ddo-tools.com")
        .header("x-forwarded-for", "203.0.113.7")
        .body(Body::empty())
        .unwrap();
    router.oneshot(request).await.unwrap()
}

#[tokio::test]
async fn a_rate_limited_response_carries_cors_headers_and_exposes_retry_after() {
    let router = app(fixture_state().with_rate_limit());
    let mut rate_limited_response = None;
    for _ in 0..200 {
        let response = response_to_cross_origin_get(router.clone(), "/v1/version").await;
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            rate_limited_response = Some(response);
            break;
        }
    }
    let response = rate_limited_response.expect("the burst of 100 runs out within 200 requests");
    assert_eq!(response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).map(|v| v.to_str().unwrap()), Some("*"));
    let exposed_headers =
        response.headers().get(header::ACCESS_CONTROL_EXPOSE_HEADERS).map(|v| v.to_str().unwrap().to_lowercase());
    assert!(exposed_headers.is_some_and(|names| names.contains("retry-after")), "{:?}", response.headers());
    assert!(response.headers().get(header::RETRY_AFTER).is_some(), "{:?}", response.headers());
}

#[tokio::test]
async fn a_client_can_sustain_five_requests_per_second_after_the_burst() {
    let router = app(fixture_state().with_rate_limit());
    for _ in 0..100 {
        assert_eq!(response_to_cross_origin_get(router.clone(), "/v1/version").await.status(), StatusCode::OK);
    }
    let started = std::time::Instant::now();
    for _ in 0..20 {
        tokio::time::sleep(std::time::Duration::from_millis(210)).await;
        assert_eq!(response_to_cross_origin_get(router.clone(), "/v1/version").await.status(), StatusCode::OK);
    }
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}

#[tokio::test]
async fn a_not_found_response_carries_cors_headers() {
    let response = response_to_cross_origin_get(app(fixture_state().with_rate_limit()), "/v1/items/999999").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).map(|v| v.to_str().unwrap()), Some("*"));
    let response = response_to_cross_origin_get(app(fixture_state()), "/no-such-route").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).map(|v| v.to_str().unwrap()), Some("*"));
}

#[tokio::test]
async fn item_detail_lists_only_pack_wide_drops_in_adventure_packs() {
    let (_, _, crossbow) =
        get(&format!("/v1/items/{}", id_of_item_named("Light Crossbow of the Golden Age").await)).await;
    assert_eq!(crossbow["quests"], serde_json::json!([]));
    let packs = crossbow["adventure_packs"].as_array().unwrap();
    assert_eq!(packs.len(), 1, "{packs:?}");
    assert_eq!(keys_of(&packs[0]), ["chest", "id", "is_rare", "loot_type", "name", "wiki_url"]);
    assert_eq!(packs[0]["wiki_url"], "https://ddowiki.com/page/Magic_of_Myth_Drannor", "the page named after the pack");
    assert_eq!(
        (&packs[0]["name"], &packs[0]["loot_type"], &packs[0]["chest"], &packs[0]["is_rare"]),
        (
            &serde_json::json!("Magic of Myth Drannor"),
            &serde_json::json!("chest"),
            &serde_json::json!("any end chest"),
            &serde_json::json!(true)
        )
    );
    let (_, _, ring) = get(&format!("/v1/items/{}", id_of_item_named("Acrobat's Ring").await)).await;
    assert_eq!(ring["adventure_packs"], serde_json::json!([]));
}

#[tokio::test]
async fn detail_pack_locations_only_list_sources_that_are_pack_wide() {
    let (_, _, ring) = get(&format!("/v1/items/{}", id_of_item_named("Acrobat's Ring").await)).await;
    assert!(ring["adventure_packs"].as_array().unwrap().is_empty());
    assert_eq!(ring["sources"][0]["kind"], "quest_chain");

    let (_, _, page) = get("/v1/augments?q=elemental+absorption").await;
    let (_, _, augment) = get(&format!("/v1/augments/{}", page["augments"][0]["id"])).await;
    assert!(augment["adventure_packs"].as_array().unwrap().is_empty());
    assert_eq!(augment["sources"][0]["kind"], "quest");

    let (_, _, crossbow) =
        get(&format!("/v1/items/{}", id_of_item_named("Light Crossbow of the Golden Age").await)).await;
    assert_eq!(crossbow["adventure_packs"].as_array().unwrap().len(), 1);
    assert_eq!(crossbow["sources"][0]["kind"], "adventure_pack");
}

#[tokio::test]
async fn detail_drop_locations_are_unique_for_every_fixture_item_and_augment() {
    let source_packs = source_pack_names(&rusqlite::Connection::open(fixture_db_path()).unwrap()).unwrap();
    for resource in ["items", "augments"] {
        let (_, _, rows) = get_list_rows(&format!("/v1/{resource}")).await;
        for row in rows.as_array().unwrap() {
            let path = format!("/v1/{resource}/{}", row["id"]);
            let (status, _, detail) = get(&path).await;
            assert_eq!(status, StatusCode::OK, "{path}");
            detail_has_unique_drop_locations(&detail, &path, &source_packs).unwrap();
        }
    }
}

#[tokio::test]
async fn detail_drop_validation_rejects_repeated_and_rolled_up_locations() {
    let source_packs = source_pack_names(&rusqlite::Connection::open(fixture_db_path()).unwrap()).unwrap();
    let (_, _, mut ring) = get(&format!("/v1/items/{}", id_of_item_named("Acrobat's Ring").await)).await;
    ring["quests"].as_array_mut().unwrap().push(serde_json::json!({"name": "The Grotto"}));
    ring["quests"].as_array_mut().unwrap().push(serde_json::json!({"name": "The Grotto"}));
    assert!(detail_has_unique_drop_locations(&ring, "/v1/items/fixture", &source_packs)
        .unwrap_err()
        .contains("duplicate"));

    let (_, _, mut ring) = get(&format!("/v1/items/{}", id_of_item_named("Acrobat's Ring").await)).await;
    ring["adventure_packs"].as_array_mut().unwrap().push(serde_json::json!({
        "id": 1, "name": "Free to Play", "loot_type": null, "chest": null, "is_rare": false
    }));
    assert!(detail_has_unique_drop_locations(&ring, "/v1/items/fixture", &source_packs)
        .unwrap_err()
        .contains("pack-wide"));

    let (_, _, mut crossbow) =
        get(&format!("/v1/items/{}", id_of_item_named("Light Crossbow of the Golden Age").await)).await;
    crossbow["sources"].as_array_mut().unwrap().push(serde_json::json!({
        "kind": "quest_chain", "id": 999, "chest": "any end chest"
    }));
    let mut source_packs_with_overlap = source_packs;
    source_packs_with_overlap.entry(("quest_chain".into(), 999)).or_default().insert("Magic of Myth Drannor".into());
    assert!(detail_has_unique_drop_locations(&crossbow, "/v1/items/fixture", &source_packs_with_overlap)
        .unwrap_err()
        .contains("repeats a"));
}

#[tokio::test]
async fn adventure_pack_detail_lists_the_loot_any_of_its_quests_drops() {
    let (_, _, packs) = get_list_rows("/v1/adventure-packs").await;
    let pack_id = packs.as_array().unwrap().iter().find(|pack| pack["name"] == "Magic of Myth Drannor").unwrap()["id"]
        .as_i64()
        .unwrap();
    let (status, _, pack) = get(&format!("/v1/adventure-packs/{pack_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(keys_of(&pack), ["augments", "id", "is_free_to_play", "items", "name"]);
    assert_eq!(pack["is_free_to_play"], false);
    let items = pack["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(keys_of(&items[0]), ["chest", "id", "is_rare", "loot_type", "minimum_level", "name", "slot"]);
    assert_eq!(items[0]["name"], "Light Crossbow of the Golden Age");
    assert_eq!(items[0]["is_rare"], true);
    assert_eq!(pack["augments"], serde_json::json!([]));
    let (status, _, _) = get("/v1/adventure-packs/999999").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn item_and_augment_detail_list_every_source_in_one_array() {
    let (_, _, band) = get(&format!("/v1/items/{}", id_of_item_named("Band of Diani ir'Wynarn").await)).await;
    let sources = band["sources"].as_array().unwrap();
    assert_eq!(
        keys_of(&sources[0]),
        ["character_level", "chest", "cost", "id", "is_rare", "kind", "loot_type", "name", "tier", "wiki_url"]
    );
    let source_summaries: Vec<(&str, &str, Option<&str>, Option<&str>)> = sources
        .iter()
        .map(|source| {
            (
                source["kind"].as_str().unwrap(),
                source["name"].as_str().unwrap(),
                source["loot_type"].as_str(),
                source["tier"].as_str(),
            )
        })
        .collect();
    assert_eq!(
        source_summaries,
        [("quest", "Project Nemesis", Some("raid"), None), ("saga", "Masterminds of Sharn", None, Some("epic"))],
        "quests first, then chains, sagas and packs"
    );
    assert_eq!(sources[0]["id"], band["quests"][0]["id"]);
    assert_eq!(sources[0]["wiki_url"], "https://ddowiki.com/page/Project_Nemesis");
    assert_eq!(sources[1]["wiki_url"], "https://ddowiki.com/page/Masterminds_of_Sharn_(saga)");

    let (_, _, crossbow) =
        get(&format!("/v1/items/{}", id_of_item_named("Light Crossbow of the Golden Age").await)).await;
    assert_eq!(crossbow["sources"][0]["kind"], "adventure_pack");
    assert_eq!(crossbow["sources"][0]["id"], crossbow["adventure_packs"][0]["id"]);
    assert_eq!(crossbow["sources"][0]["wiki_url"], "https://ddowiki.com/page/Magic_of_Myth_Drannor");
    let (_, _, ring) = get(&format!("/v1/items/{}", id_of_item_named("Acrobat's Ring").await)).await;
    assert_eq!(ring["sources"][0]["kind"], "quest_chain");
    assert_eq!(ring["sources"][0]["loot_type"], Value::Null);

    let (_, _, list) = get("/v1/augments?q=elemental+absorption").await;
    let (_, _, augment) = get(&format!("/v1/augments/{}", list["augments"][0]["id"])).await;
    assert_eq!(augment["adventure_packs"], serde_json::json!([]));
    assert_eq!(augment["sources"][0]["kind"], "quest");
    assert_eq!(augment["sources"][0]["name"], "Land of Lamordia");
    assert_eq!(augment["sources"][0]["chest"], "vornir frosthelm's chest");
    assert_eq!(augment["sources"][0]["wiki_url"], "https://ddowiki.com/page/Land_of_Lamordia");
}

#[tokio::test]
async fn item_detail_lists_the_crafting_systems_that_make_it() {
    let (_, _, orb) = get(&format!("/v1/items/{}", id_of_item_named("Thunder-Forged Orb").await)).await;
    let (_, _, crafting_systems) = get_list_rows("/v1/crafting-systems").await;
    let thunder_forged = crafting_systems.as_array().unwrap().iter().find(|system| system["name"] == "Thunder-Forged");
    let thunder_forged_id = &thunder_forged.expect("the fixture wiki records Thunder-Forged")["id"];
    assert_eq!(
        orb["crafting_systems"],
        serde_json::json!([{
            "id": thunder_forged_id,
            "name": "Thunder-Forged",
            "is_rare": false,
            "wiki_url": "https://ddowiki.com/page/Thunder-Forged"
        }])
    );
    let sources = orb["sources"].as_array().unwrap();
    assert_eq!(sources.len(), 1, "{sources:?}");
    assert_eq!(
        (&sources[0]["kind"], &sources[0]["id"], &sources[0]["name"], &sources[0]["wiki_url"]),
        (
            &serde_json::json!("crafting_system"),
            thunder_forged_id,
            &serde_json::json!("Thunder-Forged"),
            &serde_json::json!("https://ddowiki.com/page/Thunder-Forged")
        )
    );
    let (_, _, version) = get("/v1/version").await;
    assert_eq!(version["counts"]["crafting_system_sources"], 2, "the Orb and Visor of Fraz-Urb'luu");
}

#[tokio::test]
async fn item_detail_lists_the_challenge_pack_whose_ingredients_buy_it() {
    let (_, _, ring) = get(&format!("/v1/items/{}", id_of_item_named("Epic Ring of the Stalker").await)).await;
    let pack_id = id_in_list_named("/v1/adventure-packs", "Vaults of the Artificers").await;
    assert_eq!(
        ring["challenge_packs"],
        serde_json::json!([{
            "id": pack_id,
            "name": "Vaults of the Artificers",
            "is_rare": false,
            "wiki_url": "https://ddowiki.com/page/Vaults_of_the_Artificers"
        }])
    );
    assert_eq!(ring["adventure_packs"], serde_json::json!([]));
    assert_eq!(
        (&ring["sources"][0]["kind"], &ring["sources"][0]["id"]),
        (&serde_json::json!("challenge"), &serde_json::json!(pack_id))
    );
    let (_, _, pack) = get(&format!("/v1/adventure-packs/{pack_id}")).await;
    assert_eq!(pack["items"], serde_json::json!([]), "the pack detail lists pack-wide drops only");
    let (_, _, version) = get("/v1/version").await;
    assert_eq!(version["counts"]["challenge_rewards"], 1);
}

#[tokio::test]
async fn item_detail_lists_the_starter_level_that_earns_an_iconic_item() {
    let (_, _, lenses) = get(&format!("/v1/items/{}", id_of_item_named("Blood-Red Lenses").await)).await;
    assert_eq!(lenses["starter_rewards"], serde_json::json!([{ "character_level": 15 }]));
    assert_eq!(
        lenses["sources"],
        serde_json::json!([{
            "kind": "starter",
            "id": null,
            "name": "Advance to level 15",
            "loot_type": null,
            "chest": null,
            "is_rare": false,
            "tier": null,
            "character_level": 15,
            "cost": null,
            "wiki_url": null
        }])
    );
    let (_, _, version) = get("/v1/version").await;
    assert_eq!(version["counts"]["starter_items"], 1);
}

#[tokio::test]
async fn vendors_list_their_location_pack_page_and_item_count_and_detail_their_items() {
    let (status, _, vendors) = get_list_rows("/v1/vendors").await;
    assert_eq!(status, StatusCode::OK);
    let vendor = &vendors[0];
    assert_eq!(keys_of(vendor), ["id", "item_count", "location", "name", "pack", "wiki_url"]);
    assert_eq!(
        (&vendor["name"], &vendor["location"], &vendor["pack"], &vendor["item_count"]),
        (
            &serde_json::json!("Morten Edgewright"),
            &serde_json::json!("House Jorasco"),
            &serde_json::json!("Free to Play"),
            &serde_json::json!(2)
        )
    );
    let (status, _, detail) = get(&format!("/v1/vendors/{}", vendor["id"])).await;
    assert_eq!(status, StatusCode::OK);
    assert_no_provenance(&detail);
    let items = detail["items"].as_array().unwrap();
    assert_eq!(keys_of(&items[0]), ["cost", "id", "is_rare", "minimum_level", "name", "slot"]);
    assert_eq!(
        items.iter().map(|item| (item["name"].as_str().unwrap(), item["cost"].as_str())).collect::<Vec<_>>(),
        [("Alabaster of the Twelve", Some("10 Planar Shards")), ("Ethereal Great Crossbow", None)]
    );
    let (status, _, _) = get("/v1/vendors/999999").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (_, _, crossbow) = get(&format!("/v1/items/{}", id_of_item_named("Ethereal Great Crossbow").await)).await;
    assert_eq!(
        crossbow["vendors"],
        serde_json::json!([{
            "id": vendor["id"],
            "name": "Morten Edgewright",
            "location": "House Jorasco",
            "cost": null,
            "is_rare": false,
            "wiki_url": "https://ddowiki.com/page/Morten_Edgewright"
        }])
    );
    assert_eq!(
        (&crossbow["sources"][0]["kind"], &crossbow["sources"][0]["wiki_url"]),
        (&serde_json::json!("vendor"), &serde_json::json!("https://ddowiki.com/page/Morten_Edgewright"))
    );
    let (_, _, alabaster) = get(&format!("/v1/items/{}", id_of_item_named("Alabaster of the Twelve").await)).await;
    let vendor_source = alabaster["sources"].as_array().unwrap().iter().find(|source| source["kind"] == "vendor");
    assert_eq!(vendor_source.unwrap()["cost"], "10 Planar Shards");
}

#[tokio::test]
async fn events_list_their_page_and_item_count_and_detail_their_items() {
    let (status, _, events) = get_list_rows("/v1/events").await;
    assert_eq!(status, StatusCode::OK);
    let event = &events[0];
    assert_eq!(keys_of(event), ["id", "item_count", "name", "wiki_url"]);
    assert_eq!(
        (&event["name"], &event["item_count"]),
        (&serde_json::json!("Treasure of Crystal Cove"), &serde_json::json!(3))
    );
    let (status, _, detail) = get(&format!("/v1/events/{}", event["id"])).await;
    assert_eq!(status, StatusCode::OK);
    assert_no_provenance(&detail);
    let item_names: Vec<&str> =
        detail["items"].as_array().unwrap().iter().map(|item| item["name"].as_str().unwrap()).collect();
    assert_eq!(item_names, ["Acrobat's Ring", "Bold Trinket", "Ratkiller (legacy) (level 4)"]);
    assert_eq!(keys_of(&detail["items"][0]), ["id", "is_rare", "minimum_level", "name", "slot"]);
    let (status, _, _) = get("/v1/events/999999").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (_, _, trinket) = get(&format!("/v1/items/{}", id_of_item_named("Bold Trinket").await)).await;
    assert_eq!(
        trinket["events"],
        serde_json::json!([{
            "id": event["id"],
            "name": "Treasure of Crystal Cove",
            "is_rare": false,
            "wiki_url": "https://ddowiki.com/page/Treasure_of_Crystal_Cove"
        }])
    );
    let (_, _, version) = get("/v1/version").await;
    let counts = &version["counts"];
    assert_eq!(
        (&counts["vendors"], &counts["vendor_items"], &counts["events"], &counts["event_items"]),
        (&serde_json::json!(1), &serde_json::json!(2), &serde_json::json!(1), &serde_json::json!(3))
    );
}

#[tokio::test]
async fn version_counts_every_source_by_kind() {
    let (_, _, version) = get("/v1/version").await;
    let counts = &version["counts"];
    assert_eq!((&counts["pack_loot"], &counts["pack_augment_loot"]), (&serde_json::json!(1), &serde_json::json!(0)));
    let source_count_by_kind: i64 = [
        "quest_loot",
        "quest_augment_loot",
        "quest_chain_rewards",
        "saga_rewards",
        "pack_loot",
        "pack_augment_loot",
        "crafting_system_sources",
        "challenge_rewards",
        "starter_items",
        "vendor_items",
        "event_items",
    ]
    .iter()
    .map(|count_name| counts[count_name].as_i64().unwrap())
    .sum();
    assert_eq!(counts["sources"].as_i64().unwrap(), source_count_by_kind);
}

#[tokio::test]
async fn item_pack_surfaces_follow_every_source_kind() {
    let mut wiki = fixture_wiki();
    wiki.events[0].items.clear();
    wiki.crafting_systems.iter_mut().find(|system| system.name == "Thunder-Forged").unwrap().pack =
        Some("Free to Play".into());
    wiki.vendors.extend(
        ddo_etl::wiki::WikiOverrides::from_toml_files(&[(
            "vendors_test.toml",
            "[[vendor]]\nname = \"Test Vendor Without a Pack\"\npage = \"https://ddowiki.com/page/Test_Vendor\"\n\
         read = \"2026-10-03\"\nitems = [\"Green Steel Weave Boots\"]\n",
        )])
        .unwrap()
        .vendors,
    );
    let state = fixture_state_with_wiki(&wiki, "pack-sources");
    let (status, _, page) = get_from(state.clone(), "/v1/items?limit=10000").await;
    assert_eq!(status, StatusCode::OK);
    let items = page["items"].as_array().unwrap();
    for (name, kind, pack_name) in [
        ("Acrobat's Ring", "quest_chain", Some("Free to Play")),
        ("Thunder-Forged Orb", "crafting_system", Some("Free to Play")),
        ("Ethereal Great Crossbow", "vendor", Some("Free to Play")),
        ("Epic Ring of the Stalker", "challenge", Some("Vaults of the Artificers")),
        ("Light Crossbow of the Golden Age", "adventure_pack", Some("Magic of Myth Drannor")),
        ("Bold Trinket", "event", None),
        ("Blood-Red Lenses", "starter", None),
        ("Visor of Fraz-Urb'luu", "crafting_system", None),
        ("Green Steel Weave Boots", "vendor", None),
    ] {
        let item = items.iter().find(|item| item["name"] == name).unwrap();
        assert_eq!(item["pack"], serde_json::json!(pack_name), "{name}");
        let (status, _, detail) = get_from(state.clone(), &format!("/v1/items/{}", item["id"])).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(detail["sources"].as_array().unwrap().len(), 1, "{name}");
        let source = &detail["sources"][0];
        assert_eq!(source["kind"], kind, "{name}");
        let packs = detail["adventure_packs"].as_array().unwrap();
        assert_eq!(packs.len(), usize::from(kind == "adventure_pack"), "{name}");
        if kind == "adventure_pack" {
            let pack_name = pack_name.unwrap();
            assert_eq!(packs[0]["name"], pack_name, "{name}");
            for field in ["loot_type", "is_rare", "chest"] {
                assert_eq!(packs[0][field], source[field], "{name}: {field}");
            }
            assert_eq!(packs[0]["wiki_url"], format!("https://ddowiki.com/page/{}", pack_name.replace(' ', "_")));
        }
        for searched_pack in ["Free to Play", "Vaults of the Artificers", "Magic of Myth Drannor"] {
            for parameter in ["pack", "q"] {
                let encoded_pack = searched_pack.replace(' ', "%20");
                let (status, _, matches) =
                    get_from(state.clone(), &format!("/v1/items?{parameter}={encoded_pack}&limit=10000")).await;
                assert_eq!(status, StatusCode::OK);
                assert_eq!(
                    item_names(&matches).contains(&name),
                    pack_name == Some(searched_pack),
                    "{name}: {parameter}={searched_pack}"
                );
            }
        }
    }
    let unsourced_item = items.iter().find(|item| item["name"] == "Legendary Green Steel Belt").unwrap();
    assert!(unsourced_item["pack"].is_null());
    let (_, _, unsourced_detail) = get_from(state.clone(), &format!("/v1/items/{}", unsourced_item["id"])).await;
    assert_eq!(unsourced_detail["sources"], serde_json::json!([]));
    assert_eq!(unsourced_detail["adventure_packs"], serde_json::json!([]));
    let (_, _, unknown_pack) = get_from(state, "/v1/items?pack=Unknown%20Pack").await;
    assert_eq!(unknown_pack["total"], 0);
}

#[tokio::test]
async fn item_pack_query_count_does_not_grow_with_rows_or_sources() {
    static QUERY_COUNT: AtomicUsize = AtomicUsize::new(0);
    static PACK_QUERY_COUNT: AtomicUsize = AtomicUsize::new(0);
    let state = fixture_state();
    state
        .read_db(|db| {
            db.trace_v2(
                TraceEventCodes::SQLITE_TRACE_STMT,
                Some(|event| {
                    if let TraceEvent::Stmt(statement, _) = event {
                        QUERY_COUNT.fetch_add(1, Ordering::SeqCst);
                        if statement.sql().contains("loot_adventure_packs") {
                            PACK_QUERY_COUNT.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                }),
            );
            Ok(())
        })
        .await
        .unwrap();
    for filter in ["", "&pack=Free%20to%20Play", "&q=free%20to%20play"] {
        for limit in [1, 10000] {
            QUERY_COUNT.store(0, Ordering::SeqCst);
            let (status, _, page) = get_from(state.clone(), &format!("/v1/items?limit={limit}{filter}")).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(QUERY_COUNT.load(Ordering::SeqCst), 2, "one count and one page query: {filter}, limit {limit}");
            assert!(page["total"].as_i64().unwrap() > 1);
            assert_eq!(
                page["items"].as_array().unwrap().len(),
                if limit == 1 { 1 } else { page["total"].as_u64().unwrap() as usize }
            );
        }
    }
    for (item_name, source_count) in [("Ethereal Great Crossbow", 1), ("Five Rings", 2)] {
        let item_id = id_of_item_named(item_name).await;
        PACK_QUERY_COUNT.store(0, Ordering::SeqCst);
        let (status, _, detail) = get_from(state.clone(), &format!("/v1/items/{item_id}")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(detail["sources"].as_array().unwrap().len(), source_count);
        assert_eq!(PACK_QUERY_COUNT.load(Ordering::SeqCst), 0, "detail does not read the roll-up: {item_name}");
    }
}

#[tokio::test]
async fn saga_sources_preserve_rarity_while_item_pack_filters_roll_up_quests() {
    let detail = item_detail_named("Five%20Rings").await;
    let saga_sources: Vec<_> =
        detail["sources"].as_array().unwrap().iter().filter(|source| source["kind"] == "saga").collect();
    assert_eq!(saga_sources.len(), 2);
    assert_eq!(
        saga_sources.iter().map(|source| source["is_rare"].as_bool().unwrap()).collect::<Vec<_>>(),
        [false, true]
    );
    assert_eq!(detail["adventure_packs"], serde_json::json!([]));
    let (_, _, page) = get("/v1/items?q=Five%20Rings").await;
    assert_eq!(
        page["items"][0]["pack"], "Chill of Ravenloft",
        "the alphabetically first of its saga's two quest packs"
    );
    for pack_name in ["Chill%20of%20Ravenloft", "Masterminds%20of%20Sharn"] {
        for parameter in ["pack", "q"] {
            let (_, _, matches) = get(&format!("/v1/items?{parameter}={pack_name}&limit=10000")).await;
            assert!(item_names(&matches).contains(&"Five Rings"), "{parameter}={pack_name}");
        }
    }
}

#[tokio::test]
async fn quest_sources_preserve_distinct_chests_for_items_and_augments() {
    for (case_index, (first_chest, second_chest)) in [
        (Some("end chest"), Some("end chest")),
        (Some("end chest"), Some("optional chest")),
        (Some("end chest"), None),
        (None, Some("end chest")),
        (None, None),
    ]
    .into_iter()
    .enumerate()
    {
        let mut wiki = fixture_wiki();
        for (quest_name, chest) in [("The Grotto", first_chest), ("Redemption", second_chest)] {
            let chest_field = chest.map_or(String::new(), |chest| format!(", chest = {chest:?}"));
            let quest_loot = format!(
                "[[quest]]\nname = {quest_name:?}\npage = \"https://ddowiki.com/page/Test\"\nread = \"2026-10-03\"\n\
                 items = [{{ name = \"Bold Trinket\"{chest_field} }}, {{ name = \"Bold Trinket\", loot_type = \"reward\" }}]\n\
                 augments = [{{ name = \"Lunar Gem of Magical Protection (Heroic)\"{chest_field} }}]\n"
            );
            wiki.quest_loot.extend(
                ddo_etl::wiki::WikiOverrides::from_toml_files(&[("quest_loot_test.toml", &quest_loot)])
                    .unwrap()
                    .quest_loot,
            );
        }
        let state = fixture_state_with_wiki(&wiki, &format!("pack-chests-{case_index}"));
        for (resource, search_text) in
            [("items", "Bold%20Trinket"), ("augments", "Lunar%20Gem%20of%20Magical%20Protection")]
        {
            let (_, _, page) = get_from(state.clone(), &format!("/v1/{resource}?q={search_text}")).await;
            let (status, _, detail) =
                get_from(state.clone(), &format!("/v1/{resource}/{}", page[resource][0]["id"])).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(detail["adventure_packs"], serde_json::json!([]));
            let quests: Vec<_> =
                detail["quests"].as_array().unwrap().iter().filter(|quest| quest["pack"] == "Free to Play").collect();
            assert_eq!(quests.len(), 2 * (1 + usize::from(resource == "items")), "{resource}: {detail}");
            let chest_rows: Vec<_> = quests.iter().filter(|quest| quest["loot_type"] == "chest").collect();
            let mut observed_chests: Vec<_> = chest_rows.iter().map(|quest| quest["chest"].as_str()).collect();
            observed_chests.sort();
            let mut requested_chests = vec![first_chest, second_chest];
            requested_chests.sort();
            assert_eq!(observed_chests, requested_chests, "{resource}: case {case_index}");
            assert!(quests.iter().all(|quest| quest["is_rare"] == false));
            if resource == "items" {
                assert_eq!(quests.iter().filter(|quest| quest["loot_type"] == "reward").count(), 2);
            }
        }
    }
}

#[tokio::test]
async fn items_list_counts_pack_wide_drops_in_the_pack_and_rare_filters() {
    let (_, _, magic) = get("/v1/items?pack=Magic%20of%20Myth%20Drannor").await;
    let names: Vec<&str> =
        magic["items"].as_array().unwrap().iter().map(|item| item["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"Light Crossbow of the Golden Age"), "{names:?}");
    let crossbow = magic["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["name"] == "Light Crossbow of the Golden Age")
        .unwrap();
    assert_eq!(
        (&crossbow["pack"], &crossbow["is_rare"]),
        (&serde_json::json!("Magic of Myth Drannor"), &serde_json::json!(true))
    );
}

fn assert_never_cached(response: &axum::response::Response, what: &str) {
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).map(|v| v.to_str().unwrap()),
        Some("no-store"),
        "{what} ({}) must not be cached: {:?}",
        response.status(),
        response.headers()
    );
    assert!(response.headers().get(header::ETAG).is_none(), "{what} must carry no ETag: {:?}", response.headers());
}

#[tokio::test]
async fn error_responses_are_never_cached() {
    for (path, expected_status) in [
        ("/v1/items/999999", StatusCode::NOT_FOUND),
        ("/v1/items?category=Hat", StatusCode::BAD_REQUEST),
        ("/no-such-route", StatusCode::NOT_FOUND),
        ("/icons/items/Quarterstaff_6a.png", StatusCode::NOT_FOUND),
    ] {
        let response = response_to_cross_origin_get(app(fixture_state()), path).await;
        assert_eq!(response.status(), expected_status, "{path}");
        assert_never_cached(&response, path);
    }

    let router = app(fixture_state().with_rate_limit());
    let mut rate_limited_response = None;
    for _ in 0..200 {
        let response = response_to_cross_origin_get(router.clone(), "/v1/effects").await;
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            rate_limited_response = Some(response);
            break;
        }
    }
    assert_never_cached(&rate_limited_response.expect("the burst of 100 runs out within 200 requests"), "a 429");

    let response_without_client_address = app(fixture_state().with_rate_limit())
        .oneshot(Request::get("/v1/effects").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response_without_client_address.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_never_cached(&response_without_client_address, "the rate limiter's 500");
}

#[tokio::test]
async fn version_is_revalidated_on_every_use_and_answers_304_while_unchanged() {
    let (_, headers, _) = get("/v1/version").await;
    assert_eq!(headers.get(header::CACHE_CONTROL).unwrap(), "no-cache");
    let etag = headers.get(header::ETAG).expect("version keeps its ETag").clone();
    let response = app(fixture_state())
        .oneshot(Request::get("/v1/version").header(header::IF_NONE_MATCH, etag).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(response.headers().get(header::CACHE_CONTROL).unwrap(), "no-cache");
}

#[tokio::test]
async fn data_responses_are_fresh_for_five_minutes_then_revalidated() {
    const DATA_CACHE_CONTROL: &str = "public, max-age=300, stale-while-revalidate=3600";
    let icons_dir = std::env::temp_dir().join(format!("ddo-api-cache-icons-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&icons_dir);
    ddo_etl::icons::export_icons(&fixture_data_files_dir(), &icons_dir).unwrap();
    let router = app(fixture_state().with_icons_dir(&icons_dir));
    let response_to = |request: Request<Body>| router.clone().oneshot(request);

    let data_response = response_to(Request::get("/v1/items?q=sireth").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(data_response.status(), StatusCode::OK);
    assert_eq!(data_response.headers().get(header::CACHE_CONTROL).unwrap(), DATA_CACHE_CONTROL);
    let etag = data_response.headers().get(header::ETAG).unwrap().clone();
    let not_modified_response = response_to(
        Request::get("/v1/items?q=sireth").header(header::IF_NONE_MATCH, etag).body(Body::empty()).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(not_modified_response.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(not_modified_response.headers().get(header::CACHE_CONTROL).unwrap(), DATA_CACHE_CONTROL);

    for path in ["/v1/dump.sqlite", "/icons/items/Quarterstaff_6a.png"] {
        let response = response_to(Request::get(path).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        assert_eq!(response.headers().get(header::CACHE_CONTROL).unwrap(), DATA_CACHE_CONTROL, "{path}");
    }
}

#[tokio::test]
async fn a_new_api_build_changes_every_etag_even_on_the_same_dataset() {
    let etag_from = |state: AppState| async move {
        let response =
            app(state).oneshot(Request::get("/v1/items?q=sireth").body(Body::empty()).unwrap()).await.unwrap();
        response.headers().get(header::ETAG).unwrap().clone()
    };
    let first_build_etag = etag_from(fixture_state().with_api_commit("aaaaaaa")).await;
    let second_build_etag = etag_from(fixture_state().with_api_commit("bbbbbbb")).await;
    assert_ne!(first_build_etag, second_build_etag);

    let response = app(fixture_state().with_api_commit("bbbbbbb"))
        .oneshot(Request::get("/v1/version").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let version: Value = serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(version["api_commit"], "bbbbbbb");
}
