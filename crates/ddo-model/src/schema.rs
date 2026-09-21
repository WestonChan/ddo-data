//! The DDL. One statement batch, idempotent (`IF NOT EXISTS` throughout), with `CHECK`
//! constraints generated from the enums so the two can never disagree.
//!
//! Column comments name the DDOBuilderV2 element a value comes from (`<Name>`), or `computed`.

use crate::enums::{
    sql_in_list, AbilityOwner, ArmorType, FeatSource, Handedness, ItemCategory, LootType, ModifierSource,
    RequirementGroup, RequirementOwner, SaveProgression, TreeKind,
};
use std::sync::LazyLock;

/// Bumped whenever the DDL changes shape. Stored in `schema_version`.
pub const SCHEMA_VERSION: i64 = 1;

static DDL_TEXT: LazyLock<String> = LazyLock::new(|| {
    let item_category = sql_in_list(ItemCategory::ALL.iter().map(|c| c.as_str()));
    let handedness = sql_in_list(Handedness::ALL.iter().map(|h| h.as_str()));
    let armor_type = sql_in_list(ArmorType::ALL.iter().map(|a| a.as_str()));
    let loot_type = sql_in_list(LootType::ALL.iter().map(|l| l.as_str()));
    let modifier_source = sql_in_list(ModifierSource::ALL.iter().map(|s| s.as_str()));
    let requirement_owner = sql_in_list(RequirementOwner::ALL.iter().map(|o| o.as_str()));
    let feat_source = sql_in_list(FeatSource::ALL.iter().map(|f| f.as_str()));
    let ability_owner = sql_in_list(AbilityOwner::ALL.iter().map(|o| o.as_str()));
    let save_progression = sql_in_list(SaveProgression::ALL.iter().map(|s| s.as_str()));
    let tree_kind = sql_in_list(TreeKind::ALL.iter().map(|k| k.as_str()));
    let requirement_group = sql_in_list(RequirementGroup::ALL.iter().map(|g| g.as_str()));
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
    is_raid    INTEGER NOT NULL DEFAULT 0 CHECK (is_raid IN (0, 1))  -- <IsRaid/> present
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
    name              TEXT    NOT NULL UNIQUE,        -- <Name>
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
    wiki_url          TEXT                            -- computed from name
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

-- Bonuses: a typed number applied to a stat. Deduplicated across items.
CREATE TABLE IF NOT EXISTS bonuses (
    id            INTEGER PRIMARY KEY,
    name          TEXT    NOT NULL,                   -- computed '{{stat}} +{{value}}'
    description   TEXT,                               -- ItemBuffs.xml <DisplayText> with placeholders filled
    stat_id       INTEGER NOT NULL REFERENCES stats(id),
    bonus_type_id INTEGER REFERENCES bonus_types(id), -- <Buff><BonusType>, normalised
    value         INTEGER,                            -- <Buff><Value1>
    value2        INTEGER                             -- <Buff><Value2>
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_bonuses_unique
    ON bonuses(stat_id, COALESCE(bonus_type_id, -1), COALESCE(value, -1), COALESCE(value2, -1));
CREATE INDEX IF NOT EXISTS idx_bonuses_stat ON bonuses(stat_id);

CREATE TABLE IF NOT EXISTS item_bonuses (
    item_id    INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    bonus_id   INTEGER NOT NULL REFERENCES bonuses(id),
    sort_order INTEGER NOT NULL,                      -- <Buff> position within the item
    PRIMARY KEY (item_id, sort_order)
);
CREATE INDEX IF NOT EXISTS idx_item_bonuses_bonus ON item_bonuses(bonus_id);

-- Effects: a named property that is not a number on a stat (Vorpal, Ghost Touch, Feather Falling).
CREATE TABLE IF NOT EXISTS effects (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,                 -- <Buff><Type>
    description TEXT                                  -- ItemBuffs.xml <DisplayText>
);

CREATE TABLE IF NOT EXISTS item_effects (
    item_id    INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    effect_id  INTEGER NOT NULL REFERENCES effects(id),
    sort_order INTEGER NOT NULL,                      -- <Buff> position within the item
    value      INTEGER,                               -- <Buff><Value1>, when present
    target     TEXT,                                  -- <Buff><Item>, e.g. 'Fire', 'All'
    PRIMARY KEY (item_id, sort_order)
);
CREATE INDEX IF NOT EXISTS idx_item_effects_effect ON item_effects(effect_id);

CREATE TABLE IF NOT EXISTS item_augment_slots (
    item_id    INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    sort_order INTEGER NOT NULL,                      -- <ItemAugment> position
    slot_id    INTEGER NOT NULL REFERENCES augment_slot_types(id),
    PRIMARY KEY (item_id, sort_order)
);

-- Content upstream has fixed for a socket. One row per slot is a crafted upgrade already applied;
-- several rows are the choices a crafting step offers; no rows is an ordinary open socket.
CREATE TABLE IF NOT EXISTS item_augment_slot_options (
    item_id      INTEGER NOT NULL,
    slot_order   INTEGER NOT NULL,
    option_order INTEGER NOT NULL,                    -- <Augment> position within the <ItemAugment>
    name         TEXT    NOT NULL,                    -- <Augment><Name>
    description  TEXT,                                -- <Augment><Description>
    min_level    INTEGER,                             -- <Augment><MinLevel>
    PRIMARY KEY (item_id, slot_order, option_order),
    FOREIGN KEY (item_id, slot_order) REFERENCES item_augment_slots(item_id, sort_order) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS quest_loot (
    quest_id  INTEGER NOT NULL REFERENCES quests(id) ON DELETE CASCADE,
    item_id   INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    loot_type TEXT CHECK (loot_type {loot_type}),     -- computed from <DropLocation> and quest is_raid
    PRIMARY KEY (quest_id, item_id)
);
CREATE INDEX IF NOT EXISTS idx_quest_loot_item ON quest_loot(item_id);

-- Modifiers and requirements: the two grammars every family shares ---------------
--
-- A modifier is one upstream <Effect>, stored faithfully: the engine (Phases 6–8) reads these.
-- `bonuses` rows are derived from the simple ones for display; see the V3 roadmap entry.
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

CREATE TABLE IF NOT EXISTS feat_bonuses (
    feat_id    INTEGER NOT NULL REFERENCES feats(id) ON DELETE CASCADE,
    bonus_id   INTEGER NOT NULL REFERENCES bonuses(id),
    sort_order INTEGER NOT NULL,
    PRIMARY KEY (feat_id, sort_order)
);

-- A toggled or automatic combat stance an ability provides.
CREATE TABLE IF NOT EXISTS stances (
    id              INTEGER PRIMARY KEY,
    owner_kind      TEXT    NOT NULL CHECK (owner_kind {ability_owner}),
    owner_id        INTEGER NOT NULL,
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
    icon        TEXT
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
    weapon_class       TEXT                            -- <WeaponClass>
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

CREATE TABLE IF NOT EXISTS augment_bonuses (
    augment_id INTEGER NOT NULL REFERENCES augments(id) ON DELETE CASCADE,
    bonus_id   INTEGER NOT NULL REFERENCES bonuses(id),
    sort_order INTEGER NOT NULL,
    PRIMARY KEY (augment_id, sort_order)
);

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
    description    TEXT,                               -- <Buff><Description>
    UNIQUE (set_id, equipped_count)
);

CREATE TABLE IF NOT EXISTS set_bonus_items (
    set_id  INTEGER NOT NULL REFERENCES set_bonuses(id) ON DELETE CASCADE,
    item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    PRIMARY KEY (set_id, item_id)
);

CREATE TABLE IF NOT EXISTS filigrees (
    id          INTEGER PRIMARY KEY,
    name        TEXT    NOT NULL UNIQUE,               -- <Filigree><Name>
    description TEXT,                                  -- <Description>
    icon        TEXT,                                  -- <Icon>
    menu        TEXT,                                  -- <Menu>
    set_id      INTEGER REFERENCES set_bonuses(id)     -- <SetBonus>
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
"#
    )
});

/// The full DDL as one batch.
pub fn ddl() -> &'static str {
    &DDL_TEXT
}
