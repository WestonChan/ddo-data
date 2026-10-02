use super::quest_series::{quest_chains_including, sagas_including};
use crate::db::{convert_to_booleans, json_row, json_rows, whole_table_json};
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(adventure_packs))
        .routes(routes!(adventure_pack_detail))
        .routes(routes!(patrons))
        .routes(routes!(quests))
        .routes(routes!(quest_detail))
}

const QUEST_SELECT: &str =
    "SELECT q.id, q.name, p.name AS pack, pt.name AS patron, q.level, q.epic_level, q.favor, q.is_raid,
        q.epic_name, q.difficulties, q.is_challenge, q.max_level, q.is_free_to_play,
        q.legendary_level, q.zone, q.bestowed_by, q.flagging, q.provenance
   FROM quests q LEFT JOIN adventure_packs p ON p.id = q.pack_id LEFT JOIN patrons pt ON pt.id = q.patron_id";
const QUEST_FLAG_COLUMNS: &[&str] = &["is_raid", "is_challenge", "is_free_to_play"];

#[utoipa::path(
    get,
    path = "/v1/adventure-packs",
    tag = "quests",
    summary = "List adventure packs",
    description = "Every adventure pack and expansion by name with whether it is free to play. /v1/items accepts \
                   these names in `pack`; /v1/adventure-packs/{id} adds the loot credited to the whole pack.",
    responses((status = 200, description = "The whole table", body = Vec<Value>))
)]
async fn adventure_packs(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    whole_table_json(state, "SELECT id, name, is_free_to_play FROM adventure_packs ORDER BY name", &["is_free_to_play"])
        .await
}

#[utoipa::path(
    get,
    path = "/v1/adventure-packs/{id}",
    tag = "quests",
    summary = "Get an adventure pack",
    description = "One adventure pack as the list returns it (`id`, `name`, `is_free_to_play`), plus the loot any \
                   of its quests drops, which Maetrim's drop text credits to the pack rather than to a quest \
                   (`Magic of Myth Drannor, any end chest`): `items`, each with its `minimum_level` and `slot`, and \
                   `augments`, each with its `family` and `min_level`. Every loot row carries `id`, `name`, \
                   `loot_type` (chest or reward), `is_rare` and `chest` (the chest his text names, lower-cased, null \
                   when it names none and on every `reward` row), the same link item and augment detail \
                   `adventure_packs` report from the other side. Both arrays are sorted by name, then loot type, \
                   and empty when nothing is credited to the pack as a whole; the loot of one quest of the pack is \
                   on /v1/quests/{id}.",
    params(("id" = i64, Path, description = "The adventure pack's numeric id from /v1/adventure-packs")),
    responses(
        (status = 200, description = "The adventure pack with the items and augments any of its quests drops", body = Value),
        (status = 404, description = "No adventure pack has this id", body = crate::error::ErrorBody)
    )
)]
async fn adventure_pack_detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut pack = json_row(db, "SELECT id, name, is_free_to_play FROM adventure_packs WHERE id = ?1", [id])?;
            convert_to_booleans(&mut pack, &["is_free_to_play"]);
            pack["items"] = Value::Array(loot_rows(
                db,
                "SELECT i.id, i.name, loot.loot_type, loot.is_rare, loot.chest, i.minimum_level, es.name AS slot
                   FROM sources loot JOIN items i ON i.id = loot.item_id
                   LEFT JOIN equipment_slots es ON es.id = i.slot_id
                  WHERE loot.pack_id = ?1 ORDER BY i.name, i.id, loot.loot_type",
                id,
            )?);
            pack["augments"] = Value::Array(loot_rows(
                db,
                "SELECT a.id, a.name, loot.loot_type, loot.is_rare, loot.chest, a.family, a.min_level
                   FROM sources loot JOIN augments a ON a.id = loot.augment_id
                  WHERE loot.pack_id = ?1 ORDER BY a.name, a.id, loot.loot_type",
                id,
            )?);
            Ok(Json(pack))
        })
        .await
}

#[utoipa::path(
    get,
    path = "/v1/patrons",
    tag = "quests",
    summary = "List patrons",
    description = "The favor patrons (The Coin Lords, House Kundarak, ...) that quests belong to.",
    responses((status = 200, description = "The whole table", body = Vec<Value>))
)]
async fn patrons(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    whole_table_json(state, "SELECT id, name FROM patrons ORDER BY name", &[]).await
}

#[utoipa::path(
    get,
    path = "/v1/quests",
    tag = "quests",
    summary = "List quests",
    description = "Every quest, adventure area and challenge with its pack, patron, heroic and epic levels, favor, \
                   whether it is a raid, the `epic_name` its epic version goes by when that differs (null \
                   otherwise), and the `difficulties` it offers as an array in the order casual, normal, hard, \
                   elite, reaper, solo. Challenges (the Cannith and Eveningstar challenge instances) have \
                   `is_challenge` true and a level range from `level` to `max_level`; regular quests have \
                   `max_level` null. Read from ddowiki, since Maetrim's files carry none of them: \
                   `is_free_to_play` (the quest itself, not its pack), `legendary_level`, `zone` (where it takes \
                   place), `bestowed_by` (the quest giver) and `flagging` (free text on what must be run first), \
                   each null or false when the wiki has not been read for that quest. `provenance` is `maetrim` for a quest \
                   from his files and `wiki` for one read from ddowiki because his files lack it, replaced by his \
                   as soon as his files carry a quest of that name. Item and augment detail responses reference these in `quests`; \
                   /v1/quests/{id} adds the items and augments each one drops.",
    responses((status = 200, description = "The whole table with each quest's wiki facts", body = Vec<Value>))
)]
async fn quests(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    let quests = state
        .read_db(|db| {
            let mut quests = json_rows(db, &format!("{QUEST_SELECT} ORDER BY q.name"), [])?;
            for quest in &mut quests {
                convert_to_booleans(quest, QUEST_FLAG_COLUMNS);
            }
            Ok(quests)
        })
        .await?;
    Ok(Json(quests))
}

#[utoipa::path(
    get,
    path = "/v1/quests/{id}",
    tag = "quests",
    summary = "Get a quest",
    description = "One quest as the list returns it, plus the loot it drops: `items`, each named item linked to the \
                   quest with its `minimum_level` and `slot`, and `augments`, each augment with its `family` and \
                   `min_level`. Every loot row carries `id`, `name`, `loot_type` (chest, raid or reward), `is_rare` \
                   and `chest` (the chest Maetrim's drop text names, lower-cased, null when it names none and on every `reward` row), the same \
                   link item and augment detail `quests` report from the other side. An item or augment that is both \
                   a chest (or raid) drop and an end reward of the quest appears once per loot type. Both arrays are \
                   sorted by name, then loot type, and empty when nothing is known to drop there. `quest_chains` and `sagas` \
                   name (`id`, `name`, and the ddowiki page each was read from as `wiki_url`) the quest chains and sagas the quest belongs to, whose end rewards come from \
                   their NPCs rather than from the quest; see /v1/quest-chains/{id} and /v1/sagas/{id}.",
    params(("id" = i64, Path, description = "The quest's numeric id from /v1/quests")),
    responses(
        (status = 200, description = "The quest with the items and augments it drops", body = Value),
        (status = 404, description = "No quest has this id", body = crate::error::ErrorBody)
    )
)]
async fn quest_detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut quest = json_row(db, &format!("{QUEST_SELECT} WHERE q.id = ?1"), [id])?;
            convert_to_booleans(&mut quest, QUEST_FLAG_COLUMNS);
            quest["items"] = Value::Array(loot_rows(
                db,
                "SELECT i.id, i.name, loot.loot_type, loot.is_rare, loot.chest, i.minimum_level, es.name AS slot
                   FROM sources loot JOIN items i ON i.id = loot.item_id
                   LEFT JOIN equipment_slots es ON es.id = i.slot_id
                  WHERE loot.quest_id = ?1 ORDER BY i.name, i.id, loot.loot_type",
                id,
            )?);
            quest["quest_chains"] = Value::Array(quest_chains_including(db, id)?);
            quest["sagas"] = Value::Array(sagas_including(db, id)?);
            quest["augments"] = Value::Array(loot_rows(
                db,
                "SELECT a.id, a.name, loot.loot_type, loot.is_rare, loot.chest, a.family, a.min_level
                   FROM sources loot JOIN augments a ON a.id = loot.augment_id
                  WHERE loot.quest_id = ?1 ORDER BY a.name, a.id, loot.loot_type",
                id,
            )?);
            Ok(Json(quest))
        })
        .await
}

fn loot_rows(db: &rusqlite::Connection, sql: &str, source_id: i64) -> Result<Vec<Value>, ApiError> {
    let mut loot_rows = json_rows(db, sql, [source_id])?;
    for loot_row in &mut loot_rows {
        convert_to_booleans(loot_row, &["is_rare"]);
    }
    Ok(loot_rows)
}

pub(super) fn quests_dropping_via(
    db: &rusqlite::Connection,
    loot_id_column: &str,
    loot_id: i64,
) -> Result<Vec<Value>, ApiError> {
    let mut quests = json_rows(
        db,
        &format!(
            "SELECT q.id, q.name, q.level, q.epic_level, q.is_raid, q.difficulties, q.is_free_to_play, q.provenance, ap.name AS pack,
                    pt.name AS patron, loot.loot_type, loot.is_rare, loot.chest
               FROM sources loot JOIN quests q ON q.id = loot.quest_id
               LEFT JOIN adventure_packs ap ON ap.id = q.pack_id LEFT JOIN patrons pt ON pt.id = q.patron_id
              WHERE loot.{loot_id_column} = ?1 ORDER BY q.name, loot.loot_type"
        ),
        [loot_id],
    )?;
    for quest in &mut quests {
        convert_to_booleans(quest, &["is_raid", "is_free_to_play", "is_rare"]);
    }
    Ok(quests)
}

pub(super) fn adventure_packs_dropping_via(
    db: &rusqlite::Connection,
    loot_id_column: &str,
    loot_id: i64,
) -> Result<Vec<Value>, ApiError> {
    let mut adventure_packs = json_rows(
        db,
        &format!(
            "SELECT p.id, p.name, loot.loot_type, loot.is_rare, loot.chest
               FROM sources loot JOIN adventure_packs p ON p.id = loot.pack_id
              WHERE loot.{loot_id_column} = ?1 ORDER BY p.name, loot.loot_type"
        ),
        [loot_id],
    )?;
    for adventure_pack in &mut adventure_packs {
        convert_to_booleans(adventure_pack, &["is_rare"]);
    }
    Ok(adventure_packs)
}

pub(super) fn sources_via(
    db: &rusqlite::Connection,
    loot_id_column: &str,
    loot_id: i64,
) -> Result<Vec<Value>, ApiError> {
    let mut sources = json_rows(
        db,
        &format!(
            "SELECT loot.kind, COALESCE(q.id, c.id, s.id, p.id, cs.id) AS id,
                    COALESCE(q.name, c.name, s.name, p.name, cs.name) AS name, loot.loot_type, loot.chest,
                    loot.is_rare, loot.tier, COALESCE(c.wiki_url, s.wiki_url, cs.page) AS wiki_url
               FROM sources loot LEFT JOIN quests q ON q.id = loot.quest_id
               LEFT JOIN quest_chains c ON c.id = loot.chain_id LEFT JOIN sagas s ON s.id = loot.saga_id
               LEFT JOIN adventure_packs p ON p.id = loot.pack_id
               LEFT JOIN crafting_systems cs ON cs.id = loot.crafting_system_id
              WHERE loot.{loot_id_column} = ?1
              ORDER BY CASE loot.kind WHEN 'quest' THEN 1 WHEN 'quest_chain' THEN 2 WHEN 'saga' THEN 3
                                      WHEN 'adventure_pack' THEN 4 WHEN 'crafting_system' THEN 5 ELSE 6 END,
                       name, loot.loot_type,
                       CASE loot.tier WHEN 'heroic' THEN 1 WHEN 'epic' THEN 2 WHEN 'legendary' THEN 3 ELSE 4 END"
        ),
        [loot_id],
    )?;
    for source in &mut sources {
        convert_to_booleans(source, &["is_rare"]);
        if source["wiki_url"].is_null() {
            if let Some(source_name) = source["name"].as_str() {
                source["wiki_url"] = Value::String(wiki_page_url(source_name));
            }
        }
    }
    Ok(sources)
}

fn wiki_page_url(page_name: &str) -> String {
    let encoded_page_name: String = page_name
        .replace(' ', "_")
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')' => {
                char::from(byte).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect();
    format!("https://ddowiki.com/page/{encoded_page_name}")
}
