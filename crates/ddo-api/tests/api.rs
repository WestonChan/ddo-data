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
    assert_eq!(json["counts"]["items"], 55, "54 of Maetrim's and the wiki fixture's axe");
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
    assert_eq!(first_page["total"], 52, "the three legacy items are left out by default");
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
    assert_eq!(unfiltered["total"], 52);
    let (status, _, _) = get("/v1/items?category=Hat").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "unknown category is a client error");
}

#[tokio::test]
async fn items_list_leaves_out_legacy_items_unless_asked_to_include_them() {
    let legacy_names = ["+3 Combustion Scorched Battle Axe", "Allegiance (historic)", "Ratkiller (legacy) (level 4)"];
    for path in ["/v1/items?limit=10000", "/v1/items?limit=10000&include_legacy=false"] {
        let (status, _, current) = get(path).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(current["total"], 52, "{path}");
        let rows = current["items"].as_array().unwrap();
        assert!(rows.iter().all(|row| row["is_legacy"] == false), "{path}");
        assert!(legacy_names.iter().all(|name| rows.iter().all(|row| row["name"] != *name)), "{path}");
    }
    let (_, _, with_legacy) = get("/v1/items?limit=10000&include_legacy=true").await;
    assert_eq!(with_legacy["total"], 55);
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
async fn items_filter_by_any_of_several_stat_enchantments_given_as_repeated_keys() {
    let (status, _, repeated) = get("/v1/items?enchantment=Strength&enchantment=Charisma").await;
    assert_eq!(status, StatusCode::OK, "repeated enchantment keys are rejected: {repeated}");
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

    let (status, _, unknown) = get("/v1/items?enchantment=Strength&enchantment=Strenght").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "an unknown enchantment is a client error, not an empty page");
    assert_eq!(unknown["error"], r#"unknown enchantment "Strenght""#);
    let (status, _, retired) = get("/v1/items?stat=Strength").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "`stat` gave way to `enchantment`: {retired}");
}

#[tokio::test]
async fn items_filter_by_an_effect_whose_name_contains_a_comma() {
    let (status, _, poisoned) = get("/v1/items?enchantment=Constitution%20Poison%2C%20Lesser").await;
    assert_eq!(status, StatusCode::OK, "a comma inside one enchantment name must not split it: {poisoned}");
    assert_eq!(item_names(&poisoned), ["Ivy Wraps (Level 15)"]);

    let (_, _, poison_or_vorpal) =
        get("/v1/items?enchantment=Constitution%20Poison%2C%20Lesser&enchantment=Vorpal").await;
    assert_eq!(item_names(&poison_or_vorpal), ["Ivy Wraps (Level 15)", "Light Crossbow of the Golden Age"]);

    let (status, _, comma_list) = get("/v1/items?enchantment=Strength,Charisma").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "a comma no longer separates names: {comma_list}");
    assert_eq!(comma_list["error"], r#"unknown enchantment "Strength,Charisma""#);
}

#[tokio::test]
async fn items_filter_by_effect_enchantments_alone_or_beside_stats() {
    let (status, _, vorpal) = get("/v1/items?enchantment=Vorpal").await;
    assert_eq!(status, StatusCode::OK, "{vorpal}");
    assert_eq!(item_names(&vorpal), ["Light Crossbow of the Golden Age"]);

    let (_, _, effect_or_stat) = get("/v1/items?enchantment=Freedom%20of%20Movement&enchantment=Charisma").await;
    assert_eq!(
        item_names(&effect_or_stat),
        ["Epic Kundarak Delving Boots", "Kundarak Delving Boots", "Ring of the Kraken", "Sireth, Spear of the Sky"],
        "three items carry the Freedom of Movement effect and the ring a Charisma bonus"
    );

    let (_, _, effect_and_search_text) = get("/v1/items?enchantment=Freedom%20of%20Movement&q=sireth").await;
    assert_eq!(
        item_names(&effect_and_search_text),
        ["Sireth, Spear of the Sky"],
        "other filters still narrow the match"
    );

    let (status, _, lower_case) = get("/v1/items?enchantment=vorpal").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "names match case-sensitively, as `stat` did: {lower_case}");
}

#[tokio::test]
async fn items_match_a_stat_enchantment_on_their_sets_tiers_only_when_set_bonuses_are_included() {
    let (_, _, own_bonuses_only) = get("/v1/items?enchantment=Sneak%20Attack%20Dice").await;
    assert_eq!(own_bonuses_only["total"], 0, "no fixture item has its own Sneak Attack Dice bonus");
    let (status, _, with_set_bonuses) =
        get("/v1/items?enchantment=Sneak%20Attack%20Dice&include_set_bonuses=true").await;
    assert_eq!(status, StatusCode::OK, "{with_set_bonuses}");
    assert_eq!(item_names(&with_set_bonuses), ["Kundarak Delving Boots"], "its set's three-piece tier gives the dice");

    let (_, _, own_or_set) =
        get("/v1/items?enchantment=Hide&enchantment=Physical%20Resistance%20Rating&include_set_bonuses=true").await;
    assert_eq!(
        item_names(&own_or_set),
        ["Kundarak Delving Boots", "Legendary Cloak of Winter"],
        "the boots' own Hide bonus still matches beside the cloak's Eminence of Winter PRR tier"
    );
    let (_, _, explicitly_excluded) =
        get("/v1/items?enchantment=Sneak%20Attack%20Dice&include_set_bonuses=false").await;
    assert_eq!(explicitly_excluded["total"], 0);
}

fn enchantment_named<'a>(enchantments: &'a Value, name: &str, kind: &str) -> Option<&'a Value> {
    enchantments.as_array().unwrap().iter().find(|row| row["name"] == name && row["kind"] == kind)
}

#[tokio::test]
async fn enchantments_list_every_stat_and_effect_an_item_carries_with_its_item_count() {
    let (status, _, enchantments) = get("/v1/enchantments").await;
    assert_eq!(status, StatusCode::OK, "{enchantments}");
    assert_eq!(
        enchantment_named(&enchantments, "Strength", "stat"),
        Some(&serde_json::json!({ "name": "Strength", "kind": "stat", "item_count": 3 }))
    );
    assert_eq!(
        enchantment_named(&enchantments, "Freedom of Movement", "effect"),
        Some(&serde_json::json!({ "name": "Freedom of Movement", "kind": "effect", "item_count": 3 }))
    );
    assert!(enchantment_named(&enchantments, "Sneak Attack Dice", "stat").is_none(), "only a set tier carries it");
    assert_eq!(
        enchantment_named(&enchantments, "Universal Spell Power", "stat"),
        Some(&serde_json::json!({ "name": "Universal Spell Power", "kind": "stat", "item_count": 3 })),
        "the legacy +3 Combustion Scorched Battle Axe is not counted, as the default item list leaves it out"
    );
    assert!(enchantment_named(&enchantments, "Fire Spell Power", "stat").is_none(), "only a legacy item carries it");
    assert!(enchantment_named(&enchantments, "ElfBane", "effect").is_none(), "only a legacy item carries it");
    let rows = enchantments.as_array().unwrap();
    assert_eq!(rows.len(), 100, "56 stats and 44 effects carried by fixture items that are not legacy");
    assert!(rows.iter().all(|row| row["item_count"].as_i64().unwrap() > 0));
    let names: Vec<&str> = rows.iter().map(|row| row["name"].as_str().unwrap()).collect();
    let mut sorted_names = names.clone();
    sorted_names.sort_unstable();
    assert_eq!(names, sorted_names, "ordered by name");
}

#[tokio::test]
async fn enchantments_narrow_by_search_text_and_kind() {
    let (_, _, resistances) = get("/v1/enchantments?q=RESISTANCE").await;
    let resistance_names: Vec<&str> =
        resistances.as_array().unwrap().iter().map(|row| row["name"].as_str().unwrap()).collect();
    assert_eq!(
        resistance_names,
        ["Cold Resistance", "Electric Resistance", "Fire Resistance", "Physical and Magical Resistance Rating"]
    );

    let (_, _, effects) = get("/v1/enchantments?kind=effect").await;
    let effects = effects.as_array().unwrap();
    assert_eq!(effects.len(), 43);
    assert!(effects.iter().all(|row| row["kind"] == "effect"), "kind=effect lists a stat");
    let (_, _, strength_stats) = get("/v1/enchantments?kind=stat&q=strength").await;
    assert_eq!(strength_stats, serde_json::json!([{ "name": "Strength", "kind": "stat", "item_count": 3 }]));

    let (status, _, unknown_kind) = get("/v1/enchantments?kind=bonus").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(unknown_kind["error"], r#"unknown kind "bonus""#);
}

async fn id_in_list_named(list_path: &str, name: &str) -> i64 {
    let (_, _, rows) = get(list_path).await;
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
        ["Argenti's Armor", "Battle Axe of the Oozing Hunger"],
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
            "Legendary Ring of Unbridled Might"
        ],
        "a name starting with the text comes before names merely containing it"
    );
    assert_eq!(ring["total"], 8);
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
    assert_eq!(jewelry["total"], 15);
    let (_, _, free_to_play) = get("/v1/items?q=free%20to%20play").await;
    assert_eq!(
        item_names(&free_to_play),
        ["Battle Axe of the Oozing Hunger"],
        "any pack it drops in matches, not only the first one the row shows"
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
    let bonus_lines: Vec<String> = first_tier["bonuses"]
        .as_array()
        .unwrap_or_else(|| panic!("an option lists its bonuses: {first_tier}"))
        .iter()
        .map(|bonus| format!("{} {} {}", bonus["stat"], bonus["bonus_type"], bonus["value"]))
        .collect();
    assert_eq!(bonus_lines, ["\"Spell Penetration\" \"Equipment\" 1", "\"Armor Class\" \"Insight\" 1"]);
    assert_eq!(first_tier["modifiers"][1]["effect_type"], "ACBonus");
    assert!(first_tier["grants_slot"].is_null());
    assert_eq!(first_tier["sets"], serde_json::json!([]));
    assert_eq!(axe["augment_slots"][1]["options"][0]["grants_slot"], "purple");
    assert!(
        !axe["bonuses"].as_array().unwrap().iter().any(|bonus| bonus["stat"] == "Spell Penetration"),
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
    let (_, _, quest_list) = get("/v1/quests").await;
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
async fn quests_hide_whether_maetrim_or_the_wiki_supplied_them() {
    let (_, _, quests) = get("/v1/quests").await;
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
    let (_, _, sets) = get("/v1/sets").await;
    let winter = sets.as_array().unwrap().iter().find(|s| s["name"] == "Eminence of Winter").unwrap();
    let (_, _, set) = get(&format!("/v1/sets/{}", winter["id"])).await;
    assert!(set["tiers"].as_array().unwrap().iter().all(|t| t["equipped_count"].as_i64().unwrap() >= 2));
    assert_eq!(set["items"][0]["name"], "Legendary Cloak of Winter");
    let two_piece_tier = set["tiers"].as_array().unwrap().iter().find(|t| t["equipped_count"] == 2).unwrap();
    assert_eq!(two_piece_tier["bonuses"][0]["stat"], "Physical Resistance Rating", "tier {two_piece_tier}");
    assert_eq!(two_piece_tier["bonuses"][0]["value"], 30);
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
        ("enchantment", _) => "Strength",
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
async fn openapi_lists_every_item_filter_in_the_route_description_and_parameters() {
    let (_, _, spec) = get("/v1/openapi.json").await;
    let operation = &spec["paths"]["/v1/items"]["get"];
    let documented_names: Vec<&str> =
        operation["parameters"].as_array().unwrap().iter().map(|param| param["name"].as_str().unwrap()).collect();
    let item_filters = [
        "q",
        "slot",
        "category",
        "min_level",
        "max_level",
        "pack",
        "raid",
        "rare",
        "quest",
        "quest_chain",
        "saga",
        "enchantment",
        "include_set_bonuses",
        "include_legacy",
        "limit",
        "offset",
    ];
    let route_description = operation["description"].as_str().unwrap();
    for filter_name in item_filters {
        assert!(documented_names.contains(&filter_name), "/v1/items does not document {filter_name}");
        assert!(route_description.contains(&format!("`{filter_name}`")), "/v1/items description omits `{filter_name}`");
    }
    assert!(!documented_names.contains(&"stat"), "/v1/items still documents the retired `stat`");
    let enchantment_description =
        operation["parameters"].as_array().unwrap().iter().find(|p| p["name"] == "enchantment").unwrap()["description"]
            .as_str()
            .unwrap();
    for expected_phrase in
        ["repeated keys", "may contain commas", "/v1/enchantments", "/v1/stats", "effect", "item_effects"]
    {
        assert!(
            enchantment_description.contains(expected_phrase),
            "missing {expected_phrase:?}: {enchantment_description}"
        );
    }
    assert!(
        !enchantment_description.contains("comma-separated"),
        "`enchantment` no longer splits on commas: {enchantment_description}"
    );
    assert!(
        route_description.contains("repeated `enchantment` keys"),
        "the route description must say several enchantments are repeated keys: {route_description}"
    );
    assert!(
        route_description.contains("stat bonus") && route_description.contains("named effect"),
        "the route description must say `enchantment` matches stat bonuses and named effects: {route_description}"
    );
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
    let (_, _, list) = get("/v1/crafting-systems").await;
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

    let (_, _, quests) = get("/v1/quests").await;
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
    let (status, _, chains) = get("/v1/quest-chains").await;
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
    let (_, _, chains) = get("/v1/quest-chains").await;
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
    let (status, _, sagas) = get("/v1/sagas").await;
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
async fn a_not_found_response_carries_cors_headers() {
    let response = response_to_cross_origin_get(app(fixture_state().with_rate_limit()), "/v1/items/999999").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).map(|v| v.to_str().unwrap()), Some("*"));
    let response = response_to_cross_origin_get(app(fixture_state()), "/no-such-route").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).map(|v| v.to_str().unwrap()), Some("*"));
}

#[tokio::test]
async fn item_detail_lists_the_adventure_packs_whose_quests_all_drop_it() {
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
async fn adventure_pack_detail_lists_the_loot_any_of_its_quests_drops() {
    let (_, _, packs) = get("/v1/adventure-packs").await;
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
    let (_, _, crafting_systems) = get("/v1/crafting-systems").await;
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
    let pack_id = id_in_list_named("/v1/adventure-packs", "Secrets of the Artificers").await;
    assert_eq!(
        ring["challenge_packs"],
        serde_json::json!([{
            "id": pack_id,
            "name": "Secrets of the Artificers",
            "is_rare": false,
            "wiki_url": "https://ddowiki.com/page/Secrets_of_the_Artificers"
        }])
    );
    assert_eq!(ring["adventure_packs"], serde_json::json!([]), "a challenge reward is no pack-wide drop");
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
    let (status, _, vendors) = get("/v1/vendors").await;
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
    let (status, _, events) = get("/v1/events").await;
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
        let response = response_to_cross_origin_get(router.clone(), "/v1/stats").await;
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            rate_limited_response = Some(response);
            break;
        }
    }
    assert_never_cached(&rate_limited_response.expect("the burst of 100 runs out within 200 requests"), "a 429");

    let response_without_client_address = app(fixture_state().with_rate_limit())
        .oneshot(Request::get("/v1/stats").body(Body::empty()).unwrap())
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
