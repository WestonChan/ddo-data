use anyhow::{Context, Result};
use ddo_etl::corrections::Corrections;
use rusqlite::Connection;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Hard,
    Warn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Passed,
    Failed,
    Warned,
    Skipped,
}

pub struct IntegrityOptions {
    pub corrections: Corrections,
    pub allowed_empty_tables: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offender {
    pub name: String,
    pub id: Option<i64>,
    pub detail: String,
}

enum OffenderQuery {
    Sql(&'static str),
    Built(fn(&Connection, &IntegrityOptions) -> Result<Findings>),
}

#[derive(Default)]
struct Findings {
    offenders: Option<Vec<Offender>>,
    notes: Vec<String>,
}

pub struct IntegrityCheck {
    pub name: &'static str,
    pub severity: Severity,
    pub description: &'static str,
    offender_query: OffenderQuery,
    shown_offender_limit: usize,
    top_detail_limit: usize,
}

const SHOWN_OFFENDER_LIMIT: usize = 10;

const DROP_LOCATION_HEAD_SQL: &str = "COALESCE(NULLIF(TRIM(CASE WHEN instr(i.drop_location, ',') > 0 \
     THEN substr(i.drop_location, 1, instr(i.drop_location, ',') - 1) ELSE i.drop_location END), ''), \
     '(no drop location)')";

const WIKI_SOURCED_TABLES: [&str; 5] = ["items", "quests", "augments", "quest_chains", "sagas"];

pub const INTEGRITY_CHECKS: &[IntegrityCheck] = &[
    IntegrityCheck {
        name: "drops_reference_existing_rows",
        severity: Severity::Hard,
        description: "every foreign key in the database, the drops table's source, item and augment ids among them, \
                      names an existing row (PRAGMA foreign_key_check)",
        offender_query: OffenderQuery::Sql(
            "SELECT \"table\", rowid, 'references a missing ' || parent || ' row' FROM pragma_foreign_key_check()",
        ),
        shown_offender_limit: SHOWN_OFFENDER_LIMIT,
        top_detail_limit: 0,
    },
    IntegrityCheck {
        name: "items_have_names_and_slots",
        severity: Severity::Hard,
        description: "every item has a non-blank name and an equipment slot that exists; no item category is slotless",
        offender_query: OffenderQuery::Sql(
            "SELECT i.name, i.id, CASE WHEN TRIM(i.name) = '' THEN 'blank name' \
             ELSE 'slot ' || COALESCE(i.slot_id, 'null') || ' is not an equipment slot' END \
             FROM items i LEFT JOIN equipment_slots s ON s.id = i.slot_id \
             WHERE TRIM(i.name) = '' OR s.id IS NULL",
        ),
        shown_offender_limit: SHOWN_OFFENDER_LIMIT,
        top_detail_limit: 0,
    },
    IntegrityCheck {
        name: "quests_have_packs",
        severity: Severity::Hard,
        description: "every quest that is not a challenge belongs to an adventure pack",
        offender_query: OffenderQuery::Sql(
            "SELECT name, id, 'no adventure pack' FROM quests WHERE is_challenge = 0 AND pack_id IS NULL",
        ),
        shown_offender_limit: SHOWN_OFFENDER_LIMIT,
        top_detail_limit: 0,
    },
    IntegrityCheck {
        name: "wiki_rows_have_pages",
        severity: Severity::Hard,
        description: "every row the wiki supplies (source = 'wiki') carries the ddowiki page it was read from, \
                      in each table that has a wiki_url column",
        offender_query: OffenderQuery::Built(wiki_rows_without_pages),
        shown_offender_limit: SHOWN_OFFENDER_LIMIT,
        top_detail_limit: 0,
    },
    IntegrityCheck {
        name: "legacy_items_hidden",
        severity: Severity::Hard,
        description: "every item flagged is_legacy has a reason: a (legacy) or (historic) name, an is_legacy \
                      correction, or no drops row (its only sources are retired)",
        offender_query: OffenderQuery::Built(legacy_items_without_a_reason),
        shown_offender_limit: SHOWN_OFFENDER_LIMIT,
        top_detail_limit: 0,
    },
    IntegrityCheck {
        name: "items_without_a_source",
        severity: Severity::Warn,
        description: "items (other than legacy ones) with no drops row; no table links an item as a crafting output \
                      yet, so drops is the only source. Becomes HARD once vendor, event, crafting, challenge and \
                      starter-gear sources exist; the drop_location heads below are the work list for them",
        offender_query: OffenderQuery::Built(items_without_a_source),
        shown_offender_limit: SHOWN_OFFENDER_LIMIT,
        top_detail_limit: 15,
    },
    IntegrityCheck {
        name: "effects_named_after_stats",
        severity: Severity::Warn,
        description: "effects whose name equals a stat's ignoring case and spaces: buffs the buff map should turn \
                      into bonuses on that stat",
        offender_query: OffenderQuery::Sql(
            "SELECT e.name, e.id, 'named like the stat ' || s.name FROM effects e \
             JOIN stats s ON lower(replace(e.name, ' ', '')) = lower(replace(s.name, ' ', '')) ORDER BY e.name",
        ),
        shown_offender_limit: usize::MAX,
        top_detail_limit: 0,
    },
    IntegrityCheck {
        name: "untyped_item_bonuses",
        severity: Severity::Warn,
        description: "item bonuses with no bonus type, so they stack with everything; the top stats are listed",
        offender_query: OffenderQuery::Sql(
            "SELECT i.name, i.id, s.name FROM item_bonuses ib JOIN bonuses b ON b.id = ib.bonus_id \
             JOIN items i ON i.id = ib.item_id JOIN stats s ON s.id = b.stat_id \
             WHERE b.bonus_type_id IS NULL ORDER BY i.name",
        ),
        shown_offender_limit: SHOWN_OFFENDER_LIMIT,
        top_detail_limit: 10,
    },
];

fn has_column(db: &Connection, table: &str, column: &str) -> Result<bool> {
    let column_count: i64 =
        db.query_row("SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2", [table, column], |row| row.get(0))?;
    Ok(column_count > 0)
}

fn offenders_from_sql(db: &Connection, sql: &str) -> Result<Vec<Offender>> {
    let mut statement = db.prepare(sql).with_context(|| format!("preparing {sql}"))?;
    let offenders = statement
        .query_map([], |row| {
            Ok(Offender {
                name: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                id: row.get(1)?,
                detail: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(offenders)
}

fn wiki_rows_without_pages(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let mut queries = Vec::new();
    let mut notes = Vec::new();
    for table in WIKI_SOURCED_TABLES {
        if has_column(db, table, "wiki_url")? {
            queries.push(format!(
                "SELECT name, id, '{table} row from the wiki has no wiki_url' FROM {table} \
                 WHERE source = 'wiki' AND (wiki_url IS NULL OR TRIM(wiki_url) = '')"
            ));
        } else {
            notes.push(format!("{table} has no wiki_url column; the wiki file parser requires each entry's page"));
        }
    }
    let offenders = offenders_from_sql(db, &queries.join(" UNION ALL "))?;
    Ok(Findings { offenders: Some(offenders), notes })
}

fn legacy_items_without_a_reason(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    if !has_column(db, "items", "is_legacy")? {
        return Ok(Findings { offenders: None, notes: vec!["items.is_legacy is absent".to_string()] });
    }
    let offenders = offenders_from_sql(
        db,
        "SELECT i.name, i.id, 'is_legacy, yet it drops, its name says neither (legacy) nor (historic), \
         and no correction sets is_legacy' FROM items i \
         WHERE i.is_legacy = 1 \
         AND lower(i.name) NOT LIKE '%(legacy)%' AND lower(i.name) NOT LIKE '%(historic)%' \
         AND NOT EXISTS (SELECT 1 FROM corrections c WHERE c.kind = 'item' AND c.name = i.name \
                         AND c.field = 'is_legacy') \
         AND EXISTS (SELECT 1 FROM drops d WHERE d.item_id = i.id)",
    )?;
    Ok(Findings { offenders: Some(offenders), notes: Vec::new() })
}

fn items_without_a_source(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let has_is_legacy = has_column(db, "items", "is_legacy")?;
    let legacy_filter = if has_is_legacy { "AND i.is_legacy = 0" } else { "" };
    let offenders = offenders_from_sql(
        db,
        &format!(
            "SELECT i.name, i.id, {DROP_LOCATION_HEAD_SQL} FROM items i \
             WHERE NOT EXISTS (SELECT 1 FROM drops d WHERE d.item_id = i.id) {legacy_filter} ORDER BY i.name"
        ),
    )?;
    let notes = if has_is_legacy { Vec::new() } else { vec!["items.is_legacy is absent; no item is excluded".into()] };
    Ok(Findings { offenders: Some(offenders), notes })
}

fn findings_of(check: &IntegrityCheck, db: &Connection, options: &IntegrityOptions) -> Result<Findings> {
    match check.offender_query {
        OffenderQuery::Sql(sql) => Ok(Findings { offenders: Some(offenders_from_sql(db, sql)?), notes: Vec::new() }),
        OffenderQuery::Built(build_findings) => build_findings(db, options),
    }
}

fn top_details_of(offenders: &[Offender], limit: usize) -> Vec<(String, usize)> {
    let mut count_by_detail: Vec<(String, usize)> = Vec::new();
    for offender in offenders {
        match count_by_detail.iter_mut().find(|(detail, _)| *detail == offender.detail) {
            Some((_, count)) => *count += 1,
            None => count_by_detail.push((offender.detail.clone(), 1)),
        }
    }
    count_by_detail.sort_by(|(left_detail, left_count), (right_detail, right_count)| {
        right_count.cmp(left_count).then_with(|| left_detail.cmp(right_detail))
    });
    count_by_detail.truncate(limit);
    count_by_detail
}

pub struct CheckOutcome {
    pub name: &'static str,
    pub severity: Severity,
    pub description: &'static str,
    pub status: CheckStatus,
    pub offenders: Vec<Offender>,
    pub top_details: Vec<(String, usize)>,
    pub notes: Vec<String>,
    shown_offender_limit: usize,
}

fn outcome_of(check: &IntegrityCheck, db: &Connection, options: &IntegrityOptions) -> Result<CheckOutcome> {
    let findings = findings_of(check, db, options).with_context(|| format!("running check {}", check.name))?;
    let (status, offenders) = match findings.offenders {
        None => (CheckStatus::Skipped, Vec::new()),
        Some(offenders) if offenders.is_empty() => (CheckStatus::Passed, offenders),
        Some(offenders) => match check.severity {
            Severity::Hard => (CheckStatus::Failed, offenders),
            Severity::Warn => (CheckStatus::Warned, offenders),
        },
    };
    Ok(CheckOutcome {
        name: check.name,
        severity: check.severity,
        description: check.description,
        status,
        top_details: top_details_of(&offenders, check.top_detail_limit),
        offenders,
        notes: findings.notes,
        shown_offender_limit: check.shown_offender_limit,
    })
}

pub struct IntegrityReport {
    pub outcomes: Vec<CheckOutcome>,
}

impl IntegrityReport {
    pub fn outcome(&self, check_name: &str) -> Option<&CheckOutcome> {
        self.outcomes.iter().find(|outcome| outcome.name == check_name)
    }

    pub fn failed_hard_check_names(&self) -> Vec<&'static str> {
        self.outcomes
            .iter()
            .filter(|outcome| outcome.status == CheckStatus::Failed)
            .map(|outcome| outcome.name)
            .collect()
    }
}

fn offender_count_text(offender_count: usize) -> String {
    match offender_count {
        1 => "1 offender".to_string(),
        _ => format!("{offender_count} offenders"),
    }
}

impl fmt::Display for CheckOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status_text = match self.status {
            CheckStatus::Passed => "ok".to_string(),
            CheckStatus::Skipped => "skipped".to_string(),
            CheckStatus::Failed => format!("FAIL ({})", offender_count_text(self.offenders.len())),
            CheckStatus::Warned => format!("WARN ({})", offender_count_text(self.offenders.len())),
        };
        writeln!(formatter, "check {}: {status_text}", self.name)?;
        if !matches!(self.status, CheckStatus::Passed) {
            writeln!(formatter, "  {}", self.description)?;
        }
        for offender in self.offenders.iter().take(self.shown_offender_limit) {
            let id_text = offender.id.map(|id| format!(" #{id}")).unwrap_or_default();
            writeln!(formatter, "  {:?}{id_text}: {}", offender.name, offender.detail)?;
        }
        if self.offenders.len() > self.shown_offender_limit {
            writeln!(formatter, "  ... and {} more", self.offenders.len() - self.shown_offender_limit)?;
        }
        if !self.top_details.is_empty() {
            writeln!(formatter, "  top {}:", self.top_details.len())?;
            for (detail, count) in &self.top_details {
                writeln!(formatter, "  {count:>6}  {detail}")?;
            }
        }
        for note in &self.notes {
            writeln!(formatter, "  note: {note}")?;
        }
        Ok(())
    }
}

impl fmt::Display for IntegrityReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for outcome in &self.outcomes {
            write!(formatter, "{outcome}")?;
        }
        let warned_count = self.outcomes.iter().filter(|outcome| outcome.status == CheckStatus::Warned).count();
        match self.failed_hard_check_names().as_slice() {
            [] => write!(formatter, "integrity: every HARD check passed, {warned_count} WARN check(s) reported"),
            failed_names => write!(formatter, "integrity: HARD check(s) failed: {}", failed_names.join(", ")),
        }
    }
}

pub fn integrity_report(db: &Connection, options: &IntegrityOptions) -> Result<IntegrityReport> {
    let outcomes =
        INTEGRITY_CHECKS.iter().map(|check| outcome_of(check, db, options)).collect::<Result<Vec<CheckOutcome>>>()?;
    Ok(IntegrityReport { outcomes })
}
