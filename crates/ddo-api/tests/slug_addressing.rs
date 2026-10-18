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
        let original = common::export_fixture_data(&fixtures, &data_dir);
        let mut redirects = String::new();
        for kind in EntityKind::ALL {
            let slug: String = original.query_row(&format!("SELECT slug FROM {} ORDER BY slug LIMIT 1", kind.table_name()), [], |row| row.get(0)).unwrap();
            redirects.push_str(&format!("[[redirect]]\nkind = {:?}\nfrom = {:?}\nto = {slug:?}\nsince = \"2026-10-17\"\n\n", kind.as_str(), format!("former-{}-{slug}", kind.as_str())));
        }
        redirects.push_str("[[removed]]\nkind = \"items\"\nslug = \"removed-item\"\nsince = \"2026-10-17\"\nreason = \"Removed fixture\"\n");
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
    let (status, _, _) = response("/v1/items/removed-item").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let slug: String =
        db.query_row("SELECT slug FROM effects WHERE slug GLOB '[0-9]-*' LIMIT 1", [], |row| row.get(0)).unwrap();
    let (status, _, body) = response(&format!("/v1/effects/{slug}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["slug"], slug);
}
