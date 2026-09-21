//! Seed tables: the rows that exist in every database regardless of what the game data says.
//! They are the reference vocabularies the ETL maps onto and the API exposes as lookups.

use crate::enums::{BonusType, DamageCategory, EquipmentSlot, WeaponProficiency};
use crate::stats::STATS;
use rusqlite::{params, Connection};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BonusTypeSeed {
    pub bonus_type: BonusType,
}

pub const BONUS_TYPES: &[BonusTypeSeed] = &{
    let mut out = [BonusTypeSeed { bonus_type: BonusType::Enhancement }; BonusType::ALL.len()];
    let mut i = 0;
    while i < BonusType::ALL.len() {
        out[i] = BonusTypeSeed { bonus_type: BonusType::ALL[i] };
        i += 1;
    }
    out
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EquipmentSlotSeed {
    pub slot: EquipmentSlot,
}

pub const EQUIPMENT_SLOTS: &[EquipmentSlotSeed] = &{
    let mut out = [EquipmentSlotSeed { slot: EquipmentSlot::MainHand }; EquipmentSlot::ALL.len()];
    let mut i = 0;
    while i < EquipmentSlot::ALL.len() {
        out[i] = EquipmentSlotSeed { slot: EquipmentSlot::ALL[i] };
        i += 1;
    }
    out
};

/// A weapon or shield type, spelled as DDO spells it. `proficiency` is `None` for shields, orbs,
/// rune arms and pet collars, which no weapon-proficiency feat governs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeaponType {
    pub id: i64,
    pub name: &'static str,
    pub proficiency: Option<WeaponProficiency>,
    pub is_shield: bool,
}

const fn weapon(id: i64, name: &'static str, proficiency: WeaponProficiency) -> WeaponType {
    WeaponType { id, name, proficiency: Some(proficiency), is_shield: false }
}

const fn shield(id: i64, name: &'static str) -> WeaponType {
    WeaponType { id, name, proficiency: None, is_shield: true }
}

const fn implement(id: i64, name: &'static str) -> WeaponType {
    WeaponType { id, name, proficiency: None, is_shield: false }
}

use WeaponProficiency::{Exotic, Martial, Simple};

pub const WEAPON_TYPES: &[WeaponType] = &[
    weapon(1, "Bastard Sword", Exotic),
    weapon(2, "Battle Axe", Martial),
    shield(3, "Buckler"),
    weapon(4, "Club", Simple),
    implement(5, "Collar"),
    weapon(6, "Dagger", Simple),
    weapon(7, "Dart", Simple),
    weapon(8, "Dwarven Axe", Exotic),
    weapon(9, "Falchion", Martial),
    weapon(10, "Great Crossbow", Exotic),
    weapon(11, "Great Axe", Martial),
    weapon(12, "Great Club", Martial),
    weapon(13, "Great Sword", Martial),
    weapon(14, "Hand Axe", Martial),
    weapon(15, "Handwraps", Simple),
    weapon(16, "Heavy Crossbow", Simple),
    weapon(17, "Heavy Mace", Simple),
    weapon(18, "Heavy Pick", Martial),
    weapon(19, "Kama", Exotic),
    weapon(20, "Khopesh", Exotic),
    weapon(21, "Kukri", Martial),
    shield(22, "Large Shield"),
    weapon(23, "Light Crossbow", Simple),
    weapon(24, "Light Hammer", Martial),
    weapon(25, "Light Mace", Simple),
    weapon(26, "Light Pick", Martial),
    weapon(27, "Longbow", Martial),
    weapon(28, "Longsword", Martial),
    weapon(29, "Maul", Martial),
    weapon(30, "Morningstar", Simple),
    implement(31, "Orb"),
    weapon(32, "Quarterstaff", Simple),
    weapon(33, "Rapier", Martial),
    weapon(34, "Repeating Heavy Crossbow", Exotic),
    weapon(35, "Repeating Light Crossbow", Exotic),
    weapon(36, "Scimitar", Martial),
    weapon(37, "Shortbow", Martial),
    weapon(38, "Shortsword", Martial),
    weapon(39, "Shuriken", Exotic),
    weapon(40, "Sickle", Simple),
    shield(41, "Small Shield"),
    weapon(42, "Throwing Axe", Martial),
    weapon(43, "Throwing Dagger", Simple),
    weapon(44, "Throwing Hammer", Martial),
    shield(45, "Tower Shield"),
    weapon(46, "Warhammer", Martial),
    implement(47, "Rune Arm"),
    shield(48, "Cosmetic Shield"),
];

impl WeaponType {
    pub fn by_name(name: &str) -> Option<&'static WeaponType> {
        WEAPON_TYPES.iter().find(|w| w.name == name)
    }

    /// Thrown weapons are the ones a character can hold a stack of.
    pub const fn is_thrown(&self) -> bool {
        matches!(self.id, 7 | 39 | 42 | 43 | 44)
    }

    /// Bows and crossbows. Held in the main hand but attack at range.
    pub const fn is_ranged(&self) -> bool {
        matches!(self.id, 10 | 16 | 23 | 27 | 34 | 35 | 37)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DamageType {
    pub id: i64,
    pub name: &'static str,
    pub category: DamageCategory,
}

const fn damage(id: i64, name: &'static str, category: DamageCategory) -> DamageType {
    DamageType { id, name, category }
}

use DamageCategory::{Alignment, Elemental, Energy, Physical, Untyped};

pub const DAMAGE_TYPES: &[DamageType] = &[
    damage(1, "Slashing", Physical),
    damage(2, "Piercing", Physical),
    damage(3, "Bludgeoning", Physical),
    damage(4, "Fire", Elemental),
    damage(5, "Cold", Elemental),
    damage(6, "Electric", Elemental),
    damage(7, "Acid", Elemental),
    damage(8, "Sonic", Elemental),
    damage(9, "Good", Alignment),
    damage(10, "Evil", Alignment),
    damage(11, "Lawful", Alignment),
    damage(12, "Chaotic", Alignment),
    damage(13, "Negative", Energy),
    damage(14, "Positive", Energy),
    damage(15, "Force", Energy),
    damage(16, "Light", Energy),
    damage(17, "Poison", Energy),
    damage(18, "Untyped", Untyped),
];

/// Insert every seed table. Idempotent: uses `INSERT OR REPLACE` keyed on the stable ids.
pub fn insert_all(conn: &Connection) -> rusqlite::Result<()> {
    for s in STATS {
        conn.execute(
            "INSERT OR REPLACE INTO stats (id, name, category) VALUES (?1, ?2, ?3)",
            params![s.id, s.name, s.category.as_str()],
        )?;
    }
    for b in BONUS_TYPES {
        conn.execute(
            "INSERT OR REPLACE INTO bonus_types (id, name, stacks_with_self) VALUES (?1, ?2, ?3)",
            params![b.bonus_type.id(), b.bonus_type.name(), b.bonus_type.stacks_with_self()],
        )?;
    }
    for e in EQUIPMENT_SLOTS {
        conn.execute(
            "INSERT OR REPLACE INTO equipment_slots (id, name, sort_order, category) VALUES (?1, ?2, ?1, ?3)",
            params![e.slot.id(), e.slot.name(), e.slot.category().as_str()],
        )?;
    }
    for p in WeaponProficiency::ALL {
        conn.execute(
            "INSERT OR REPLACE INTO weapon_proficiencies (id, name) VALUES (?1, ?2)",
            params![p.id(), p.name()],
        )?;
    }
    for w in WEAPON_TYPES {
        conn.execute(
            "INSERT OR REPLACE INTO weapon_types (id, name, proficiency_id, is_shield) VALUES (?1, ?2, ?3, ?4)",
            params![w.id, w.name, w.proficiency.map(WeaponProficiency::id), w.is_shield],
        )?;
    }
    for d in DAMAGE_TYPES {
        conn.execute(
            "INSERT OR REPLACE INTO damage_types (id, name, category) VALUES (?1, ?2, ?3)",
            params![d.id, d.name, d.category.as_str()],
        )?;
    }
    Ok(())
}
