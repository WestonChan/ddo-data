use crate::error::ApiError;
use ddo_model::enums::EntityKind;
use rusqlite::Connection;
use std::sync::LazyLock;

pub(super) struct VocabularySource {
    value_column: &'static str,
    default_order: &'static [&'static str],
    values_sql: String,
    counted_listing: Option<CountedListing>,
    served_by: Option<(&'static str, &'static str)>,
    entity_kind: Option<EntityKind>,
    is_case_insensitive: bool,
}

struct CountedListing {
    columns: fn(&str) -> String,
    joins: fn(&str) -> String,
}

impl VocabularySource {
    pub(super) fn new(
        value_column: &'static str,
        default_order: &'static [&'static str],
        values_sql: impl Into<String>,
    ) -> Self {
        Self {
            value_column,
            default_order,
            values_sql: values_sql.into(),
            counted_listing: None,
            served_by: None,
            entity_kind: None,
            is_case_insensitive: false,
        }
    }

    pub(super) fn of_names(names: impl IntoIterator<Item = &'static str>) -> Self {
        let selects: Vec<_> =
            names.into_iter().map(|name| format!("SELECT '{}' AS name", name.replace('\'', "''"))).collect();
        let values_sql =
            if selects.is_empty() { "SELECT NULL AS name WHERE 0".into() } else { selects.join(" UNION ALL ") };
        Self::new("name", &["name"], values_sql)
    }

    pub(super) fn with_counted_listing(mut self, columns: fn(&str) -> String, joins: fn(&str) -> String) -> Self {
        self.counted_listing = Some(CountedListing { columns, joins });
        self
    }

    pub(super) fn served_by(mut self, route: &'static str, rows_key: &'static str) -> Self {
        self.served_by = Some((route, rows_key));
        self
    }

    pub(super) fn for_entities(mut self, kind: EntityKind) -> Self {
        self.entity_kind = Some(kind);
        self
    }

    pub(super) fn case_insensitive(mut self) -> Self {
        self.is_case_insensitive = true;
        self
    }
}

macro_rules! declare_value_vocabularies {
    ($visibility:vis enum $name:ident { $($variant:ident => $source:path $(; $filter:ident = $value:literal)?),* $(,)? }) => {
        #[derive(Clone, Copy, Debug)]
        $visibility enum $name {
            $($variant),*
        }

        impl $name {
            #[cfg(test)]
            const ALL: &'static [Self] = &[$(Self::$variant),*];

            fn name(&self) -> &'static str {
                match self { $(Self::$variant => stringify!($variant)),* }
            }

            fn queries(&self) -> &'static VocabularyQueries {
                match self {
                    $(Self::$variant => {
                        static QUERIES: LazyLock<VocabularyQueries> =
                            LazyLock::new(|| VocabularyQueries::new(&$source(), &[$((stringify!($filter), $value))?]));
                        &QUERIES
                    }),*
                }
            }
        }
    };
}

declare_value_vocabularies! {
    pub(crate) enum ValueVocabulary {
        WeaponAbilities => super::items::weapon_abilities_vocabulary,
        ItemMaterials => super::items::item_materials_vocabulary,
        AugmentSlotTypes => super::items::augment_slot_types_vocabulary,
        AugmentSlotFamilies => super::items::augment_slot_families_vocabulary,
        AugmentFamilies => super::augments::augment_families_vocabulary,
        CraftingSystems => super::crafting::crafting_systems_vocabulary,
        ItemCraftingSystems => super::crafting::crafting_systems_vocabulary; has_items = true,
        EquipmentSlots => super::items::equipment_slots_vocabulary,
        ItemCategories => super::items::item_categories_vocabulary,
        AdventurePacks => super::quests::adventure_packs_vocabulary,
        Sets => super::sets::sets_vocabulary,
        Quests => super::quests::quests_vocabulary,
        QuestChains => super::quest_series::quest_chains_vocabulary,
        Sagas => super::quest_series::sagas_vocabulary,
        ItemViews => super::items::item_views_vocabulary,
        FeatSources => super::feats::feat_sources_vocabulary,
        FeatGroups => super::feats::feat_groups_vocabulary,
        FeatAcquisitions => super::feats::feat_acquisitions_vocabulary,
        SpellSchools => super::spells::spell_schools_vocabulary,
        Classes => super::classes::classes_vocabulary,
        EffectKinds => super::bonuses::effect_kinds_vocabulary,
    }
}

impl ValueVocabulary {
    pub(super) fn select_sql(&self) -> &'static str {
        &self.queries().select_sql
    }

    pub(super) fn default_order(&self) -> &'static str {
        &self.queries().default_order
    }

    pub(crate) fn reject_unknown_values(
        &self,
        db: &Connection,
        parameter: &str,
        values: &[String],
    ) -> Result<(), ApiError> {
        self.queries().reject_unknown_values(db, parameter, values)
    }

    pub(crate) fn entity_kind(&self) -> Option<EntityKind> {
        self.queries().entity_kind
    }

    pub(crate) fn first_unknown_value_index(
        &self,
        db: &Connection,
        values: &[String],
    ) -> Result<Option<usize>, ApiError> {
        self.queries().first_unknown_value_index(db, values)
    }

    pub(crate) fn unknown_value_error(&self, db: &Connection, parameter: &str, value: &str) -> ApiError {
        self.queries().unknown_value_error(db, parameter, value)
    }

    pub(crate) fn case_normalized_value<'value>(&self, value: &'value str) -> std::borrow::Cow<'value, str> {
        self.queries().case_normalized_value(value)
    }

    pub(crate) fn extension(&self) -> serde_json::Value {
        let mut extension = serde_json::json!({"name": self.name()});
        if let Some((route, rows_key, value_key)) = self.queries().served_by {
            extension["route"] = route.into();
            extension["rows_key"] = rows_key.into();
            extension["value_key"] = value_key.into();
            if !self.queries().fixed_boolean_filters.is_empty() {
                extension["parameters"] = self
                    .queries()
                    .fixed_boolean_filters
                    .iter()
                    .map(|(column, value)| ((*column).to_string(), serde_json::Value::String(value.to_string())))
                    .collect::<serde_json::Map<_, _>>()
                    .into();
            }
        }
        extension
    }
}

struct VocabularyQueries {
    select_sql: String,
    validation_sql: String,
    membership_sql: [String; 2],
    default_order: String,
    served_by: Option<(&'static str, &'static str, &'static str)>,
    fixed_boolean_filters: &'static [(&'static str, bool)],
    entity_kind: Option<EntityKind>,
    is_case_insensitive: bool,
}

impl VocabularyQueries {
    fn new(source: &VocabularySource, fixed_boolean_filters: &'static [(&'static str, bool)]) -> Self {
        let vocabulary_alias = "vocabulary";
        let listed_alias = "listed";
        let mut values_sql = source.values_sql.clone();
        for (column, value) in fixed_boolean_filters {
            values_sql =
                format!("SELECT * FROM ({values_sql}) filtered WHERE filtered.{column} = {}", i32::from(*value));
        }
        let value_column = source.value_column;
        let (list_columns, count_joins) = source.counted_listing.as_ref().map_or_else(
            || (format!("{vocabulary_alias}.*"), String::new()),
            |listing| ((listing.columns)(vocabulary_alias), (listing.joins)(vocabulary_alias)),
        );
        let default_order =
            source.default_order.iter().map(|column| format!("{listed_alias}.{column}")).collect::<Vec<_>>().join(", ");
        let mut membership_sql = ["?1", "SELECT value FROM json_each(?1)"].map(|bound_values| format!(
            "SELECT DISTINCT {listed_alias}.{value_column} COLLATE BINARY FROM ({values_sql}) {listed_alias}
             WHERE {listed_alias}.{value_column} IS NOT NULL AND {listed_alias}.{value_column} COLLATE BINARY IN ({bound_values}) LIMIT ?2"
        ));
        membership_sql[0] = format!("SELECT EXISTS ({})", membership_sql[0]);
        Self {
            select_sql: format!("SELECT {list_columns} FROM ({values_sql}) {vocabulary_alias} {count_joins}"),
            validation_sql: format!(
                "SELECT {listed_alias}.{value_column} FROM ({values_sql}) {listed_alias} WHERE {listed_alias}.{value_column} IS NOT NULL ORDER BY {default_order}"
            ),
            membership_sql,
            default_order,
            served_by: source.served_by.map(|(route, rows_key)| (route, rows_key, source.value_column)),
            fixed_boolean_filters,
            entity_kind: source.entity_kind,
            is_case_insensitive: source.is_case_insensitive,
        }
    }

    fn reject_unknown_values(&self, db: &Connection, parameter: &str, values: &[String]) -> Result<(), ApiError> {
        if let Some(index) = self.first_unknown_value_index(db, values)? {
            return Err(self.unknown_value_error(db, parameter, &values[index]));
        }
        Ok(())
    }

    fn first_unknown_value_index(&self, db: &Connection, values: &[String]) -> Result<Option<usize>, ApiError> {
        if values.is_empty() {
            return Ok(None);
        }
        let normalized = values.iter().map(|value| self.case_normalized_value(value)).collect::<Vec<_>>();
        let distinct = normalized.iter().collect::<std::collections::BTreeSet<_>>();
        if distinct.len() == 1 {
            let is_accepted = db
                .prepare_cached(&self.membership_sql[0])?
                .query_row(rusqlite::params![normalized[0].as_ref(), 1], |row| row.get::<_, bool>(0))?;
            return Ok((!is_accepted).then_some(0));
        }
        let mut statement = db.prepare_cached(&self.membership_sql[1])?;
        let matched = statement
            .query_map(
                rusqlite::params![
                    serde_json::to_string(&distinct).map_err(anyhow::Error::from)?,
                    distinct.len() as i64
                ],
                |row| row.get::<_, String>(0),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(normalized.iter().position(|value| !matched.iter().any(|name| name == value.as_ref())))
    }

    fn case_normalized_value<'value>(&self, value: &'value str) -> std::borrow::Cow<'value, str> {
        if !self.is_case_insensitive || (value.is_ascii() && !value.bytes().any(|byte| byte.is_ascii_uppercase())) {
            std::borrow::Cow::Borrowed(value)
        } else {
            std::borrow::Cow::Owned(value.to_lowercase())
        }
    }

    fn unknown_value_error(&self, db: &Connection, parameter: &str, value: &str) -> ApiError {
        let accepted = db.prepare_cached(&self.validation_sql).and_then(|mut statement| {
            statement.query_map([], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()
        });
        match accepted {
            Ok(accepted) => {
                ApiError::BadRequest(format!("unknown {parameter} {value:?}; accepted values: {}", accepted.join(", ")))
            }
            Err(error) => error.into(),
        }
    }
}

#[cfg(test)]
mod vocabulary_tests {
    use super::{ValueVocabulary, VocabularyQueries, VocabularySource};

    #[test]
    fn membership_validation_lists_on_failure_and_batches_repeated_values() {
        use rusqlite::trace::{TraceEvent, TraceEventCodes};
        use std::sync::atomic::{AtomicUsize, Ordering};

        static QUERY_COUNT: AtomicUsize = AtomicUsize::new(0);
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ddo-etl/tests/fixtures/data");
        ddo_etl::tree::build_from_data(&fixtures, &mut db).unwrap();
        for vocabulary in ValueVocabulary::ALL {
            let accepted = db
                .prepare(&vocabulary.queries().validation_sql)
                .unwrap()
                .query_map([], |row| row.get::<_, String>(0))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            db.trace_v2(
                TraceEventCodes::SQLITE_TRACE_STMT,
                Some(|event| {
                    if let TraceEvent::Stmt(_, _) = event {
                        QUERY_COUNT.fetch_add(1, Ordering::SeqCst);
                    }
                }),
            );
            let mut cases = vec![
                (vec![], None, 0),
                (accepted.clone(), None, usize::from(!accepted.is_empty())),
                (vec!["made-up".into()], Some("made-up"), 2),
            ];
            if let Some(first) = accepted.first() {
                cases.extend([
                    (vec![first.clone()], None, 1),
                    (vec![first.clone(); 1000], None, 1),
                    (vec![first.clone(), "made-up".into(), "another-unknown".into()], Some("made-up"), 2),
                ]);
            }
            for (values, unknown, expected_queries) in cases {
                QUERY_COUNT.store(0, Ordering::SeqCst);
                let result = vocabulary.reject_unknown_values(&db, "probe", &values);
                assert_eq!(QUERY_COUNT.load(Ordering::SeqCst), expected_queries, "{vocabulary:?}: {values:?}");
                if let Some(unknown) = unknown {
                    let crate::error::ApiError::BadRequest(message) = result.unwrap_err() else {
                        panic!("expected an unknown vocabulary value")
                    };
                    assert_eq!(message, format!("unknown probe {unknown:?}; accepted values: {}", accepted.join(", ")));
                } else {
                    result.unwrap();
                }
            }
            db.trace_v2(TraceEventCodes::empty(), None);
        }
    }

    #[test]
    fn nullable_vocabulary_columns_never_break_validation() {
        use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ddo-etl/tests/fixtures/data");
        ddo_etl::tree::build_from_data(&fixtures, &mut db).unwrap();
        let mut injected_columns = Vec::new();
        for vocabulary in ValueVocabulary::ALL {
            let sql = &vocabulary.queries().validation_sql;
            let statement = db.prepare(sql).unwrap();
            let columns = statement.columns_with_metadata();
            let (Some(table), Some(column)) = (columns[0].table_name(), columns[0].origin_name()) else {
                continue;
            };
            let is_not_null = db
                .query_row("SELECT \"notnull\" FROM pragma_table_info(?1) WHERE name = ?2", [table, column], |row| {
                    row.get::<_, bool>(0)
                })
                .unwrap();
            if is_not_null {
                continue;
            }
            let table = table.to_string();
            let column = column.to_string();
            drop(statement);
            let before = db
                .prepare(sql)
                .unwrap()
                .query_map([], |row| row.get::<_, String>(0))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            injected_columns.push(format!("{table}.{column}"));
            let null_table = table.clone();
            let null_column = column.clone();
            db.authorizer(Some(move |context: AuthContext<'_>| match context.action {
                AuthAction::Read { table_name, column_name }
                    if table_name == null_table && column_name == null_column =>
                {
                    Authorization::Ignore
                }
                _ => Authorization::Allow,
            }))
            .unwrap();
            for value in before.iter().map(String::as_str).chain(["made-up"]) {
                let error = vocabulary.reject_unknown_values(&db, "probe", &[value.to_string()]).unwrap_err();
                let crate::error::ApiError::BadRequest(message) = error else {
                    panic!("{vocabulary:?} with NULL in {table}.{column} returned {error:?}");
                };
                assert_eq!(message, format!("unknown probe {value:?}; accepted values: "));
            }
            db.authorizer(None::<fn(AuthContext<'_>) -> Authorization>).unwrap();
            vocabulary.reject_unknown_values(&db, "probe", &before).unwrap();
        }
        assert!(injected_columns.contains(&"feats.acquire".to_string()), "{injected_columns:?}");
        assert!(injected_columns.contains(&"json_each.value".to_string()), "{injected_columns:?}");
    }

    #[test]
    fn unresolved_entity_vocabulary_values_are_bad_requests() {
        use crate::query::{EntityFilter, VocabularyFilter};
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ddo-etl/tests/fixtures/data");
        ddo_etl::tree::build_from_data(&fixtures, &mut db).unwrap();
        db.execute_batch("DELETE FROM entity_identity").unwrap();
        for vocabulary in ValueVocabulary::ALL.iter().filter(|vocabulary| vocabulary.entity_kind().is_some()) {
            let accepted = db
                .prepare(&vocabulary.queries().validation_sql)
                .unwrap()
                .query_map([], |row| row.get::<_, String>(0))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            for value in accepted.iter().map(String::as_str).chain(["made-up"]) {
                let mut filter: EntityFilter = serde_json::from_value(serde_json::json!(value)).unwrap();
                let error = filter.validate_values(&db, "probe", *vocabulary).unwrap_err();
                let crate::error::ApiError::BadRequest(message) = error else {
                    panic!("{vocabulary:?} without an identity returned {error:?}");
                };
                assert_eq!(message, format!("unknown probe {value:?}; accepted values: {}", accepted.join(", ")));
            }
        }
    }

    #[test]
    fn static_vocabularies_preserve_quoted_names_and_allow_empty_lists() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        for names in [vec!["O'Brien", "Another"], vec![]] {
            let vocabulary = VocabularyQueries::new(&VocabularySource::of_names(names.iter().copied()), &[]);
            for name in &names {
                vocabulary.reject_unknown_values(&db, "name", &[name.to_string()]).unwrap();
            }
            let mut accepted = names;
            accepted.sort_unstable();
            let error = vocabulary.reject_unknown_values(&db, "name", &["made-up".into()]).unwrap_err();
            let crate::error::ApiError::BadRequest(message) = error else { panic!("expected an unknown value") };
            assert_eq!(message, format!("unknown name \"made-up\"; accepted values: {}", accepted.join(", ")));
        }
    }

    #[test]
    fn membership_preserves_exact_case_with_case_insensitive_columns() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE names (name TEXT COLLATE NOCASE); INSERT INTO names VALUES ('Alpha'), ('alpha')",
        )
        .unwrap();
        let vocabulary =
            VocabularyQueries::new(&VocabularySource::new("name", &["name"], "SELECT name FROM names"), &[]);
        for values in [vec!["Alpha".into()], vec!["alpha".into()], vec!["Alpha".into(), "alpha".into()]] {
            vocabulary.reject_unknown_values(&db, "probe", &values).unwrap();
        }
        for values in [vec!["ALPHA".into()], vec!["alpha".into(), "ALPHA".into(), "missing".into()]] {
            let error = vocabulary.reject_unknown_values(&db, "probe", &values).unwrap_err();
            let crate::error::ApiError::BadRequest(message) = error else { panic!("expected an unknown value") };
            assert_eq!(message, "unknown probe \"ALPHA\"; accepted values: Alpha, alpha");
        }
    }

    #[test]
    fn every_value_vocabulary_validates_without_counting() {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ddo-etl/tests/fixtures/data");
        ddo_etl::tree::build_from_data(&fixtures, &mut db).unwrap();
        let mut violations = Vec::new();
        for vocabulary in ValueVocabulary::ALL {
            let queries = vocabulary.queries();
            for sql in std::iter::once(&queries.validation_sql).chain(&queries.membership_sql) {
                for (explain, column) in [("EXPLAIN QUERY PLAN", 3), ("EXPLAIN", 1)] {
                    let mut statement = db.prepare(&format!("{explain} {sql}")).unwrap();
                    let parameters =
                        rusqlite::params_from_iter(std::iter::repeat_n("probe", statement.parameter_count()));
                    for step in statement.query_map(parameters, |row| row.get::<_, String>(column)).unwrap() {
                        let step = step.unwrap();
                        if step.contains("GROUP BY") || step.starts_with("Agg") || step == "Count" {
                            violations.push(format!("{vocabulary:?}: {step}"));
                        }
                    }
                }
            }
        }
        assert!(violations.is_empty(), "{}", violations.join("\n"));
    }

    #[test]
    fn every_value_vocabulary_avoids_correlated_table_scans() {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ddo-etl/tests/fixtures/data");
        ddo_etl::tree::build_from_data(&fixtures, &mut db).unwrap();
        let mut violations = Vec::new();
        for vocabulary in ValueVocabulary::ALL {
            let queries = vocabulary.queries();
            for sql in [&queries.select_sql, &queries.validation_sql].into_iter().chain(&queries.membership_sql) {
                let mut statement = db.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
                let parameters = rusqlite::params_from_iter(std::iter::repeat_n("probe", statement.parameter_count()));
                let plan = statement
                    .query_map(parameters, |row| {
                        Ok((row.get::<_, i64>(0)?, (row.get::<_, i64>(1)?, row.get::<_, String>(3)?)))
                    })
                    .unwrap()
                    .collect::<rusqlite::Result<std::collections::BTreeMap<_, _>>>()
                    .unwrap();
                for (parent, detail) in plan.values().filter(|(_, detail)| detail.starts_with("SCAN ")) {
                    let mut ancestor = *parent;
                    while let Some((parent, ancestor_detail)) = plan.get(&ancestor) {
                        if ancestor_detail.starts_with("CORRELATED ") {
                            violations.push(format!("{vocabulary:?}: {ancestor_detail} -> {detail}"));
                        }
                        ancestor = *parent;
                    }
                }
            }
        }
        assert!(violations.is_empty(), "{}", violations.join("\n"));
    }
}
