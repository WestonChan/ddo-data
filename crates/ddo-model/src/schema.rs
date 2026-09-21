//! The DDL. One statement batch, idempotent (`IF NOT EXISTS` throughout), with `CHECK`
//! constraints generated from the enums so the two can never disagree.
//!
//! Column comments name the DDOBuilderV2 element a value comes from (`<Name>`), or `computed`.

use crate::enums::{sql_in_list, ArmorType, Handedness, ItemCategory, LootType};
use std::sync::LazyLock;

/// Bumped whenever the DDL changes shape. Stored in `schema_version`.
pub const SCHEMA_VERSION: i64 = 1;

static DDL_TEXT: LazyLock<String> = LazyLock::new(|| {
    let item_category = sql_in_list(ItemCategory::ALL.iter().map(|c| c.as_str()));
    let handedness = sql_in_list(Handedness::ALL.iter().map(|h| h.as_str()));
    let armor_type = sql_in_list(ArmorType::ALL.iter().map(|a| a.as_str()));
    let loot_type = sql_in_list(LootType::ALL.iter().map(|l| l.as_str()));
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
"#
    )
});

/// The full DDL as one batch.
pub fn ddl() -> &'static str {
    &DDL_TEXT
}
