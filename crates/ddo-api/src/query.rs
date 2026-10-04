use crate::error::ApiError;
use axum::extract::{FromRequestParts, Query};
use axum::http::request::Parts;
use axum::http::Uri;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use utoipa::openapi::path::{Parameter, ParameterBuilder, ParameterIn, ParameterStyle};
use utoipa::openapi::Required;
use utoipa::{IntoParams, PartialSchema};

macro_rules! declare_query_parameters {
    (
        $visibility:vis struct $name:ident {
            $($(#[$attribute:meta])* $field_visibility:vis $field:ident: $field_type:ty),* $(,)?
        }
        $(repeatable: [$($repeatable_key:literal),* $(,)?])?
        $(matchable: [$($matchable_key:literal),* $(,)?])?
        $(single_value_match: [$($single_value_match_key:literal),* $(,)?])?
    ) => {
        #[derive(Default, serde::Deserialize)]
        $visibility struct $name {
            $($(#[$attribute])* $field_visibility $field: $field_type),*
        }

        impl crate::query::QueryParameters for $name {
            const KEYS: &'static [&'static str] = &[$(stringify!($field)),*];
            const REPEATABLE_KEYS: &'static [&'static str] = &[$($($repeatable_key),*)?];
            const MATCHABLE_KEYS: &'static [&'static str] = &[$($($matchable_key),*)?];
            const SINGLE_VALUE_MATCH_KEYS: &'static [&'static str] = &[$($($single_value_match_key),*)?];
        }
    };
}
pub(crate) use declare_query_parameters;

declare_query_parameters! {
    pub(crate) struct ListQuery {
        pub q: Option<String>,
        pub limit: Option<i64>,
        pub offset: Option<i64>,
        #[serde(default, deserialize_with = "repeated_sort_values")]
        pub sort: Vec<String>,
    }
    repeatable: ["sort"]
}

declare_query_parameters! {
    pub(crate) struct NoFilters {}
}

pub(crate) struct ApiQuery<T = NoFilters>(pub ListQuery, pub T);
pub(crate) struct ApiFilterQuery<T>(pub T);

pub(crate) trait QueryParameters: DeserializeOwned {
    const KEYS: &'static [&'static str];
    const REPEATABLE_KEYS: &'static [&'static str] = &[];
    const MATCHABLE_KEYS: &'static [&'static str] = &[];
    const SINGLE_VALUE_MATCH_KEYS: &'static [&'static str] = &[];
}

const REPEATED_VALUE_SEPARATOR: char = '\u{1F}';

impl<T: QueryParameters, S: Send + Sync> FromRequestParts<S> for ApiQuery<T> {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let query_string = parts.uri.query().unwrap_or_default();
        let pairs: Vec<(String, String)> = serde_urlencoded::from_str(query_string)
            .map_err(|e| ApiError::BadRequest(format!("Failed to deserialize query string: {e}")))?;
        let mut list_pairs = Vec::new();
        let mut filter_pairs = Vec::new();
        for (key, value) in pairs {
            if ListQuery::KEYS.contains(&key.as_str()) {
                list_pairs.push((key, value));
            } else if T::KEYS.contains(&key.as_str())
                || key.strip_suffix("_match").is_some_and(|filter| T::SINGLE_VALUE_MATCH_KEYS.contains(&filter))
            {
                if let Some(filter) = key.strip_suffix("_match") {
                    if T::SINGLE_VALUE_MATCH_KEYS.contains(&filter) {
                        return Err(ApiError::BadRequest(format!("{filter} does not support a match mode")));
                    }
                    if T::MATCHABLE_KEYS.contains(&filter) && !matches!(value.as_str(), "any" | "all") {
                        return Err(ApiError::BadRequest(format!("invalid {key} {value:?}; expected any or all")));
                    }
                }
                filter_pairs.push((key, value));
            } else {
                let allowed_parameters = ListQuery::KEYS
                    .iter()
                    .chain(T::KEYS)
                    .copied()
                    .map(str::to_string)
                    .chain(T::SINGLE_VALUE_MATCH_KEYS.iter().map(|key| format!("{key}_match")))
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(ApiError::BadRequest(format!(
                    "unknown query parameter {key:?}; allowed parameters: {allowed_parameters}"
                )));
            }
        }
        Ok(ApiQuery(query_from_pairs(list_pairs)?, query_from_pairs(filter_pairs)?))
    }
}

impl<T: QueryParameters, S: Send + Sync> FromRequestParts<S> for ApiFilterQuery<T> {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let pairs: Vec<(String, String)> = serde_urlencoded::from_str(parts.uri.query().unwrap_or_default())
            .map_err(|error| ApiError::BadRequest(format!("Failed to deserialize query string: {error}")))?;
        for (key, _) in &pairs {
            if !T::KEYS.contains(&key.as_str()) {
                return Err(ApiError::BadRequest(format!(
                    "unknown query parameter {key:?}; allowed parameters: {}",
                    T::KEYS.join(", ")
                )));
            }
        }
        Ok(ApiFilterQuery(query_from_pairs(pairs)?))
    }
}

fn query_from_pairs<T: QueryParameters>(pairs: Vec<(String, String)>) -> Result<T, ApiError> {
    let joined_query_string = serde_urlencoded::to_string(pairs_with_repeated_keys_joined(pairs, T::REPEATABLE_KEYS))
        .map_err(|e| ApiError::Internal(e.into()))?;
    let joined_uri: Uri = format!("/?{joined_query_string}")
        .parse()
        .map_err(|e: axum::http::uri::InvalidUri| ApiError::Internal(e.into()))?;
    let Query(query) = Query::<T>::try_from_uri(&joined_uri).map_err(|e| ApiError::BadRequest(e.body_text()))?;
    Ok(query)
}

impl IntoParams for ListQuery {
    fn into_params(_parameter_in_provider: impl Fn() -> Option<ParameterIn>) -> Vec<Parameter> {
        vec![
            query_parameter::<String>("q", "Case-insensitive name substring after trimming; blank applies no search."),
            query_parameter::<i64>("limit", "Page size, 1 to 10000; defaults to 100; out-of-range values are clamped"),
            query_parameter::<i64>("offset", "Rows to skip; defaults to 0; negative values are clamped to 0"),
            ParameterBuilder::from(query_parameter::<Vec<String>>("sort", SORT_DESCRIPTION))
                .style(Some(ParameterStyle::Form))
                .explode(Some(true))
                .build(),
        ]
    }
}

fn query_parameter<T: PartialSchema>(name: &str, description: &str) -> Parameter {
    ParameterBuilder::new()
        .name(name)
        .parameter_in(ParameterIn::Query)
        .required(Required::False)
        .description(Some(description))
        .schema(Some(T::schema()))
        .build()
}

const SORT_DESCRIPTION: &str =
    "Repeat `sort` in priority order; prefix `-` for descending; nulls sort last; an invalid field returns 400";

pub(crate) fn list_parameters(
    sortable_fields: &[(&str, &str)],
    sort_note: &str,
    search_description: Option<&str>,
) -> Vec<Parameter> {
    let mut parameters = ListQuery::into_params(|| Some(ParameterIn::Query));
    for parameter in &mut parameters {
        if parameter.name == "q" {
            if let Some(description) = search_description {
                parameter.description = Some(description.to_string());
            }
        } else if parameter.name == "sort" {
            parameter.description = Some(
                format!("{SORT_DESCRIPTION}; Allowed fields: {}. {sort_note}", allowed_sort_fields(sortable_fields))
                    .trim_end()
                    .to_string(),
            );
        }
    }
    parameters
}

macro_rules! declare_list_parameters {
    ($name:ident, $fields:expr, $note:literal) => {
        crate::query::declare_list_parameters!($name, $fields, $note, None);
    };
    ($name:ident, $fields:expr, $note:literal, $search_description:expr) => {
        struct $name;

        impl utoipa::IntoParams for $name {
            fn into_params(
                _parameter_in_provider: impl Fn() -> Option<utoipa::openapi::path::ParameterIn>,
            ) -> Vec<utoipa::openapi::path::Parameter> {
                crate::query::list_parameters($fields, $note, $search_description)
            }
        }
    };
}
pub(crate) use declare_list_parameters;

fn pairs_with_repeated_keys_joined(pairs: Vec<(String, String)>, repeatable_keys: &[&str]) -> Vec<(String, String)> {
    let mut joined_pairs: Vec<(String, String)> = Vec::with_capacity(pairs.len());
    for (key, value) in pairs {
        let is_repeatable = repeatable_keys.contains(&key.as_str());
        let value = if is_repeatable { value.replace(REPEATED_VALUE_SEPARATOR, "") } else { value };
        let earlier_pair =
            is_repeatable.then(|| joined_pairs.iter_mut().find(|(earlier_key, _)| *earlier_key == key)).flatten();
        match earlier_pair {
            Some((_, earlier_value)) => {
                earlier_value.push(REPEATED_VALUE_SEPARATOR);
                earlier_value.push_str(&value);
            }
            None => joined_pairs.push((key, value)),
        }
    }
    joined_pairs
}

pub(crate) fn repeated_key_values<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    let joined_values = String::deserialize(deserializer)?;
    Ok(joined_values
        .split(REPEATED_VALUE_SEPARATOR)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect())
}

pub(crate) fn repeated_integer_values<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<i64>, D::Error> {
    let joined_values = String::deserialize(deserializer)?;
    joined_values
        .split(REPEATED_VALUE_SEPARATOR)
        .map(|value| value.parse::<i64>().map_err(serde::de::Error::custom))
        .collect()
}

fn repeated_sort_values<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    let joined_values = String::deserialize(deserializer)?;
    Ok(joined_values.split(REPEATED_VALUE_SEPARATOR).map(str::to_string).collect())
}

pub(crate) fn sort_order(
    sort_keys: &[String],
    sortable_fields: &[(&str, &str)],
    default_order: &str,
) -> Result<String, ApiError> {
    if sort_keys.is_empty() {
        return Ok(default_order.to_string());
    }
    let mut order_parts = Vec::with_capacity(sort_keys.len() * 2);
    for sort_key in sort_keys {
        let (field_name, direction) = match sort_key.strip_prefix('-') {
            Some(field_name) => (field_name, "DESC"),
            None => (sort_key.as_str(), "ASC"),
        };
        let Some((_, expression)) = sortable_fields.iter().find(|(allowed_name, _)| *allowed_name == field_name) else {
            let allowed = allowed_sort_fields(sortable_fields);
            return Err(ApiError::BadRequest(format!("invalid sort {sort_key:?}; allowed fields: {allowed}")));
        };
        order_parts.push(format!("({expression}) IS NULL"));
        order_parts.push(format!("{expression} {direction}"));
    }
    Ok(order_parts.join(", "))
}

fn allowed_sort_fields(sortable_fields: &[(&str, &str)]) -> String {
    sortable_fields.iter().map(|(name, _)| *name).collect::<Vec<_>>().join(", ")
}
