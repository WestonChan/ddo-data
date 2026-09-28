mod db;
mod docs;
pub mod error;
mod etag;
mod routes;
pub mod state;

pub use state::AppState;

use axum::http::{header, HeaderValue, Method};
use axum::response::Redirect;
use axum::routing::get;
use axum::{middleware, Json, Router};
use std::sync::Arc;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::SmartIpKeyExtractor;
use tower_governor::GovernorLayer;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;
use utoipa_axum::router::OpenApiRouter;
use utoipa_scalar::{Scalar, Servable};

fn version_docs(state: &AppState, version: &'static str) -> (Router<AppState>, utoipa::openapi::OpenApi) {
    let (router, mut api) =
        OpenApiRouter::with_openapi(routes::v1::openapi()).merge(routes::v1::router()).split_for_parts();
    api.info.version = format!("{version} · dataset {}", state.dataset().upstream_sha);
    docs::attach_examples(&mut api, routes::v1::EXAMPLES);
    (router, api)
}

fn mount_docs(router: Router<AppState>, version: &'static str, api: utoipa::openapi::OpenApi) -> Router<AppState> {
    let spec = Arc::new(api.clone());
    router.merge(Scalar::with_url(format!("/{version}/docs"), api)).route(
        &format!("/{version}/openapi.json"),
        get(move || {
            let spec = spec.clone();
            async move { Json((*spec).clone()) }
        }),
    )
}

pub fn app(state: AppState) -> Router {
    let (api_router, api) = version_docs(&state, "v1");
    let mut router = mount_docs(api_router, "v1", api);
    if let Some(dir) = state.icons_dir() {
        router = router.nest_service("/icons", ServeDir::new(dir));
    }
    let latest = routes::LATEST;
    router = router
        .route("/docs", get(move || async move { Redirect::permanent(&format!("/{latest}/docs")) }))
        .route("/openapi.json", get(move || async move { Redirect::permanent(&format!("/{latest}/openapi.json")) }))
        .route("/", get(move || async move { Redirect::permanent(&format!("/{latest}/docs")) }))
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
            .per_second(5)
            .burst_size(100)
            .key_extractor(SmartIpKeyExtractor)
            .finish()
            .expect("valid governor config");
        router = router.layer(GovernorLayer::new(config));
    }
    router.with_state(state)
}
