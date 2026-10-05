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
mod response_schemas;
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
                       **Shapes.** Each resource has numeric ids; family ids and stat ids are separate spaces and may overlap. Every list returns \
                       `{ total, limit, offset, <name>: [...] }`, and detail routes return an entity with its related collections. \
                       Booleans are JSON booleans, absent values are `null`, and unknown ids return 404.\n\n\
                       **Query parameters.** Different filters are AND-ed. On `/v1/items`, repeat `slot`, `category`, \
                       `pack`, `set`, `quest`, `quest_chain`, `saga` or `bonus` for any matching value; comma-separated \
                       lists are never used. `pack_match`, `set_match`, `quest_match`, `quest_chain_match`, `saga_match` and \
                       `bonus_match` accept `any` (default) or `all`; `slot_match` and `category_match` return 400 \
                       because an item has one slot and category. `q` matches a case-insensitive name substring unless \
                       a route says otherwise. `limit` defaults to 100 and clamps to 1–10000; `offset` defaults to zero \
                       and clamps to nonnegative values; `total` counts before paging. Repeat `sort` keys in priority \
                       order and prefix `-` for descending; nulls sort last in either direction. Each route lists its \
                       sort fields, and invalid or unknown query keys return 400 naming the bad key and accepted keys.\n\n\
                       **Effects and bonuses.** An effect is one line of equipment text; its owner link carries values and \
                       sometimes a bonus type, while its bonus rows derive zero or more typed bonuses from those \
                       amounts. Stats are effects identified by `is_stat`; direct stat links carry a value and type. \
                       Groups expand one level into member stats, with the group named on each derived bonus. \
                       Item, augment and set details render effect templates in owner order. Each line has a concise \
                       `name`, a displayed `verbose_name`, and a separate prose `description`; effect detail carriers \
                       include the same line under `line`. `/v1/effects` lists \
                       effects, groups and stats in one id space; each row's `kind` and `detail_path` identify the detail route. \
                       Its `q` also finds effects through granted stat names, never stats through effect names, and \
                       returns each effect once. Item `bonus` filters accept an effect, group, tier group or stat name, and \
                       `stat:bonus type` narrows a stat to that type. Family and stat detail page items, augments and \
                       set tiers independently with their own limit and offset parameters.\n\n\
                       **Bulk.** Download `/v1/dump.sqlite` once instead of paging. Icons are at \
                       `/icons/{family}/{icon}.png`, where `family` is `items`, `augments`, `feats`, `enhancements`, \
                       `spells`, `classes`, `filigrees`, `sets`, `sentient-gems` or `ui` and `icon` is the row's \
                       `icon` field.\n\n\
                       Requests are rate limited per IP (5 per second, bursts of 100); a limited request gets 429 with `Retry-After` in seconds. CORS allows any origin for GET, on error responses too, and exposes `Retry-After`.",
        license(name = "MIT")
    ),
    components(schemas(
        version::VersionDatasetResponse,
        response_schemas::EffectStatBonus,
        response_schemas::EffectBonusGroup,
        response_schemas::EffectTierPosition,
        response_schemas::EffectLine,
        response_schemas::EffectTierDetail,
        response_schemas::EffectTierStep,
        response_schemas::EffectDamage,
        response_schemas::EffectSetTierCarrier,
        response_schemas::AdventurePacksPageResponseAdventurePacksEntry,
        response_schemas::AdventurePacksPageResponse,
        response_schemas::AdventurePacksDetailResponseItemsEntry,
        response_schemas::AdventurePacksDetailResponse,
        response_schemas::AugmentSlotTypesPageResponseAugmentSlotTypesEntry,
        response_schemas::AugmentSlotTypesPageResponse,
        response_schemas::AugmentsPageResponseAugmentsEntry,
        response_schemas::AugmentsPageResponse,
        response_schemas::AugmentsDetailResponseAdventurePacksEntry,
        response_schemas::AugmentsDetailResponseCraftingEntryCostEntry,
        response_schemas::AugmentsDetailResponseCraftingEntry,
        response_schemas::AugmentsDetailResponseModifiersEntryRequirementsEntry,
        response_schemas::AugmentsDetailResponseModifiersEntry,
        response_schemas::AugmentsDetailResponseQuestsEntry,
        response_schemas::AugmentsDetailResponseSourcesEntry,
        response_schemas::AugmentsDetailResponse,
        response_schemas::BonusTypesPageResponseBonusTypesEntry,
        response_schemas::BonusTypesPageResponse,
        response_schemas::ClassesPageResponseClassesEntry,
        response_schemas::ClassesPageResponse,
        response_schemas::ClassesDetailResponseAutomaticFeatsEntry,
        response_schemas::ClassesDetailResponseFeatSlotsEntry,
        response_schemas::ClassesDetailResponseFeatsEntry,
        response_schemas::ClassesDetailResponseSpellsEntry,
        response_schemas::ClassesDetailResponse,
        response_schemas::ClickiesPageResponseClickiesEntry,
        response_schemas::ClickiesPageResponse,
        response_schemas::CraftingSystemsPageResponseCraftingSystemsEntry,
        response_schemas::CraftingSystemsPageResponse,
        response_schemas::CraftingSystemsDetailResponseIngredientsEntry,
        response_schemas::CraftingSystemsDetailResponseRecipesEntryAugmentsEntry,
        response_schemas::CraftingSystemsDetailResponseRecipesEntryCostEntry,
        response_schemas::CraftingSystemsDetailResponseRecipesEntry,
        response_schemas::CraftingSystemsDetailResponse,
        response_schemas::DamageTypesPageResponseDamageTypesEntry,
        response_schemas::DamageTypesPageResponse,
        response_schemas::EffectsPageResponseEffectsEntryBonusTypesEntry,
        response_schemas::EffectsPageResponseEffectsEntry,
        response_schemas::EffectsPageResponse,
        response_schemas::EffectsDetailResponseAugmentsAugmentsEntry,
        response_schemas::EffectsDetailResponseAugments,
        response_schemas::EffectsDetailResponseItemsItemsEntry,
        response_schemas::EffectsDetailResponseItems,
        response_schemas::EffectsDetailResponseSetTiers,
        response_schemas::EffectsDetailResponseBonusesEntry,
        response_schemas::EffectsDetailResponse,
        response_schemas::EnhancementTreesPageResponseEnhancementTreesEntryRequirementsEntry,
        response_schemas::EnhancementTreesPageResponseEnhancementTreesEntry,
        response_schemas::EnhancementTreesPageResponse,
        response_schemas::EnhancementTreesDetailResponseEnhancementsEntryDcsEntry,
        response_schemas::EnhancementTreesDetailResponseEnhancementsEntryModifiersEntry,
        response_schemas::EnhancementTreesDetailResponseEnhancementsEntryRequirementsEntry,
        response_schemas::EnhancementTreesDetailResponseEnhancementsEntry,
        response_schemas::EnhancementTreesDetailResponseRequirementsEntry,
        response_schemas::EnhancementTreesDetailResponse,
        response_schemas::EquipmentSlotsPageResponseEquipmentSlotsEntry,
        response_schemas::EquipmentSlotsPageResponse,
        response_schemas::EventsPageResponseEventsEntry,
        response_schemas::EventsPageResponse,
        response_schemas::EventsDetailResponseItemsEntry,
        response_schemas::EventsDetailResponse,
        response_schemas::FeatsPageResponseFeatsEntry,
        response_schemas::FeatsPageResponse,
        response_schemas::FeatsDetailResponseAttack,
        response_schemas::FeatsDetailResponseAutoAcquireRequirementsEntry,
        response_schemas::FeatsDetailResponseBonusesEntry,
        response_schemas::FeatsDetailResponseConditionalGroupsEntryRequirementsEntry,
        response_schemas::FeatsDetailResponseConditionalGroupsEntry,
        response_schemas::FeatsDetailResponseDcsEntry,
        response_schemas::FeatsDetailResponseFollowOnModifiersEntry,
        response_schemas::FeatsDetailResponseModifiersEntryRequirementsEntry,
        response_schemas::FeatsDetailResponseModifiersEntry,
        response_schemas::FeatsDetailResponseRequirementsEntry,
        response_schemas::FeatsDetailResponseStancesEntry,
        response_schemas::FeatsDetailResponseSubItemsEntry,
        response_schemas::FeatsDetailResponseThisAttackModifiersEntry,
        response_schemas::FeatsDetailResponse,
        response_schemas::FiligreesPageResponseFiligreesEntryModifiersEntry,
        response_schemas::FiligreesPageResponseFiligreesEntry,
        response_schemas::FiligreesPageResponse,
        response_schemas::GuildBuffsPageResponseGuildBuffsEntryModifiersEntry,
        response_schemas::GuildBuffsPageResponseGuildBuffsEntry,
        response_schemas::GuildBuffsPageResponse,
        response_schemas::ItemsPageResponseItemsEntry,
        response_schemas::ItemsPageResponse,
        response_schemas::ItemsDetailResponseAdventurePacksEntry,
        response_schemas::ItemsDetailResponseArmor,
        response_schemas::ItemsDetailResponseAugmentSlotsEntryOptionsEntryModifiersEntry,
        response_schemas::ItemsDetailResponseAugmentSlotsEntryOptionsEntrySetsEntry,
        response_schemas::ItemsDetailResponseAugmentSlotsEntryOptionsEntry,
        response_schemas::ItemsDetailResponseAugmentSlotsEntry,
        response_schemas::ItemsDetailResponseChallengePacksEntry,
        response_schemas::ItemsDetailResponseClickiesEntry,
        response_schemas::ItemsDetailResponseCraftingSystemsEntry,
        response_schemas::ItemsDetailResponseEventsEntry,
        response_schemas::ItemsDetailResponseModifiersEntry,
        response_schemas::ItemsDetailResponseQuestChainsEntry,
        response_schemas::ItemsDetailResponseQuestsEntry,
        response_schemas::ItemsDetailResponseSagasEntry,
        response_schemas::ItemsDetailResponseSet,
        response_schemas::ItemsDetailResponseSourcesEntry,
        response_schemas::ItemsDetailResponseStarterRewardsEntry,
        response_schemas::ItemsDetailResponseVendorsEntry,
        response_schemas::ItemsDetailResponseWeapon,
        response_schemas::ItemsDetailResponse,
        response_schemas::OptionalBuffsPageResponseOptionalBuffsEntryModifiersEntryRequirementsEntry,
        response_schemas::OptionalBuffsPageResponseOptionalBuffsEntryModifiersEntry,
        response_schemas::OptionalBuffsPageResponseOptionalBuffsEntry,
        response_schemas::OptionalBuffsPageResponse,
        response_schemas::PatronsPageResponsePatronsEntry,
        response_schemas::PatronsPageResponse,
        response_schemas::QuestChainsPageResponseQuestChainsEntry,
        response_schemas::QuestChainsPageResponse,
        response_schemas::QuestChainsDetailResponseQuestsEntry,
        response_schemas::QuestChainsDetailResponseRewardsEntry,
        response_schemas::QuestChainsDetailResponse,
        response_schemas::QuestsPageResponseQuestsEntry,
        response_schemas::QuestsPageResponse,
        response_schemas::QuestsDetailResponseQuestChainsEntry,
        response_schemas::QuestsDetailResponse,
        response_schemas::RacesPageResponseRacesEntry,
        response_schemas::RacesPageResponse,
        response_schemas::RacesDetailResponseAbilityModifiersEntry,
        response_schemas::RacesDetailResponseFeatSlotsEntry,
        response_schemas::RacesDetailResponseFeatsEntry,
        response_schemas::RacesDetailResponseGrantedFeatsEntry,
        response_schemas::RacesDetailResponse,
        response_schemas::SagasPageResponseSagasEntry,
        response_schemas::SagasPageResponse,
        response_schemas::SagasDetailResponseQuestsEntry,
        response_schemas::SagasDetailResponseRewardsEntry,
        response_schemas::SagasDetailResponse,
        response_schemas::SentientGemsPageResponseSentientGemsEntry,
        response_schemas::SentientGemsPageResponse,
        response_schemas::SetsPageResponseSetsEntry,
        response_schemas::SetsPageResponse,
        response_schemas::SetsDetailResponseAugmentsEntry,
        response_schemas::SetsDetailResponseFiligreesEntryModifiersEntry,
        response_schemas::SetsDetailResponseFiligreesEntry,
        response_schemas::SetsDetailResponseItemsEntry,
        response_schemas::SetsDetailResponseTiersEntryModifiersEntry,
        response_schemas::SetsDetailResponseTiersEntry,
        response_schemas::SetsDetailResponse,
        response_schemas::SpellsPageResponseSpellsEntry,
        response_schemas::SpellsPageResponse,
        response_schemas::SpellsDetailResponseClassesEntry,
        response_schemas::SpellsDetailResponseDamageEntry,
        response_schemas::SpellsDetailResponseDcsEntry,
        response_schemas::SpellsDetailResponse,
        response_schemas::StancesPageResponseStancesEntryRequirementsEntry,
        response_schemas::StancesPageResponseStancesEntry,
        response_schemas::StancesPageResponse,
        response_schemas::VendorsPageResponseVendorsEntry,
        response_schemas::VendorsPageResponse,
        response_schemas::VendorsDetailResponseItemsEntry,
        response_schemas::VendorsDetailResponse,
        response_schemas::WeaponTypesPageResponseWeaponTypesEntry,
        response_schemas::WeaponTypesPageResponse,
    )),
    tags(
        (name = "meta", description = "Dataset version, schema version and row counts"),
        (name = "items", description = "Equipment, its effects, sockets, vocabularies and sources"),
        (name = "augments", description = "Augments, compatible sockets, effects and crafting recipes"),
        (name = "bonuses", description = "Effect families, stats, bonus types and carrier backlinks"),
        (name = "sets", description = "Gear and filigree sets, tiers, filigrees and sentient gems"),
        (name = "crafting", description = "Crafting systems, ingredients and recipes"),
        (name = "quests", description = "Quests, challenges, packs, patrons, chains and sagas"),
        (name = "sources", description = "Vendors and events that offer equipment"),
        (name = "spells", description = "Spells and item clickies"),
        (name = "feats", description = "Feats and their requirements, attacks and modifiers"),
        (name = "enhancements", description = "Enhancement, epic destiny and reaper trees"),
        (name = "classes", description = "Class progression, feats and spell lists"),
        (name = "races", description = "Playable races, traits and granted feats"),
        (name = "stances", description = "Standalone toggleable combat stances"),
        (name = "buffs", description = "Guild buffs and optional planner buffs"),
        (name = "bulk", description = "Complete SQLite dataset download")
    )
)]
struct ApiDoc;

declare_response_examples! { "v1":
    ("/v1/version", "version"),
    ("/v1/bonus-types", "bonus-types"),
    ("/v1/effects", "effects"),
    ("/v1/effects/{id}", "effects_id"),
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
