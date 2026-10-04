use crate::db::{convert_to_booleans, json_row, json_rows, paged_rows, ListPage};
use crate::error::ApiError;
use crate::query::{declare_list_parameters, ApiQuery, ListQuery};
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
                "s.id, s.name, p.name AS pack, s.wiki_url,
                    (SELECT COUNT(*) FROM quest_chain_quests sq WHERE sq.chain_id = s.id) AS quest_count,
                    (SELECT COUNT(*) FROM sources sr WHERE sr.chain_id = s.id) AS reward_count
               FROM quest_chains s LEFT JOIN adventure_packs p ON p.id = s.pack_id"
            }
            Self::Sagas => {
                "s.id, s.name, p.name AS pack, s.wiki_url,
                    (SELECT COUNT(*) FROM saga_quests sq WHERE sq.saga_id = s.id) AS quest_count,
                    (SELECT COUNT(*) FROM sources sr WHERE sr.saga_id = s.id) AS reward_count
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
                   FROM sources sr JOIN items i ON i.id = sr.item_id
                   LEFT JOIN equipment_slots es ON es.id = i.slot_id
                  WHERE sr.chain_id = ?1 ORDER BY i.name, i.id"
            }
            Self::Sagas => {
                "SELECT i.id, i.name, sr.tier, sr.is_rare, i.minimum_level, es.name AS slot
                   FROM sources sr JOIN items i ON i.id = sr.item_id
                   LEFT JOIN equipment_slots es ON es.id = i.slot_id
                  WHERE sr.saga_id = ?1
                  ORDER BY CASE sr.tier WHEN 'heroic' THEN 1 WHEN 'epic' THEN 2 WHEN 'legendary' THEN 3 ELSE 4 END,
                           i.name, i.id"
            }
        }
    }
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

const QUEST_SERIES_SORT_FIELDS: &[(&str, &str)] = &[
    ("name", "listed.name"),
    ("id", "listed.id"),
    ("pack", "listed.pack"),
    ("quest_count", "listed.quest_count"),
    ("reward_count", "listed.reward_count"),
    ("wiki_url", "wiki_url"),
];

fn quest_series_page(db: &Connection, table: QuestSeriesTable, query: &ListQuery) -> Result<ListPage, ApiError> {
    let select_sql = format!("SELECT {}", table.columns_and_joins());
    paged_rows(db, &select_sql, query, "listed.name", "listed.name", QUEST_SERIES_SORT_FIELDS)
}

pub(super) fn quest_chains_rewarding(db: &Connection, item_id: i64) -> Result<Vec<Value>, ApiError> {
    let mut quest_chains = json_rows(
        db,
        "SELECT c.id, c.name, cr.is_rare, c.wiki_url FROM sources cr JOIN quest_chains c ON c.id = cr.chain_id
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
        "SELECT s.id, s.name, sr.tier, sr.is_rare, s.wiki_url FROM sources sr JOIN sagas s ON s.id = sr.saga_id
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

declare_list_parameters!(QuestChainsParameters, QUEST_SERIES_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/quest-chains",
    tag = "quests",
    summary = "List quest chains",
    description = "Lists quest chains with pack and reward counts.",
    params(
        QuestChainsParameters,
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `quest_chains` page", body = crate::routes::v1::response_schemas::QuestChainsPageResponse),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn quest_chains(State(state): State<AppState>, ApiQuery(query, _): ApiQuery) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            Ok(Json(quest_series_page(db, QuestSeriesTable::QuestChains, &query)?.into_json("quest_chains")))
        })
        .await
}

#[utoipa::path(
    get,
    path = "/v1/quest-chains/{id}",
    tag = "quests",
    summary = "Get a quest chain",
    description = "Returns a quest chain with its quests and end rewards.",
    params(("id" = i64, Path, description = "The quest chain's numeric id from /v1/quest-chains")),
    responses(
        (status = 200, description = "The quest chain with its quests and rewards", body = crate::routes::v1::response_schemas::QuestChainsDetailResponse),
        (status = 404, description = "No quest chain has this id", body = crate::error::ErrorBody)
    )
)]
async fn quest_chain_detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    let quest_chain = state.read_db(move |db| quest_series_detail(db, QuestSeriesTable::QuestChains, id)).await?;
    Ok(Json(quest_chain))
}

declare_list_parameters!(SagasParameters, QUEST_SERIES_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/sagas",
    tag = "quests",
    summary = "List sagas",
    description = "Lists sagas with pack, quest and reward counts.",
    params(
        SagasParameters,
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `sagas` page", body = crate::routes::v1::response_schemas::SagasPageResponse),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn sagas(State(state): State<AppState>, ApiQuery(query, _): ApiQuery) -> Result<Json<Value>, ApiError> {
    state.read_db(move |db| Ok(Json(quest_series_page(db, QuestSeriesTable::Sagas, &query)?.into_json("sagas")))).await
}

#[utoipa::path(
    get,
    path = "/v1/sagas/{id}",
    tag = "quests",
    summary = "Get a saga",
    description = "Returns a saga with its quests and tiered end rewards.",
    params(("id" = i64, Path, description = "The saga's numeric id from /v1/sagas")),
    responses(
        (status = 200, description = "The saga with its quests and rewards", body = crate::routes::v1::response_schemas::SagasDetailResponse),
        (status = 404, description = "No saga has this id", body = crate::error::ErrorBody)
    )
)]
async fn saga_detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    let saga = state.read_db(move |db| quest_series_detail(db, QuestSeriesTable::Sagas, id)).await?;
    Ok(Json(saga))
}
