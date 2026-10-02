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

#[utoipa::path(
    get,
    path = "/v1/vendors",
    tag = "sources",
    summary = "List vendors",
    description = "Every vendor (an NPC or place that sells items or trades them for tokens, read from ddowiki, since \
                   Maetrim's files have none), ordered by name, with the `location` the wiki gives (free text, null \
                   when it gives none), its adventure `pack` (null when it names none), \
                   the `wiki_url` it was read from, and `item_count`, the items it offers: those the wiki page lists \
                   plus the items whose drop text names the vendor. Empty until a vendor has been read from ddowiki.",
    responses((status = 200, description = "Every vendor with its location, pack and item count", body = Vec<Value>))
)]
async fn vendors(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    let vendors = state.read_db(|db| json_rows(db, &format!("{VENDOR_SELECT} ORDER BY v.name"), [])).await?;
    Ok(Json(vendors))
}

#[utoipa::path(
    get,
    path = "/v1/vendors/{id}",
    tag = "sources",
    summary = "Get a vendor",
    description = "One vendor as the list returns it, plus `items`, the items it offers, sorted by name, each with \
                   `id`, `name`, `cost` (what the wiki says it asks, free text, null when unknown), `is_rare`, \
                   `minimum_level` and equipment `slot`; empty when nothing is known. Item detail `vendors` reports \
                   the same link from the other side.",
    params(("id" = i64, Path, description = "The vendor's numeric id from /v1/vendors")),
    responses(
        (status = 200, description = "The vendor with the items it offers", body = Value),
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

#[utoipa::path(
    get,
    path = "/v1/events",
    tag = "sources",
    summary = "List events",
    description = "Every event (a festival or limited-time event whose rewards are items, such as Treasure of Crystal \
                   Cove or The Night Revels, read from ddowiki, since Maetrim's files have none), ordered by name, \
                   with the `wiki_url` it was read from, and `item_count`, the items it \
                   rewards: those the wiki page lists plus the items whose drop text names the event. Empty until an \
                   event has been read from ddowiki.",
    responses((status = 200, description = "Every event with its item count", body = Vec<Value>))
)]
async fn events(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    let events = state.read_db(|db| json_rows(db, &format!("{EVENT_SELECT} ORDER BY e.name"), [])).await?;
    Ok(Json(events))
}

#[utoipa::path(
    get,
    path = "/v1/events/{id}",
    tag = "sources",
    summary = "Get an event",
    description = "One event as the list returns it, plus `items`, the items it rewards, sorted by name, each with \
                   `id`, `name`, `is_rare`, `minimum_level` and equipment `slot`; empty when nothing is known. Item \
                   detail `events` reports the same link from the other side.",
    params(("id" = i64, Path, description = "The event's numeric id from /v1/events")),
    responses(
        (status = 200, description = "The event with the items it rewards", body = Value),
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

pub(super) fn vendors_offering(db: &Connection, item_id: i64) -> Result<Vec<Value>, ApiError> {
    let mut vendors = json_rows(
        db,
        "SELECT v.id, v.name, v.location, vi.cost, vi.is_rare, v.wiki_url
           FROM sources vi JOIN vendors v ON v.id = vi.vendor_id WHERE vi.item_id = ?1 ORDER BY v.name",
        [item_id],
    )?;
    for vendor in &mut vendors {
        convert_to_booleans(vendor, &["is_rare"]);
    }
    Ok(vendors)
}

pub(super) fn events_rewarding(db: &Connection, item_id: i64) -> Result<Vec<Value>, ApiError> {
    let mut events = json_rows(
        db,
        "SELECT e.id, e.name, ei.is_rare, e.wiki_url
           FROM sources ei JOIN events e ON e.id = ei.event_id WHERE ei.item_id = ?1 ORDER BY e.name",
        [item_id],
    )?;
    for event in &mut events {
        convert_to_booleans(event, &["is_rare"]);
    }
    Ok(events)
}
