use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub type SourcePackNames = BTreeMap<(String, i64), BTreeSet<String>>;

pub fn source_pack_names(db: &rusqlite::Connection) -> rusqlite::Result<SourcePackNames> {
    let mut source_packs = SourcePackNames::new();
    let mut statement = db.prepare(
        "SELECT DISTINCT s.kind, COALESCE(s.quest_id, s.chain_id, s.saga_id), p.name
         FROM loot_adventure_packs lp JOIN sources s ON s.id = lp.source_id
         JOIN adventure_packs p ON p.id = lp.pack_id
         WHERE s.kind IN ('quest', 'quest_chain', 'saga')",
    )?;
    let rows = statement
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?)))?;
    for row in rows {
        let (kind, id, pack_name) = row?;
        source_packs.entry((kind, id)).or_default().insert(pack_name);
    }
    Ok(source_packs)
}

pub fn detail_has_unique_drop_locations(
    detail: &Value,
    path: &str,
    source_packs: &SourcePackNames,
) -> Result<(), String> {
    for field in [
        "sources",
        "quests",
        "quest_chains",
        "sagas",
        "adventure_packs",
        "challenge_packs",
        "crafting_systems",
        "vendors",
        "events",
        "starter_rewards",
    ] {
        let Some(rows) = detail.get(field).and_then(Value::as_array) else {
            continue;
        };
        let mut seen = BTreeSet::new();
        for row in rows {
            if !seen.insert(row.to_string()) {
                return Err(format!("{path}.{field}: duplicate drop location {row}"));
            }
        }
    }
    let sources = detail["sources"].as_array().ok_or_else(|| format!("{path}: missing sources"))?;
    let packs = detail["adventure_packs"].as_array().ok_or_else(|| format!("{path}: missing adventure_packs"))?;
    let pack_sources: Vec<&Value> = sources.iter().filter(|source| source["kind"] == "adventure_pack").collect();
    if packs.len() != pack_sources.len() {
        return Err(format!(
            "{path}.adventure_packs: expected {} pack-wide drops, got {}",
            pack_sources.len(),
            packs.len()
        ));
    }
    for pack in packs {
        if !pack_sources.iter().any(|source| {
            ["id", "name", "loot_type", "chest", "is_rare"].iter().all(|field| pack[*field] == source[*field])
        }) {
            return Err(format!("{path}.adventure_packs: no pack-wide source for {pack}"));
        }
        for source in
            sources.iter().filter(|source| matches!(source["kind"].as_str(), Some("quest" | "quest_chain" | "saga")))
        {
            let Some(source_id) = source["id"].as_i64() else {
                continue;
            };
            let source_key = (source["kind"].as_str().unwrap().to_owned(), source_id);
            let same_pack =
                source_packs.get(&source_key).is_some_and(|names| names.contains(pack["name"].as_str().unwrap_or("")));
            if same_pack && source["chest"] == pack["chest"] {
                return Err(format!("{path}.adventure_packs: repeats a {} drop in {}", source["kind"], pack["name"]));
            }
        }
    }
    Ok(())
}
