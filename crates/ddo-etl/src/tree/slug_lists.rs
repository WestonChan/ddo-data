use super::annotations::EntityIdentity;
use super::kinds::RedirectsFile;
use anyhow::{ensure, Context, Result};
use ddo_model::enums::EntityKind;
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::path::Path;

const EXPIRED_SINCE: &str = "since < date(?1, '-30 days')";
pub const PRUNE_REDIRECTS_COMMAND: &str = "cargo run -p ddo-etl -- prune-redirects --data data";

pub fn redirect_violations(db: &Connection) -> Result<Vec<String>> {
    let identities = db
        .prepare(&format!("SELECT {} FROM entity_identity", EntityIdentity::select_columns()))?
        .query_map([], EntityIdentity::from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let current = identities
        .iter()
        .map(|identity| ((identity.kind.as_str(), identity.slug.as_str()), identity))
        .collect::<BTreeMap<_, _>>();
    let redirects = db
        .prepare("SELECT kind, from_slug, to_slug FROM served_redirects ORDER BY kind, from_slug, to_slug")?
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut violations = Vec::new();
    let mut targets = BTreeMap::new();
    for (kind, from, to) in &redirects {
        if let Some(previous) = targets.insert((kind.as_str(), from.as_str()), to.as_str()) {
            violations.push(format!("duplicate served redirect {kind} {from:?}: to {previous:?} and {to:?}"));
        }
    }
    let mut conflicting_entities = Vec::new();
    for (kind, from, to) in &redirects {
        if !from.is_empty() && from.bytes().all(|byte| byte.is_ascii_digit()) {
            violations.push(format!("redirect {kind} {from:?} -> {to:?}: from must not be all digits"));
        }
        let Some(entity_kind) = EntityKind::parse(kind) else {
            violations.push(format!("redirect {kind} {from:?} -> {to:?}: unknown kind"));
            continue;
        };
        if !current.contains_key(&(entity_kind.table_name(), to.as_str())) {
            let retarget = targets
                .get(&(kind.as_str(), to.as_str()))
                .map_or(String::new(), |next| format!("; {to:?} redirects to {next:?}; retarget to {next:?}"));
            violations.push(format!("redirect {kind} {from:?} -> {to:?}: target is not current{retarget}"));
        }
        if let Some(identity) = current.get(&(entity_kind.table_name(), from.as_str())) {
            conflicting_entities.push((format!("redirect {kind} {from:?} -> {to:?}: from is current"), *identity));
        }
    }
    let authored = RedirectsFile::from_rows(db)?;
    for removed in &authored.removed {
        let kind = removed.kind.as_str();
        if !removed.slug.is_empty() && removed.slug.bytes().all(|byte| byte.is_ascii_digit()) {
            violations
                .push(format!("removed {kind} {:?} ({:?}): slug must not be all digits", removed.slug, removed.reason));
        }
        if let Some(identity) = current.get(&(removed.kind.table_name(), removed.slug.as_str())) {
            conflicting_entities
                .push((format!("removed {kind} {:?} ({:?}) is current", removed.slug, removed.reason), *identity));
        }
        if let Some(to) = targets.get(&(kind, removed.slug.as_str())) {
            violations.push(format!(
                "{kind} {:?} is both redirected to {to:?} and removed ({:?})",
                removed.slug, removed.reason
            ));
        }
    }
    if !conflicting_entities.is_empty() {
        let paths = super::DataFilePaths::from_database(db)?;
        for (message, identity) in conflicting_entities {
            let path = paths.path(&identity.kind, identity.entity_id)?;
            violations.push(format!("{message}: {:?} in {}", identity.name, path.display()));
        }
    }
    Ok(violations)
}

fn redirect_check_date(db: &Connection, today: Option<&str>) -> Result<String> {
    let date: String = db.query_row("SELECT COALESCE(?1, date('now'))", [today], |row| row.get(0))?;
    let valid: bool = db.query_row(
        "SELECT ?1 GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]' AND date(?1, '+0 days') IS ?1",
        [&date],
        |row| row.get(0),
    )?;
    ensure!(valid, "invalid date {date:?}; expected YYYY-MM-DD");
    Ok(date)
}

pub fn redirect_expiry_violations(db: &Connection, today: Option<&str>) -> Result<Vec<String>> {
    let today = redirect_check_date(db, today)?;
    let mut violations = Vec::new();
    for (table, column) in [("redirects", "from_slug"), ("removed_slugs", "slug")] {
        let mut select = db.prepare(&format!("SELECT kind, {column}, since, since > ?1 FROM {table} WHERE {EXPIRED_SINCE} OR since > ?1 ORDER BY kind, {column}"))?;
        for row in select.query_map([&today], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, bool>(3)?))
        })? {
            let (kind, slug, since, future) = row?;
            let reason = if future { "is in the future; correct the date" } else { "is older than 30 days" };
            violations.push(format!("{table} {kind} {slug:?}: since {since} {reason} (today {today}); prune expired rows with `{PRUNE_REDIRECTS_COMMAND}`"));
        }
    }
    Ok(violations)
}

pub fn prune_redirects(root: &Path, today: Option<&str>) -> Result<usize> {
    let path = root.join("redirects.toml");
    let contents = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let super::schemas::SourceFile::Redirects(file) = super::schemas::read_file(root, &path, &contents)? else {
        anyhow::bail!("{} is not a redirects file", path.display());
    };
    let db = Connection::open_in_memory()?;
    db.execute_batch(ddo_model::ddl())?;
    file.write_redirects(&db)?;
    let today = redirect_check_date(&db, today)?;
    let mut pruned = 0;
    for table in ["redirects", "removed_slugs"] {
        pruned += db.execute(&format!("DELETE FROM {table} WHERE {EXPIRED_SINCE}"), [&today])?;
    }
    let file = RedirectsFile::from_rows(&db)?;
    super::write_data_file(&path, &toml::to_string_pretty(&file)?)?;
    Ok(pruned)
}
