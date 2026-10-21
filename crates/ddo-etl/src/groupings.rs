use anyhow::{Context, Result};
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Clone)]
pub struct GroupingBonus {
    pub target_id: i64,
    pub bonus_type_id: Option<i64>,
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
    pub home_bonus_type_id: Option<i64>,
    pub default_value: Option<i64>,
    pub bonuses: Vec<GroupingBonus>,
    pub stat_ids: BTreeSet<i64>,
    pub is_wrapper: bool,
}

impl Grouping {
    pub fn accepts_value_at_type(&self, bonus_type_id: i64) -> bool {
        self.bonuses.iter().all(|bonus| {
            bonus.amount_from == 1
                && bonus.constant.is_none()
                && bonus.scale == 1.0
                && bonus.rounding == "down"
                && bonus.trigger_id.is_none()
                && bonus.bonus_type_id.is_none_or(|fixed| fixed == bonus_type_id)
        }) && self.bonuses.iter().map(|bonus| bonus.bonus_type_id).collect::<BTreeSet<_>>().len() == 1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GroupingPairKind {
    Wrapper,
    LinkValued,
    Ladder,
}

#[derive(Debug)]
pub struct GroupingPair<'a> {
    pub first: &'a Grouping,
    pub second: &'a Grouping,
    pub kind: GroupingPairKind,
}

#[derive(Debug, Default)]
pub struct Groupings {
    pub by_stat_set: HashMap<BTreeSet<i64>, Vec<Grouping>>,
}

impl Groupings {
    pub fn from_database(db: &Connection) -> Result<Self> {
        let mut statement = db.prepare(
            "SELECT id, name, slug, is_stat, is_group, verbose_name_template, description_template,
                    set_bonus_line_template, home_bonus_type_id, default_value FROM effects ORDER BY name",
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
                        home_bonus_type_id: row.get(8)?,
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
            "SELECT effect_id, target_effect_id, bonus_type_id, amount_from, constant, scale, rounding, trigger_id
               FROM effect_bonuses ORDER BY effect_id, sort_order",
        )?;
        for row in statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                GroupingBonus {
                    target_id: row.get(1)?,
                    bonus_type_id: row.get(2)?,
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
        let mut groupings = Self::default();
        for mut effect in effects_by_id.into_values() {
            if effect.bonuses.is_empty()
                || effect.bonuses.iter().any(|bonus| !matches!(bonus.amount_from, 1 | 2) || bonus.constant.is_some())
            {
                continue;
            }
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
            if effect.stat_ids.len() >= 2 {
                groupings.by_stat_set.entry(effect.stat_ids.clone()).or_default().push(effect);
            }
        }
        for members in groupings.by_stat_set.values_mut() {
            members.sort_by(|first, second| first.name.cmp(&second.name));
        }
        Ok(groupings)
    }

    pub fn pairs(&self) -> Vec<GroupingPair<'_>> {
        let mut pairs = Vec::new();
        for members in self.by_stat_set.values() {
            for (index, first) in members.iter().enumerate() {
                for second in &members[index + 1..] {
                    let kind = if ladder_name(&first.name) == ladder_name(&second.name) {
                        GroupingPairKind::Ladder
                    } else if first.is_wrapper || second.is_wrapper {
                        GroupingPairKind::Wrapper
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
mod tests {
    use super::*;
    use rusqlite::trace::{TraceEvent, TraceEventCodes};
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
