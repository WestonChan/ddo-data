use anyhow::{Context, Result};
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Clone)]
pub struct GroupingBonus {
    pub target_id: i64,
    pub bonus_type: Option<String>,
    pub amount_from: i64,
    pub constant: Option<i64>,
    pub scale: f64,
    pub rounding: String,
    pub trigger_id: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct Grouping {
    pub id: i64,
    pub name: String,
    pub slug: String,
    pub is_group: bool,
    pub verbose_name_template: String,
    pub description_template: Option<String>,
    pub set_bonus_line_template: Option<String>,
    pub home_bonus_type: Option<String>,
    pub default_value: Option<i64>,
    pub bonuses: Vec<GroupingBonus>,
    pub stat_ids: BTreeSet<i64>,
    pub is_wrapper: bool,
}

impl Grouping {
    fn is_link_valued(&self) -> bool {
        !self.bonuses.is_empty()
            && self.bonuses.iter().all(|bonus| {
                matches!(bonus.amount_from, 1 | 2)
                    && bonus.constant.is_none()
                    && bonus.scale == 1.0
                    && bonus.rounding == "down"
                    && bonus.trigger_id.is_none()
            })
    }

    pub fn accepts_value_at_type(&self, bonus_type: &str) -> bool {
        !self.verbose_name_template.contains("{2}")
            && !self.description_template.as_deref().is_some_and(|template| template.contains("{2}"))
            && self.bonuses.iter().all(|bonus| {
                bonus.amount_from == 1
                    && bonus.constant.is_none()
                    && bonus.scale == 1.0
                    && bonus.rounding == "down"
                    && bonus.trigger_id.is_none()
                    && bonus.bonus_type.as_deref().is_none_or(|fixed| fixed == bonus_type)
            })
            && self.bonuses.iter().map(|bonus| bonus.bonus_type.as_deref()).collect::<BTreeSet<_>>().len() == 1
    }
}

pub fn name_from_stats(stats: &BTreeSet<String>) -> Result<String> {
    let names = ddo_model::stats::STATS
        .iter()
        .filter(|stat| stats.contains(stat.name))
        .map(|stat| stat.name)
        .collect::<Vec<_>>();
    anyhow::ensure!(
        names.len() == stats.len() && names.len() >= 2,
        "group requires at least two known stats: {stats:?}"
    );
    let words = names.iter().map(|name| name.split_whitespace().collect::<Vec<_>>()).collect::<Vec<_>>();
    let common_suffix = (1..words.iter().map(Vec::len).min().unwrap())
        .take_while(|length| {
            words.iter().all(|name| name[name.len() - length..] == words[0][words[0].len() - length..])
        })
        .last()
        .unwrap_or(0);
    let mut prefixes = words.iter().map(|name| name[..name.len() - common_suffix].join(" ")).collect::<Vec<_>>();
    let last = prefixes.pop().unwrap();
    let mut name = format!("{} and {last}", prefixes.join(", "));
    if common_suffix > 0 {
        name.push(' ');
        name.push_str(&words[0][words[0].len() - common_suffix..].join(" "));
    }
    Ok(name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GroupingPairKind {
    Wrapper,
    LinkValued,
    Ladder,
    ConstantValued,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupingPairDomain {
    LinkValued,
    AllBonuses,
}

#[derive(Debug)]
pub struct GroupingPair<'a> {
    pub first: &'a Grouping,
    pub second: &'a Grouping,
    pub kind: GroupingPairKind,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ValuedProseDuplicate {
    pub prose_name: String,
    pub target_name: String,
}

fn valued_prose_target(name: &str) -> Option<String> {
    let lowered = name.to_lowercase();
    let text = lowered.trim();
    if !text.starts_with(['+', '-']) {
        return None;
    }
    let number_end = text
        .char_indices()
        .find(|(index, letter)| *index > 0 && !letter.is_ascii_digit() && *letter != '.')
        .map_or(text.len(), |(index, _)| index);
    text[..number_end].parse::<f64>().ok()?;
    let amount_suffix = &text[number_end..];
    let mut target = amount_suffix.strip_prefix('%').unwrap_or(amount_suffix).trim();
    for bonus_type in ddo_model::enums::BonusType::ALL {
        let prefix = format!("{} ", bonus_type.name().to_lowercase());
        if let Some(rest) = target.strip_prefix(&prefix) {
            target = rest;
            break;
        }
    }
    target = target.strip_prefix("bonus to ").unwrap_or(target);
    Some(target.strip_suffix(" bonus").unwrap_or(target).to_string())
}

#[derive(Debug, Default)]
pub struct Groupings {
    by_name: HashMap<String, i64>,
    pub by_stat_set: HashMap<BTreeSet<String>, Vec<Grouping>>,
    pub by_id: BTreeMap<i64, Grouping>,
}

impl Groupings {
    pub(crate) fn effect_named(&self, name: &str) -> Option<&Grouping> {
        self.by_name.get(name).and_then(|id| self.by_id.get(id))
    }

    pub(crate) fn grouping_named(&self, name: &str) -> Option<&Grouping> {
        self.effect_named(name).filter(|effect| effect.stat_ids.len() >= 2 && effect.is_link_valued())
    }

    pub(crate) fn expanded_stat_names(&self, name: &str) -> Result<BTreeSet<String>> {
        let effect = self.effect_named(name).with_context(|| format!("unknown effect {name:?}"))?;
        if ddo_model::stats::Stat::by_name(name).is_some() {
            return Ok(BTreeSet::from([name.to_string()]));
        }
        let mut stats = BTreeSet::new();
        for bonus in &effect.bonuses {
            let target = &self.by_id[&bonus.target_id];
            if target.is_group {
                stats.extend(target.bonuses.iter().map(|member| self.by_id[&member.target_id].name.clone()));
            } else {
                stats.insert(target.name.clone());
            }
        }
        Ok(stats)
    }

    pub fn valued_prose_duplicates(&self) -> Vec<ValuedProseDuplicate> {
        let names = self
            .by_id
            .values()
            .filter(|effect| ddo_model::stats::Stat::by_name(&effect.name).is_some())
            .map(|effect| effect.name.as_str())
            .chain(self.by_stat_set.values().flatten().map(|grouping| grouping.name.as_str()))
            .collect::<BTreeSet<_>>();
        let mut duplicates = Vec::new();
        for effect in self.by_id.values().filter(|effect| !effect.is_group && effect.bonuses.is_empty()) {
            let Some(target) = valued_prose_target(&effect.name) else { continue };
            for name in &names {
                let normalized = name.to_lowercase();
                if target == normalized.strip_suffix(" bonus").unwrap_or(&normalized) {
                    duplicates.push(ValuedProseDuplicate {
                        prose_name: effect.name.clone(),
                        target_name: (*name).to_string(),
                    });
                }
            }
        }
        duplicates
            .sort_by(|left, right| (&left.prose_name, &left.target_name).cmp(&(&right.prose_name, &right.target_name)));
        duplicates
    }

    pub fn from_database(db: &Connection) -> Result<Self> {
        let mut statement = db.prepare(
            "SELECT e.id, e.name, e.slug, e.is_stat, e.is_group, e.verbose_name_template, e.description_template,
                    e.set_bonus_line_template, bt.name, e.default_value FROM effects e
               LEFT JOIN bonus_types bt ON bt.id = e.home_bonus_type_id ORDER BY e.name",
        )?;
        let effects = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, bool>(3)?,
                    Grouping {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        slug: row.get(2)?,
                        is_group: row.get(4)?,
                        verbose_name_template: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                        description_template: row.get(6)?,
                        set_bonus_line_template: row.get(7)?,
                        home_bonus_type: row.get(8)?,
                        default_value: row.get(9)?,
                        bonuses: Vec::new(),
                        stat_ids: BTreeSet::new(),
                        is_wrapper: false,
                    },
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let stat_ids =
            effects.iter().filter(|(is_stat, _)| *is_stat).map(|(_, effect)| effect.id).collect::<BTreeSet<_>>();
        let mut effects_by_id = effects.into_iter().map(|(_, effect)| (effect.id, effect)).collect::<BTreeMap<_, _>>();
        let mut statement = db.prepare(
            "SELECT b.effect_id, b.target_effect_id, bt.name, b.amount_from, b.constant, b.scale, b.rounding, b.trigger_id
               FROM effect_bonuses b LEFT JOIN bonus_types bt ON bt.id = b.bonus_type_id
               ORDER BY b.effect_id, b.sort_order",
        )?;
        for row in statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                GroupingBonus {
                    target_id: row.get(1)?,
                    bonus_type: row.get(2)?,
                    amount_from: row.get(3)?,
                    constant: row.get(4)?,
                    scale: row.get(5)?,
                    rounding: row.get(6)?,
                    trigger_id: row.get(7)?,
                },
            ))
        })? {
            let (effect_id, bonus) = row?;
            effects_by_id.get_mut(&effect_id).context("bonus references an absent effect")?.bonuses.push(bonus);
        }
        let members_by_group = effects_by_id
            .values()
            .filter(|effect| effect.is_group)
            .map(|effect| (effect.id, effect.bonuses.iter().map(|bonus| bonus.target_id).collect::<BTreeSet<_>>()))
            .collect::<BTreeMap<_, _>>();
        let by_name = effects_by_id.values().map(|effect| (effect.name.clone(), effect.id)).collect();
        let mut groupings = Self { by_name, by_stat_set: HashMap::new(), by_id: effects_by_id.clone() };
        for mut effect in effects_by_id.into_values() {
            effect.is_wrapper = !effect.is_group
                && effect.bonuses.len() == 1
                && members_by_group.contains_key(&effect.bonuses[0].target_id);
            for bonus in &effect.bonuses {
                if let Some(members) = members_by_group.get(&bonus.target_id) {
                    effect.stat_ids.extend(members);
                } else if stat_ids.contains(&bonus.target_id) {
                    effect.stat_ids.insert(bonus.target_id);
                }
            }
            groupings.by_id.insert(effect.id, effect.clone());
            if effect.stat_ids.len() >= 2 && effect.is_link_valued() {
                let names = effect.stat_ids.iter().map(|id| groupings.by_id[id].name.clone()).collect();
                groupings.by_stat_set.entry(names).or_default().push(effect);
            }
        }
        for members in groupings.by_stat_set.values_mut() {
            members.sort_by(|first, second| first.name.cmp(&second.name));
        }
        Ok(groupings)
    }

    pub fn pairs(&self, domain: GroupingPairDomain) -> Vec<GroupingPair<'_>> {
        let mut members_by_stats = BTreeMap::<_, Vec<_>>::new();
        for effect in self.by_id.values() {
            if effect.stat_ids.len() >= 2 && (domain == GroupingPairDomain::AllBonuses || effect.is_link_valued()) {
                members_by_stats.entry(&effect.stat_ids).or_default().push(effect);
            }
        }
        let mut pairs = Vec::new();
        for members in members_by_stats.values_mut() {
            members.sort_by_key(|effect| &effect.name);
            for (index, first) in members.iter().enumerate() {
                for second in &members[index + 1..] {
                    let kind = if ladder_name(&first.name) == ladder_name(&second.name) {
                        GroupingPairKind::Ladder
                    } else if first.is_wrapper || second.is_wrapper {
                        GroupingPairKind::Wrapper
                    } else if first.bonuses.iter().chain(&second.bonuses).any(|bonus| bonus.constant.is_some()) {
                        GroupingPairKind::ConstantValued
                    } else {
                        GroupingPairKind::LinkValued
                    };
                    pairs.push(GroupingPair { first, second, kind });
                }
            }
        }
        pairs.sort_by(|first, second| {
            (first.kind, &first.first.name, &first.second.name).cmp(&(
                second.kind,
                &second.first.name,
                &second.second.name,
            ))
        });
        pairs
    }
}

fn ladder_name(name: &str) -> &str {
    name.rsplit_once(' ')
        .filter(|(_, suffix)| {
            !suffix.is_empty() && suffix.chars().all(|letter| matches!(letter, 'I' | 'V' | 'X' | 'L' | 'C' | 'D' | 'M'))
        })
        .map_or(name, |(base, _)| base)
}

#[cfg(test)]
pub(crate) fn fixture_groupings() -> &'static Groupings {
    static GROUPINGS: std::sync::OnceLock<Groupings> = std::sync::OnceLock::new();
    GROUPINGS.get_or_init(|| {
        let mut db = Connection::open_in_memory().unwrap();
        crate::tree::build_from_data(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data"),
            &mut db,
        )
        .unwrap();
        Groupings::from_database(&db).unwrap()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::trace::{TraceEvent, TraceEventCodes};
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn valued_prose_names_duplicate_stats_and_groups_but_not_conditional_lines() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data");
        let scratch = tempfile::tempdir().unwrap();
        for relative in [
            "upstream/commit.toml",
            "upstream/source-aliases.toml",
            "effects/bonus-types.toml",
            "effects/stats/ranged-power.toml",
            "effects/stats/doublestrike.toml",
            "effects/stats/doubleshot.toml",
            "effects/stat-groups/doublestrike-and-doubleshot.toml",
        ] {
            let destination = scratch.path().join(relative);
            std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
            std::fs::copy(source.join(relative), destination).unwrap();
        }
        for name in [
            "+5 Ranged Power",
            "+2 Doublestrike and Doubleshot",
            "-6% Artifact bonus to RANGED POWER Bonus",
            "+5 Artifact bonus to hit and damage versus Evil creatures",
        ] {
            let path = scratch.path().join("effects").join(format!("{}.toml", ddo_model::identity::slug(name)));
            std::fs::write(
                path,
                format!("name = {name:?}\nverbose_name_template = {name:?}\ndescription_template = {name:?}\n"),
            )
            .unwrap();
        }
        let mut db = Connection::open_in_memory().unwrap();
        let error = crate::tree::build_from_data(scratch.path(), &mut db).unwrap_err().to_string();
        for name in ["+5 Ranged Power", "+2 Doublestrike and Doubleshot", "-6% Artifact bonus to RANGED POWER Bonus"] {
            assert!(error.contains(name), "{error}");
            std::fs::remove_file(
                scratch.path().join("effects").join(format!("{}.toml", ddo_model::identity::slug(name))),
            )
            .unwrap();
        }
        let mut db = Connection::open_in_memory().unwrap();
        crate::tree::build_from_data(scratch.path(), &mut db).unwrap();
        assert!(Groupings::from_database(&db).unwrap().valued_prose_duplicates().is_empty());
        for malformed in ["", "+", "+. Ranged Power", "Ranged Power"] {
            assert_eq!(valued_prose_target(malformed), None, "{malformed}");
        }
    }

    #[test]
    fn stat_order_and_repeated_targets_do_not_change_the_group_name() {
        let first = ["Fire Spell Power", "Cold Spell Power", "Electric Spell Power", "Positive Spell Power"];
        let second = [
            "Cold Spell Power",
            "Fire Spell Power",
            "Positive Spell Power",
            "Electric Spell Power",
            "Fire Spell Power",
        ];
        for names in [first.as_slice(), second.as_slice()] {
            let stats = names.iter().map(|name| (*name).to_string()).collect();
            assert_eq!(name_from_stats(&stats).unwrap(), "Fire, Cold, Electric and Positive Spell Power");
        }
        assert!(name_from_stats(&BTreeSet::new()).is_err());
        assert!(name_from_stats(&BTreeSet::from(["Fire Spell Power".to_string()])).is_err());
        assert!(name_from_stats(&BTreeSet::from(["Fire Spell Power".to_string(), "Unknown".to_string()])).is_err());
    }

    #[test]
    fn only_single_slot_and_compatible_type_definitions_accept_one_value() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let scratch = tempfile::tempdir().unwrap();
        for directory in ["data", "effect-groups/data"] {
            let root = source.join(directory);
            for entry in
                walkdir::WalkDir::new(&root).into_iter().map(Result::unwrap).filter(|entry| entry.file_type().is_file())
            {
                let destination = scratch.path().join(entry.path().strip_prefix(&root).unwrap());
                std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
                std::fs::copy(entry.path(), destination).unwrap();
            }
        }
        let mut db = Connection::open_in_memory().unwrap();
        crate::tree::build_from_data(scratch.path(), &mut db).unwrap();
        let index = Groupings::from_database(&db).unwrap();
        assert!(!index.by_stat_set.values().flatten().any(|grouping| grouping.name == "Riposte"));
        let named = |name: &str| index.by_stat_set.values().flatten().find(|grouping| grouping.name == name).unwrap();
        assert!(named("Power of the Frozen Thunderstorm").accepts_value_at_type("Artifact"));
        assert!(!named("Deception").accepts_value_at_type("Equipment"));
        assert!(!named("Lifeblood of the Undead Prince").accepts_value_at_type("Competence"));
        assert!(!named("Lifeblood of the Undead Prince").accepts_value_at_type("Profane"));
    }

    #[test]
    fn exact_constant_matches_are_reported_without_containment_matches() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let scratch = tempfile::tempdir().unwrap();
        for relative in [
            "upstream/commit.toml",
            "upstream/source-aliases.toml",
            "effects/bonus-types.toml",
            "effects/stats/trip-dc.toml",
            "effects/stats/sunder-dc.toml",
            "effects/stats/stun-dc.toml",
            "effects/baphomets-tactics.toml",
            "effects/stat-groups/combat-mastery.toml",
        ] {
            let destination = scratch.path().join(relative);
            std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
            std::fs::copy(source.join(relative), destination).unwrap();
        }
        let mastery = std::fs::read_to_string(source.join("effects/stat-groups/combat-mastery.toml")).unwrap();
        let partial = mastery.rsplit_once("[[bonus]]").unwrap().0.replace("Combat Mastery", "Trip and Sunder DC");
        std::fs::write(scratch.path().join("effects/trip-and-sunder-dc.toml"), partial).unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        crate::tree::build_from_data(scratch.path(), &mut db).unwrap();
        let index = Groupings::from_database(&db).unwrap();
        let pairs = index.pairs(GroupingPairDomain::AllBonuses);
        let matches = pairs
            .iter()
            .filter(|pair| pair.first.name == "Baphomet's Tactics")
            .map(|pair| pair.second.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(matches, ["Combat Mastery"]);
        assert!(index.grouping_named("Baphomet's Tactics").is_none());
        assert!(index.pairs(GroupingPairDomain::LinkValued).is_empty());
    }

    #[test]
    fn index_uses_two_queries_for_one_and_fifty_effects() {
        static SELECTS: AtomicUsize = AtomicUsize::new(0);
        fn trace(event: TraceEvent<'_>) {
            if let TraceEvent::Stmt(statement, _) = event {
                if statement.sql().trim_start().to_uppercase().starts_with("SELECT") {
                    SELECTS.fetch_add(1, Ordering::SeqCst);
                }
            }
        }
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let original = std::fs::read_to_string(source.join("effects/power-of-the-frozen-thunderstorm.toml"))
            .unwrap()
            .replace("home_bonus_type = \"Equipment\"\n", "");
        for effect_count in [1, 50] {
            let scratch = tempfile::tempdir().unwrap();
            let root = scratch.path();
            std::fs::create_dir_all(root.join("upstream")).unwrap();
            std::fs::create_dir_all(root.join("effects/stats")).unwrap();
            std::fs::write(root.join("upstream/source-aliases.toml"), "").unwrap();
            std::fs::copy(source.join("upstream/commit.toml"), root.join("upstream/commit.toml")).unwrap();
            std::fs::copy(source.join("effects/bonus-types.toml"), root.join("effects/bonus-types.toml")).unwrap();
            for stat in ["cold-spell-power", "electric-spell-power", "sonic-spell-power"] {
                std::fs::copy(
                    source.join(format!("effects/stats/{stat}.toml")),
                    root.join(format!("effects/stats/{stat}.toml")),
                )
                .unwrap();
            }
            for index in 0..effect_count {
                let contents = original.replace("Power of the Frozen Thunderstorm", &format!("Example Group {index}"));
                std::fs::write(root.join(format!("effects/example-group-{index}.toml")), contents).unwrap();
            }
            let mut db = Connection::open_in_memory().unwrap();
            crate::tree::build_from_data(root, &mut db).unwrap();
            db.trace_v2(TraceEventCodes::SQLITE_TRACE_STMT, Some(trace));
            SELECTS.store(0, Ordering::SeqCst);
            let groupings = Groupings::from_database(&db).unwrap();
            db.trace_v2(TraceEventCodes::empty(), None);
            assert_eq!(groupings.by_stat_set.values().map(Vec::len).sum::<usize>(), effect_count);
            assert_eq!(SELECTS.load(Ordering::SeqCst), 2, "{effect_count} effects");
        }
    }
}
