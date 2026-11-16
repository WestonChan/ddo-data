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
    checks_membership_first: bool,
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
            checks_membership_first: false,
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

    pub(super) fn with_membership_check(mut self) -> Self {
        self.checks_membership_first = true;
        self
    }
}

macro_rules! declare_value_vocabularies {
    ($visibility:vis enum $name:ident { $($variant:ident => $source:path),* $(,)? }) => {
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
                            LazyLock::new(|| VocabularyQueries::new(&$source()));
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

    pub(crate) fn is_case_insensitive(&self) -> bool {
        self.queries().is_case_insensitive
    }

    pub(crate) fn extension(&self) -> serde_json::Value {
        let mut extension = serde_json::json!({"name": self.name()});
        if let Some((route, rows_key, value_key)) = self.queries().served_by {
            extension["route"] = route.into();
            extension["rows_key"] = rows_key.into();
            extension["value_key"] = value_key.into();
        }
        extension
    }
}

struct VocabularyQueries {
    select_sql: String,
    validation_sql: String,
    membership_sql: Option<String>,
    default_order: String,
    served_by: Option<(&'static str, &'static str, &'static str)>,
    entity_kind: Option<EntityKind>,
    is_case_insensitive: bool,
}

impl VocabularyQueries {
    fn new(source: &VocabularySource) -> Self {
        let vocabulary_alias = "vocabulary";
        let listed_alias = "listed";
        let values_sql = &source.values_sql;
        let value_column = source.value_column;
        let (list_columns, count_joins) = source.counted_listing.as_ref().map_or_else(
            || (format!("{vocabulary_alias}.*"), String::new()),
            |listing| ((listing.columns)(vocabulary_alias), (listing.joins)(vocabulary_alias)),
        );
        let default_order =
            source.default_order.iter().map(|column| format!("{listed_alias}.{column}")).collect::<Vec<_>>().join(", ");
        Self {
            select_sql: format!("SELECT {list_columns} FROM ({values_sql}) {vocabulary_alias} {count_joins}"),
            validation_sql: format!(
                "SELECT {listed_alias}.{value_column} FROM ({values_sql}) {listed_alias} ORDER BY {default_order}"
            ),
            membership_sql: source.checks_membership_first.then(|| {
                format!("SELECT EXISTS (SELECT 1 FROM ({values_sql}) {listed_alias} WHERE {listed_alias}.{value_column} = ?1)")
            }),
            default_order,
            served_by: source.served_by.map(|(route, rows_key)| (route, rows_key, source.value_column)),
            entity_kind: source.entity_kind,
            is_case_insensitive: source.is_case_insensitive,
        }
    }

    fn reject_unknown_values(&self, db: &Connection, parameter: &str, values: &[String]) -> Result<(), ApiError> {
        if values.is_empty() {
            return Ok(());
        }
        if let ([value], Some(sql)) = (values, &self.membership_sql) {
            if db.prepare_cached(sql)?.query_row([value], |row| row.get::<_, bool>(0))? {
                return Ok(());
            }
        }
        let mut statement = db.prepare_cached(&self.validation_sql)?;
        let accepted = statement.query_map([], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        for value in values {
            if !accepted.contains(value) {
                return Err(ApiError::BadRequest(format!(
                    "unknown {parameter} {value:?}; accepted values: {}",
                    accepted.join(", ")
                )));
            }
        }
        Ok(())
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
        let accepted = db
            .prepare("SELECT DISTINCT family FROM augments ORDER BY family")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert!(accepted.len() > 1);
        db.trace_v2(
            TraceEventCodes::SQLITE_TRACE_STMT,
            Some(|event| {
                if let TraceEvent::Stmt(_, _) = event {
                    QUERY_COUNT.fetch_add(1, Ordering::SeqCst);
                }
            }),
        );
        for (values, unknown, expected_queries) in [
            (vec![], None, 0),
            (vec![accepted[0].clone()], None, 1),
            (accepted.clone(), None, 1),
            (vec!["made-up".into()], Some("made-up"), 2),
            (vec![accepted[0].clone(), "made-up".into(), "another-unknown".into()], Some("made-up"), 1),
        ] {
            QUERY_COUNT.store(0, Ordering::SeqCst);
            let result = ValueVocabulary::AugmentFamilies.reject_unknown_values(&db, "family", &values);
            assert_eq!(QUERY_COUNT.load(Ordering::SeqCst), expected_queries, "{values:?}");
            if let Some(unknown) = unknown {
                let crate::error::ApiError::BadRequest(message) = result.unwrap_err() else {
                    panic!("expected an unknown family")
                };
                assert_eq!(message, format!("unknown family {unknown:?}; accepted values: {}", accepted.join(", ")));
            } else {
                result.unwrap();
            }
        }
    }

    #[test]
    fn static_vocabularies_preserve_quoted_names_and_allow_empty_lists() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        for names in [vec!["O'Brien", "Another"], vec![]] {
            let vocabulary = VocabularyQueries::new(&VocabularySource::of_names(names.iter().copied()));
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
    fn every_value_vocabulary_validates_without_counting() {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ddo-etl/tests/fixtures/data");
        ddo_etl::tree::build_from_data(&fixtures, &mut db).unwrap();
        let mut violations = Vec::new();
        for vocabulary in ValueVocabulary::ALL {
            let queries = vocabulary.queries();
            for sql in std::iter::once(queries.validation_sql.as_str()).chain(queries.membership_sql.as_deref()) {
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
            for sql in [queries.select_sql.as_str(), queries.validation_sql.as_str()]
                .into_iter()
                .chain(queries.membership_sql.as_deref())
            {
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
