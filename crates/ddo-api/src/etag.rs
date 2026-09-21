//! Strong ETags from the dataset version and the request, so a CDN or browser can revalidate
//! with `If-None-Match` and get a 304 without the database being touched.

use crate::state::AppState;
use axum::extract::{Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::hash::{DefaultHasher, Hash, Hasher};

pub async fn etag(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let mut hasher = DefaultHasher::new();
    // Not the method: a HEAD must yield the same tag as the GET it stands in for.
    state.dataset().upstream_sha.hash(&mut hasher);
    request.uri().path().hash(&mut hasher);
    request.uri().query().hash(&mut hasher);
    let tag = format!("\"{:016x}\"", hasher.finish());
    let version =
        HeaderValue::from_str(&state.dataset().upstream_sha).unwrap_or_else(|_| HeaderValue::from_static("unknown"));

    let matches = request
        .headers()
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').any(|candidate| candidate.trim() == tag || candidate.trim() == "*"));
    if matches {
        let mut response = StatusCode::NOT_MODIFIED.into_response();
        response.headers_mut().insert(header::ETAG, HeaderValue::from_str(&tag).expect("hex etag"));
        response.headers_mut().insert("x-dataset-version", version);
        return response;
    }

    let mut response = next.run(request).await;
    if response.status().is_success() {
        response.headers_mut().insert(header::ETAG, HeaderValue::from_str(&tag).expect("hex etag"));
    }
    response.headers_mut().insert("x-dataset-version", version);
    response
}
