use super::quest_series::{quest_chains_including, sagas_including};
use crate::db::paged_rows;
use crate::db::{convert_to_booleans, json_row, json_rows, paged_table_json, TableListSource};
use crate::error::ApiError;
use crate::query::{declare_list_parameters, ApiQuery};
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
        q.legendary_level, q.zone, q.bestowed_by, q.flagging
   FROM quests q LEFT JOIN adventure_packs p ON p.id = q.pack_id LEFT JOIN patrons pt ON pt.id = q.patron_id";
const QUEST_FLAG_COLUMNS: &[&str] = &["is_raid", "is_challenge", "is_free_to_play"];

const ADVENTURE_PACKS_SORT_FIELDS: &[(&str, &str)] =
    &[("name", "listed.name"), ("id", "listed.id"), ("is_free_to_play", "is_free_to_play")];

declare_list_parameters!(AdventurePacksParameters, ADVENTURE_PACKS_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/adventure-packs",
    tag = "quests",
    summary = "List adventure packs",
    description = "Lists adventure packs and free-to-play status.",
    params(
        AdventurePacksParameters,
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `adventure_packs` page", body = crate::routes::v1::response_schemas::AdventurePacksPageResponse),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn adventure_packs(State(state): State<AppState>, ApiQuery(query, _): ApiQuery) -> Result<Json<Value>, ApiError> {
    paged_table_json(
        state,
        query,
        TableListSource {
            select_sql: "SELECT id, name, is_free_to_play FROM adventure_packs",
            rows_key: "adventure_packs",
            name_column: "listed.name",
            default_order: "listed.name",
            sortable_fields: ADVENTURE_PACKS_SORT_FIELDS,
            flag_columns: &["is_free_to_play"],
        },
    )
    .await
}

#[utoipa::path(
    get,
    path = "/v1/adventure-packs/{id}",
    tag = "quests",
    summary = "Get an adventure pack",
    description = "Returns an adventure pack with its quests, challenges and loot.",
    params(("id" = i64, Path, description = "The adventure pack's numeric id from /v1/adventure-packs")),
    responses(
        (status = 200, description = "The adventure pack with the items and augments any of its quests drops", body = crate::routes::v1::response_schemas::AdventurePacksDetailResponse),
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
                  WHERE loot.kind = 'adventure_pack' AND loot.pack_id = ?1 ORDER BY i.name, i.id, loot.loot_type",
                id,
            )?);
            pack["augments"] = Value::Array(loot_rows(
                db,
                "SELECT a.id, a.name, loot.loot_type, loot.is_rare, loot.chest, a.family, a.min_level
                   FROM sources loot JOIN augments a ON a.id = loot.augment_id
                  WHERE loot.kind = 'adventure_pack' AND loot.pack_id = ?1 ORDER BY a.name, a.id, loot.loot_type",
                id,
            )?);
            Ok(Json(pack))
        })
        .await
}

const PATRONS_SORT_FIELDS: &[(&str, &str)] = &[("name", "listed.name"), ("id", "listed.id")];

declare_list_parameters!(PatronsParameters, PATRONS_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/patrons",
    tag = "quests",
    summary = "List patrons",
    description = "Lists patrons that award favor for quests.",
    params(
        PatronsParameters,
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `patrons` page", body = crate::routes::v1::response_schemas::PatronsPageResponse),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn patrons(State(state): State<AppState>, ApiQuery(query, _): ApiQuery) -> Result<Json<Value>, ApiError> {
    paged_table_json(
        state,
        query,
        TableListSource {
            select_sql: "SELECT id, name FROM patrons",
            rows_key: "patrons",
            name_column: "listed.name",
            default_order: "listed.name",
            sortable_fields: PATRONS_SORT_FIELDS,
            flag_columns: &[],
        },
    )
    .await
}

const QUESTS_SORT_FIELDS: &[(&str, &str)] = &[
    ("name", "listed.name"),
    ("id", "listed.id"),
    ("level", "listed.level"),
    ("epic_level", "listed.epic_level"),
    ("legendary_level", "listed.legendary_level"),
    ("favor", "listed.favor"),
    ("pack", "listed.pack"),
    ("patron", "listed.patron"),
    ("bestowed_by", "bestowed_by"),
    ("epic_name", "epic_name"),
    ("flagging", "flagging"),
    ("is_challenge", "is_challenge"),
    ("is_free_to_play", "is_free_to_play"),
    ("is_raid", "is_raid"),
    ("max_level", "max_level"),
    ("zone", "zone"),
];

declare_list_parameters!(QuestsParameters, QUESTS_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/quests",
    tag = "quests",
    summary = "List quests",
    description = "Lists quests with level, pack, patron and wiki facts.",
    params(
        QuestsParameters,
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `quests` page", body = crate::routes::v1::response_schemas::QuestsPageResponse),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn quests(State(state): State<AppState>, ApiQuery(query, _): ApiQuery) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let select_sql = QUEST_SELECT;
            let mut page = paged_rows(db, select_sql, &query, "listed.name", "listed.name", QUESTS_SORT_FIELDS)?;
            for row in &mut page.rows {
                convert_to_booleans(row, QUEST_FLAG_COLUMNS);
            }
            Ok(Json(page.into_json("quests")))
        })
        .await
}

#[utoipa::path(
    get,
    path = "/v1/quests/{id}",
    tag = "quests",
    summary = "Get a quest",
    description = "Returns a quest with difficulties, loot, rewards and wiki facts.",
    params(("id" = i64, Path, description = "The quest's numeric id from /v1/quests")),
    responses(
        (status = 200, description = "The quest with the items and augments it drops", body = crate::routes::v1::response_schemas::QuestsDetailResponse),
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
            "SELECT q.id, q.name, q.level, q.epic_level, q.is_raid, q.difficulties, q.is_free_to_play, ap.name AS pack,
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
              WHERE loot.kind = 'adventure_pack' AND loot.{loot_id_column} = ?1
              ORDER BY p.name, loot.loot_type, loot.is_rare, loot.chest"
        ),
        [loot_id],
    )?;
    for adventure_pack in &mut adventure_packs {
        convert_to_booleans(adventure_pack, &["is_rare"]);
        if let Some(pack_name) = adventure_pack["name"].as_str() {
            adventure_pack["wiki_url"] = Value::String(wiki_page_url(pack_name));
        }
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
            "SELECT loot.kind, COALESCE(q.id, c.id, s.id, p.id, cs.id, v.id, e.id) AS id,
                    COALESCE(q.name, c.name, s.name, p.name, cs.name, v.name, e.name,
                             'Advance to level ' || loot.character_level) AS name,
                    loot.loot_type, loot.chest, loot.is_rare, loot.tier, loot.character_level, loot.cost,
                    COALESCE(c.wiki_url, s.wiki_url, cs.page, v.wiki_url, e.wiki_url) AS wiki_url
               FROM sources loot LEFT JOIN quests q ON q.id = loot.quest_id
               LEFT JOIN quest_chains c ON c.id = loot.chain_id LEFT JOIN sagas s ON s.id = loot.saga_id
               LEFT JOIN adventure_packs p ON p.id = loot.pack_id
               LEFT JOIN crafting_systems cs ON cs.id = loot.crafting_system_id
               LEFT JOIN vendors v ON v.id = loot.vendor_id LEFT JOIN events e ON e.id = loot.event_id
              WHERE loot.{loot_id_column} = ?1
              ORDER BY CASE loot.kind WHEN 'quest' THEN 1 WHEN 'quest_chain' THEN 2 WHEN 'saga' THEN 3
                                      WHEN 'adventure_pack' THEN 4 WHEN 'challenge' THEN 5
                                      WHEN 'crafting_system' THEN 6 WHEN 'vendor' THEN 7 WHEN 'event' THEN 8
                                      ELSE 9 END,
                       name, loot.loot_type,
                       CASE loot.tier WHEN 'heroic' THEN 1 WHEN 'epic' THEN 2 WHEN 'legendary' THEN 3 ELSE 4 END"
        ),
        [loot_id],
    )?;
    for source in &mut sources {
        convert_to_booleans(source, &["is_rare"]);
        if source["wiki_url"].is_null() && source["kind"] != "starter" {
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

pub(super) fn challenge_packs_rewarding(db: &rusqlite::Connection, item_id: i64) -> Result<Vec<Value>, ApiError> {
    let mut challenge_packs = json_rows(
        db,
        "SELECT p.id, p.name, loot.is_rare FROM sources loot JOIN adventure_packs p ON p.id = loot.pack_id
          WHERE loot.kind = 'challenge' AND loot.item_id = ?1 ORDER BY p.name",
        [item_id],
    )?;
    for challenge_pack in &mut challenge_packs {
        convert_to_booleans(challenge_pack, &["is_rare"]);
        if let Some(pack_name) = challenge_pack["name"].as_str() {
            challenge_pack["wiki_url"] = Value::String(wiki_page_url(pack_name));
        }
    }
    Ok(challenge_packs)
}

pub(super) fn starter_rewards_of(db: &rusqlite::Connection, item_id: i64) -> Result<Vec<Value>, ApiError> {
    json_rows(
        db,
        "SELECT character_level FROM sources WHERE kind = 'starter' AND item_id = ?1 ORDER BY character_level",
        [item_id],
    )
}
