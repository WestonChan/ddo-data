use crate::enums::BonusType;

pub const ALWAYS_UNNAMED_TYPES: &[BonusType] = &[BonusType::Enhancement, BonusType::Equipment, BonusType::Feat];

pub const FIXED_EFFECT_BONUS_TYPES_SQL: &str = "SELECT eb.effect_id, MIN(eb.bonus_type_id) AS bonus_type_id,
    MIN(bt.name) AS name FROM effect_bonuses eb LEFT JOIN bonus_types bt ON bt.id = eb.bonus_type_id
    GROUP BY eb.effect_id HAVING COUNT(DISTINCT eb.bonus_type_id) = 1 AND COUNT(eb.bonus_type_id) = COUNT(*)";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatHomeType {
    Competence,
    Resistance,
    Profane,
}

impl StatHomeType {
    pub const fn bonus_type(self) -> BonusType {
        match self {
            Self::Competence => BonusType::Competence,
            Self::Resistance => BonusType::Resistance,
            Self::Profane => BonusType::Profane,
        }
    }
}

pub const STAT_HOME_TYPES: &[(&str, StatHomeType)] = &[
    ("Balance", StatHomeType::Competence),
    ("Bluff", StatHomeType::Competence),
    ("Concentration", StatHomeType::Competence),
    ("Diplomacy", StatHomeType::Competence),
    ("Disable Device", StatHomeType::Competence),
    ("Haggle", StatHomeType::Competence),
    ("Heal", StatHomeType::Competence),
    ("Hide", StatHomeType::Competence),
    ("Intimidate", StatHomeType::Competence),
    ("Jump", StatHomeType::Competence),
    ("Listen", StatHomeType::Competence),
    ("Move Silently", StatHomeType::Competence),
    ("Open Lock", StatHomeType::Competence),
    ("Perform", StatHomeType::Competence),
    ("Repair", StatHomeType::Competence),
    ("Search", StatHomeType::Competence),
    ("Spellcraft", StatHomeType::Competence),
    ("Spot", StatHomeType::Competence),
    ("Swim", StatHomeType::Competence),
    ("Tumble", StatHomeType::Competence),
    ("Use Magic Device", StatHomeType::Competence),
    ("Accuracy", StatHomeType::Competence),
    ("Deadly", StatHomeType::Competence),
    ("Healing Amplification", StatHomeType::Competence),
    ("Negative Healing Amplification", StatHomeType::Profane),
    ("Fortitude Save", StatHomeType::Resistance),
    ("Reflex Save", StatHomeType::Resistance),
    ("Will Save", StatHomeType::Resistance),
    ("Enchantment Save", StatHomeType::Resistance),
    ("Illusion Save", StatHomeType::Resistance),
    ("Spell Save", StatHomeType::Resistance),
];

pub fn is_always_unnamed(bonus_type: BonusType) -> bool {
    ALWAYS_UNNAMED_TYPES.contains(&bonus_type)
}

pub fn stat_home_type(name: &str) -> Option<BonusType> {
    STAT_HOME_TYPES.iter().find(|(stat, _)| *stat == name).map(|(_, home)| home.bonus_type())
}

pub fn home_declaration_error(bonus_type: BonusType) -> Option<String> {
    is_always_unnamed(bonus_type)
        .then(|| format!("home_bonus_type {:?} is always unnamed and must not be declared", bonus_type.name()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repair_amplification_uses_only_always_unnamed_types() {
        assert_eq!(stat_home_type("Repair Amplification"), None);
        assert!(is_always_unnamed(BonusType::Enhancement));
        assert!(!is_always_unnamed(BonusType::Competence));
    }

    #[test]
    fn stat_home_types_allow_only_the_three_reviewed_types() {
        let kinds = [StatHomeType::Competence, StatHomeType::Resistance, StatHomeType::Profane];
        assert_eq!(
            kinds.map(StatHomeType::bonus_type),
            [BonusType::Competence, BonusType::Resistance, BonusType::Profane]
        );
        for kind in kinds {
            match kind {
                StatHomeType::Competence | StatHomeType::Resistance | StatHomeType::Profane => {}
            }
        }
        assert_eq!(ALWAYS_UNNAMED_TYPES, &[BonusType::Enhancement, BonusType::Equipment, BonusType::Feat]);
    }

    #[test]
    fn stat_home_names_are_known_unique_and_not_always_unnamed() {
        let mut names = std::collections::BTreeSet::new();
        for &(name, home) in STAT_HOME_TYPES {
            assert!(crate::stats::Stat::by_name(name).is_some(), "unknown stat {name}");
            assert!(names.insert(name), "duplicate stat {name}");
            assert_eq!(home_declaration_error(home.bonus_type()), None);
        }
        for &bonus_type in ALWAYS_UNNAMED_TYPES {
            assert!(home_declaration_error(bonus_type).is_some());
        }
    }
}
