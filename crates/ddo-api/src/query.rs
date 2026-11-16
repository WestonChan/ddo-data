use crate::entity_slugs::resolve_entity_slug;
use crate::error::ApiError;
use crate::routes::v1::ValueVocabulary;
use axum::extract::{FromRequestParts, Query};
use axum::http::request::Parts;
use axum::http::Uri;
use rusqlite::Connection;
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
        $(vocabularies: { $($vocabulary_field:ident => $vocabulary:ident, $description:literal $(; example = $example:literal)?),* $(,)? })?
        $(matchable: [$($matchable_key:literal),* $(,)?])?
        $(single_value_match: [$($single_value_match_key:literal),* $(,)?])?
    ) => {
        #[derive(Default, serde::Deserialize)]
        $visibility struct $name {
            $($(#[$attribute])* $field_visibility $field: $field_type),*
        }

        impl crate::query::QueryParameters for $name {
            const KEYS: &'static [&'static str] = &[$(stringify!($field)),*];
            const REPEATABLE_FIELDS: &'static [(&'static str, bool)] = &[$((stringify!($field), <$field_type as crate::query::QueryParameterType>::IS_REPEATABLE)),*];
            const MATCHABLE_KEYS: &'static [&'static str] = &[$($($matchable_key),*)?];
            const SINGLE_VALUE_MATCH_KEYS: &'static [&'static str] = &[$($($single_value_match_key),*)?];
            const EFFECT_NAME_FIELDS: &'static [(&'static str, bool)] = &[$((stringify!($field), <$field_type as crate::query::QueryParameterType>::IS_EFFECT_NAME)),*];

            fn validate_filters(&mut self, db: &rusqlite::Connection) -> Result<(), crate::error::ApiError> {
                let _ = db;
                $($(crate::query::VocabularyFilter::validate_values(
                    &mut self.$vocabulary_field, db, stringify!($vocabulary_field), crate::routes::v1::ValueVocabulary::$vocabulary,
                )?;)*)?
                Ok(())
            }
        }

        impl utoipa::IntoParams for $name {
            fn into_params(_parameter_in_provider: impl Fn() -> Option<utoipa::openapi::path::ParameterIn>) -> Vec<utoipa::openapi::path::Parameter> {
                let vocabulary_parameters = vec![$($({
                    let parameter = crate::query::vocabulary_parameter(
                    stringify!($vocabulary_field), $description, &Self::default().$vocabulary_field,
                    crate::routes::v1::ValueVocabulary::$vocabulary,
                );
                $(let parameter = utoipa::openapi::path::ParameterBuilder::from(parameter).example(Some(serde_json::json!($example))).build();)?
                parameter
                }),*)?];
                crate::query::declared_name_parameters::<Self>().into_iter().chain(vocabulary_parameters).collect()
            }
        }
    };
}
pub(crate) use declare_query_parameters;

#[derive(Default, Deserialize)]
#[serde(transparent)]
pub(crate) struct EffectName(pub(crate) String);

impl From<String> for EffectName {
    fn from(name: String) -> Self {
        Self(name)
    }
}

pub(crate) trait QueryParameterType {
    const IS_EFFECT_NAME: bool = false;
    const IS_REPEATABLE: bool = false;
}

impl QueryParameterType for String {}
impl QueryParameterType for i64 {}
impl QueryParameterType for bool {}
impl QueryParameterType for EffectName {
    const IS_EFFECT_NAME: bool = true;
}
impl<T: QueryParameterType> QueryParameterType for Option<T> {
    const IS_EFFECT_NAME: bool = T::IS_EFFECT_NAME;
}
impl<T: QueryParameterType> QueryParameterType for Vec<T> {
    const IS_EFFECT_NAME: bool = T::IS_EFFECT_NAME;
    const IS_REPEATABLE: bool = true;
}

pub(crate) trait VocabularyFilter: QueryParameterType {
    fn validate_values(
        &mut self,
        db: &Connection,
        parameter: &str,
        vocabulary: ValueVocabulary,
    ) -> Result<(), ApiError>;
}

fn validate_text_values(
    values: &mut [String],
    db: &Connection,
    parameter: &str,
    vocabulary: ValueVocabulary,
) -> Result<(), ApiError> {
    if vocabulary.is_case_insensitive() {
        for value in values.iter_mut() {
            *value = value.to_lowercase();
        }
    }
    vocabulary.reject_unknown_values(db, parameter, values)
}

impl VocabularyFilter for Vec<String> {
    fn validate_values(
        &mut self,
        db: &Connection,
        parameter: &str,
        vocabulary: ValueVocabulary,
    ) -> Result<(), ApiError> {
        validate_text_values(self, db, parameter, vocabulary)
    }
}

impl VocabularyFilter for Option<String> {
    fn validate_values(
        &mut self,
        db: &Connection,
        parameter: &str,
        vocabulary: ValueVocabulary,
    ) -> Result<(), ApiError> {
        if let Some(value) = self {
            validate_text_values(std::slice::from_mut(value), db, parameter, vocabulary)?;
        }
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct EntityFilter {
    texts: Vec<String>,
    ids: Vec<i64>,
}

impl EntityFilter {
    pub(crate) fn ids(&self) -> &[i64] {
        &self.ids
    }
}

impl<'de> Deserialize<'de> for EntityFilter {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self { texts: repeated_key_values(deserializer)?, ids: Vec::new() })
    }
}

impl QueryParameterType for EntityFilter {
    const IS_REPEATABLE: bool = true;
}

impl VocabularyFilter for EntityFilter {
    fn validate_values(
        &mut self,
        db: &Connection,
        parameter: &str,
        vocabulary: ValueVocabulary,
    ) -> Result<(), ApiError> {
        let kind = vocabulary
            .entity_kind()
            .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("{parameter} requires an entity vocabulary")))?;
        self.ids = self
            .texts
            .iter()
            .map(|text| {
                if let Some(entity) = resolve_entity_slug(db, kind, text)? {
                    return Ok(entity.id);
                }
                vocabulary.reject_unknown_values(db, parameter, std::slice::from_ref(text))?;
                Err(ApiError::Internal(anyhow::anyhow!("accepted {parameter} {text:?} has no entity identity")))
            })
            .collect::<Result<_, ApiError>>()?;
        Ok(())
    }
}

pub(crate) fn vocabulary_parameter<T: VocabularyFilter>(
    name: &str,
    description: &str,
    _: &T,
    vocabulary: ValueVocabulary,
) -> Parameter {
    let parameter = if T::IS_REPEATABLE {
        ParameterBuilder::from(query_parameter::<Vec<String>>(name, description))
            .style(Some(ParameterStyle::Form))
            .explode(Some(true))
            .build()
    } else {
        query_parameter::<String>(name, description)
    };
    ParameterBuilder::from(parameter)
        .extensions(Some([("x-vocabulary", vocabulary.extension())].into_iter().collect()))
        .build()
}

pub(crate) fn declared_name_parameters<T: QueryParameters>() -> Vec<Parameter> {
    let mut parameters = Vec::new();
    for &(name, is_effect_name) in T::EFFECT_NAME_FIELDS {
        if is_effect_name {
            let parameter = query_parameter::<Vec<String>>(
                name,
                "Repeat effect, ladder, stat or alias names, or effect and alias slugs; `Constitution:Insight` narrows by type.",
            );
            parameters.push(
                ParameterBuilder::from(parameter)
                    .style(Some(ParameterStyle::Form))
                    .explode(Some(true))
                    .extensions(Some([("x-effect-name", true)].into_iter().collect()))
                    .build(),
            );
        }
    }
    for key in T::MATCHABLE_KEYS {
        let mut parameter = query_parameter::<String>(
            &format!("{key}_match"),
            &format!("Use `all` to require every {}, or `any` by default.", key.replace('_', " ")),
        );
        parameter.extensions = Some([("x-match-key", *key)].into_iter().collect());
        parameters.push(parameter);
    }
    for key in T::SINGLE_VALUE_MATCH_KEYS {
        let mut parameter = query_parameter::<String>(
            &format!("{key}_match"),
            &format!("Unsupported because an item has one {key}; this parameter returns 400."),
        );
        parameter.extensions = Some(
            [("x-match-key", serde_json::json!(key)), ("x-single-value", serde_json::json!(true))]
                .into_iter()
                .collect(),
        );
        parameters.push(parameter);
    }
    parameters
}

pub(crate) struct EffectSlugParameters;

impl IntoParams for EffectSlugParameters {
    fn into_params(_parameter_in_provider: impl Fn() -> Option<ParameterIn>) -> Vec<Parameter> {
        vec![ParameterBuilder::from(query_parameter::<String>(
            "slug",
            "Current effect slug or numeric id; alias slugs redirect to their canonical effect.",
        ))
        .parameter_in(ParameterIn::Path)
        .required(Required::True)
        .extensions(Some([("x-effect-name", true)].into_iter().collect()))
        .build()]
    }
}

declare_query_parameters! {
    pub(crate) struct ListQuery {
        pub q: Option<String>,
        pub limit: Option<i64>,
        pub offset: Option<i64>,
        #[serde(default, deserialize_with = "repeated_sort_values")]
        pub sort: Vec<String>,
    }
}

declare_query_parameters! {
    pub(crate) struct NoFilters {}
}

pub(crate) struct Unvalidated<T> {
    parameters: T,
}

impl<T: QueryParameters> Unvalidated<T> {
    pub(crate) fn validated(mut self, db: &Connection) -> Result<T, ApiError> {
        self.parameters.validate_filters(db)?;
        Ok(self.parameters)
    }
}

pub(crate) struct ApiQuery<T = NoFilters>(pub ListQuery, pub Unvalidated<T>);
pub(crate) struct ApiFilterQuery<T>(pub Unvalidated<T>);

pub(crate) trait QueryParameters: DeserializeOwned {
    const KEYS: &'static [&'static str];
    const REPEATABLE_FIELDS: &'static [(&'static str, bool)] = &[];
    const MATCHABLE_KEYS: &'static [&'static str] = &[];
    const SINGLE_VALUE_MATCH_KEYS: &'static [&'static str] = &[];
    const EFFECT_NAME_FIELDS: &'static [(&'static str, bool)] = &[];
    fn validate_filters(&mut self, db: &Connection) -> Result<(), ApiError>;
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
        Ok(ApiQuery(query_from_pairs(list_pairs)?, Unvalidated { parameters: query_from_pairs(filter_pairs)? }))
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
        Ok(ApiFilterQuery(Unvalidated { parameters: query_from_pairs(pairs)? }))
    }
}

fn query_from_pairs<T: QueryParameters>(pairs: Vec<(String, String)>) -> Result<T, ApiError> {
    let repeatable_keys = T::REPEATABLE_FIELDS
        .iter()
        .filter_map(|(key, is_repeatable)| is_repeatable.then_some(*key))
        .collect::<Vec<_>>();
    let joined_query_string = serde_urlencoded::to_string(pairs_with_repeated_keys_joined(pairs, &repeatable_keys))
        .map_err(|e| ApiError::Internal(e.into()))?;
    let joined_uri: Uri = format!("/?{joined_query_string}")
        .parse()
        .map_err(|e: axum::http::uri::InvalidUri| ApiError::Internal(e.into()))?;
    let Query(query) = Query::<T>::try_from_uri(&joined_uri).map_err(|e| ApiError::BadRequest(e.body_text()))?;
    Ok(query)
}

fn common_list_parameters() -> Vec<Parameter> {
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

pub(crate) struct SearchParameters {
    description: Option<&'static str>,
    matches_effect_names: bool,
}

pub(crate) const EFFECT_NAME_SEARCH: SearchParameters =
    SearchParameters { description: None, matches_effect_names: true };

impl SearchParameters {
    pub(crate) fn with_description(mut self, description: &'static str) -> Self {
        self.description = Some(description);
        self
    }
}

impl From<Option<&'static str>> for SearchParameters {
    fn from(description: Option<&'static str>) -> Self {
        Self { description, matches_effect_names: false }
    }
}

pub(crate) fn list_parameters(
    sortable_fields: &[(&str, &str)],
    sort_note: &str,
    search: impl Into<SearchParameters>,
) -> Vec<Parameter> {
    let search = search.into();
    let mut parameters = common_list_parameters();
    for parameter in &mut parameters {
        if parameter.name == "q" {
            if let Some(description) = search.description {
                parameter.description = Some(description.to_string());
            }
            if search.matches_effect_names {
                parameter.extensions = Some([("x-effect-name", true)].into_iter().collect());
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

pub(crate) fn repeated_key_values<'de, D: Deserializer<'de>, T: From<String>>(
    deserializer: D,
) -> Result<Vec<T>, D::Error> {
    let joined_values = String::deserialize(deserializer)?;
    Ok(joined_values
        .split(REPEATED_VALUE_SEPARATOR)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|text| T::from(text.to_string()))
        .collect())
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

#[cfg(test)]
#[path = "../tests/common/mod.rs"]
mod fixture_files;

#[cfg(test)]
mod vocabulary_filter_tests {
    use super::*;
    use rusqlite::Connection;

    declare_query_parameters! {
        struct DeclaredFilters {
            #[serde(default, deserialize_with = "repeated_key_values")]
            family: Vec<String>,
            socket: Option<String>,
            #[serde(default)]
            crafting_system: EntityFilter,
        }
        vocabularies: {
            family => AugmentSlotFamilies, "Repeat socket family names to match any family.",
            socket => AugmentSlotTypes, "Choose a case-insensitive socket label."; example = "sun",
            crafting_system => CraftingSystems, "Repeat crafting system slugs or numeric ids.",
        }
    }

    fn fixture_database() -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ddo-etl/tests/fixtures/data");
        ddo_etl::tree::build_from_data(&fixtures, &mut db).unwrap();
        db
    }

    async fn extracted_filters(query: &str) -> Unvalidated<DeclaredFilters> {
        let (mut parts, ()) = axum::http::Request::get(format!("/?{query}")).body(()).unwrap().into_parts();
        let ApiQuery(_, filters) = ApiQuery::<DeclaredFilters>::from_request_parts(&mut parts, &()).await.unwrap();
        filters
    }

    fn bad_request_message(error: ApiError) -> String {
        let ApiError::BadRequest(message) = error else { panic!("expected a bad request, received {error:?}") };
        message
    }

    #[tokio::test]
    async fn declared_vocabulary_filters_validate_repeated_and_optional_values() {
        let db = fixture_database();
        let families = db
            .prepare("SELECT DISTINCT family FROM augment_slot_types ORDER BY family")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        let query = serde_urlencoded::to_string([("family", families[0].as_str()), ("family", "made-up")]).unwrap();
        let error = extracted_filters(&query).await.validated(&db).err().unwrap();
        assert_eq!(
            bad_request_message(error),
            format!("unknown family \"made-up\"; accepted values: {}", families.join(", "))
        );
        let labels = db
            .prepare("SELECT label FROM augment_slot_types ORDER BY family, label")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        let error = extracted_filters("socket=made-up").await.validated(&db).err().unwrap();
        assert_eq!(
            bad_request_message(error),
            format!("unknown socket \"made-up\"; accepted values: {}", labels.join(", "))
        );
        let filters = extracted_filters("socket=SuN&family=&family=standard").await.validated(&db).unwrap();
        assert_eq!(filters.socket.as_deref(), Some("sun"));
        assert_eq!(filters.family, ["standard"]);
        let filters = extracted_filters("").await.validated(&db).unwrap();
        assert!(filters.family.is_empty());
        assert!(filters.socket.is_none());
        assert!(filters.crafting_system.ids().is_empty());
    }

    #[tokio::test]
    async fn both_query_extractors_require_vocabulary_validation() {
        let db = fixture_database();
        let (mut parts, ()) = axum::http::Request::get("/?socket=made-up").body(()).unwrap().into_parts();
        let ApiFilterQuery(filters) =
            ApiFilterQuery::<DeclaredFilters>::from_request_parts(&mut parts, &()).await.unwrap();
        assert!(filters.validated(&db).is_err());
    }

    #[tokio::test]
    async fn entity_filters_resolve_current_redirected_and_numeric_addresses() {
        let original = fixture_database();
        let (id, slug): (i64, String) = original
            .query_row("SELECT id, slug FROM crafting_systems ORDER BY slug LIMIT 1", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        let root = std::env::temp_dir().join(format!("ddo-query-redirects-{}", std::process::id()));
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ddo-etl/tests/fixtures/data");
        super::fixture_files::copy_files(&fixtures, &root);
        std::fs::write(root.join("redirects.toml"), format!("[[redirect]]\nkind = \"crafting-systems\"\nfrom = \"former-system\"\nto = {slug:?}\nsince = \"2026-11-15\"\n")).unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        ddo_etl::tree::build_from_data(&root, &mut db).unwrap();
        let query = serde_urlencoded::to_string([
            ("crafting_system", slug),
            ("crafting_system", id.to_string()),
            ("crafting_system", "former-system".to_string()),
        ])
        .unwrap();
        let filters = extracted_filters(&query).await.validated(&db).unwrap();
        assert_eq!(filters.crafting_system.ids(), [id, id, id]);
        let error = extracted_filters("crafting_system=made-up").await.validated(&db).err().unwrap();
        assert!(bad_request_message(error).starts_with("unknown crafting_system \"made-up\"; accepted values: "));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn vocabulary_parameters_derive_repeatability_and_serving_routes() {
        let parameters = serde_json::to_value(DeclaredFilters::into_params(|| None)).unwrap();
        let parameters = parameters.as_array().unwrap();
        let family = parameters.iter().find(|parameter| parameter["name"] == "family").unwrap();
        assert_eq!(family["schema"]["type"], "array");
        assert_eq!(family["style"], "form");
        assert_eq!(family["explode"], true);
        assert_eq!(family["x-vocabulary"]["name"], "AugmentSlotFamilies");
        assert_eq!(family["x-vocabulary"]["route"], "/v1/augment-slot-types");
        assert_eq!(family["x-vocabulary"]["rows_key"], "augment_slot_types");
        assert_eq!(family["x-vocabulary"]["value_key"], "family");
        let socket = parameters.iter().find(|parameter| parameter["name"] == "socket").unwrap();
        assert_eq!(socket["schema"]["type"], "string");
        assert_eq!(socket["example"], "sun");
        assert!(socket.get("explode").is_none());
    }
}
