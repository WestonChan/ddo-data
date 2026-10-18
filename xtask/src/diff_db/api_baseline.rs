use anyhow::{ensure, Context, Result};
use rusqlite::{Connection, OpenFlags};
use std::io::Write;
use std::path::{Path, PathBuf};

pub(super) fn baseline_dump(base_url: &str) -> Result<PathBuf> {
    let version = super::api_response(base_url, "/v1/version")?;
    let cache = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/api-baselines");
    cached_dump(&cache, &version, || super::api_bytes(base_url, "/v1/dump.sqlite"))
}

fn baseline_dataset_sha(version: &serde_json::Value) -> Result<&str> {
    version["dataset"]["upstream_sha"].as_str().context("API version has no dataset.upstream_sha")
}

fn cached_dump(
    cache: &Path,
    version: &serde_json::Value,
    download: impl FnOnce() -> Result<Vec<u8>>,
) -> Result<PathBuf> {
    let dataset_sha = baseline_dataset_sha(version)?;
    ensure!(
        !dataset_sha.is_empty()
            && dataset_sha.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')),
        "invalid dataset SHA {dataset_sha:?}"
    );
    std::fs::create_dir_all(cache).with_context(|| format!("creating {}", cache.display()))?;
    let (identity_kind, identity) = if let Some(commit) = version["api_commit"].as_str() {
        ("api", commit)
    } else {
        ("local", version["dataset"]["built_at"].as_str().context("local API version has no dataset.built_at")?)
    };
    ensure!(!identity.is_empty(), "API cache identity must not be empty");
    let encoded_identity = identity.bytes().map(|byte| format!("{byte:02x}")).collect::<String>();
    let destination = cache.join(format!("{dataset_sha}-{identity_kind}-{encoded_identity}.sqlite"));
    if destination.exists() {
        validate_dataset(&destination, dataset_sha)?;
        return Ok(destination);
    }
    let mut pending = tempfile::NamedTempFile::new_in(cache)?;
    pending.write_all(&download()?)?;
    pending.flush()?;
    validate_dataset(pending.path(), dataset_sha)?;
    pending.persist(&destination)?;
    Ok(destination)
}

fn validate_dataset(path: &Path, expected_sha: &str) -> Result<()> {
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let actual: String = db.query_row("SELECT upstream_sha FROM dataset_version", [], |row| row.get(0))?;
    ensure!(actual == expected_sha, "{}: expected dataset {expected_sha}, found {actual}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_cache_downloads_once_and_validates_the_dataset() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/ddo-etl/tests/fixtures");
        let scratch = tempfile::tempdir().unwrap();
        let database = scratch.path().join("fixture.sqlite");
        let mut db = Connection::open(&database).unwrap();
        ddo_etl::tree::build_from_data(&fixtures.join("data"), &mut db).unwrap();
        drop(db);
        use http_body_util::BodyExt;
        use tower::ServiceExt;
        let router = ddo_api::app(ddo_api::AppState::open(&database).unwrap());
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let version = runtime.block_on(async {
            let response = router
                .oneshot(axum::http::Request::get("/v1/version").body(axum::body::Body::empty()).unwrap())
                .await
                .unwrap();
            serde_json::from_slice::<serde_json::Value>(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap()
        });
        assert_eq!(baseline_dataset_sha(&version).unwrap(), "fixture");
        let bytes = std::fs::read(database).unwrap();
        let cache = scratch.path().join("cache");
        let cached = cached_dump(&cache, &version, || Ok(bytes.clone())).unwrap();
        assert_eq!(cached_dump(&cache, &version, || panic!("cache hit must not download")).unwrap(), cached);
        let mut other_api = version.clone();
        other_api["api_commit"] = serde_json::json!("other-api");
        let other_cached = cached_dump(&cache, &other_api, || Ok(bytes.clone())).unwrap();
        assert_ne!(cached, other_cached, "different API commits must not share a dump");
        let mut local = version.clone();
        local["api_commit"] = serde_json::Value::Null;
        let local_cached = cached_dump(&cache, &local, || Ok(bytes.clone())).unwrap();
        local["dataset"]["built_at"] = serde_json::json!("2026-10-08T01:00:00Z");
        assert_ne!(local_cached, cached_dump(&cache, &local, || Ok(bytes.clone())).unwrap());
        assert!(cached_dump(
            &cache,
            &serde_json::json!({"dataset":{"upstream_sha":"different-sha"},"api_commit":"other-api"}),
            || Ok(bytes)
        )
        .unwrap_err()
        .to_string()
        .contains("fixture"));
        assert!(cached_dump(&cache, &serde_json::json!({"dataset":{"upstream_sha":"../escape"}}), || panic!(
            "invalid key must not download"
        ))
        .is_err());
        assert!(!cache.join("different-sha.sqlite").exists());
    }
}
