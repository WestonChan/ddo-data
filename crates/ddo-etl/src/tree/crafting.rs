use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

fn prefer_unqualified<T>(matches: &mut Vec<T>, variant: Option<&str>, row_variant: impl Fn(&T) -> Option<&str>) {
    if variant.is_none() && matches.iter().any(|row| row_variant(row).is_none()) {
        matches.retain(|row| row_variant(row).is_none());
    }
}

pub(crate) fn socket_option_set_slug(label: &str, name: &str, variant: Option<&str>) -> String {
    format!("{}-{}", crate::tree::slug(label), crate::tree::qualified_slug(name, variant))
}

pub(crate) fn socket_option_set_id(db: &Connection, label: &str, name: &str, variant: Option<&str>) -> Result<i64> {
    optional_socket_option_set_id(db, label, name, variant)?
        .with_context(|| format!("socket option set {name:?} on {label:?} matched 0 rows"))
}

fn optional_socket_option_set_id(
    db: &Connection,
    label: &str,
    name: &str,
    variant: Option<&str>,
) -> Result<Option<i64>> {
    let mut query = db.prepare_cached("SELECT s.id,s.variant FROM socket_option_sets s JOIN augment_slot_types l ON l.id=s.slot_id WHERE l.label=?1 AND s.name=?2 AND (?3 IS NULL OR s.variant=?3)")?;
    let mut matches = query
        .query_map(params![label, name, variant], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    prefer_unqualified(&mut matches, variant, |row| row.1.as_deref());
    ensure!(
        matches.len() <= 1,
        "socket option set {name:?} on {label:?} matched {} rows: {:?}; specify set_variant when ambiguous",
        matches.len(),
        matches.iter().map(|(_, variant)| variant).collect::<Vec<_>>()
    );
    Ok(matches.first().map(|row| row.0))
}

pub(crate) fn recipe_focus_path(note: &str) -> Option<String> {
    let word_after = |label: &str| {
        note.split_once(label)
            .and_then(|(_, remainder)| remainder.split(|character: char| !character.is_ascii_alphabetic()).next())
            .filter(|word| !word.is_empty())
    };
    if let (Some(invasion), Some(subjugation)) = (word_after("Invasion focus of "), word_after("Subjugation focus of "))
    {
        return Some(format!("Invasion {invasion} / Subjugation {subjugation}"));
    }
    if let (Some(focus), Some(gem), Some(essence)) =
        (word_after("focus type "), word_after("gem type "), word_after("essence type "))
    {
        return Some(format!("{focus}:{gem}:{essence}"));
    }
    note.split_once("(page code ")
        .and_then(|(_, remainder)| remainder.split_once(')'))
        .map(|(page_code, _)| page_code.to_string())
}

pub(crate) fn ingredient_id(
    db: &Connection,
    system_id: Option<i64>,
    name: &str,
    variant: Option<&str>,
    tier: Option<&str>,
) -> Result<i64> {
    let mut query=db.prepare_cached("SELECT id,variant,tier FROM crafting_ingredients WHERE name=?1 AND (system_id=?2 OR system_id IS NULL) AND (?3 IS NULL OR variant=?3) AND (?4 IS NULL OR tier=?4)")?;
    let mut matches = query
        .query_map(params![name, system_id, variant, tier], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, String>(2)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    prefer_unqualified(&mut matches, variant, |row| row.1.as_deref());
    ensure!(
        matches.len() == 1,
        "ingredient {name:?} matched {} rows: {:?}; specify its variant or legacy tier when ambiguous",
        matches.len(),
        matches.iter().map(|(_, variant, tier)| (variant, tier)).collect::<Vec<_>>()
    );
    Ok(matches[0].0)
}

pub(crate) fn qualified_reference_id(
    db: &Connection,
    table: &'static str,
    name: &str,
    variant: Option<&str>,
) -> Result<i64> {
    let mut query =
        db.prepare_cached(&format!("SELECT id,variant FROM {table} WHERE name=?1 AND (?2 IS NULL OR variant=?2)"))?;
    let mut matches = query
        .query_map(params![name, variant], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    prefer_unqualified(&mut matches, variant, |row| row.1.as_deref());
    ensure!(
        matches.len() == 1,
        "{table} {name:?} matched {} rows: {:?}; specify its variant when ambiguous",
        matches.len(),
        matches.iter().map(|(_, variant)| variant).collect::<Vec<_>>()
    );
    Ok(matches[0].0)
}

pub(crate) fn cost_references(
    db: &Connection,
    system_id: i64,
    ingredient: Option<&str>,
    item: Option<&str>,
    variant: Option<&str>,
    tier: Option<&str>,
) -> Result<(Option<i64>, Option<i64>)> {
    ensure!(ingredient.is_some() != item.is_some(), "a cost row must name exactly one ingredient or item");
    Ok(match (ingredient, item) {
        (Some(name), None) => (Some(ingredient_id(db, Some(system_id), name, variant, tier)?), None),
        (None, Some(name)) => (None, Some(qualified_reference_id(db, "items", name, variant)?)),
        _ => unreachable!("validated cost reference"),
    })
}

pub(crate) fn recipe_set_option_id(
    db: &Connection,
    step_id: Option<i64>,
    set: &str,
    variant: Option<&str>,
    name: &str,
) -> Result<i64> {
    recipe_set_option_id_if_set_exists(db, step_id, set, variant, name)?
        .with_context(|| format!("socket option set {set:?} matched 0 rows on its step socket"))
}

pub(crate) fn recipe_set_option_id_if_set_exists(
    db: &Connection,
    step_id: Option<i64>,
    set: &str,
    variant: Option<&str>,
    name: &str,
) -> Result<Option<i64>> {
    let label: Option<String> = db
        .query_row(
            "SELECT l.label FROM crafting_steps step JOIN augment_slot_types l ON l.id=step.slot_id WHERE step.id=?1",
            [step_id],
            |row| row.get(0),
        )
        .optional()?;
    ensure!(label.is_some(), "recipe set option {name:?} in {set:?} requires a socketed step");
    let Some(set_id) = optional_socket_option_set_id(db, label.as_deref().unwrap(), set, variant)? else {
        return Ok(None);
    };
    let mut query = db.prepare_cached("SELECT id FROM socket_options WHERE set_id=?1 AND name=?2")?;
    let matches =
        query.query_map(params![set_id, name], |row| row.get::<_, i64>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        matches.len() == 1,
        "recipe set option {name:?} in {set:?} matched {} rows on its step socket: {:?}",
        matches.len(),
        variant
    );
    Ok(Some(matches[0]))
}
