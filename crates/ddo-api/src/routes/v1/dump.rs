use crate::error::ApiError;
use crate::state::AppState;
use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderValue};
use axum::response::{IntoResponse, Response};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(dump))
}

#[utoipa::path(
    get,
    path = "/v1/dump.sqlite",
    tag = "bulk",
    summary = "Download the database",
    description = "The whole dataset as one SQLite file (about 14 MB), the same file every other endpoint reads. \
                   Prefer this to paging the list endpoints when you need everything; the filename carries the \
                   dataset commit so you can tell copies apart.",
    responses((status = 200, description = "The SQLite database file", content_type = "application/vnd.sqlite3"))
)]
async fn dump(State(state): State<AppState>) -> Result<Response, ApiError> {
    let path = state.db_path().to_path_buf();
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("opening {}: {e}", path.display())))?;
    let len = file.metadata().await.ok().map(|m| m.len());
    let body = Body::from_stream(tokio_util_stream(file));
    let name = format!("ddo-{}.sqlite", &state.dataset().upstream_sha[..state.dataset().upstream_sha.len().min(12)]);
    let mut response = body.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/vnd.sqlite3"));
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{name}\"")).expect("ascii filename"),
    );
    if let Some(len) = len {
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from(len));
    }
    Ok(response)
}

fn tokio_util_stream(
    file: tokio::fs::File,
) -> impl futures_core_stream::Stream<Item = Result<bytes::Bytes, std::io::Error>> {
    futures_core_stream::unfold(file, |mut file| async move {
        use tokio::io::AsyncReadExt;
        let mut buf = vec![0u8; 64 * 1024];
        match file.read(&mut buf).await {
            Ok(0) => None,
            Ok(n) => {
                buf.truncate(n);
                Some((Ok(bytes::Bytes::from(buf)), file))
            }
            Err(e) => Some((Err(e), file)),
        }
    })
}

mod futures_core_stream {
    pub use futures_util::stream::unfold;
    pub use futures_util::Stream;
}
