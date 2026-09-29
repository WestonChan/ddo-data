mod db;
mod docs;
pub mod error;
mod etag;
mod query;
mod routes;
pub mod state;

pub use docs::ResponseExample;
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
use utoipa::openapi::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_scalar::{Scalar, Servable};

fn versioned_api(state: &AppState, api_version: &'static str) -> (Router<AppState>, OpenApi) {
    let (router, mut spec) =
        OpenApiRouter::with_openapi(routes::v1::openapi()).merge(routes::v1::router()).split_for_parts();
    spec.info.version = format!("{api_version} · dataset {}", state.dataset_version().upstream_sha);
    docs::attach_examples(&mut spec, routes::v1::RESPONSE_EXAMPLES);
    (router, spec)
}

fn mount_docs(router: Router<AppState>, api_version: &'static str, spec: OpenApi) -> Router<AppState> {
    let shared_spec = Arc::new(spec.clone());
    router.merge(Scalar::with_url(format!("/{api_version}/docs"), spec)).route(
        &format!("/{api_version}/openapi.json"),
        get(move || {
            let shared_spec = shared_spec.clone();
            async move { Json((*shared_spec).clone()) }
        }),
    )
}

pub fn v1_response_examples() -> &'static [ResponseExample] {
    routes::v1::RESPONSE_EXAMPLES
}

pub fn app(state: AppState) -> Router {
    let (api_router, spec) = versioned_api(&state, "v1");
    let mut router = mount_docs(api_router, "v1", spec);
    if let Some(icons_dir) = state.icons_dir() {
        router = router.nest_service("/icons", ServeDir::new(icons_dir));
    }
    let latest_version = routes::LATEST_VERSION;
    router = router
        .route("/docs", get(move || async move { Redirect::permanent(&format!("/{latest_version}/docs")) }))
        .route(
            "/openapi.json",
            get(move || async move { Redirect::permanent(&format!("/{latest_version}/openapi.json")) }),
        )
        .route("/", get(move || async move { Redirect::permanent(&format!("/{latest_version}/docs")) }))
        .layer(middleware::from_fn_with_state(state.clone(), etag::apply_etag))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=86400, stale-while-revalidate=604800"),
        ))
        .layer(CompressionLayer::new())
        .layer(CorsLayer::new().allow_origin(Any).allow_methods([Method::GET, Method::HEAD]).allow_headers(Any))
        .layer(TraceLayer::new_for_http());

    if state.is_rate_limited() {
        let rate_limit_config = GovernorConfigBuilder::default()
            .per_second(5)
            .burst_size(100)
            .key_extractor(SmartIpKeyExtractor)
            .finish()
            .expect("valid governor config");
        router = router.layer(GovernorLayer::new(rate_limit_config));
    }
    router.with_state(state)
}
