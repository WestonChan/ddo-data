//! Read-only public API over the DDO Tools game database.
//!
//! The database is immutable for the lifetime of a deployment (it is baked into the image), so
//! every response carries a strong ETag derived from the dataset version and the request, a long
//! `Cache-Control`, and an `X-Dataset-Version` header. A CDN in front can cache everything.

mod db;
pub mod error;
mod etag;
mod routes;
pub mod state;

pub use state::AppState;

use axum::http::{header, HeaderValue, Method};
use axum::routing::get;
use axum::{middleware, Json, Router};
use std::sync::Arc;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::SmartIpKeyExtractor;
use tower_governor::GovernorLayer;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_scalar::{Scalar, Servable};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "DDO Tools data API",
        description = "Dungeons & Dragons Online game data parsed from Maetrim's DDOBuilderV2, served read-only. \
                       Every response is immutable for a dataset version: honour ETag / If-None-Match and Cache-Control. \
                       For bulk access download /v1/dump.sqlite once instead of paging.",
        license(name = "MIT")
    ),
    tags(
        (name = "meta", description = "Dataset and schema information"),
        (name = "lookups", description = "Small reference vocabularies"),
        (name = "items", description = "Equipment"),
        (name = "augments", description = "Augments and the sockets they fit"),
        (name = "sets", description = "Gear sets and filigrees"),
        (name = "feats", description = "Feats from the standard list, classes and races"),
        (name = "characters", description = "Races and classes"),
        (name = "enhancements", description = "Enhancement trees"),
        (name = "spells", description = "Spells and item clickies"),
        (name = "bulk", description = "Whole-dataset downloads")
    )
)]
struct ApiDoc;

/// The router with every middleware attached. Rate limiting is applied only when the state asks
/// for it, since it needs a real peer address.
pub fn app(state: AppState) -> Router {
    let (api_router, mut api) =
        OpenApiRouter::with_openapi(ApiDoc::openapi()).merge(routes::router()).split_for_parts();
    api.info.version = format!("dataset {}", state.dataset().upstream_sha);
    let spec = Arc::new(api.clone());

    let mut router = api_router
        .merge(Scalar::with_url("/docs", api))
        .route(
            "/openapi.json",
            get(move || {
                let spec = spec.clone();
                async move { Json((*spec).clone()) }
            }),
        )
        .route("/", get(|| async { axum::response::Redirect::permanent("/docs") }))
        .layer(middleware::from_fn_with_state(state.clone(), etag::etag))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=86400, stale-while-revalidate=604800"),
        ))
        .layer(CompressionLayer::new())
        .layer(CorsLayer::new().allow_origin(Any).allow_methods([Method::GET, Method::HEAD]).allow_headers(Any))
        .layer(TraceLayer::new_for_http());

    if state.rate_limited() {
        let config = GovernorConfigBuilder::default()
            // A page load fires a few dozen requests; a scraper looping ids does not get far.
            .per_second(5)
            .burst_size(100)
            .key_extractor(SmartIpKeyExtractor)
            .finish()
            .expect("valid governor config");
        router = router.layer(GovernorLayer::new(config));
    }
    router.with_state(state)
}
