use crate::error::ApiError;
use rusqlite::Connection;
use std::sync::LazyLock;

pub(super) struct VocabularySource {
    pub(super) value_column: &'static str,
    pub(super) default_order: &'static [&'static str],
    pub(super) values_sql: String,
    pub(super) options: VocabularyOptions,
}

#[derive(Default)]
pub(super) struct VocabularyOptions {
    list_columns: Option<fn(&str) -> String>,
    count_joins: Option<fn(&str) -> String>,
}

impl VocabularyOptions {
    pub(super) fn with_list_columns(mut self, columns: fn(&str) -> String) -> Self {
        self.list_columns = Some(columns);
        self
    }

    pub(super) fn with_count_joins(mut self, joins: fn(&str) -> String) -> Self {
        self.count_joins = Some(joins);
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

            fn queries(&self) -> &'static VocabularyQueries {
                match self {
                    $(Self::$variant => {
                        static QUERIES: LazyLock<VocabularyQueries> =
                            LazyLock::new(|| VocabularyQueries::new(&$source));
                        &QUERIES
                    }),*
                }
            }
        }
    };
}

declare_value_vocabularies! {
    pub(super) enum ValueVocabulary {
        WeaponAbilities => super::items::WEAPON_ABILITIES_VOCABULARY,
        ItemMaterials => super::items::ITEM_MATERIALS_VOCABULARY,
        AugmentSlotTypes => super::items::AUGMENT_SLOT_TYPES_VOCABULARY,
        AugmentSlotFamilies => super::items::AUGMENT_SLOT_FAMILIES_VOCABULARY,
        CraftingSystems => super::crafting::CRAFTING_SYSTEMS_VOCABULARY,
    }
}

impl ValueVocabulary {
    pub(super) fn select_sql(&self) -> &'static str {
        &self.queries().select_sql
    }

    pub(super) fn default_order(&self) -> &'static str {
        &self.queries().default_order
    }

    pub(super) fn reject_unknown_values(
        &self,
        db: &Connection,
        parameter: &str,
        values: &[String],
    ) -> Result<(), ApiError> {
        self.queries().reject_unknown_values(db, parameter, values)
    }
}

struct VocabularyQueries {
    select_sql: String,
    validation_sql: String,
    default_order: String,
}

impl VocabularyQueries {
    fn new(source: &VocabularySource) -> Self {
        let vocabulary_alias = "vocabulary";
        let listed_alias = "listed";
        let values_sql = &source.values_sql;
        let value_column = source.value_column;
        let list_columns = source
            .options
            .list_columns
            .map_or_else(|| format!("{vocabulary_alias}.*"), |columns| columns(vocabulary_alias));
        let count_joins = source.options.count_joins.map(|joins| joins(vocabulary_alias)).unwrap_or_default();
        let default_order =
            source.default_order.iter().map(|column| format!("{listed_alias}.{column}")).collect::<Vec<_>>().join(", ");
        Self {
            select_sql: format!("SELECT {list_columns} FROM ({values_sql}) {vocabulary_alias} {count_joins}"),
            validation_sql: format!(
                "SELECT {listed_alias}.{value_column} FROM ({values_sql}) {listed_alias} ORDER BY {default_order}"
            ),
            default_order,
        }
    }

    fn reject_unknown_values(&self, db: &Connection, parameter: &str, values: &[String]) -> Result<(), ApiError> {
        if values.is_empty() {
            return Ok(());
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
    use super::ValueVocabulary;

    #[test]
    fn every_value_vocabulary_validates_without_counting() {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ddo-etl/tests/fixtures/data");
        ddo_etl::tree::build_from_data(&fixtures, &mut db).unwrap();
        let mut violations = Vec::new();
        for vocabulary in ValueVocabulary::ALL {
            let sql = &vocabulary.queries().validation_sql;
            for (explain, column) in [("EXPLAIN QUERY PLAN", 3), ("EXPLAIN", 1)] {
                let mut statement = db.prepare(&format!("{explain} {sql}")).unwrap();
                for step in statement.query_map([], |row| row.get::<_, String>(column)).unwrap() {
                    let step = step.unwrap();
                    if step.contains("GROUP BY") || step.starts_with("Agg") || step == "Count" {
                        violations.push(format!("{vocabulary:?}: {step}"));
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
            let mut statement = db.prepare(&format!("EXPLAIN QUERY PLAN {}", vocabulary.select_sql())).unwrap();
            let plan = statement
                .query_map([], |row| Ok((row.get::<_, i64>(0)?, (row.get::<_, i64>(1)?, row.get::<_, String>(3)?))))
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
        assert!(violations.is_empty(), "{}", violations.join("\n"));
    }
}
