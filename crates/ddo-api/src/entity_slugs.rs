use crate::{error::ApiError, AppState};
use axum::extract::{FromRequestParts, MatchedPath, Path};
use axum::http::request::Parts;
use axum::response::{IntoResponse, Redirect, Response};
use ddo_model::enums::EntityKind;
use rusqlite::{Connection, OptionalExtension};

pub struct ResolvedEntitySlug {
    pub id: i64,
    pub slug: String,
    pub is_redirect: bool,
}

pub fn resolve_entity_slug(
    db: &Connection,
    kind: EntityKind,
    text: &str,
) -> rusqlite::Result<Option<ResolvedEntitySlug>> {
    let current = db
        .query_row(
            "SELECT entity_id, slug FROM entity_identity WHERE kind = ?1 AND slug = ?2",
            [kind.table_name(), text],
            |row| Ok(ResolvedEntitySlug { id: row.get(0)?, slug: row.get(1)?, is_redirect: false }),
        )
        .optional()?;
    if current.is_some() {
        return Ok(current);
    }
    let redirected = db.query_row(
        "SELECT e.entity_id, e.slug FROM served_redirects r JOIN entity_identity e ON e.kind = ?1 AND e.slug = r.to_slug WHERE r.kind = ?2 AND r.from_slug = ?3",
        [kind.table_name(), kind.as_str(), text],
        |row| Ok(ResolvedEntitySlug { id: row.get(0)?, slug: row.get(1)?, is_redirect: true }),
    ).optional()?;
    if redirected.is_some() {
        return Ok(redirected);
    }
    if !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()) {
        if let Ok(id) = text.parse::<i64>() {
            return db
                .query_row(
                    "SELECT entity_id, slug FROM entity_identity WHERE kind = ?1 AND entity_id = ?2",
                    rusqlite::params![kind.table_name(), id],
                    |row| Ok(ResolvedEntitySlug { id: row.get(0)?, slug: row.get(1)?, is_redirect: false }),
                )
                .optional();
        }
    }
    Ok(None)
}

pub(crate) struct EntityPath(pub(crate) i64);

impl FromRequestParts<AppState> for EntityPath {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let matched = MatchedPath::from_request_parts(parts, state).await.map_err(IntoResponse::into_response)?;
        let kind = matched.as_str().split('/').nth(2).and_then(EntityKind::parse).ok_or_else(|| {
            ApiError::Internal(anyhow::anyhow!("detail route has no declared entity kind")).into_response()
        })?;
        let Path(text) = Path::<String>::from_request_parts(parts, state).await.map_err(IntoResponse::into_response)?;
        let resolved = state
            .read_db(move |db| resolve_entity_slug(db, kind, &text)?.ok_or(ApiError::NotFound))
            .await
            .map_err(IntoResponse::into_response)?;
        if resolved.is_redirect {
            let mut location = format!("/v1/{}/{}", kind.as_str(), resolved.slug);
            if let Some(query) = parts.uri.query() {
                location.push('?');
                location.push_str(query);
            }
            return Err(Redirect::permanent(&location).into_response());
        }
        Ok(Self(resolved.id))
    }
}
