use anyhow::{ensure, Result};
use rusqlite::{params, Connection};

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
    let matches = query
        .query_map(params![name, system_id, variant, tier], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, String>(2)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
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
    let matches = query
        .query_map(params![name, variant], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
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
    let has_socket: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM crafting_steps WHERE id=?1 AND slot_id IS NOT NULL)",
        [step_id],
        |row| row.get(0),
    )?;
    ensure!(has_socket, "recipe set option {name:?} in {set:?} requires a socketed step");
    let mut query = db.prepare_cached("SELECT o.id,s.variant FROM socket_options o JOIN socket_option_sets s ON s.id=o.set_id JOIN crafting_steps step ON step.slot_id=s.slot_id WHERE step.id=?1 AND s.name=?2 AND (?3 IS NULL OR s.variant=?3) AND o.name=?4")?;
    let matches = query
        .query_map(params![step_id, set, variant, name], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        matches.len() == 1,
        "recipe set option {name:?} in {set:?} matched {} rows on its step socket: {:?}",
        matches.len(),
        matches.iter().map(|(_, variant)| variant).collect::<Vec<_>>()
    );
    Ok(matches[0].0)
}
