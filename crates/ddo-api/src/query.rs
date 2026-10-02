use crate::error::ApiError;
use axum::extract::{FromRequestParts, Query};
use axum::http::request::Parts;
use axum::http::Uri;
use serde::de::DeserializeOwned;

pub(crate) struct ApiQuery<T>(pub T);

pub(crate) trait QueryParameters: DeserializeOwned {
    const REPEATABLE_KEYS: &'static [&'static str] = &[];
}

impl<T: QueryParameters, S: Send + Sync> FromRequestParts<S> for ApiQuery<T> {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let query_string = parts.uri.query().unwrap_or_default();
        let pairs: Vec<(String, String)> = serde_urlencoded::from_str(query_string)
            .map_err(|e| ApiError::BadRequest(format!("Failed to deserialize query string: {e}")))?;
        let joined_query_string =
            serde_urlencoded::to_string(pairs_with_repeated_keys_joined(pairs, T::REPEATABLE_KEYS))
                .map_err(|e| ApiError::Internal(e.into()))?;
        let joined_uri: Uri = format!("/?{joined_query_string}")
            .parse()
            .map_err(|e: axum::http::uri::InvalidUri| ApiError::Internal(e.into()))?;
        let Query(query) = Query::<T>::try_from_uri(&joined_uri).map_err(|e| ApiError::BadRequest(e.body_text()))?;
        Ok(ApiQuery(query))
    }
}

fn pairs_with_repeated_keys_joined(pairs: Vec<(String, String)>, repeatable_keys: &[&str]) -> Vec<(String, String)> {
    let mut joined_pairs: Vec<(String, String)> = Vec::with_capacity(pairs.len());
    for (key, value) in pairs {
        let earlier_pair = repeatable_keys
            .contains(&key.as_str())
            .then(|| joined_pairs.iter_mut().find(|(earlier_key, _)| *earlier_key == key))
            .flatten();
        match earlier_pair {
            Some((_, earlier_value)) => {
                earlier_value.push(',');
                earlier_value.push_str(&value);
            }
            None => joined_pairs.push((key, value)),
        }
    }
    joined_pairs
}

pub(crate) fn comma_separated_values(list_text: &str) -> Vec<String> {
    list_text.split(',').map(str::trim).filter(|value| !value.is_empty()).map(str::to_string).collect()
}
