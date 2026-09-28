mod db;
mod docs;
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
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_scalar::{Scalar, Servable};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "DDO Tools data API",
        description = "Dungeons & Dragons Online game data, parsed from Maetrim's DDOBuilderV2 data files and served \
                       read-only. Items, augments, set bonuses, filigrees, feats, races, classes, enhancement trees, \
                       spells and clickies, plus the reference vocabularies they share.\n\n\
                       **Caching.** The dataset is rebuilt only when DDOBuilderV2 changes, so every response is \
                       immutable for a dataset version. Each carries a strong `ETag`, a day-long `Cache-Control` and \
                       an `X-Dataset-Version` header naming the DDOBuilderV2 commit; send `If-None-Match` and expect \
                       304. `/v1/version` reports the current commit.\n\n\
                       **Shapes.** Every entity has a numeric `id`. List endpoints that take filters return \
                       `{ total, limit, offset, <name>: [...] }` and page with `limit` (max 10000) and `offset`; \
                       lookup lists return a bare array. `<name>/{id}` returns the full entity with its child \
                       collections. Booleans are JSON booleans; absent values are `null`. Unknown ids are 404 with \
                       `{ \"error\": ... }`.\n\n\
                       **Bulk.** Download `/v1/dump.sqlite` once instead of paging. Icons are at \
                       `/icons/{family}/{icon}.png`, where `family` is `items`, `augments`, `feats`, `enhancements`, \
                       `spells`, `classes`, `filigrees`, `sets`, `sentient-gems` or `ui` and `icon` is the row's \
                       `icon` field.\n\n\
                       Requests are rate limited per IP (5 per second, bursts of 100). CORS allows any origin for GET.",
        license(name = "MIT")
    ),
    tags(
        (name = "meta", description = "Which DDOBuilderV2 commit the data came from, the schema version and row counts"),
        (name = "lookups", description = "The reference vocabularies other responses name things by: stats, bonus types, slots, weapon and damage types, sockets, packs, patrons, quests"),
        (name = "items", description = "Equipment: weapons, armor, shields, jewelry and clothing with their bonuses, sockets and drop sources"),
        (name = "augments", description = "Augments and crafting-family inserts, with the sockets each one fits"),
        (name = "sets", description = "Gear set bonuses, filigree sets and the filigrees themselves"),
        (name = "feats", description = "Feats from the standard list and those granted by classes and races, with requirements and effects"),
        (name = "characters", description = "Playable races and classes with their progressions, granted feats and spell lists"),
        (name = "enhancements", description = "Enhancement, epic destiny and reaper trees with every enhancement and selection"),
        (name = "spells", description = "Spells with damage, saves and class lists, and the clickies items grant"),
        (name = "bulk", description = "The whole dataset as one SQLite download")
    )
)]
struct ApiDoc;

pub fn app(state: AppState) -> Router {
    let (api_router, mut api) =
        OpenApiRouter::with_openapi(ApiDoc::openapi()).merge(routes::router()).split_for_parts();
    api.info.version = format!("dataset {}", state.dataset().upstream_sha);
    docs::attach_examples(&mut api);
    let spec = Arc::new(api.clone());

    let mut router = api_router.merge(Scalar::with_url("/docs", api));
    if let Some(dir) = state.icons_dir() {
        router = router.nest_service("/icons", ServeDir::new(dir));
    }
    router = router
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
            .per_second(5)
            .burst_size(100)
            .key_extractor(SmartIpKeyExtractor)
            .finish()
            .expect("valid governor config");
        router = router.layer(GovernorLayer::new(config));
    }
    router.with_state(state)
}
