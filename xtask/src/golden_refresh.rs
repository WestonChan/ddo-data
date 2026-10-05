use anyhow::{bail, Context, Result};
use rusqlite::Connection;
use serde_json::json;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
struct ItemCandidate {
    id: i64,
    name: String,
    minimum_level: i64,
    is_legacy: bool,
    read_day: Option<i64>,
    effects: Vec<String>,
}

fn select_items(
    candidates: Vec<ItemCandidate>,
    mut coverage: BTreeMap<String, usize>,
    today: i64,
    fresh_days: i64,
    count: usize,
) -> Vec<ItemCandidate> {
    let mut eligible: Vec<_> = candidates
        .into_iter()
        .filter(|candidate| {
            candidate.minimum_level >= 29
                && !candidate.is_legacy
                && candidate.read_day.is_none_or(|read_day| today - read_day >= fresh_days)
        })
        .collect();
    let mut selected = Vec::new();
    while !eligible.is_empty() && selected.len() < count {
        eligible.sort_by(|left, right| {
            let rarity = |candidate: &ItemCandidate| {
                candidate
                    .effects
                    .iter()
                    .map(|effect| coverage.get(effect).copied().unwrap_or_default())
                    .min()
                    .unwrap_or(usize::MAX)
            };
            rarity(left)
                .cmp(&rarity(right))
                .then_with(|| left.read_day.cmp(&right.read_day))
                .then_with(|| left.name.cmp(&right.name))
        });
        let candidate = eligible.remove(0);
        for effect in &candidate.effects {
            *coverage.entry(effect.clone()).or_default() += 1;
        }
        selected.push(candidate);
    }
    selected
}

const REASON_CODES: &[&str] = &[
    "value_in_wiki_name",
    "set_line_form",
    "source_wording",
    "source_line_style",
    "wiki_may_be_stale",
    "nonstandard_tooltip",
    "value_disagreement",
    "folds",
    "grouping",
];

pub fn merge_golden(mut current: Value, incoming: Value) -> Result<Value> {
    let incoming_object = incoming.as_object().context("golden input must be an object")?;
    if incoming_object.keys().any(|key| !matches!(key.as_str(), "items" | "augments" | "sets" | "not_found")) {
        bail!("golden input accepts only items, augments, sets and not_found");
    }
    if ["items", "augments", "sets"].iter().all(|collection| {
        incoming_object.get(*collection).is_none_or(|value| value.as_array().is_some_and(Vec::is_empty))
    }) {
        bail!("golden input contains no owners");
    }
    for collection in ["items", "augments", "sets"] {
        let name_key = match collection {
            "items" => "item",
            "augments" => "augment",
            _ => "set",
        };
        let old_entries = current[collection].as_array_mut().context("existing golden owner list")?;
        let new_entries = incoming_object
            .get(collection)
            .map(|value| value.as_array().context("incoming golden owner list"))
            .transpose()?;
        let Some(new_entries) = new_entries else { continue };
        let mut seen = std::collections::BTreeSet::new();
        for entry in new_entries {
            validate_entry(entry, collection, name_key)?;
            let name = entry[name_key].as_str().context("golden owner name")?;
            if !seen.insert(name) {
                bail!("duplicate {collection} owner {name:?} in merge input");
            }
            if let Some(index) = old_entries.iter().position(|old| old[name_key] == name) {
                let old_day = date_day(old_entries[index]["read"].as_str().context("old read date")?)?;
                let new_day = date_day(entry["read"].as_str().context("new read date")?)?;
                if new_day < old_day {
                    bail!("{collection} {name:?} was read before the existing entry");
                }
                old_entries[index] = entry.clone();
            } else {
                old_entries.push(entry.clone());
            }
        }
    }
    Ok(current)
}

pub fn golden_merge_changes(current: &Value, incoming: &Value) -> Result<Vec<String>> {
    let mut changes = Vec::new();
    for (collection, name_key) in [("items", "item"), ("augments", "augment"), ("sets", "set")] {
        let Some(new_entries) = incoming.get(collection) else { continue };
        for entry in new_entries.as_array().context("incoming golden owner list")? {
            let name = entry[name_key].as_str().context("incoming golden owner name")?;
            let old = current[collection]
                .as_array()
                .context("existing golden owner list")?
                .iter()
                .find(|old| old[name_key] == name);
            let change = match old {
                Some(old) if old == entry => format!("{collection} {name}: unchanged"),
                Some(old) => format!(
                    "{collection} {name}: read {} -> {}, {} -> {} lines, {} -> {} recorded differences",
                    old["read"].as_str().unwrap_or("?"),
                    entry["read"].as_str().unwrap_or("?"),
                    golden_line_count(old, collection),
                    golden_line_count(entry, collection),
                    known_difference_count(old, collection),
                    known_difference_count(entry, collection)
                ),
                None => format!("{collection} {name}: added {} lines", golden_line_count(entry, collection)),
            };
            changes.push(change);
        }
    }
    Ok(changes)
}

fn golden_lines<'owner>(owner: &'owner Value, collection: &str) -> Vec<&'owner Value> {
    if collection == "sets" {
        owner["tiers"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|tier| tier["lines"].as_array().into_iter().flatten())
            .collect()
    } else {
        owner["lines"].as_array().into_iter().flatten().collect()
    }
}

fn golden_line_count(owner: &Value, collection: &str) -> usize {
    golden_lines(owner, collection).len()
}

fn known_difference_count(owner: &Value, collection: &str) -> usize {
    golden_lines(owner, collection)
        .iter()
        .map(|line| line["known_differences"].as_object().map_or(0, |differences| differences.len()))
        .sum()
}

fn validate_entry(entry: &Value, collection: &str, name_key: &str) -> Result<()> {
    let name = entry[name_key].as_str().filter(|name| !name.is_empty()).context("owner name")?;
    let page = entry["page"].as_str().context("owner page")?;
    if !page.starts_with("https://ddowiki.com/page/") {
        bail!("{collection} {name:?}: invalid wiki page {page:?}");
    }
    date_day(entry["read"].as_str().context("owner read date")?)?;
    let lines = if collection == "sets" {
        entry["tiers"]
            .as_array()
            .context("golden set tiers")?
            .iter()
            .map(|tier| {
                tier["pieces"].as_i64().context("set tier pieces")?;
                tier["lines"].as_array().context("set tier lines")
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
    } else {
        entry["lines"].as_array().context("golden owner lines")?.iter().collect()
    };
    for line in lines {
        let kind = line["kind"].as_str().context("golden line kind")?;
        if !matches!(
            kind,
            "enchantment"
                | "set_tier_line"
                | "enhancement_bonus"
                | "augment_slot"
                | "choice_option"
                | "rare_roll"
                | "set_bonus"
                | "sub_effect"
                | "upgrade"
                | "spell_charges"
                | "ours_only"
                | "choice_header"
        ) {
            bail!("{collection} {name:?}: unknown golden line kind {kind:?}");
        }
        if matches!(kind, "enchantment" | "set_tier_line" | "enhancement_bonus") || !line["wiki_text"].is_null() {
            line["wiki_text"].as_str().context("golden wiki text")?;
        }
        if !line["wiki_tooltip"].is_null() {
            line["wiki_tooltip"].as_str().context("golden wiki tooltip")?;
        }
        if !line["tooltip_is_standard"].is_null() {
            line["tooltip_is_standard"].as_bool().context("golden tooltip_is_standard")?;
        }
        if line["tooltip_is_standard"] == true && line["wiki_tooltip"].is_null() {
            bail!("{collection} {name:?}: standard tooltip needs its text");
        }
        if !line["ours_line"].is_null() {
            line["ours"].as_str().context("matched effect name")?;
            line["ours_line"]["index"].as_u64().context("matched effect line index")?;
            line["ours_line"]["template"].as_str().context("matched effect template")?;
        }
        if let Some(differences) = line.get("known_differences").filter(|value| !value.is_null()) {
            for (field, difference) in differences.as_object().context("known differences object")? {
                if !matches!(field.as_str(), "name" | "verbose_name" | "description" | "enhancement_bonus") {
                    bail!("{collection} {name:?}: unknown difference field {field:?}");
                }
                let code = difference["code"].as_str().context("known difference code")?;
                if !REASON_CODES.contains(&code) {
                    bail!("{collection} {name:?}: unknown difference code {code:?}");
                }
                difference.get("served").context("known difference served value")?;
            }
        }
    }
    Ok(())
}

fn read_age_days(date: &str, today: i64) -> Result<i64> {
    Ok(today - date_day(date)?)
}

fn date_day(date: &str) -> Result<i64> {
    let parts = date.split('-').collect::<Vec<_>>();
    if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        bail!("invalid read date {date:?}");
    }
    let year: i64 = parts[0].parse()?;
    let month: usize = parts[1].parse()?;
    let day: i64 = parts[2].parse()?;
    if !(1970..=2200).contains(&year) || !(1..=12).contains(&month) {
        bail!("invalid read date {date:?}");
    }
    let leap = |year: i64| year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let month_days = [31, if leap(year) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if day < 1 || day > month_days[month - 1] {
        bail!("invalid read date {date:?}");
    }
    let year_days: i64 = (1970..year).map(|prior| if leap(prior) { 366 } else { 365 }).sum();
    Ok(year_days + month_days[..month - 1].iter().sum::<i64>() + day - 1)
}

pub fn golden_age_report(corpus: &Value, today: i64) -> Result<String> {
    let mut age_bands = [0_usize; 3];
    let mut stale_effects = BTreeSet::new();
    for collection in ["items", "augments", "sets"] {
        for owner in corpus[collection].as_array().context("golden owner list")? {
            let age = read_age_days(owner["read"].as_str().context("golden read date")?, today)?;
            age_bands[if age <= 90 {
                0
            } else if age <= 180 {
                1
            } else {
                2
            }] += 1;
            if age > 180 {
                let lines = if collection == "sets" {
                    owner["tiers"]
                        .as_array()
                        .context("golden set tiers")?
                        .iter()
                        .flat_map(|tier| tier["lines"].as_array().into_iter().flatten())
                        .collect::<Vec<_>>()
                } else {
                    owner["lines"].as_array().context("golden owner lines")?.iter().collect()
                };
                for line in lines {
                    if let Some(effect) = line["ours"].as_str() {
                        stale_effects.insert(effect.to_string());
                    }
                }
            }
        }
    }
    Ok(format!(
        "read ages: 0-90 days {}, 91-180 days {}, over 180 days {}; effects needing a re-read: {}",
        age_bands[0],
        age_bands[1],
        age_bands[2],
        if stale_effects.is_empty() {
            "none".to_string()
        } else {
            stale_effects.into_iter().collect::<Vec<_>>().join(", ")
        }
    ))
}

pub fn today_day() -> Result<i64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64 / 86_400)
}

pub struct SampleRequest {
    pub count: usize,
    pub fresh_days: i64,
    pub items: Vec<String>,
    pub augments: Vec<String>,
    pub sets: Vec<String>,
    pub effects: Vec<String>,
    pub effect_likes: Vec<String>,
}

pub fn golden_sample(db_path: &Path, request: &SampleRequest) -> Result<Value> {
    if request.count == 0 || request.fresh_days < 0 {
        bail!("--count must be positive and --fresh-days nonnegative");
    }
    let db = Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let corpus: Value = serde_json::from_str(include_str!("../data/tooltip_golden.json"))?;
    let today = today_day()?;
    let mut coverage = BTreeMap::new();
    let mut read_days = BTreeMap::new();
    for owner in corpus["items"].as_array().context("golden items")? {
        let name = owner["item"].as_str().context("golden item name")?;
        read_days.insert(name.to_string(), date_day(owner["read"].as_str().context("golden item read date")?)?);
        for line in owner["lines"].as_array().context("golden item lines")? {
            if let Some(effect) = line["ours"].as_str() {
                *coverage.entry(effect.to_string()).or_default() += 1;
            }
        }
    }
    let mut candidates = Vec::new();
    let mut item_metadata = BTreeMap::new();
    let item_effect_names = effect_names_by_owner(&db, "item_effects", "item_id")?;
    let mut statement = db.prepare("SELECT id, name, minimum_level, is_legacy FROM items ORDER BY name")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<i64>>(2)?, row.get::<_, i64>(3)? != 0))
    })?;
    for row in rows {
        let (id, name, minimum_level, is_legacy) = row?;
        let effects = item_effect_names.get(&id).cloned().unwrap_or_default();
        item_metadata.insert(id, (name.clone(), minimum_level));
        candidates.push(ItemCandidate {
            id,
            read_day: read_days.get(&name).copied(),
            name,
            minimum_level: minimum_level.unwrap_or_default(),
            is_legacy,
            effects,
        });
    }
    let has_effect_filter = !request.effects.is_empty() || !request.effect_likes.is_empty();
    let mut selected = BTreeSet::new();
    for name in &request.items {
        let candidate = candidates
            .iter()
            .find(|candidate| &candidate.name == name)
            .with_context(|| format!("unknown item {name:?}"))?;
        selected.insert(candidate.id);
    }
    let selected_candidates = if has_effect_filter {
        let matching_effects = matching_effect_names(&db, &request.effects, &request.effect_likes)?;
        let filtered = candidates
            .into_iter()
            .filter(|candidate| candidate.effects.iter().any(|effect| matching_effects.contains(effect)))
            .collect();
        select_items(filtered, coverage, today, 0, request.count)
    } else if request.items.is_empty() && request.augments.is_empty() && request.sets.is_empty() {
        select_items(candidates, coverage, today, request.fresh_days, request.count)
    } else {
        Vec::new()
    };
    for candidate in selected_candidates {
        selected.insert(candidate.id);
    }
    let mut items = Vec::new();
    for id in selected {
        let (name, minimum_level) = item_metadata.get(&id).context("selected item metadata")?;
        items.push(json!({"item_id": id, "item": name, "minimum_level": minimum_level, "lines": effect_lines(&db, "item_effects", "item_id", id)?}));
    }
    let matching_effects = matching_effect_names(&db, &request.effects, &request.effect_likes)?;
    let mut augments = Vec::new();
    let augment_effect_names = effect_names_by_owner(&db, "augment_effects", "augment_id")?;
    let mut augment_statement =
        db.prepare("SELECT id, name, COALESCE(min_level, 0) FROM augments ORDER BY name, id")?;
    for row in augment_statement
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?)))?
    {
        let (id, name, min_level) = row?;
        let requested = request.augments.iter().any(|requested| requested == &name);
        let matches = has_effect_filter
            && min_level >= 29
            && augment_effect_names
                .get(&id)
                .is_some_and(|names| names.iter().any(|effect| matching_effects.contains(effect)));
        if requested || (matches && items.len() + augments.len() < request.count) {
            augments.push(json!({"augment": name, "lines": effect_lines(&db, "augment_effects", "augment_id", id)?}));
        }
    }
    for name in &request.augments {
        if !augments.iter().any(|entry| entry["augment"] == *name) {
            bail!("unknown augment {name:?}");
        }
    }
    let mut sets = Vec::new();
    let tier_effect_names = effect_names_by_owner(&db, "set_bonus_tier_effects", "tier_id")?;
    let mut modern_sets = BTreeSet::new();
    let mut modern_set_statement = db.prepare("SELECT DISTINCT sbi.set_id FROM set_bonus_items sbi JOIN items i ON i.id = sbi.item_id WHERE i.minimum_level >= 29 AND i.is_legacy = 0")?;
    for row in modern_set_statement.query_map([], |row| row.get::<_, i64>(0))? {
        modern_sets.insert(row?);
    }
    let mut tiers_by_set: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    let mut tiers_statement = db.prepare("SELECT set_id, id FROM set_bonus_tiers ORDER BY set_id, equipped_count")?;
    for row in tiers_statement.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))? {
        let (set_id, tier_id) = row?;
        tiers_by_set.entry(set_id).or_default().push(tier_id);
    }
    let mut set_statement = db.prepare("SELECT id, name FROM set_bonuses ORDER BY name")?;
    for row in set_statement.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))? {
        let (id, name) = row?;
        let requested = request.sets.iter().any(|requested| requested == &name);
        let matches = tiers_by_set.get(&id).is_some_and(|tier_ids| {
            tier_ids.iter().any(|tier_id| {
                tier_effect_names
                    .get(tier_id)
                    .is_some_and(|names| names.iter().any(|effect| matching_effects.contains(effect)))
            })
        });
        if requested
            || (has_effect_filter
                && modern_sets.contains(&id)
                && matches
                && items.len() + augments.len() + sets.len() < request.count)
        {
            sets.push(set_sample(&db, id, &name)?);
        }
    }
    for name in &request.sets {
        if !sets.iter().any(|entry| entry["set"] == *name) {
            bail!("unknown set {name:?}");
        }
    }
    Ok(json!({"items": items, "augments": augments, "sets": sets}))
}

fn matching_effect_names(db: &Connection, names: &[String], patterns: &[String]) -> Result<BTreeSet<String>> {
    let mut matching = BTreeSet::new();
    for name in names {
        let exists: bool =
            db.query_row("SELECT EXISTS(SELECT 1 FROM effects WHERE name = ?1)", [name], |row| row.get(0))?;
        if !exists {
            bail!("unknown effect {name:?}");
        }
        matching.insert(name.clone());
    }
    for pattern in patterns {
        let mut statement = db.prepare("SELECT name FROM effects WHERE name LIKE ?1 ORDER BY name")?;
        for row in statement.query_map([pattern], |row| row.get::<_, String>(0))? {
            matching.insert(row?);
        }
    }
    Ok(matching)
}

fn effect_names_by_owner(db: &Connection, link_table: &str, owner_column: &str) -> Result<BTreeMap<i64, Vec<String>>> {
    let mut statement = db.prepare(&format!("SELECT link.{owner_column}, e.name FROM {link_table} link JOIN effects e ON e.id = link.effect_id ORDER BY link.{owner_column}, link.sort_order"))?;
    let mut names = BTreeMap::new();
    for row in statement.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))? {
        let (id, name) = row?;
        names.entry(id).or_insert_with(Vec::new).push(name);
    }
    Ok(names)
}

fn effect_lines(db: &Connection, link_table: &str, owner_column: &str, id: i64) -> Result<Vec<Value>> {
    let mut statement = db.prepare(&format!("SELECT e.name, e.verbose_name_template, bt.name, link.value, link.value2 FROM {link_table} link JOIN effects e ON e.id = link.effect_id LEFT JOIN bonus_types bt ON bt.id = link.bonus_type_id WHERE link.{owner_column} = ?1 ORDER BY link.sort_order"))?;
    let lines = statement.query_map([id], |row| Ok(json!({"effect": row.get::<_, String>(0)?, "template": row.get::<_, Option<String>>(1)?, "type": row.get::<_, Option<String>>(2)?, "value": row.get::<_, Option<i64>>(3)?, "value2": row.get::<_, Option<i64>>(4)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(lines)
}

fn set_sample(db: &Connection, id: i64, name: &str) -> Result<Value> {
    let mut tiers = Vec::new();
    let mut statement =
        db.prepare("SELECT id, equipped_count FROM set_bonus_tiers WHERE set_id = ?1 ORDER BY equipped_count")?;
    for row in statement.query_map([id], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))? {
        let (tier_id, pieces) = row?;
        tiers.push(json!({"pieces": pieces, "lines": effect_lines(db, "set_bonus_tier_effects", "tier_id", tier_id)?}));
    }
    Ok(json!({"set": name, "tiers": tiers}))
}

#[cfg(test)]
mod tests {
    use super::{golden_age_report, merge_golden, read_age_days, select_items, ItemCandidate};
    use serde_json::json;
    use std::collections::BTreeMap;

    fn item(id: i64, minimum_level: i64, read_day: Option<i64>, effect: &str) -> ItemCandidate {
        ItemCandidate {
            id,
            name: format!("Item {id}"),
            minimum_level,
            is_legacy: false,
            read_day,
            effects: vec![effect.to_string()],
        }
    }

    #[test]
    fn sample_prefers_modern_uncovered_lines_and_skips_recent_reads() {
        let mut legacy = item(5, 32, None, "Rare");
        legacy.is_legacy = true;
        let candidates = vec![
            item(1, 28, None, "Rare"),
            item(2, 32, Some(19500), "Rare"),
            item(3, 32, None, "Common"),
            item(4, 32, None, "Rare"),
            legacy,
        ];
        let selected = select_items(candidates, BTreeMap::from([("Common".to_string(), 9)]), 19530, 90, 2);
        assert_eq!(selected.iter().map(|candidate| candidate.id).collect::<Vec<_>>(), [4, 3]);
    }

    #[test]
    fn merging_a_reread_replaces_the_owner_and_rejects_unknown_reasons() {
        let line = json!({"kind":"enchantment", "wiki_text":"Sharp +2", "wiki_tooltip":"Sharp +2: Passive.", "tooltip_is_standard":true, "ours":"Sharp", "ours_line":{"index":0,"template":"Sharp {1}","type":"Equipment","value":2,"value2":null}});
        let mut old_line = line.clone();
        old_line["known_differences"] = json!({"description":{"code":"source_wording","served":"Different prose."}});
        let current = json!({"items": [{"item": "Sword", "read": "2026-01-01", "page": "https://ddowiki.com/page/Item:Sword", "lines": [old_line]}], "augments": [], "sets": []});
        let reread = json!({"items": [{"item": "Sword", "read": "2026-10-05", "page": "https://ddowiki.com/page/Item:Sword", "lines": [line]}], "augments": [], "sets": []});
        let merged = merge_golden(current, reread).unwrap();
        assert_eq!(merged["items"].as_array().unwrap().len(), 1);
        assert_eq!(merged["items"][0]["read"], "2026-10-05");
        assert!(merged["items"][0]["lines"][0]["known_differences"].is_null());
        let invalid = json!({"items": [{"item": "Sword", "read": "2026-10-05", "page": "https://ddowiki.com/page/Item:Sword", "lines": [{"kind":"enchantment", "wiki_text":"Sharp +2", "wiki_tooltip":"Sharp +2: Passive.", "tooltip_is_standard":true, "known_differences": {"name": {"code": "invented", "served": "Sword"}}}]}]});
        assert!(merge_golden(merged, invalid).is_err());
    }

    #[test]
    fn existing_golden_corpus_is_a_valid_merge_input() {
        let corpus: serde_json::Value = serde_json::from_str(include_str!("../data/tooltip_golden.json")).unwrap();
        assert_eq!(merge_golden(corpus.clone(), corpus.clone()).unwrap(), corpus);
    }

    #[test]
    fn staleness_uses_calendar_dates() {
        assert_eq!(read_age_days("2026-10-05", 20731).unwrap(), 0);
        assert_eq!(read_age_days("2026-10-04", 20731).unwrap(), 1);
        assert!(read_age_days("2026-02-30", 20731).is_err());
        let corpus = json!({
            "items": [{"read": "2026-10-05", "lines": [{"ours": "Fresh"}]}, {"read": "2026-01-01", "lines": [{"ours": "DR 30/Good"}]}],
            "augments": [], "sets": []
        });
        let report = golden_age_report(&corpus, 20731).unwrap();
        assert!(report.contains("0-90 days 1"));
        assert!(report.contains("over 180 days 1"));
        assert!(report.contains("DR 30/Good"));
    }
}
