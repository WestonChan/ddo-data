mod augments;
mod bonuses;
mod buffs;
mod classes;
mod crafting;
mod dump;
mod enhancement_trees;
mod feats;
mod items;
mod quest_series;
mod quests;
mod races;
mod sets;
mod spells;
mod stances;
mod vendors_and_events;
mod version;

use crate::docs::declare_response_examples;
use crate::state::AppState;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "DDO Tools data API",
        description = "Dungeons & Dragons Online game data, parsed from Maetrim's DDOBuilderV2 data files and served \
                       read-only. Items, augments, set bonuses, filigrees, sentient gems, feats, stances, guild \
                       and optional buffs, races, classes, enhancement trees, spells and clickies, plus the \
                       reference vocabularies they share, and the quest facts, quest chains, sagas, vendors, events and crafting recipes read from ddowiki.\n\n\
                       **Caching.** The dataset is rebuilt only when DDOBuilderV2 changes, so a response changes only \
                       with a new deployment. Each successful response carries a strong `ETag`, an \
                       `X-Dataset-Version` header naming the DDOBuilderV2 commit and \
                       `Cache-Control: public, max-age=300, stale-while-revalidate=3600`: fresh for five minutes, \
                       then revalidated with `If-None-Match`, which answers 304 unless the dataset or the API build \
                       changed (the `ETag` hashes both), so a \
                       deploy is visible within minutes. `/v1/dump.sqlite` and the icons follow the same policy. \
                       `/v1/version` reports the current commit and carries `Cache-Control: no-cache`, so every use \
                       revalidates it. Error responses (every 4xx and 5xx, 429 included) carry \
                       `Cache-Control: no-store` and no `ETag`.\n\n\
                       **Shapes.** Every entity has a numeric `id`. List endpoints that take filters return \
                       `{ total, limit, offset, <name>: [...] }` and page with `limit` (max 10000) and `offset`; \
                       lookup lists return a bare array. `<name>/{id}` returns the full entity with its child \
                       collections. Booleans are JSON booleans; absent values are `null`. Unknown ids are 404 with \
                       `{ \"error\": ... }`.\n\n\
                       **Bulk.** Download `/v1/dump.sqlite` once instead of paging. Icons are at \
                       `/icons/{family}/{icon}.png`, where `family` is `items`, `augments`, `feats`, `enhancements`, \
                       `spells`, `classes`, `filigrees`, `sets`, `sentient-gems` or `ui` and `icon` is the row's \
                       `icon` field.\n\n\
                       Requests are rate limited per IP (5 per second, bursts of 100); a limited request gets 429 with `Retry-After` in seconds. CORS allows any origin for GET, on error responses too, and exposes `Retry-After`.",
        license(name = "MIT")
    ),
    tags(
        (name = "meta", description = "Which DDOBuilderV2 commit the data came from, the schema version and row counts"),
        (name = "items", description = "Equipment: weapons, armor, shields, jewelry and clothing with their bonuses, sockets and drop sources, plus the slot, weapon, damage and socket vocabularies they use. A few items are read from ddowiki until DDOBuilderV2 carries them"),
        (name = "augments", description = "Augments and crafting-family inserts, with the sockets each one fits and the crafting recipes that yield them"),
        (name = "crafting", description = "Crafting systems from the wiki: ingredients, and recipes that turn ingredients into the augments Maetrim's files carry"),
        (name = "sets", description = "Gear set bonuses, sentient-weapon filigree sets, the filigrees themselves and the sentient gems they slot into"),
        (name = "feats", description = "Feats from the standard list and those granted by classes and races, with requirements and effects"),
        (name = "races", description = "Playable races with ability modifiers, granted feats and racial feats"),
        (name = "classes", description = "Classes and archetypes with progressions, feat slots and spell lists"),
        (name = "stances", description = "Combat and defensive stances a character can toggle, with their requirements and effects"),
        (name = "buffs", description = "Guild buffs and the spell, potion and party buffs a planner can toggle on, with their effects"),
        (name = "enhancements", description = "Enhancement, epic destiny and reaper trees with every enhancement and selection"),
        (name = "spells", description = "Spells with damage, saves and class lists, and the clickies items grant"),
        (name = "bonuses", description = "The stats a bonus can apply to, the bonus types that decide whether two bonuses stack, and the enchantments (stats and named effects) items carry, which /v1/items filters by"),
        (name = "quests", description = "Quests, challenges, adventure packs and favor patrons: the sources items drop from, with each quest's free-to-play status, legendary level, zone, quest giver and flagging from ddowiki, and the quests DDOBuilderV2 lacks read whole from ddowiki. Quest chains and sagas from ddowiki are listed here too, each with its quests and the end rewards its NPC gives once they are done"),
        (name = "sources", description = "Vendors and events from ddowiki: the NPCs that sell or trade items and the festivals that reward them, each with its items. Item detail `sources` also names the quests, chains, sagas, packs, challenge packs, crafting systems and starter levels an item comes from"),
        (name = "bulk", description = "The whole dataset as one SQLite download")
    )
)]
struct ApiDoc;

declare_response_examples! { "v1":
    ("/v1/version", "version"),
    ("/v1/stats", "stats"),
    ("/v1/bonus-types", "bonus-types"),
    ("/v1/enchantments", "enchantments"),
    ("/v1/equipment-slots", "equipment-slots"),
    ("/v1/weapon-types", "weapon-types"),
    ("/v1/damage-types", "damage-types"),
    ("/v1/augment-slot-types", "augment-slot-types"),
    ("/v1/adventure-packs", "adventure-packs"),
    ("/v1/adventure-packs/{id}", "adventure-packs_id"),
    ("/v1/patrons", "patrons"),
    ("/v1/quests", "quests"),
    ("/v1/quests/{id}", "quests_id"),
    ("/v1/quest-chains", "quest-chains"),
    ("/v1/quest-chains/{id}", "quest-chains_id"),
    ("/v1/sagas", "sagas"),
    ("/v1/sagas/{id}", "sagas_id"),
    ("/v1/vendors", "vendors"),
    ("/v1/vendors/{id}", "vendors_id"),
    ("/v1/events", "events"),
    ("/v1/events/{id}", "events_id"),
    ("/v1/items", "items"),
    ("/v1/items/{id}", "items_id"),
    ("/v1/augments", "augments"),
    ("/v1/augments/{id}", "augments_id"),
    ("/v1/crafting-systems", "crafting-systems"),
    ("/v1/crafting-systems/{id}", "crafting-systems_id"),
    ("/v1/sets", "sets"),
    ("/v1/sets/{id}", "sets_id"),
    ("/v1/filigrees", "filigrees"),
    ("/v1/sentient-gems", "sentient-gems"),
    ("/v1/feats", "feats"),
    ("/v1/feats/{id}", "feats_id"),
    ("/v1/stances", "stances"),
    ("/v1/guild-buffs", "guild-buffs"),
    ("/v1/optional-buffs", "optional-buffs"),
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

pub(crate) fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

pub(crate) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .merge(version::router())
        .merge(items::router())
        .merge(augments::router())
        .merge(crafting::router())
        .merge(sets::router())
        .merge(feats::router())
        .merge(stances::router())
        .merge(buffs::router())
        .merge(races::router())
        .merge(classes::router())
        .merge(enhancement_trees::router())
        .merge(spells::router())
        .merge(bonuses::router())
        .merge(quests::router())
        .merge(quest_series::router())
        .merge(vendors_and_events::router())
        .merge(dump::router())
}
