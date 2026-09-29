use crate::error::ApiError;
use crate::state::AppState;
use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderValue};
use axum::response::{IntoResponse, Response};
use futures_util::stream::{unfold, Stream};
use tokio::io::AsyncReadExt;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(database_dump))
}

const CHUNK_SIZE_BYTES: usize = 64 * 1024;

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
async fn database_dump(State(state): State<AppState>) -> Result<Response, ApiError> {
    let db_path = state.db_path().to_path_buf();
    let db_file = tokio::fs::File::open(&db_path)
        .await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("opening {}: {e}", db_path.display())))?;
    let file_size_bytes = db_file.metadata().await.ok().map(|metadata| metadata.len());
    let body = Body::from_stream(file_chunks(db_file));
    let upstream_sha = &state.dataset_version().upstream_sha;
    let file_name = format!("ddo-{}.sqlite", &upstream_sha[..upstream_sha.len().min(12)]);
    let mut response = body.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/vnd.sqlite3"));
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{file_name}\"")).expect("ascii filename"),
    );
    if let Some(file_size_bytes) = file_size_bytes {
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from(file_size_bytes));
    }
    Ok(response)
}

fn file_chunks(file: tokio::fs::File) -> impl Stream<Item = Result<bytes::Bytes, std::io::Error>> {
    unfold(file, |mut file| async move {
        let mut chunk = vec![0u8; CHUNK_SIZE_BYTES];
        match file.read(&mut chunk).await {
            Ok(0) => None,
            Ok(read_byte_count) => {
                chunk.truncate(read_byte_count);
                Some((Ok(bytes::Bytes::from(chunk)), file))
            }
            Err(e) => Some((Err(e), file)),
        }
    })
}
