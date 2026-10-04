use crate::enums::{
    AbilityOwner, ArmorType, CorrectionKind, CraftingTier, EnhancementTreeKind, FeatSource, Handedness, ItemCategory,
    LootType, ModifierSource, Provenance, RequirementGroupKind, RequirementOwner, SagaTier, SaveProgression,
    SourceKind,
};
use std::sync::LazyLock;

pub const SCHEMA_VERSION: i64 = 21;

fn sql_in_clause<'a>(allowed_values: impl Iterator<Item = &'a str>) -> String {
    let quoted_values: Vec<String> = allowed_values.map(|value| format!("'{value}'")).collect();
    format!("IN ({})", quoted_values.join(", "))
}

static DDL: LazyLock<String> = LazyLock::new(|| {
    let item_category = sql_in_clause(ItemCategory::ALL.iter().map(|c| c.as_str()));
    let provenance = sql_in_clause(Provenance::ALL.iter().map(|s| s.as_str()));
    let handedness = sql_in_clause(Handedness::ALL.iter().map(|h| h.as_str()));
    let armor_type = sql_in_clause(ArmorType::ALL.iter().map(|a| a.as_str()));
    let loot_type = sql_in_clause(LootType::ALL.iter().map(|l| l.as_str()));
    let modifier_source = sql_in_clause(ModifierSource::ALL.iter().map(|s| s.as_str()));
    let requirement_owner = sql_in_clause(RequirementOwner::ALL.iter().map(|o| o.as_str()));
    let feat_source = sql_in_clause(FeatSource::ALL.iter().map(|f| f.as_str()));
    let ability_owner = sql_in_clause(AbilityOwner::ALL.iter().map(|o| o.as_str()));
    let save_progression = sql_in_clause(SaveProgression::ALL.iter().map(|s| s.as_str()));
    let tree_kind = sql_in_clause(EnhancementTreeKind::ALL.iter().map(|k| k.as_str()));
    let requirement_group = sql_in_clause(RequirementGroupKind::ALL.iter().map(|g| g.as_str()));
    let crafting_tier = sql_in_clause(CraftingTier::ALL.iter().map(|t| t.as_str()));
    let correction_kind = sql_in_clause(CorrectionKind::ALL.iter().map(|k| k.as_str()));
    let source_kind = sql_in_clause(SourceKind::ALL.iter().map(|k| k.as_str()));
    let source_id_for_kind: Vec<String> = SourceKind::ALL
        .iter()
        .map(|kind| format!("WHEN '{}' THEN {} IS NOT NULL", kind.as_str(), kind.identifying_column()))
        .collect();
    let source_id_for_kind = source_id_for_kind.join(" ");
    let source_id_count: Vec<String> =
        SourceKind::identifying_columns().iter().map(|column| format!("({column} IS NOT NULL)")).collect();
    let source_id_count = source_id_count.join(" + ");
    let kinds_with_loot_type =
        sql_in_clause(SourceKind::ALL.iter().filter(|kind| kind.has_loot_type()).map(|kind| kind.as_str()));
    let source_ids = SourceKind::identifying_columns().join(", ");
    let saga_tier = sql_in_clause(SagaTier::ALL.iter().map(|t| t.as_str()));
    let quest_series_columns = format!(
        "name     TEXT    NOT NULL UNIQUE,
    pack_id  INTEGER REFERENCES adventure_packs(id),
    provenance TEXT  NOT NULL CHECK (provenance {provenance}),
    wiki_url TEXT    NOT NULL"
    );
    let quest_series_quest_columns = "quest_id   INTEGER NOT NULL REFERENCES quests(id) ON DELETE CASCADE,
    sort_order INTEGER NOT NULL,";
    format!(
        r#"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS schema_version (
    version    INTEGER NOT NULL,
    applied_at TEXT    NOT NULL DEFAULT (datetime('now'))
);

-- Which upstream commit the game data was parsed from, and when.
CREATE TABLE IF NOT EXISTS dataset_version (
    upstream_sha TEXT NOT NULL,
    built_at     TEXT NOT NULL
);

-- Seed tables ----------------------------------------------------------------

CREATE TABLE IF NOT EXISTS stats (
    id       INTEGER PRIMARY KEY,
    name     TEXT    NOT NULL UNIQUE,
    category TEXT    NOT NULL
);

CREATE TABLE IF NOT EXISTS bonus_types (
    id               INTEGER PRIMARY KEY,
    name             TEXT    NOT NULL UNIQUE,
    stacks_with_self INTEGER NOT NULL DEFAULT 0 CHECK (stacks_with_self IN (0, 1))
);

CREATE TABLE IF NOT EXISTS equipment_slots (
    id         INTEGER PRIMARY KEY,
    name       TEXT    NOT NULL UNIQUE,
    sort_order INTEGER NOT NULL,
    category   TEXT    NOT NULL
);

CREATE TABLE IF NOT EXISTS weapon_proficiencies (
    id   INTEGER PRIMARY KEY,
    name TEXT    NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS weapon_types (
    id             INTEGER PRIMARY KEY,
    name           TEXT    NOT NULL UNIQUE,
    proficiency_id INTEGER REFERENCES weapon_proficiencies(id),
    is_shield      INTEGER NOT NULL DEFAULT 0 CHECK (is_shield IN (0, 1))
);

CREATE TABLE IF NOT EXISTS damage_types (
    id       INTEGER PRIMARY KEY,
    name     TEXT    NOT NULL UNIQUE,
    category TEXT    NOT NULL
);

-- Vocabularies filled from the data ------------------------------------------

CREATE TABLE IF NOT EXISTS item_materials (
    id   INTEGER PRIMARY KEY,
    name TEXT    NOT NULL UNIQUE                      -- <Material>, normalised
);

-- A socket an item can carry: a gem colour, or one crafting family's slot. `label` is what the
-- UI shows; family/variant/qualifier are its decomposition so nothing parses a label.
CREATE TABLE IF NOT EXISTS augment_slot_types (
    id        INTEGER PRIMARY KEY,
    label     TEXT NOT NULL UNIQUE,                   -- computed from <ItemAugment><Type>
    family    TEXT NOT NULL,                          -- 'standard', 'dino', 'lamordia', 'slavers', 'upgrade', 'crafting'
    variant   TEXT NOT NULL,                          -- the colour, tier, or family parameter
    qualifier TEXT                                    -- 'weapon' / 'armor' / 'accessory' / 'legendary'
);

CREATE TABLE IF NOT EXISTS adventure_packs (
    id              INTEGER PRIMARY KEY,
    name            TEXT    NOT NULL UNIQUE,          -- <Quest><AdventurePack>
    is_free_to_play INTEGER NOT NULL DEFAULT 0 CHECK (is_free_to_play IN (0, 1))
);

CREATE TABLE IF NOT EXISTS patrons (
    id   INTEGER PRIMARY KEY,
    name TEXT    NOT NULL UNIQUE                      -- <Patron><Name>
);

CREATE TABLE IF NOT EXISTS quests (
    id         INTEGER PRIMARY KEY,
    name       TEXT    NOT NULL UNIQUE,               -- <Quest><Name>
    pack_id    INTEGER REFERENCES adventure_packs(id),
    patron_id  INTEGER REFERENCES patrons(id),
    level      INTEGER,                               -- first <Levels> entry (heroic)
    epic_level INTEGER,                               -- second <Levels> entry, if any
    favor      INTEGER,                               -- <Favor>
    is_raid    INTEGER NOT NULL DEFAULT 0 CHECK (is_raid IN (0, 1)),  -- <IsRaid/> present
    epic_name  TEXT,                                  -- <EpicName>: the epic version's name, when it differs
    difficulties TEXT  NOT NULL DEFAULT '[]',         -- JSON array of the <Casual/> … <Solo/> flags present
    is_challenge INTEGER NOT NULL DEFAULT 0 CHECK (is_challenge IN (0, 1)),  -- from Challenges.xml, not Quests.xml
    max_level  INTEGER,                               -- a challenge's <LevelRange> high end; level is the low end
    is_free_to_play INTEGER NOT NULL DEFAULT 0 CHECK (is_free_to_play IN (0, 1)),  -- data/wiki quests `free_to_play`
    legendary_level INTEGER,                          -- data/wiki quests `legendary_level`
    zone       TEXT,                                  -- data/wiki quests `zone`: the page's "Takes place in"
    bestowed_by TEXT,                                 -- data/wiki quests `bestowed_by`
    flagging   TEXT,                                  -- data/wiki quests `flagging`, free text
    provenance TEXT    NOT NULL DEFAULT 'maetrim' CHECK (provenance {provenance}),  -- 'wiki' for a data/wiki quests entry that creates the quest
    CHECK (is_challenge = 1 OR pack_id IS NOT NULL)
);

-- Items --------------------------------------------------------------------------

-- Upstream items the ETL saw and deliberately left out, so a consumer (and the coverage diff) can
-- tell "not in the data" from "excluded on purpose".
CREATE TABLE IF NOT EXISTS excluded_items (
    name   TEXT NOT NULL PRIMARY KEY,                 -- <Name>
    reason TEXT NOT NULL                              -- e.g. 'cosmetic-only slots'
);

CREATE TABLE IF NOT EXISTS items (
    id                INTEGER PRIMARY KEY,
    name              TEXT    NOT NULL UNIQUE CHECK (TRIM(name) <> ''),  -- <Name>
    slot_id           INTEGER NOT NULL REFERENCES equipment_slots(id),  -- computed from <EquipmentSlot>
    item_category     TEXT    NOT NULL CHECK (item_category {item_category}),  -- computed
    item_type         TEXT,                           -- <Weapon> or <Armor> subtype ('Longsword', 'Docent')
    minimum_level     INTEGER,                        -- <MinLevel>
    enhancement_bonus INTEGER,                        -- <Buff> WeaponEnchantment / ArmorEnchantment / ShieldEnchantment Value1
    material_id       INTEGER REFERENCES item_materials(id),
    race_required     TEXT,                           -- <Requirements><Requirement><Type>Race*</Type>
    icon              TEXT,                           -- <Icon>, a key into ItemImages/
    description       TEXT,                           -- <Description>
    drop_location     TEXT,                           -- <DropLocation>, free text
    set_bonus         TEXT,                           -- <SetBonus>, name only until the set tables land
    accepts_sentience INTEGER NOT NULL DEFAULT 0 CHECK (accepts_sentience IN (0, 1)),  -- <IsAcceptsSentience/>
    is_minor_artifact INTEGER NOT NULL DEFAULT 0 CHECK (is_minor_artifact IN (0, 1)),  -- <MinorArtifact/>
    is_legacy         INTEGER NOT NULL DEFAULT 0 CHECK (is_legacy IN (0, 1)),  -- a '(legacy)' or '(historic)' name, every <DropLocation> segment a data/legacy_drop_sources.toml text, or an is_legacy correction
    wiki_url          TEXT    NOT NULL CHECK (wiki_url GLOB 'https://ddowiki.com/page/?*'),  -- computed from name, or data/wiki items `page`
    provenance        TEXT    NOT NULL DEFAULT 'maetrim' CHECK (provenance {provenance})  -- 'wiki' for a data/wiki items entry
);
CREATE INDEX IF NOT EXISTS idx_items_slot ON items(slot_id);
CREATE INDEX IF NOT EXISTS idx_items_minimum_level ON items(minimum_level);
CREATE INDEX IF NOT EXISTS idx_items_category ON items(item_category);

CREATE TABLE IF NOT EXISTS item_weapon_stats (
    item_id               INTEGER PRIMARY KEY REFERENCES items(id) ON DELETE CASCADE,
    weapon_type_id        INTEGER NOT NULL REFERENCES weapon_types(id),  -- <Weapon>
    base_dice_count       INTEGER,                    -- <BaseDice><Number>
    base_dice_sides       INTEGER,                    -- <BaseDice><Sides>
    base_dice_bonus       INTEGER,                    -- <BaseDice><Bonus>
    damage_multiplier     REAL,                       -- <WeaponDamage>, the [W] multiplier
    critical_threat_range INTEGER,                    -- <CriticalThreatRange>, count of threatening faces
    critical_multiplier   INTEGER,                    -- <CriticalMultiplier>
    attack_modifier       TEXT,                       -- <AttackModifier> ability, comma-joined if several
    damage_modifier       TEXT,                       -- <DamageModifier>
    handedness            TEXT CHECK (handedness {handedness}),  -- computed from <EquipmentSlot> and weapon type
    damage                TEXT,                       -- computed display string, e.g. '3.60[1d10] + 7 Good, Magic, Pierce, Slash'
    critical              TEXT                        -- computed display string, e.g. '16-20 / x2'
);

CREATE TABLE IF NOT EXISTS item_dr_bypass (
    item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    bypass  TEXT    NOT NULL,                         -- <DRBypass>: a damage type or a material
    PRIMARY KEY (item_id, bypass)
);

CREATE TABLE IF NOT EXISTS item_armor_stats (
    item_id             INTEGER PRIMARY KEY REFERENCES items(id) ON DELETE CASCADE,
    armor_type          TEXT NOT NULL CHECK (armor_type {armor_type}),  -- <Armor>, or 'Shield'
    armor_bonus         INTEGER,                      -- <ArmorBonus>
    max_dex_bonus       INTEGER,                      -- <MaximumDexterityBonus>
    arcane_spell_failure INTEGER,                     -- <ArcaneSpellFailure>
    armor_check_penalty INTEGER,                      -- <ArmorCheckPenalty>
    shield_bonus        INTEGER,                      -- <ShieldBonus>
    damage_reduction    INTEGER,                      -- <DamageReduction>
    mithral_body        INTEGER,                      -- <MithralBody>, docents only
    adamantine_body     INTEGER                       -- <AdamantineBody>, docents only
);

CREATE TABLE IF NOT EXISTS enchantment_ladders (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE CHECK (TRIM(name) <> '')
);

CREATE TABLE IF NOT EXISTS enchantments (
    id                   INTEGER PRIMARY KEY,
    name                 TEXT NOT NULL UNIQUE CHECK (TRIM(name) <> ''),
    text_template        TEXT NOT NULL,
    description_template TEXT,
    amount_count         INTEGER NOT NULL CHECK (amount_count BETWEEN 0 AND 2),
    default_value        INTEGER,
    default_value2       INTEGER,
    wiki_url             TEXT CHECK (wiki_url IS NULL OR wiki_url GLOB 'https://ddowiki.com/page/?*'),
    stacking_note        TEXT,
    ladder_id            INTEGER REFERENCES enchantment_ladders(id),
    ladder_rank          INTEGER CHECK (ladder_rank IS NULL OR ladder_rank > 0),
    CHECK (default_value IS NULL OR amount_count >= 1),
    CHECK (default_value2 IS NULL OR amount_count >= 2),
    CHECK ((ladder_id IS NULL) = (ladder_rank IS NULL)),
    UNIQUE (ladder_id, ladder_rank)
);

CREATE TABLE IF NOT EXISTS enchantment_stats (
    enchantment_id INTEGER NOT NULL REFERENCES enchantments(id) ON DELETE CASCADE,
    stat_id        INTEGER NOT NULL REFERENCES stats(id),
    bonus_type_id  INTEGER REFERENCES bonus_types(id),
    amount_from    INTEGER NOT NULL CHECK (amount_from BETWEEN 0 AND 2),
    constant       INTEGER,
    scale          REAL NOT NULL DEFAULT 1,
    rounding       TEXT NOT NULL DEFAULT 'down' CHECK (rounding IN ('down', 'up', 'nearest')),
    sort_order     INTEGER NOT NULL,
    CHECK ((amount_from = 0) = (constant IS NOT NULL)),
    CHECK (scale > 0),
    CHECK (amount_from <> 0 OR (scale = 1 AND rounding = 'down')),
    UNIQUE (enchantment_id, stat_id, bonus_type_id)
);
CREATE INDEX IF NOT EXISTS idx_enchantment_stats_stat ON enchantment_stats(stat_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_enchantment_stats_identity
    ON enchantment_stats(enchantment_id, stat_id, COALESCE(bonus_type_id, -1));

CREATE TABLE IF NOT EXISTS item_enchantments (
    item_id        INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    enchantment_id INTEGER NOT NULL REFERENCES enchantments(id),
    bonus_type_id  INTEGER REFERENCES bonus_types(id),
    value          INTEGER,
    value2         INTEGER,
    sort_order     INTEGER NOT NULL,
    CHECK ((value IS NULL) <= (value2 IS NULL)),
    PRIMARY KEY (item_id, sort_order)
);
CREATE INDEX IF NOT EXISTS idx_item_enchantments_enchantment ON item_enchantments(enchantment_id);

CREATE TABLE IF NOT EXISTS item_augment_slots (
    item_id    INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    sort_order INTEGER NOT NULL,                      -- <ItemAugment> position
    slot_id    INTEGER NOT NULL REFERENCES augment_slot_types(id),
    PRIMARY KEY (item_id, sort_order)
);

-- The <Augment> choices upstream fixes for a socket: the upgrade tiers a player unlocks on Quenched, Smoldering,
-- Energized, Thunder-Forged, Attuned to Heroism and Upgradeable items, or the choices a crafting step offers;
-- no rows is an ordinary open socket. Whatever an option gives (a socket, set membership, enchantments, the raw <Effect>
-- rows as modifiers with source_kind 'item_augment_slot_option') stays on the option until the player picks or
-- unlocks it, so none of it is the item's own socket, set or enchantment.
CREATE TABLE IF NOT EXISTS item_augment_slot_options (
    id           INTEGER PRIMARY KEY,
    item_id      INTEGER NOT NULL,
    slot_order   INTEGER NOT NULL,
    option_order INTEGER NOT NULL,                    -- <Augment> position within the <ItemAugment>
    name         TEXT    NOT NULL,                    -- <Augment><Name>
    description  TEXT,                                -- <Augment><Description>
    min_level    INTEGER,                             -- <Augment><MinLevel>
    icon         TEXT,                                -- <Augment><Icon>
    UNIQUE (item_id, slot_order, option_order),
    FOREIGN KEY (item_id, slot_order) REFERENCES item_augment_slots(item_id, sort_order) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS item_augment_slot_option_grants (
    option_id  INTEGER NOT NULL REFERENCES item_augment_slot_options(id) ON DELETE CASCADE,
    sort_order INTEGER NOT NULL,                      -- <GrantAugment> or <AddAugment> position within the option
    slot_id    INTEGER NOT NULL REFERENCES augment_slot_types(id),  -- the socket the option adds
    PRIMARY KEY (option_id, sort_order)
);

CREATE TABLE IF NOT EXISTS item_augment_slot_option_sets (
    option_id INTEGER NOT NULL REFERENCES item_augment_slot_options(id) ON DELETE CASCADE,
    set_id    INTEGER NOT NULL REFERENCES set_bonuses(id),  -- <SetBonus>: a set the option makes the item count toward
    PRIMARY KEY (option_id, set_id)
);
CREATE INDEX IF NOT EXISTS idx_item_augment_slot_option_sets_set ON item_augment_slot_option_sets(set_id);

CREATE TABLE IF NOT EXISTS item_augment_slot_option_enchantments (
    option_id      INTEGER NOT NULL REFERENCES item_augment_slot_options(id) ON DELETE CASCADE,
    enchantment_id INTEGER NOT NULL REFERENCES enchantments(id),
    bonus_type_id  INTEGER REFERENCES bonus_types(id),
    value          INTEGER,
    value2         INTEGER,
    sort_order     INTEGER NOT NULL,
    CHECK ((value IS NULL) <= (value2 IS NULL)),
    PRIMARY KEY (option_id, sort_order)
);
CREATE INDEX IF NOT EXISTS idx_item_augment_slot_option_enchantments_enchantment
    ON item_augment_slot_option_enchantments(enchantment_id);

-- Quest chains and sagas (data/wiki quest_chains and sagas) ---------------------------
--
-- Both give an end reward from an NPC after several quests, not from any one quest: a quest chain is ddowiki's
-- "story arc", a saga the saga system's reward NPC. Maetrim's files have neither, so every row is a wiki row;
-- the name, quest link and reward columns are shared and only a saga reward carries a tier.
CREATE TABLE IF NOT EXISTS quest_chains (
    id       INTEGER PRIMARY KEY,
    {quest_series_columns}
);

CREATE TABLE IF NOT EXISTS quest_chain_quests (
    chain_id   INTEGER NOT NULL REFERENCES quest_chains(id) ON DELETE CASCADE,
    {quest_series_quest_columns}
    PRIMARY KEY (chain_id, quest_id)
);
CREATE INDEX IF NOT EXISTS idx_quest_chain_quests_quest ON quest_chain_quests(quest_id);

CREATE TABLE IF NOT EXISTS sagas (
    id       INTEGER PRIMARY KEY,
    {quest_series_columns}
);

CREATE TABLE IF NOT EXISTS saga_quests (
    saga_id    INTEGER NOT NULL REFERENCES sagas(id) ON DELETE CASCADE,
    {quest_series_quest_columns}
    PRIMARY KEY (saga_id, quest_id)
);
CREATE INDEX IF NOT EXISTS idx_saga_quests_quest ON saga_quests(quest_id);

-- An NPC or place that sells or trades items, from data/wiki vendors files; his files have none.
CREATE TABLE IF NOT EXISTS vendors (
    id         INTEGER PRIMARY KEY,
    name       TEXT    NOT NULL UNIQUE,
    location   TEXT,                                   -- where the vendor stands, free text as the wiki writes it
    pack_id    INTEGER REFERENCES adventure_packs(id),
    provenance TEXT    NOT NULL CHECK (provenance {provenance}),
    wiki_url   TEXT    NOT NULL
);

-- A festival or limited-time event whose rewards are items, from data/wiki events files; his files have none.
CREATE TABLE IF NOT EXISTS events (
    id         INTEGER PRIMARY KEY,
    name       TEXT    NOT NULL UNIQUE,
    provenance TEXT    NOT NULL CHECK (provenance {provenance}),
    wiki_url   TEXT    NOT NULL
);

-- Every place an item or augment drops or is given, one row per source, loot and loot type (and saga tier):
-- a quest's chest, raid or end-reward loot, read from Maetrim's <DropLocation> and "Drops in" description text and
-- data/wiki quest_loot; a quest chain's or saga's end reward, from data/wiki quest_chains and sagas and the drop
-- text crediting one; loot any quest of an adventure pack drops, from drop text naming the pack and no quest; a
-- challenge pack's turn-in reward (pack_id), a wiki crafting system's output, a vendor's or an event's item, from
-- drop text naming them (or a data/source_aliases.toml text) and data/wiki vendors and events; and an iconic
-- character's starter item, from "Advance to level N", with that character_level.
-- loot_type is set exactly on quest and pack drops; chest is the lower-cased phrase after the quest or pack
-- name, never on a reward; tier is the saga reward list, null when the source names none and on every other kind;
-- cost is what a vendor asks, as the wiki writes it.
-- The unique index keeps one row per source, loot, loot type and tier, which a primary key over nullable columns
-- would not.
CREATE TABLE IF NOT EXISTS sources (
    id          INTEGER PRIMARY KEY,
    kind        TEXT    NOT NULL CHECK (kind {source_kind}),
    quest_id    INTEGER REFERENCES quests(id) ON DELETE CASCADE,
    chain_id    INTEGER REFERENCES quest_chains(id) ON DELETE CASCADE,
    saga_id     INTEGER REFERENCES sagas(id) ON DELETE CASCADE,
    pack_id     INTEGER REFERENCES adventure_packs(id) ON DELETE CASCADE,
    crafting_system_id INTEGER REFERENCES crafting_systems(id) ON DELETE CASCADE,
    vendor_id   INTEGER REFERENCES vendors(id) ON DELETE CASCADE,
    event_id    INTEGER REFERENCES events(id) ON DELETE CASCADE,
    character_level INTEGER CHECK (character_level IS NULL OR character_level >= 1),
    item_id     INTEGER REFERENCES items(id) ON DELETE CASCADE,
    augment_id  INTEGER REFERENCES augments(id) ON DELETE CASCADE,
    loot_type   TEXT    CHECK (loot_type {loot_type}),
    chest       TEXT,
    is_rare     INTEGER NOT NULL DEFAULT 0 CHECK (is_rare IN (0, 1)),
    tier        TEXT    CHECK (tier {saga_tier}),
    cost        TEXT,
    CHECK ({source_id_count} = 1),
    CHECK (CASE kind {source_id_for_kind} ELSE 0 END),
    CHECK ((item_id IS NOT NULL) + (augment_id IS NOT NULL) = 1),
    CHECK ((loot_type IS NOT NULL) = (kind {kinds_with_loot_type})),
    CHECK (chest IS NULL OR COALESCE(loot_type, 'reward') <> 'reward'),
    CHECK (chest IS NULL OR chest NOT LIKE '%reward%'),
    CHECK (tier IS NULL OR kind = 'saga'),
    CHECK (cost IS NULL OR kind = 'vendor')
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_sources_source_loot ON sources(
    kind, COALESCE({source_ids}), COALESCE(item_id, 0), COALESCE(augment_id, 0),
    COALESCE(loot_type, ''), COALESCE(tier, '')
);
CREATE INDEX IF NOT EXISTS idx_sources_item ON sources(item_id) WHERE item_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_sources_augment ON sources(augment_id) WHERE augment_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_sources_quest ON sources(quest_id) WHERE quest_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_sources_pack ON sources(pack_id) WHERE pack_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_sources_crafting_system ON sources(crafting_system_id) WHERE crafting_system_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_sources_vendor ON sources(vendor_id) WHERE vendor_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_sources_event ON sources(event_id) WHERE event_id IS NOT NULL;

-- Modifiers and requirements: the two grammars every family shares ---------------
--
-- A modifier is one upstream <Effect>, stored faithfully: the engine (Phases 6–8) reads these.
-- Enchantment stat rows come from mapped effects; modifiers keep the upstream mechanics.
CREATE TABLE IF NOT EXISTS modifiers (
    id                   INTEGER PRIMARY KEY,
    source_kind          TEXT    NOT NULL CHECK (source_kind {modifier_source}),
    source_id            INTEGER NOT NULL,
    sort_order           INTEGER NOT NULL,             -- <Effect> position within the source
    effect_type          TEXT    NOT NULL,             -- first <Type>
    extra_types          TEXT,                         -- JSON array: further <Type> elements
    bonus                TEXT,                         -- <Bonus> as written
    bonus_type_id        INTEGER REFERENCES bonus_types(id),  -- <Bonus> mapped onto bonus_types
    amount_type          TEXT,                         -- <AType>: Simple, Stacks, TotalLevel, …
    amounts              TEXT,                         -- JSON array of numbers: <Amount>
    targets              TEXT,                         -- JSON array: <Item> elements
    value                TEXT,                         -- <Value>, e.g. a DR material
    dice_number          TEXT,                         -- <Dice><Number>, a vector like amounts
    dice_sides           TEXT,                         -- <Dice><Sides>
    dice_bonus           TEXT,                         -- <Dice><Bonus>
    dice_damage          TEXT,                         -- <Dice><Damage>
    damage               TEXT,                         -- <Damage>
    percent              INTEGER NOT NULL DEFAULT 0 CHECK (percent IN (0, 1)),
    rank                 INTEGER,                      -- <Rank>
    cap                  TEXT,                         -- <Cap>
    stack_source         TEXT,                         -- <StackSource>
    display_name         TEXT,                         -- <DisplayName>
    apply_as_item_effect INTEGER NOT NULL DEFAULT 0 CHECK (apply_as_item_effect IN (0, 1)),
    is_item_specific     INTEGER NOT NULL DEFAULT 0 CHECK (is_item_specific IN (0, 1)),
    is_rare              INTEGER NOT NULL DEFAULT 0 CHECK (is_rare IN (0, 1))  -- filigree <Rare/>
);
CREATE INDEX IF NOT EXISTS idx_modifiers_source ON modifiers(source_kind, source_id);
CREATE INDEX IF NOT EXISTS idx_modifiers_type ON modifiers(effect_type);

CREATE TABLE IF NOT EXISTS requirements (
    id          INTEGER PRIMARY KEY,
    owner_kind  TEXT    NOT NULL CHECK (owner_kind {requirement_owner}),
    owner_id    INTEGER NOT NULL,
    group_kind  TEXT    NOT NULL CHECK (group_kind {requirement_group}),  -- Requirement / RequiresOneOf / RequiresNoneOf
    group_index INTEGER NOT NULL,                      -- which <RequiresOneOf>/<RequiresNoneOf> block
    sort_order  INTEGER NOT NULL,
    req_type    TEXT    NOT NULL,                      -- <Type>: Level, Feat, Race, Stance, …
    items       TEXT,                                  -- JSON array: <Item> elements
    value       TEXT                                   -- <Value>
);
CREATE INDEX IF NOT EXISTS idx_requirements_owner ON requirements(owner_kind, owner_id);

-- Feats, races, classes ------------------------------------------------------------

-- A feat from Feats.xml, or one defined inside a class or race file (source_kind/source_id).
-- The same name can exist in several sources; resolution prefers the caller's own source, then
-- the standard list.
CREATE TABLE IF NOT EXISTS feats (
    id                INTEGER PRIMARY KEY,
    name              TEXT    NOT NULL,               -- <Name>
    source_kind       TEXT    NOT NULL CHECK (source_kind {feat_source}),
    source_id         INTEGER,                        -- classes.id or races.id
    description       TEXT,                           -- <Description>
    icon              TEXT,                           -- <Icon>
    acquire           TEXT,                           -- <Acquire>: Train, Automatic, Favor, EpicPastLife, …
    max_times_acquire INTEGER,                        -- <MaxTimesAcquire>
    sphere            TEXT,                           -- <Sphere>
    auto_acquire_ignores_requirements INTEGER NOT NULL DEFAULT 0 CHECK (auto_acquire_ignores_requirements IN (0, 1))
);
-- A class file can define the same feat name more than once (a later definition supersedes an
-- earlier one at a higher level), so the identity is the row; name lookups take the first.
CREATE INDEX IF NOT EXISTS idx_feats_name ON feats(name, source_kind, source_id);

CREATE TABLE IF NOT EXISTS feat_groups (
    feat_id    INTEGER NOT NULL REFERENCES feats(id) ON DELETE CASCADE,
    group_name TEXT    NOT NULL,                      -- <Group>: Standard, Epic Feat, Metamagics, …
    PRIMARY KEY (feat_id, group_name)
);

-- Groups a feat belongs to only when its requirements hold (<ConditionalGroup>).
CREATE TABLE IF NOT EXISTS feat_conditional_groups (
    id      INTEGER PRIMARY KEY,
    feat_id INTEGER NOT NULL REFERENCES feats(id) ON DELETE CASCADE,
    groups  TEXT    NOT NULL                          -- JSON array of group names
);

CREATE TABLE IF NOT EXISTS feat_sub_items (
    feat_id     INTEGER NOT NULL REFERENCES feats(id) ON DELETE CASCADE,
    sort_order  INTEGER NOT NULL,
    name        TEXT    NOT NULL,
    icon        TEXT,
    description TEXT,
    PRIMARY KEY (feat_id, sort_order)
);

CREATE TABLE IF NOT EXISTS feat_enchantments (
    feat_id        INTEGER NOT NULL REFERENCES feats(id) ON DELETE CASCADE,
    enchantment_id INTEGER NOT NULL REFERENCES enchantments(id),
    bonus_type_id  INTEGER REFERENCES bonus_types(id),
    value          INTEGER,
    value2         INTEGER,
    sort_order     INTEGER NOT NULL,
    CHECK ((value IS NULL) <= (value2 IS NULL)),
    PRIMARY KEY (feat_id, sort_order)
);

-- A toggled or automatic combat stance an ability provides, or one of Stances.xml's standalone stances.
CREATE TABLE IF NOT EXISTS stances (
    id              INTEGER PRIMARY KEY,
    owner_kind      TEXT    NOT NULL CHECK (owner_kind {ability_owner}),
    owner_id        INTEGER NOT NULL,                 -- 0 for 'standalone' (Stances.xml)
    sort_order      INTEGER NOT NULL,
    name            TEXT    NOT NULL,
    description     TEXT,
    icon            TEXT,
    group_name      TEXT,                             -- <Group>
    auto_controlled INTEGER NOT NULL DEFAULT 0 CHECK (auto_controlled IN (0, 1)),
    incompatible    TEXT                              -- JSON array: <IncompatibleStance>
);
CREATE INDEX IF NOT EXISTS idx_stances_owner ON stances(owner_kind, owner_id);

-- A saving-throw DC an ability imposes.
CREATE TABLE IF NOT EXISTS dcs (
    id               INTEGER PRIMARY KEY,
    owner_kind       TEXT    NOT NULL CHECK (owner_kind {ability_owner}),
    owner_id         INTEGER NOT NULL,
    sort_order       INTEGER NOT NULL,
    name             TEXT,
    description      TEXT,
    icon             TEXT,
    dc_type          TEXT,                            -- <DCType>
    dc_versus        TEXT,                            -- <DCVersus>
    mod_ability      TEXT,                            -- JSON array: <ModAbility>
    amount           TEXT,                            -- JSON array: <Amount>
    tactical         TEXT,
    other            TEXT,
    skill            TEXT,
    class_level      TEXT,                            -- <ClassLevel>
    base_class_level TEXT                             -- <BaseClassLevel>
);
CREATE INDEX IF NOT EXISTS idx_dcs_owner ON dcs(owner_kind, owner_id);

-- A special attack an ability grants (<Attack>); only its identity is kept.
CREATE TABLE IF NOT EXISTS attacks (
    id          INTEGER PRIMARY KEY,
    owner_kind  TEXT    NOT NULL CHECK (owner_kind {ability_owner}),
    owner_id    INTEGER NOT NULL,
    name        TEXT,
    description TEXT,
    icon        TEXT,
    cooldown_seconds INTEGER,                         -- <Cooldown>, first value
    duration_seconds INTEGER                          -- <FollowOn><Duration>, first value
);

CREATE TABLE IF NOT EXISTS races (
    id             INTEGER PRIMARY KEY,
    name           TEXT    NOT NULL UNIQUE,           -- <Name>
    short_name     TEXT,                              -- <ShortName>
    description    TEXT,
    starting_world TEXT,                              -- Eberron / Forgotten Realms
    build_points   TEXT,                              -- JSON array: <BuildPoints>
    iconic_class   TEXT,                              -- <IconicClass>
    is_construct   INTEGER NOT NULL DEFAULT 0 CHECK (is_construct IN (0, 1)),
    no_past_life   INTEGER NOT NULL DEFAULT 0 CHECK (no_past_life IN (0, 1)),
    skill_points   INTEGER                            -- <SkillPoints>
);

CREATE TABLE IF NOT EXISTS race_ability_modifiers (
    race_id  INTEGER NOT NULL REFERENCES races(id) ON DELETE CASCADE,
    stat_id  INTEGER NOT NULL REFERENCES stats(id),   -- <Strength>+2 etc.
    modifier INTEGER NOT NULL,
    PRIMARY KEY (race_id, stat_id)
);

CREATE TABLE IF NOT EXISTS race_granted_feats (
    race_id    INTEGER NOT NULL REFERENCES races(id) ON DELETE CASCADE,
    sort_order INTEGER NOT NULL,
    feat_name  TEXT    NOT NULL,                      -- <GrantedFeat>
    feat_id    INTEGER REFERENCES feats(id),          -- resolved: the race's own feat, else standard
    PRIMARY KEY (race_id, sort_order)
);

CREATE TABLE IF NOT EXISTS race_feat_slots (
    race_id     INTEGER NOT NULL REFERENCES races(id) ON DELETE CASCADE,
    level       INTEGER NOT NULL,
    feat_type   TEXT    NOT NULL,
    update_list TEXT,                                 -- JSON array: <FeatUpdateList>
    PRIMARY KEY (race_id, level, feat_type)
);

CREATE TABLE IF NOT EXISTS race_auto_buy_skills (
    race_id INTEGER NOT NULL REFERENCES races(id) ON DELETE CASCADE,
    skill   TEXT    NOT NULL,
    PRIMARY KEY (race_id, skill)
);

CREATE TABLE IF NOT EXISTS classes (
    id                        INTEGER PRIMARY KEY,
    name                      TEXT    NOT NULL UNIQUE,
    base_class                TEXT,                   -- <BaseClass>: archetypes name their parent
    base_class_id             INTEGER REFERENCES classes(id),
    not_heroic                INTEGER NOT NULL DEFAULT 0 CHECK (not_heroic IN (0, 1)),  -- Epic, Legendary
    description               TEXT,
    small_icon                TEXT,
    large_icon                TEXT,
    skill_points              INTEGER,
    hit_points                INTEGER,
    alignments                TEXT,                   -- JSON array: <Alignment>
    fortitude                 TEXT CHECK (fortitude {save_progression}),
    reflex                    TEXT CHECK (reflex {save_progression}),
    will                      TEXT CHECK (will {save_progression}),
    bab                       TEXT,                   -- JSON array indexed by class level
    spell_points_per_level    TEXT,                   -- JSON array indexed by class level
    casting_stats             TEXT,                   -- JSON array: <CastingStat>
    class_specific_feat_types TEXT                    -- JSON array: <ClassSpecificFeatType>
);

CREATE TABLE IF NOT EXISTS class_skills (
    class_id INTEGER NOT NULL REFERENCES classes(id) ON DELETE CASCADE,
    skill    TEXT    NOT NULL,
    PRIMARY KEY (class_id, skill)
);

CREATE TABLE IF NOT EXISTS class_auto_buy_skills (
    class_id INTEGER NOT NULL REFERENCES classes(id) ON DELETE CASCADE,
    skill    TEXT    NOT NULL,
    PRIMARY KEY (class_id, skill)
);

-- <LevelN> vectors: spell slots per spell level at each class level.
CREATE TABLE IF NOT EXISTS class_spell_slots (
    class_id    INTEGER NOT NULL REFERENCES classes(id) ON DELETE CASCADE,
    class_level INTEGER NOT NULL,
    spell_level INTEGER NOT NULL,
    slots       INTEGER NOT NULL,
    PRIMARY KEY (class_id, class_level, spell_level)
);

CREATE TABLE IF NOT EXISTS class_spells (
    class_id         INTEGER NOT NULL REFERENCES classes(id) ON DELETE CASCADE,
    spell_name       TEXT    NOT NULL,                -- <ClassSpell><Name>; spell_id resolves in the spells stage
    spell_level      INTEGER NOT NULL,
    cost             INTEGER,
    max_caster_level INTEGER,
    spell_id         INTEGER,
    PRIMARY KEY (class_id, spell_name, spell_level)
);

CREATE TABLE IF NOT EXISTS class_feat_slots (
    id            INTEGER PRIMARY KEY,
    class_id      INTEGER NOT NULL REFERENCES classes(id) ON DELETE CASCADE,
    level         INTEGER NOT NULL,
    feat_type     TEXT    NOT NULL,
    auto_populate INTEGER NOT NULL DEFAULT 0 CHECK (auto_populate IN (0, 1)),
    singular      INTEGER NOT NULL DEFAULT 0 CHECK (singular IN (0, 1)),
    update_list   TEXT                                -- JSON array: <FeatUpdateList>
);

CREATE TABLE IF NOT EXISTS class_auto_feats (
    class_id  INTEGER NOT NULL REFERENCES classes(id) ON DELETE CASCADE,
    level     INTEGER NOT NULL,
    feat_name TEXT    NOT NULL,                       -- <AutomaticFeats><Feats>
    feat_id   INTEGER REFERENCES feats(id),           -- resolved: the class's own feat, else standard
    PRIMARY KEY (class_id, level, feat_name)
);

-- Enhancement trees -----------------------------------------------------------------

CREATE TABLE IF NOT EXISTS enhancement_trees (
    id         INTEGER PRIMARY KEY,
    name       TEXT    NOT NULL UNIQUE,               -- <Name>
    version    INTEGER,                               -- <Version>
    kind       TEXT    NOT NULL CHECK (kind {tree_kind}),  -- from <IsRacialTree/> etc.; default class
    is_legacy  INTEGER NOT NULL DEFAULT 0 CHECK (is_legacy IN (0, 1)),  -- <Legacy/>: a retired version
    icon       TEXT,
    background TEXT
);

CREATE TABLE IF NOT EXISTS enhancements (
    id            INTEGER PRIMARY KEY,
    tree_id       INTEGER NOT NULL REFERENCES enhancement_trees(id) ON DELETE CASCADE,
    internal_name TEXT    NOT NULL,                   -- <InternalName>; requirements reference these
    name          TEXT    NOT NULL,
    description   TEXT,
    icon          TEXT,
    x             INTEGER,                            -- <XPosition>: column
    y             INTEGER,                            -- <YPosition>: tier row (0 = core)
    cost_per_rank TEXT,                               -- JSON array: <CostPerRank>
    ranks         INTEGER,
    min_spent     INTEGER,                            -- <MinSpent>: AP in the tree before this unlocks
    is_tier5      INTEGER NOT NULL DEFAULT 0 CHECK (is_tier5 IN (0, 1)),
    is_clickie    INTEGER NOT NULL DEFAULT 0 CHECK (is_clickie IN (0, 1)),
    arrows        TEXT,                               -- JSON array: ArrowUp, ArrowRight, …
    cooldown_seconds INTEGER,                         -- <Attack><Cooldown>, first value
    duration_seconds INTEGER,                         -- <Attack><FollowOn><Duration>, first value
    UNIQUE (tree_id, internal_name)
);

-- One of the choices a selector enhancement offers.
CREATE TABLE IF NOT EXISTS enhancement_selections (
    id             INTEGER PRIMARY KEY,
    enhancement_id INTEGER NOT NULL REFERENCES enhancements(id) ON DELETE CASCADE,
    sort_order     INTEGER NOT NULL,
    name           TEXT    NOT NULL,
    description    TEXT,
    icon           TEXT,
    cost_per_rank  TEXT,
    ranks          INTEGER,
    min_spent      INTEGER,
    is_clickie     INTEGER NOT NULL DEFAULT 0 CHECK (is_clickie IN (0, 1)),
    cooldown_seconds INTEGER,                         -- <Attack><Cooldown>, first value
    duration_seconds INTEGER,                         -- <Attack><FollowOn><Duration>, first value
    UNIQUE (enhancement_id, sort_order)
);

-- Enhancements (by internal name) a selector excludes.
CREATE TABLE IF NOT EXISTS enhancement_selector_exclusions (
    enhancement_id INTEGER NOT NULL REFERENCES enhancements(id) ON DELETE CASCADE,
    internal_name  TEXT    NOT NULL,
    PRIMARY KEY (enhancement_id, internal_name)
);

-- Spells ------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS spells (
    id               INTEGER PRIMARY KEY,
    name             TEXT    NOT NULL UNIQUE,
    description      TEXT,
    icon             TEXT,
    schools          TEXT,                            -- JSON array: <School>
    max_caster_level INTEGER,
    cost             INTEGER,                         -- <Cost>, when fixed rather than per class
    metamagics       TEXT                             -- JSON array of the metamagic flags present
);

CREATE TABLE IF NOT EXISTS spell_damage (
    id                INTEGER PRIMARY KEY,
    spell_id          INTEGER NOT NULL REFERENCES spells(id) ON DELETE CASCADE,
    sort_order        INTEGER NOT NULL,
    base_dice_number  INTEGER,
    base_dice_sides   INTEGER,
    base_dice_bonus   INTEGER,
    per_caster_levels INTEGER,                        -- bonus dice every N caster levels
    bonus_dice_number INTEGER,
    bonus_dice_sides  INTEGER,
    bonus_dice_bonus  INTEGER,
    damage            TEXT,                           -- <Damage> type
    spell_power       TEXT                            -- <SpellPower> that scales it
);

CREATE TABLE IF NOT EXISTS spell_dcs (
    id               INTEGER PRIMARY KEY,
    spell_id         INTEGER NOT NULL REFERENCES spells(id) ON DELETE CASCADE,
    sort_order       INTEGER NOT NULL,
    dc_type          TEXT,
    dc_versus        TEXT,
    schools          TEXT,                            -- JSON array
    casting_stat_mod INTEGER NOT NULL DEFAULT 0 CHECK (casting_stat_mod IN (0, 1)),
    amount           TEXT,                            -- JSON array
    mod_abilities    TEXT                             -- JSON array
);

-- Augments ------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS augments (
    id                 INTEGER PRIMARY KEY,
    name               TEXT    NOT NULL,               -- <Name>
    family             TEXT    NOT NULL,               -- source file stem: 'Ruby', 'Cannith and random item', …
    description        TEXT,                           -- <Description>
    effect_description TEXT,                           -- <EffectDescription>, joined
    min_level          INTEGER,                        -- <MinLevel>
    icon               TEXT,                           -- <Icon>
    choose_level       INTEGER NOT NULL DEFAULT 0 CHECK (choose_level IN (0, 1)),  -- <ChooseLevel/>
    levels             TEXT,                           -- JSON array: <Levels>
    level_values       TEXT,                           -- JSON array: <LevelValue>
    level_values2      TEXT,                           -- JSON array: <LevelValue2>
    dual_values        INTEGER NOT NULL DEFAULT 0 CHECK (dual_values IN (0, 1)),
    enter_value        INTEGER NOT NULL DEFAULT 0 CHECK (enter_value IN (0, 1)),
    suppress_set_bonus INTEGER NOT NULL DEFAULT 0 CHECK (suppress_set_bonus IN (0, 1)),
    set_bonus          TEXT,                           -- <SetBonus>
    adds_augment       TEXT,                           -- <AddAugment>: slot type this augment opens next
    grants_augment     TEXT,                           -- <GrantAugment>: colour slot this augment adds
    weapon_class       TEXT,                           -- <WeaponClass>
    provenance         TEXT    NOT NULL DEFAULT 'maetrim' CHECK (provenance {provenance})  -- 'wiki' for a data/wiki augments entry
);
-- Names repeat within a family (the same "Use Magic Device" exists for several slot sets), so
-- the identity is the row, not the name.
CREATE INDEX IF NOT EXISTS idx_augments_name ON augments(name);

CREATE TABLE IF NOT EXISTS augment_slots (
    augment_id INTEGER NOT NULL REFERENCES augments(id) ON DELETE CASCADE,
    slot_id    INTEGER NOT NULL REFERENCES augment_slot_types(id),  -- each <Type>
    PRIMARY KEY (augment_id, slot_id)
);
CREATE INDEX IF NOT EXISTS idx_augment_slots_slot ON augment_slots(slot_id);

CREATE TABLE IF NOT EXISTS augment_enchantments (
    augment_id     INTEGER NOT NULL REFERENCES augments(id) ON DELETE CASCADE,
    enchantment_id INTEGER NOT NULL REFERENCES enchantments(id),
    bonus_type_id  INTEGER REFERENCES bonus_types(id),
    value          INTEGER,
    value2         INTEGER,
    sort_order     INTEGER NOT NULL,
    CHECK ((value IS NULL) <= (value2 IS NULL)),
    PRIMARY KEY (augment_id, sort_order)
);

-- Crafting (data/wiki crafting) --------------------------------------------------

-- A crafting system as one wiki page describes it. Its options are Maetrim's augments in `families`.
CREATE TABLE IF NOT EXISTS crafting_systems (
    id      INTEGER PRIMARY KEY,
    name    TEXT    NOT NULL UNIQUE,
    page    TEXT    NOT NULL,                          -- the ddowiki page read
    pack_id INTEGER REFERENCES adventure_packs(id),
    npc     TEXT                                       -- the crafting NPC or station, free text
);

CREATE TABLE IF NOT EXISTS crafting_system_families (
    system_id INTEGER NOT NULL REFERENCES crafting_systems(id) ON DELETE CASCADE,
    family    TEXT    NOT NULL,                        -- augments.family
    PRIMARY KEY (system_id, family)
);

CREATE TABLE IF NOT EXISTS crafting_ingredients (
    id        INTEGER PRIMARY KEY,
    system_id INTEGER NOT NULL REFERENCES crafting_systems(id) ON DELETE CASCADE,
    name      TEXT    NOT NULL,
    tier      TEXT    NOT NULL CHECK (tier {crafting_tier}),
    bind      TEXT,                                    -- as the wiki writes it
    source    TEXT,                                    -- where it drops, free text
    UNIQUE (system_id, name, tier)
);

-- One row of a wiki recipe table: what `option` costs and the augments or socket it yields.
CREATE TABLE IF NOT EXISTS crafting_recipes (
    id             INTEGER PRIMARY KEY,
    system_id      INTEGER NOT NULL REFERENCES crafting_systems(id) ON DELETE CASCADE,
    tier           TEXT    NOT NULL CHECK (tier {crafting_tier}),
    slot_id        INTEGER REFERENCES augment_slot_types(id),   -- the socket the row's augments fill
    grants_slot_id INTEGER REFERENCES augment_slot_types(id),   -- the socket the row adds to the item
    option         TEXT    NOT NULL,                            -- the wiki's label for the row
    note           TEXT,
    sort_order     INTEGER NOT NULL                             -- position in the system's file entry
);
CREATE INDEX IF NOT EXISTS idx_crafting_recipes_system ON crafting_recipes(system_id, sort_order);

CREATE TABLE IF NOT EXISTS crafting_recipe_augments (
    recipe_id  INTEGER NOT NULL REFERENCES crafting_recipes(id) ON DELETE CASCADE,
    augment_id INTEGER NOT NULL REFERENCES augments(id) ON DELETE CASCADE,
    PRIMARY KEY (recipe_id, augment_id)
);
CREATE INDEX IF NOT EXISTS idx_crafting_recipe_augments_augment ON crafting_recipe_augments(augment_id);

CREATE TABLE IF NOT EXISTS crafting_recipe_ingredients (
    recipe_id     INTEGER NOT NULL REFERENCES crafting_recipes(id) ON DELETE CASCADE,
    ingredient_id INTEGER NOT NULL REFERENCES crafting_ingredients(id) ON DELETE CASCADE,
    quantity      INTEGER NOT NULL CHECK (quantity > 0),
    PRIMARY KEY (recipe_id, ingredient_id)
);

-- Corrections (data/corrections) ------------------------------------------------

-- A known mistake in Maetrim's data and the value that replaced it. Values are JSON (318, "text", null).
CREATE TABLE IF NOT EXISTS corrections (
    id         INTEGER PRIMARY KEY,
    kind       TEXT NOT NULL CHECK (kind {correction_kind}),
    name       TEXT NOT NULL,                          -- the row's name in Maetrim's files
    qualifier  TEXT NOT NULL DEFAULT '',               -- the part of the row it narrows to: a family, a bonus, a socket
    field      TEXT NOT NULL,
    from_value TEXT NOT NULL,                          -- his value, as JSON
    to_value   TEXT NOT NULL,                          -- the value written, as JSON
    reason     TEXT NOT NULL,
    source     TEXT NOT NULL,                          -- URL citing the right value
    read       TEXT NOT NULL,                          -- YYYY-MM-DD the source was read
    UNIQUE (kind, name, qualifier, field)
);
CREATE INDEX IF NOT EXISTS idx_corrections_kind_name ON corrections(kind, name);

-- Sets and filigrees ------------------------------------------------------------

CREATE TABLE IF NOT EXISTS set_bonuses (
    id              INTEGER PRIMARY KEY,
    name            TEXT    NOT NULL UNIQUE,           -- <SetBonus><Type>
    icon            TEXT,                              -- <Icon>
    is_filigree_set INTEGER NOT NULL DEFAULT 0 CHECK (is_filigree_set IN (0, 1))
);

CREATE TABLE IF NOT EXISTS set_bonus_tiers (
    id             INTEGER PRIMARY KEY,
    set_id         INTEGER NOT NULL REFERENCES set_bonuses(id) ON DELETE CASCADE,
    equipped_count INTEGER NOT NULL,                   -- <Buff><EquippedCount>
    UNIQUE (set_id, equipped_count)
);

CREATE TABLE IF NOT EXISTS set_bonus_tier_enchantments (
    tier_id        INTEGER NOT NULL REFERENCES set_bonus_tiers(id) ON DELETE CASCADE,
    enchantment_id INTEGER NOT NULL REFERENCES enchantments(id),
    bonus_type_id  INTEGER REFERENCES bonus_types(id),
    value          INTEGER,
    value2         INTEGER,
    sort_order     INTEGER NOT NULL,
    CHECK ((value IS NULL) <= (value2 IS NULL)),
    PRIMARY KEY (tier_id, sort_order)
);
CREATE INDEX IF NOT EXISTS idx_set_bonus_tier_enchantments_enchantment
    ON set_bonus_tier_enchantments(enchantment_id);

CREATE TABLE IF NOT EXISTS set_bonus_items (
    set_id  INTEGER NOT NULL REFERENCES set_bonuses(id) ON DELETE CASCADE,
    item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    PRIMARY KEY (set_id, item_id)
);

-- Augments whose <SetBonus> names the set: slotting one counts towards it (crafting systems, named augments).
CREATE TABLE IF NOT EXISTS set_bonus_augments (
    set_id     INTEGER NOT NULL REFERENCES set_bonuses(id) ON DELETE CASCADE,
    augment_id INTEGER NOT NULL REFERENCES augments(id) ON DELETE CASCADE,
    PRIMARY KEY (set_id, augment_id)
);
CREATE INDEX IF NOT EXISTS idx_set_bonus_augments_augment ON set_bonus_augments(augment_id);

CREATE TABLE IF NOT EXISTS filigrees (
    id          INTEGER PRIMARY KEY,
    name        TEXT    NOT NULL UNIQUE,               -- <Filigree><Name>
    description TEXT,                                  -- <Description>
    icon        TEXT,                                  -- <Icon>
    menu        TEXT,                                  -- <Menu>
    set_id      INTEGER REFERENCES set_bonuses(id)     -- <SetBonus>
);

-- Buffs --------------------------------------------------------------------------

-- A guild airship amenity; its effects are modifiers with source_kind 'guild_buff', most with
-- amount_type 'TotalLevel' and one amount per character level.
CREATE TABLE IF NOT EXISTS guild_buffs (
    id          INTEGER PRIMARY KEY,
    name        TEXT    NOT NULL UNIQUE,               -- GuildBuffs.xml <GuildBuff><Name>
    description TEXT,
    guild_level INTEGER                                -- <Level>: the guild level that unlocks it
);

-- A spell, potion, song or party buff a planner can toggle on (SelfAndPartyBuffs.xml); its effects
-- are modifiers with source_kind 'optional_buff'.
CREATE TABLE IF NOT EXISTS optional_buffs (
    id          INTEGER PRIMARY KEY,
    name        TEXT    NOT NULL UNIQUE,               -- <OptionalBuff><Name>
    icon        TEXT,
    description TEXT
);

-- A sentient jewel: the personality a sentient weapon's filigrees slot into.
CREATE TABLE IF NOT EXISTS sentient_gems (
    id          INTEGER PRIMARY KEY,
    name        TEXT    NOT NULL UNIQUE,               -- Sentient.gems.xml <Gem><Name>
    icon        TEXT,                                  -- SentientGemImages/<icon>.png
    description TEXT                                   -- <Description>: the voice actor credit
);

-- Clickies ------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS clickies (
    id          INTEGER PRIMARY KEY,
    name        TEXT    NOT NULL UNIQUE,               -- ItemClickies.xml <Spell><Name>
    description TEXT,
    icon        TEXT,
    school      TEXT
);

-- An item's clickable ability by name. Most names are ItemClickies.xml entries (`clickie_id`);
-- the rest are real spells and resolve to `spell_id` once the spells stage lands.
CREATE TABLE IF NOT EXISTS item_clickies (
    item_id    INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    sort_order INTEGER NOT NULL,
    name       TEXT    NOT NULL,                      -- <Effect><Item> of the ItemClickie effect
    clickie_id INTEGER REFERENCES clickies(id),
    spell_id   INTEGER REFERENCES spells(id),         -- when the name is a real spell instead
    PRIMARY KEY (item_id, sort_order)
);

CREATE VIEW IF NOT EXISTS loot_adventure_packs (item_id, augment_id, pack_id, source_kind, source_id) AS
SELECT DISTINCT loot.item_id, loot.augment_id, pack.id, loot.kind, loot.id
FROM sources loot
LEFT JOIN quest_chain_quests chain_quest ON chain_quest.chain_id = loot.chain_id
LEFT JOIN saga_quests saga_quest ON saga_quest.saga_id = loot.saga_id
LEFT JOIN quests quest ON quest.id = COALESCE(loot.quest_id, chain_quest.quest_id, saga_quest.quest_id)
LEFT JOIN crafting_systems crafting_system ON crafting_system.id = loot.crafting_system_id
LEFT JOIN vendors vendor ON vendor.id = loot.vendor_id
JOIN adventure_packs pack ON pack.id = COALESCE(loot.pack_id, quest.pack_id, crafting_system.pack_id, vendor.pack_id);
"#
    )
});

pub fn ddl() -> &'static str {
    &DDL
}
