use crate::db::paged_rows;
use crate::db::{convert_to_booleans, json_row, json_rows};
use crate::error::ApiError;
use crate::query::{declare_list_parameters, ApiQuery};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use rusqlite::Connection;
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(vendors))
        .routes(routes!(vendor_detail))
        .routes(routes!(events))
        .routes(routes!(event_detail))
}

const VENDOR_SELECT: &str = "SELECT v.id, v.name, v.location, p.name AS pack, v.wiki_url,
        (SELECT COUNT(*) FROM sources vi WHERE vi.kind = 'vendor' AND vi.vendor_id = v.id) AS item_count
   FROM vendors v LEFT JOIN adventure_packs p ON p.id = v.pack_id";

const EVENT_SELECT: &str = "SELECT e.id, e.name, e.wiki_url,
        (SELECT COUNT(*) FROM sources ei WHERE ei.kind = 'event' AND ei.event_id = e.id) AS item_count
   FROM events e";

const VENDORS_SORT_FIELDS: &[(&str, &str)] = &[
    ("name", "listed.name"),
    ("id", "listed.id"),
    ("pack", "listed.pack"),
    ("location", "listed.location"),
    ("item_count", "listed.item_count"),
    ("wiki_url", "wiki_url"),
];

declare_list_parameters!(VendorsParameters, VENDORS_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/vendors",
    tag = "sources",
    summary = "List vendors",
    description = "Lists vendors and the number of items each offers.",
    params(
        VendorsParameters,
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `vendors` page", body = crate::routes::v1::response_schemas::VendorsPageResponse),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn vendors(State(state): State<AppState>, ApiQuery(query, _): ApiQuery) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let select_sql = VENDOR_SELECT;
            let page = paged_rows(db, select_sql, &query, "listed.name", "listed.name", VENDORS_SORT_FIELDS)?;

            Ok(Json(page.into_json("vendors")))
        })
        .await
}

#[utoipa::path(
    get,
    path = "/v1/vendors/{id}",
    tag = "sources",
    summary = "Get a vendor",
    description = "Returns a vendor with its page, location and offered items.",
    params(("id" = i64, Path, description = "The vendor's numeric id from /v1/vendors")),
    responses(
        (status = 200, description = "The vendor with the items it offers", body = crate::routes::v1::response_schemas::VendorsDetailResponse),
        (status = 404, description = "No vendor has this id", body = crate::error::ErrorBody)
    )
)]
async fn vendor_detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut vendor = json_row(db, &format!("{VENDOR_SELECT} WHERE v.id = ?1"), [id])?;
            vendor["items"] = Value::Array(offered_items(
                db,
                "SELECT i.id, i.name, vi.cost, vi.is_rare, i.minimum_level, es.name AS slot
                   FROM sources vi JOIN items i ON i.id = vi.item_id LEFT JOIN equipment_slots es ON es.id = i.slot_id
                  WHERE vi.kind = 'vendor' AND vi.vendor_id = ?1 ORDER BY i.name, i.id",
                id,
            )?);
            Ok(Json(vendor))
        })
        .await
}

const EVENTS_SORT_FIELDS: &[(&str, &str)] =
    &[("name", "listed.name"), ("id", "listed.id"), ("item_count", "listed.item_count"), ("wiki_url", "wiki_url")];

declare_list_parameters!(EventsParameters, EVENTS_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/events",
    tag = "sources",
    summary = "List events",
    description = "Lists events and the number of items each rewards.",
    params(
        EventsParameters,
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `events` page", body = crate::routes::v1::response_schemas::EventsPageResponse),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn events(State(state): State<AppState>, ApiQuery(query, _): ApiQuery) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let select_sql = EVENT_SELECT;
            let page = paged_rows(db, select_sql, &query, "listed.name", "listed.name", EVENTS_SORT_FIELDS)?;

            Ok(Json(page.into_json("events")))
        })
        .await
}

#[utoipa::path(
    get,
    path = "/v1/events/{id}",
    tag = "sources",
    summary = "Get an event",
    description = "Returns an event with its page and rewarded items.",
    params(("id" = i64, Path, description = "The event's numeric id from /v1/events")),
    responses(
        (status = 200, description = "The event with the items it rewards", body = crate::routes::v1::response_schemas::EventsDetailResponse),
        (status = 404, description = "No event has this id", body = crate::error::ErrorBody)
    )
)]
async fn event_detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut event = json_row(db, &format!("{EVENT_SELECT} WHERE e.id = ?1"), [id])?;
            event["items"] = Value::Array(offered_items(
                db,
                "SELECT i.id, i.name, ei.is_rare, i.minimum_level, es.name AS slot
                   FROM sources ei JOIN items i ON i.id = ei.item_id LEFT JOIN equipment_slots es ON es.id = i.slot_id
                  WHERE ei.kind = 'event' AND ei.event_id = ?1 ORDER BY i.name, i.id",
                id,
            )?);
            Ok(Json(event))
        })
        .await
}

fn offered_items(db: &Connection, sql: &str, source_id: i64) -> Result<Vec<Value>, ApiError> {
    let mut items = json_rows(db, sql, [source_id])?;
    for item in &mut items {
        convert_to_booleans(item, &["is_rare"]);
    }
    Ok(items)
}
