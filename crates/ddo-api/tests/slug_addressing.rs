mod common;

use axum::body::Body;
use axum::http::{header, HeaderMap, Request, StatusCode};
use ddo_api::{app, AppState};
use ddo_model::enums::EntityKind;
use http_body_util::BodyExt;
use rusqlite::Connection;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::OnceLock;
use tower::ServiceExt;

fn fixture_path() -> &'static PathBuf {
    static DATABASE: OnceLock<PathBuf> = OnceLock::new();
    DATABASE.get_or_init(|| {
        let root = std::env::temp_dir().join(format!("ddo-slug-addressing-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let data_dir = root.join("data");
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ddo-etl/tests/fixtures");
        let original = common::copy_fixture_data(&fixtures, &data_dir);
        let mut redirects = String::new();
        for kind in EntityKind::ALL {
            let limit = if matches!(kind, EntityKind::Quest | EntityKind::QuestChain | EntityKind::Saga) { "" } else { " LIMIT 1" };
            let mut statement = original.prepare(&format!("SELECT slug FROM {} ORDER BY slug{limit}", kind.table_name())).unwrap();
            for slug in statement.query_map([], |row| row.get::<_, String>(0)).unwrap() {
                let slug = slug.unwrap();
                redirects.push_str(&format!("[[redirect]]\nkind = {:?}\nfrom = {:?}\nto = {slug:?}\nsince = \"2026-10-17\"\n\n", kind.as_str(), format!("former-{}-{slug}", kind.as_str())));
            }
        }
        redirects.push_str("[[retired]]\nkind = \"items\"\nslug = \"retired-item\"\nsince = \"2026-10-17\"\nreason = \"Retired fixture\"\n");
        std::fs::write(data_dir.join("redirects.toml"), redirects).unwrap();
        let path = root.join("fixture.db");
        let mut built = Connection::open(&path).unwrap();
        ddo_etl::tree::build_from_data(&data_dir, &mut built).unwrap();
        path
    })
}

async fn response(path: &str) -> (StatusCode, HeaderMap, Value) {
    let response = app(AppState::open(fixture_path()).unwrap())
        .oneshot(Request::get(path).header(header::ORIGIN, "https://example.com").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap() };
    (status, headers, body)
}

#[tokio::test]
async fn every_detail_route_accepts_slugs_ids_and_cached_query_preserving_redirects() {
    let (_, _, schema) = response("/v1/openapi.json").await;
    let db = Connection::open(fixture_path()).unwrap();
    let mut checked = 0;
    for path in schema["paths"].as_object().unwrap().keys().filter(|path| path.ends_with("/{slug}")) {
        let prefix = path.trim_end_matches("/{slug}");
        let kind = EntityKind::parse(prefix.trim_start_matches("/v1/")).unwrap();
        let (id, slug): (i64, String) = db
            .query_row(&format!("SELECT id, slug FROM {} ORDER BY slug LIMIT 1", kind.table_name()), [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        let current = format!("{prefix}/{slug}");
        let (status, _, body) = response(&current).await;
        assert_eq!(status, StatusCode::OK, "{current}");
        assert_eq!(body["slug"], slug);
        let (numeric_status, _, numeric) = response(&format!("{prefix}/{id}")).await;
        assert_eq!(numeric_status, StatusCode::OK);
        assert_eq!(body, numeric, "{current}");
        let query = "?q=a%20b&q=c%2Bd&limit=1";
        let (status, headers, body) = response(&format!("{prefix}/former-{}-{slug}{query}", kind.as_str())).await;
        assert_eq!(status, StatusCode::PERMANENT_REDIRECT, "{current}");
        assert_eq!(headers[header::LOCATION], format!("{current}{query}"));
        assert_eq!(headers[header::CACHE_CONTROL], "public, max-age=300, stale-while-revalidate=3600");
        assert!(!headers.contains_key(header::ETAG));
        assert_eq!(headers[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");
        assert_eq!(body, Value::Null);
        for missing in ["unknown-slug", "999999999999999999999999999999", "-1"] {
            let (status, headers, _) = response(&format!("{prefix}/{missing}")).await;
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert_eq!(headers[header::CACHE_CONTROL], "no-store");
        }
        checked += 1;
    }
    assert_eq!(checked, 16);
    let (status, _, _) = response("/v1/items/retired-item").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let slug: String =
        db.query_row("SELECT slug FROM effects WHERE slug GLOB '[0-9]-*' LIMIT 1", [], |row| row.get(0)).unwrap();
    let (status, _, body) = response(&format!("/v1/effects/{slug}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["slug"], slug);
}

#[tokio::test]
async fn item_source_filters_accept_current_former_and_numeric_values_under_any_and_all() {
    let db = Connection::open(fixture_path()).unwrap();
    for (parameter, kind, column) in [
        ("quest", EntityKind::Quest, "quest_id"),
        ("quest_chain", EntityKind::QuestChain, "chain_id"),
        ("saga", EntityKind::Saga, "saga_id"),
    ] {
        let mut statement = db.prepare(&format!("SELECT id, slug FROM {} e WHERE EXISTS (SELECT 1 FROM sources s WHERE s.{column} = e.id AND s.item_id IS NOT NULL) ORDER BY slug LIMIT 2", kind.table_name())).unwrap();
        let entities = statement
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert!(!entities.is_empty(), "{parameter}");
        for count in 1..=entities.len() {
            for match_kind in ["any", "all"] {
                let mut expected = None;
                for spelling in ["id", "slug", "former"] {
                    let parameters = entities[..count]
                        .iter()
                        .map(|(id, slug)| {
                            let value = match spelling {
                                "id" => id.to_string(),
                                "slug" => slug.clone(),
                                _ => format!("former-{}-{slug}", kind.as_str()),
                            };
                            format!("{parameter}={value}")
                        })
                        .collect::<Vec<_>>()
                        .join("&");
                    let (status, _, body) =
                        response(&format!("/v1/items?{parameters}&{parameter}_match={match_kind}&limit=10000")).await;
                    assert_eq!(status, StatusCode::OK, "{parameter} {spelling}: {body}");
                    if count == 1 {
                        assert!(body["total"].as_u64().unwrap() > 0, "{parameter}");
                    }
                    if let Some(expected) = &expected {
                        assert_eq!(&body, expected);
                    } else {
                        expected = Some(body);
                    }
                }
            }
        }
        for missing in ["unknown-slug", "999999999999999999999999999999"] {
            let (status, _, body) = response(&format!("/v1/items?{parameter}={missing}")).await;
            assert_eq!(status, StatusCode::BAD_REQUEST);
            assert!(body["error"].as_str().unwrap().contains(missing));
        }
    }
}
