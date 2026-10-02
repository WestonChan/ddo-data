use axum::extract::Request;
use axum::http::{header, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;

const DATA_CACHE_CONTROL: &str = "public, max-age=86400, stale-while-revalidate=604800";
const ERROR_CACHE_CONTROL: &str = "no-store";

pub(crate) async fn apply_cache_policy(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let is_error = response.status().as_u16() >= 400;
    let headers = response.headers_mut();
    if is_error {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(ERROR_CACHE_CONTROL));
        headers.remove(header::ETAG);
    } else if !headers.contains_key(header::CACHE_CONTROL) {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(DATA_CACHE_CONTROL));
    }
    response
}
