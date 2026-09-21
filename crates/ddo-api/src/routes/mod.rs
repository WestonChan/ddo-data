//! Every endpoint, grouped by family. Each module exposes `router()` for `OpenApiRouter`.

mod augments;
mod characters;
mod dump;
mod feats;
mod items;
mod lookups;
mod sets;
mod spells;
mod trees;
mod version;

use crate::state::AppState;
use utoipa_axum::router::OpenApiRouter;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .merge(version::router())
        .merge(lookups::router())
        .merge(items::router())
        .merge(augments::router())
        .merge(sets::router())
        .merge(feats::router())
        .merge(characters::router())
        .merge(trees::router())
        .merge(spells::router())
        .merge(dump::router())
}
