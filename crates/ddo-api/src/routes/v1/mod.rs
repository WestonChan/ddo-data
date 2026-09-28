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

use crate::docs::examples;
use crate::state::AppState;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;

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

examples! { "v1":
    ("/v1/version", "version"),
    ("/v1/stats", "stats"),
    ("/v1/bonus-types", "bonus-types"),
    ("/v1/equipment-slots", "equipment-slots"),
    ("/v1/weapon-types", "weapon-types"),
    ("/v1/damage-types", "damage-types"),
    ("/v1/augment-slot-types", "augment-slot-types"),
    ("/v1/adventure-packs", "adventure-packs"),
    ("/v1/patrons", "patrons"),
    ("/v1/quests", "quests"),
    ("/v1/items", "items"),
    ("/v1/items/{id}", "items_id"),
    ("/v1/augments", "augments"),
    ("/v1/augments/{id}", "augments_id"),
    ("/v1/sets", "sets"),
    ("/v1/sets/{id}", "sets_id"),
    ("/v1/filigrees", "filigrees"),
    ("/v1/feats", "feats"),
    ("/v1/feats/{id}", "feats_id"),
    ("/v1/races", "races"),
    ("/v1/races/{id}", "races_id"),
    ("/v1/classes", "classes"),
    ("/v1/classes/{id}", "classes_id"),
    ("/v1/enhancement-trees", "enhancement-trees"),
    ("/v1/enhancement-trees/{id}", "enhancement-trees_id"),
    ("/v1/spells", "spells"),
    ("/v1/spells/{id}", "spells_id"),
    ("/v1/clickies", "clickies"),
}

pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

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
