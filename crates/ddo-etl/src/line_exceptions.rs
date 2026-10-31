use anyhow::Result;
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy)]
pub(crate) enum DeferredSplitReason {
    MixedTypeSplit,
}

pub(crate) struct DeferredAugmentSplit {
    pub(crate) owner_slug: &'static str,
    pub(crate) issue: &'static str,
    pub(crate) reason: DeferredSplitReason,
}

impl DeferredAugmentSplit {
    pub(crate) fn description(&self) -> String {
        let reason = match self.reason {
            DeferredSplitReason::MixedTypeSplit => "mixed-type split",
        };
        format!("{}: {reason}", self.issue)
    }
}

const DEFERRED_AUGMENT_SPLITS: &[DeferredAugmentSplit] = &[
    DeferredAugmentSplit {
        owner_slug: "melancholic-converter-heroic",
        issue: "WestonChan/ddo-data#72",
        reason: DeferredSplitReason::MixedTypeSplit,
    },
    DeferredAugmentSplit {
        owner_slug: "melancholic-converter-legendary",
        issue: "WestonChan/ddo-data#72",
        reason: DeferredSplitReason::MixedTypeSplit,
    },
];

pub(crate) fn deferred_augment_split(slug: &str) -> Option<&'static DeferredAugmentSplit> {
    DEFERRED_AUGMENT_SPLITS.iter().find(|exception| exception.owner_slug == slug)
}

pub struct DeferredSplitViolation {
    pub owner_slug: String,
    pub detail: String,
}

#[derive(Default)]
struct ExceptionOwner {
    effects: Vec<crate::xml::effect::Effect>,
    links: Vec<(String, i64, Option<i64>)>,
}

pub fn deferred_split_violations(db: &Connection) -> Result<Vec<DeferredSplitViolation>> {
    violations_for(db, DEFERRED_AUGMENT_SPLITS)
}

fn violations_for(db: &Connection, exceptions: &[DeferredAugmentSplit]) -> Result<Vec<DeferredSplitViolation>> {
    let owners = exception_owners(db, exceptions)?;
    Ok(exceptions
        .iter()
        .filter_map(|exception| {
            let issue = exception.issue.split_once('#');
            let detail = if !issue.is_some_and(|(repository, number)| {
                repository.split_once('/').is_some_and(|(owner, name)| !owner.is_empty() && !name.is_empty())
                    && number.parse::<std::num::NonZeroU32>().is_ok()
            }) {
                Some("a deferred line split requires a repository issue reference".to_string())
            } else if let Some(owner) = owners.get(exception.owner_slug) {
                let needed = match exception.reason {
                    DeferredSplitReason::MixedTypeSplit => needs_mixed_type_split(owner),
                };
                (!needed).then(|| format!("{}: owner no longer needs this exception", exception.description()))
            } else {
                Some(format!("{}: owner no longer exists", exception.description()))
            };
            detail.map(|detail| DeferredSplitViolation { owner_slug: exception.owner_slug.to_string(), detail })
        })
        .collect())
}

fn exception_owners(db: &Connection, exceptions: &[DeferredAugmentSplit]) -> Result<BTreeMap<String, ExceptionOwner>> {
    let slugs = serde_json::to_string(&exceptions.iter().map(|exception| exception.owner_slug).collect::<Vec<_>>())?;
    let mut statement = db.prepare("SELECT slug FROM augments WHERE slug IN (SELECT value FROM json_each(?1))")?;
    let mut owners = statement
        .query_map([&slugs], |row| row.get::<_, String>(0))?
        .map(|slug| slug.map(|slug| (slug, ExceptionOwner::default())))
        .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
    let mut statement = db.prepare(
        "SELECT a.slug,m.effect_type,m.extra_types,m.bonus,m.amount_type,m.amounts,m.targets,m.damage
         FROM augments a JOIN modifiers m ON m.source_kind='augment' AND m.source_id=a.id
         WHERE a.slug IN (SELECT value FROM json_each(?1)) ORDER BY a.slug,m.sort_order",
    )?;
    let mut rows = statement.query([&slugs])?;
    while let Some(row) = rows.next()? {
        let slug: String = row.get(0)?;
        let mut types = vec![row.get::<_, String>(1)?];
        types.extend(serde_json::from_str::<Vec<String>>(row.get::<_, Option<String>>(2)?.as_deref().unwrap_or("[]"))?);
        owners.get_mut(&slug).expect("selected owner").effects.push(crate::xml::effect::Effect {
            types,
            bonus: row.get(3)?,
            amount_type: row.get(4)?,
            amounts: serde_json::from_str(row.get::<_, Option<String>>(5)?.as_deref().unwrap_or("[]"))?,
            targets: serde_json::from_str(row.get::<_, Option<String>>(6)?.as_deref().unwrap_or("[]"))?,
            damage: row.get(7)?,
            ..Default::default()
        });
    }
    let mut statement = db.prepare(
        "SELECT a.slug,e.name,l.bonus_type_id,l.value FROM augments a
         JOIN augment_effects l ON l.augment_id=a.id JOIN effects e ON e.id=l.effect_id
         WHERE e.is_stat=1 AND a.slug IN (SELECT value FROM json_each(?1)) ORDER BY a.slug,l.sort_order",
    )?;
    let mut rows = statement.query([&slugs])?;
    while let Some(row) = rows.next()? {
        let slug: String = row.get(0)?;
        owners.get_mut(&slug).expect("selected owner").links.push((row.get(1)?, row.get(2)?, row.get(3)?));
    }
    Ok(owners)
}

fn needs_mixed_type_split(owner: &ExceptionOwner) -> bool {
    let resolver = crate::map::effect::EffectResolver::new();
    owner.effects.iter().any(|effect| {
        let mut stats = BTreeMap::new();
        for effect_type in &effect.types {
            let mut member = effect.clone();
            member.types = vec![effect_type.clone()];
            let Ok(bonuses) = resolver.derive_bonuses(&member) else { return false };
            if bonuses.is_empty() {
                return false;
            }
            for bonus in bonuses {
                if stats.insert(bonus.stat.name, bonus.value).is_some_and(|previous| previous != bonus.value) {
                    return false;
                }
            }
        }
        let matching = owner.links.iter().filter(|(stat, _, _)| stats.contains_key(stat.as_str())).collect::<Vec<_>>();
        stats.len() > 1
            && matching.len() == stats.len()
            && matching.iter().map(|(stat, _, _)| stat.as_str()).collect::<BTreeSet<_>>()
                == stats.keys().copied().collect()
            && matching.iter().map(|(_, bonus_type, _)| *bonus_type).collect::<BTreeSet<_>>().len() > 1
            && matching.iter().all(|(stat, _, amount)| *amount == stats.get(stat.as_str()).copied())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Connection {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data");
        let mut db = Connection::open_in_memory().unwrap();
        crate::tree::build_from_data(&root, &mut db).unwrap();
        db
    }

    #[test]
    fn deferred_splits_require_their_current_mixed_type_lines() {
        let db = fixture();
        assert!(deferred_split_violations(&db).unwrap().is_empty());
        db.execute("UPDATE augment_effects SET bonus_type_id=(SELECT id FROM bonus_types WHERE name='Competence') WHERE augment_id=(SELECT id FROM augments WHERE slug='melancholic-converter-heroic')", []).unwrap();
        let violations = deferred_split_violations(&db).unwrap();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].owner_slug, "melancholic-converter-heroic");
        assert!(violations[0].detail.contains("no longer needs"));
    }

    #[test]
    fn deferred_splits_require_the_owner_to_exist() {
        let db = fixture();
        db.execute("UPDATE augments SET slug='renamed-melancholic' WHERE slug='melancholic-converter-heroic'", [])
            .unwrap();
        let violations = deferred_split_violations(&db).unwrap();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].owner_slug, "melancholic-converter-heroic");
        assert!(violations[0].detail.contains("no longer exists"));
    }

    #[test]
    fn deferred_splits_require_an_issue_reference() {
        let db = fixture();
        for issue in ["", "#72", "WestonChan/ddo-data", "WestonChan/ddo-data#0", "WestonChan/ddo-data#next"] {
            let exceptions = [DeferredAugmentSplit {
                owner_slug: "melancholic-converter-heroic",
                issue,
                reason: DeferredSplitReason::MixedTypeSplit,
            }];
            let violations = violations_for(&db, &exceptions).unwrap();
            assert_eq!(violations.len(), 1, "{issue:?}");
            assert!(violations[0].detail.contains("issue reference"));
        }
    }

    #[test]
    fn deferred_split_query_count_is_independent_of_exception_count() {
        use rusqlite::trace::{TraceEvent, TraceEventCodes};
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SELECTS: AtomicUsize = AtomicUsize::new(0);
        fn trace(event: TraceEvent<'_>) {
            if let TraceEvent::Stmt(statement, _) = event {
                if statement.sql().trim_start().starts_with("SELECT") {
                    SELECTS.fetch_add(1, Ordering::SeqCst);
                }
            }
        }
        let db = fixture();
        for count in [1, DEFERRED_AUGMENT_SPLITS.len()] {
            db.trace_v2(TraceEventCodes::SQLITE_TRACE_STMT, Some(trace));
            SELECTS.store(0, Ordering::SeqCst);
            assert!(violations_for(&db, &DEFERRED_AUGMENT_SPLITS[..count]).unwrap().is_empty());
            db.trace_v2(TraceEventCodes::empty(), None);
            assert_eq!(SELECTS.load(Ordering::SeqCst), 3, "{count} owners");
        }
    }
}
