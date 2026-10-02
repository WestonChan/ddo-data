use crate::state::AppState;
use axum::extract::{Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::hash::{DefaultHasher, Hash, Hasher};

const DATASET_VERSION_HEADER: &str = "x-dataset-version";

pub(crate) async fn apply_etag(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let upstream_sha = &state.dataset_version().upstream_sha;
    let mut hasher = DefaultHasher::new();
    upstream_sha.hash(&mut hasher);
    state.api_commit().hash(&mut hasher);
    request.uri().path().hash(&mut hasher);
    request.uri().query().hash(&mut hasher);
    let etag = format!("\"{:016x}\"", hasher.finish());
    let dataset_version = HeaderValue::from_str(upstream_sha).unwrap_or_else(|_| HeaderValue::from_static("unknown"));

    let is_not_modified =
        request.headers().get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()).is_some_and(|v| {
            v.split(',').any(|offered_etag| offered_etag.trim() == etag || offered_etag.trim() == "*")
        });
    if is_not_modified {
        let mut response = StatusCode::NOT_MODIFIED.into_response();
        response.headers_mut().insert(header::ETAG, HeaderValue::from_str(&etag).expect("hex etag"));
        response.headers_mut().insert(DATASET_VERSION_HEADER, dataset_version);
        return response;
    }

    let mut response = next.run(request).await;
    if response.status().is_success() {
        response.headers_mut().insert(header::ETAG, HeaderValue::from_str(&etag).expect("hex etag"));
    }
    response.headers_mut().insert(DATASET_VERSION_HEADER, dataset_version);
    response
}
