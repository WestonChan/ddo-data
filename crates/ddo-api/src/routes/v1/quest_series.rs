use crate::db::{convert_to_booleans, json_row, json_rows};
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use rusqlite::Connection;
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(quest_chains))
        .routes(routes!(quest_chain_detail))
        .routes(routes!(sagas))
        .routes(routes!(saga_detail))
}

#[derive(Clone, Copy)]
enum QuestSeriesTable {
    QuestChains,
    Sagas,
}

impl QuestSeriesTable {
    fn columns_and_joins(self) -> &'static str {
        match self {
            Self::QuestChains => {
                "s.id, s.name, p.name AS pack, s.source, s.wiki_url,
                    (SELECT COUNT(*) FROM quest_chain_quests sq WHERE sq.chain_id = s.id) AS quest_count,
                    (SELECT COUNT(*) FROM quest_chain_rewards sr WHERE sr.chain_id = s.id) AS reward_count
               FROM quest_chains s LEFT JOIN adventure_packs p ON p.id = s.pack_id"
            }
            Self::Sagas => {
                "s.id, s.name, p.name AS pack, s.source, s.wiki_url,
                    (SELECT COUNT(*) FROM saga_quests sq WHERE sq.saga_id = s.id) AS quest_count,
                    (SELECT COUNT(*) FROM saga_rewards sr WHERE sr.saga_id = s.id) AS reward_count
               FROM sagas s LEFT JOIN adventure_packs p ON p.id = s.pack_id"
            }
        }
    }

    fn quests_sql(self) -> &'static str {
        match self {
            Self::QuestChains => {
                "SELECT q.id, q.name, q.level FROM quest_chain_quests sq JOIN quests q ON q.id = sq.quest_id
                  WHERE sq.chain_id = ?1 ORDER BY sq.sort_order"
            }
            Self::Sagas => {
                "SELECT q.id, q.name, q.level FROM saga_quests sq JOIN quests q ON q.id = sq.quest_id
                  WHERE sq.saga_id = ?1 ORDER BY sq.sort_order"
            }
        }
    }

    fn rewards_sql(self) -> &'static str {
        match self {
            Self::QuestChains => {
                "SELECT i.id, i.name, sr.is_rare, i.minimum_level, es.name AS slot
                   FROM quest_chain_rewards sr JOIN items i ON i.id = sr.item_id
                   LEFT JOIN equipment_slots es ON es.id = i.slot_id
                  WHERE sr.chain_id = ?1 ORDER BY i.name, i.id"
            }
            Self::Sagas => {
                "SELECT i.id, i.name, sr.tier, sr.is_rare, i.minimum_level, es.name AS slot
                   FROM saga_rewards sr JOIN items i ON i.id = sr.item_id
                   LEFT JOIN equipment_slots es ON es.id = i.slot_id
                  WHERE sr.saga_id = ?1
                  ORDER BY CASE sr.tier WHEN 'heroic' THEN 1 WHEN 'epic' THEN 2 WHEN 'legendary' THEN 3 ELSE 4 END,
                           i.name, i.id"
            }
        }
    }
}

fn quest_series_list(db: &Connection, table: QuestSeriesTable) -> Result<Vec<Value>, ApiError> {
    json_rows(db, &format!("SELECT {} ORDER BY s.name", table.columns_and_joins()), [])
}

fn quest_series_detail(db: &Connection, table: QuestSeriesTable, id: i64) -> Result<Value, ApiError> {
    let mut quest_series = json_row(db, &format!("SELECT {} WHERE s.id = ?1", table.columns_and_joins()), [id])?;
    quest_series["quests"] = Value::Array(json_rows(db, table.quests_sql(), [id])?);
    let mut rewards = json_rows(db, table.rewards_sql(), [id])?;
    for reward in &mut rewards {
        convert_to_booleans(reward, &["is_rare"]);
    }
    quest_series["rewards"] = Value::Array(rewards);
    Ok(quest_series)
}

pub(super) fn quest_chains_rewarding(db: &Connection, item_id: i64) -> Result<Vec<Value>, ApiError> {
    let mut quest_chains = json_rows(
        db,
        "SELECT c.id, c.name, cr.is_rare, c.wiki_url FROM quest_chain_rewards cr JOIN quest_chains c ON c.id = cr.chain_id
          WHERE cr.item_id = ?1 ORDER BY c.name",
        [item_id],
    )?;
    for quest_chain in &mut quest_chains {
        convert_to_booleans(quest_chain, &["is_rare"]);
    }
    Ok(quest_chains)
}

pub(super) fn sagas_rewarding(db: &Connection, item_id: i64) -> Result<Vec<Value>, ApiError> {
    let mut sagas = json_rows(
        db,
        "SELECT s.id, s.name, sr.tier, sr.is_rare, s.wiki_url FROM saga_rewards sr JOIN sagas s ON s.id = sr.saga_id
          WHERE sr.item_id = ?1
          ORDER BY s.name, CASE sr.tier WHEN 'heroic' THEN 1 WHEN 'epic' THEN 2 WHEN 'legendary' THEN 3 ELSE 4 END",
        [item_id],
    )?;
    for saga in &mut sagas {
        convert_to_booleans(saga, &["is_rare"]);
    }
    Ok(sagas)
}

pub(super) fn quest_chains_including(db: &Connection, quest_id: i64) -> Result<Vec<Value>, ApiError> {
    json_rows(
        db,
        "SELECT c.id, c.name, c.wiki_url FROM quest_chain_quests cq JOIN quest_chains c ON c.id = cq.chain_id
          WHERE cq.quest_id = ?1 ORDER BY c.name",
        [quest_id],
    )
}

pub(super) fn sagas_including(db: &Connection, quest_id: i64) -> Result<Vec<Value>, ApiError> {
    json_rows(
        db,
        "SELECT s.id, s.name, s.wiki_url FROM saga_quests sq JOIN sagas s ON s.id = sq.saga_id
          WHERE sq.quest_id = ?1 ORDER BY s.name",
        [quest_id],
    )
}

#[utoipa::path(
    get,
    path = "/v1/quest-chains",
    tag = "quests",
    summary = "List quest chains",
    description = "Every quest chain (what ddowiki calls a story arc: a run of quests whose end reward an NPC gives \
                   after the last of them, not any one quest), ordered by name, with its adventure `pack` (null \
                   when the wiki names none), `source` (always `wiki`, since Maetrim's files have no chains), the \
                   `wiki_url` it was read from, and how many quests and end rewards it has (`quest_count`, \
                   `reward_count`). The rewards are those the wiki page lists plus the items whose drop text \
                   credits the chain with its end reward. Empty until a chain has been read from ddowiki; \
                   /v1/sagas lists the saga system's rewards.",
    responses((status = 200, description = "Every quest chain with its pack and counts", body = Vec<Value>))
)]
async fn quest_chains(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    let quest_chains = state.read_db(|db| quest_series_list(db, QuestSeriesTable::QuestChains)).await?;
    Ok(Json(quest_chains))
}

#[utoipa::path(
    get,
    path = "/v1/quest-chains/{id}",
    tag = "quests",
    summary = "Get a quest chain",
    description = "One quest chain as the list returns it, plus `quests`, its quests in the order they are run, \
                   each with `id`, `name` and heroic `level` (see /v1/quests/{id}), and `rewards`, the items its \
                   end reward offers, sorted by name, each with `id`, `name`, `is_rare` (the wiki or Maetrim's \
                   drop text marks it rare), `minimum_level` and equipment `slot`. Either array is empty when \
                   nothing is known.",
    params(("id" = i64, Path, description = "The quest chain's numeric id from /v1/quest-chains")),
    responses(
        (status = 200, description = "The quest chain with its quests and rewards", body = Value),
        (status = 404, description = "No quest chain has this id", body = crate::error::ErrorBody)
    )
)]
async fn quest_chain_detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    let quest_chain = state.read_db(move |db| quest_series_detail(db, QuestSeriesTable::QuestChains, id)).await?;
    Ok(Json(quest_chain))
}

#[utoipa::path(
    get,
    path = "/v1/sagas",
    tag = "quests",
    summary = "List sagas",
    description = "Every saga (a set of quests whose saga NPC gives an end reward once they are all done, in \
                   heroic, epic and legendary reward lists), ordered by name, with its adventure `pack` (null \
                   when the wiki names none), `source` (always `wiki`, since Maetrim's files have no sagas), the \
                   `wiki_url` it was read from, and how many quests and end rewards it has (`quest_count`, \
                   `reward_count`, one per item and tier). The rewards are those the wiki page lists plus the \
                   items whose drop text credits the saga. Empty until a saga has been read from ddowiki; \
                   /v1/quest-chains lists quest chains.",
    responses((status = 200, description = "Every saga with its pack and counts", body = Vec<Value>))
)]
async fn sagas(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    let sagas = state.read_db(|db| quest_series_list(db, QuestSeriesTable::Sagas)).await?;
    Ok(Json(sagas))
}

#[utoipa::path(
    get,
    path = "/v1/sagas/{id}",
    tag = "quests",
    summary = "Get a saga",
    description = "One saga as the list returns it, plus `quests`, its quests in the wiki's order, each with \
                   `id`, `name` and heroic `level`, and `rewards`, the items its end reward offers, sorted by \
                   `tier` (heroic, epic, legendary, then null when neither the wiki nor the drop text names one) \
                   and then name, each with `id`, `name`, `tier`, `is_rare`, `minimum_level` and equipment `slot`. \
                   An item offered at two tiers appears once per tier. Either array is empty when nothing is known.",
    params(("id" = i64, Path, description = "The saga's numeric id from /v1/sagas")),
    responses(
        (status = 200, description = "The saga with its quests and rewards", body = Value),
        (status = 404, description = "No saga has this id", body = crate::error::ErrorBody)
    )
)]
async fn saga_detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    let saga = state.read_db(move |db| quest_series_detail(db, QuestSeriesTable::Sagas, id)).await?;
    Ok(Json(saga))
}
