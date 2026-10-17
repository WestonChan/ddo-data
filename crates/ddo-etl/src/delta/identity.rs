use crate::database_tables::{table_columns, table_names, table_rows};
use anyhow::{ensure, Context, Result};
use rusqlite::{types::Value as SqlValue, Connection};
use std::collections::{BTreeMap, BTreeSet};

pub type RowFields = BTreeMap<String, SqlValue>;

pub struct IdentityIndex {
    rows: BTreeMap<(String, i64), RowFields>,
    foreign_keys: BTreeMap<(String, String), String>,
    feat_groups: BTreeMap<i64, Vec<String>>,
    resolved: BTreeMap<(String, i64), String>,
    resolving: BTreeSet<(String, i64)>,
    pairing_errors: BTreeMap<(String, i64), String>,
}

impl IdentityIndex {
    pub fn from_db(db: &Connection) -> Result<Self> {
        let mut rows = BTreeMap::new();
        let mut foreign_keys = BTreeMap::new();
        for table in table_names(db)? {
            let columns = table_columns(db, &table)?;
            if columns.iter().any(|column| column == "id") && !table.starts_with("effect_vocabulary_") {
                for values in table_rows(db, &table)? {
                    let fields: RowFields = columns.iter().cloned().zip(values).collect();
                    if let Some(SqlValue::Integer(id)) = fields.get("id") {
                        rows.insert((table.clone(), *id), fields);
                    }
                }
            }
            let mut statement = db.prepare(&format!("PRAGMA foreign_key_list(\"{table}\")"))?;
            let references = statement.query_map([], |row| {
                Ok((row.get::<_, String>(3)?, row.get::<_, String>(2)?, row.get::<_, String>(4)?))
            })?;
            for reference in references {
                let (column, target_table, target_column) = reference?;
                if target_column == "id" {
                    foreign_keys.insert((table.clone(), column), target_table);
                }
            }
        }
        let mut feat_groups: BTreeMap<i64, Vec<String>> = BTreeMap::new();
        let mut statement = db.prepare("SELECT feat_id, group_name FROM feat_groups ORDER BY group_name")?;
        for group in statement.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))? {
            let (feat_id, group_name) = group?;
            feat_groups.entry(feat_id).or_default().push(group_name);
        }
        Ok(Self {
            rows,
            foreign_keys,
            feat_groups,
            resolved: BTreeMap::new(),
            resolving: BTreeSet::new(),
            pairing_errors: BTreeMap::new(),
        })
    }

    pub fn from_db_without_columns(db: &Connection, columns: &[&str]) -> Result<Self> {
        let mut index = Self::from_db(db)?;
        for fields in index.rows.values_mut() {
            for column in columns {
                fields.remove(*column);
            }
        }
        Ok(index)
    }

    pub fn identity(&mut self, table: &str, id: i64) -> Result<String> {
        let key = (table.to_string(), id);
        if let Some(reason) = self.pairing_errors.get(&key) {
            anyhow::bail!("table {table} id {id}: {reason}");
        }
        if let Some(identity) = self.resolved.get(&key) {
            return Ok(identity.clone());
        }
        let fields = self.rows.get(&key).with_context(|| format!("table {table} id {id} has no row"))?.clone();
        ensure!(self.resolving.insert(key.clone()), "cyclic identity through {table} id {id}");
        if let Some(SqlValue::Text(slug)) = fields.get("slug") {
            let identity = format!("{table}/{slug}");
            self.resolving.remove(&key);
            self.resolved.insert(key, identity.clone());
            return Ok(identity);
        }
        if table == "corrections" {
            let identity = ["kind", "name", "qualifier", "field"]
                .iter()
                .map(|column| fields.get(*column).map(scalar_text).unwrap_or_default())
                .collect::<Vec<_>>()
                .join("/");
            self.resolving.remove(&key);
            self.resolved.insert(key, identity.clone());
            return Ok(identity);
        }
        let mut parts = vec![table.to_string()];
        if let Some(label) = fields.get("name").or_else(|| fields.get("label")) {
            parts.push(scalar_text(label));
            for column in identity_qualifiers(table) {
                if let Some(value) = fields.get(*column) {
                    parts.push(format!("{column}={}", self.normalized_cell(table, column, value, &fields)?));
                }
            }
            if table == "feats" {
                parts.push(format!("groups={:?}", self.feat_groups.get(&id).cloned().unwrap_or_default()));
            }
        } else {
            for (column, value) in &fields {
                if matches!(column.as_str(), "id" | "drop_text_segment" | "source_match" | "cost" | "icon") {
                    continue;
                }
                parts.push(format!("{column}={}", self.normalized_cell(table, column, value, &fields)?));
            }
        }
        let identity = parts.join("/");
        self.resolving.remove(&key);
        self.resolved.insert(key, identity.clone());
        Ok(identity)
    }

    pub fn pair_response_names(&mut self, current: &mut Self) -> Result<()> {
        let mut baseline_names: BTreeMap<(String, String, Option<String>), Vec<i64>> = BTreeMap::new();
        let variant_tables = self
            .rows
            .iter()
            .filter_map(|((table, _), fields)| fields.contains_key("variant").then_some(table.clone()))
            .collect::<BTreeSet<_>>();
        let names = |rows: &BTreeMap<(String, i64), RowFields>| {
            let mut names: BTreeMap<(String, String, Option<String>), Vec<i64>> = BTreeMap::new();
            for ((table, id), fields) in rows {
                let Some(name) = fields.get("name").or_else(|| fields.get("label")).and_then(sql_text) else {
                    continue;
                };
                let variant = variant_tables
                    .contains(table)
                    .then(|| fields.get("variant").and_then(sql_text).map(str::to_string))
                    .flatten();
                names.entry((table.clone(), name.to_string(), variant)).or_default().push(*id);
            }
            names
        };
        baseline_names.extend(names(&self.rows));
        let current_names = names(&current.rows);
        for ((table, name, variant), baseline_ids) in baseline_names {
            let current_ids =
                current_names.get(&(table.clone(), name.clone(), variant.clone())).cloned().unwrap_or_default();
            if baseline_ids.len() != 1 || current_ids.len() > 1 {
                let reason = format!(
                    "ambiguous name {name:?} variant {variant:?}: {} baseline rows and {} current rows",
                    baseline_ids.len(),
                    current_ids.len()
                );
                for id in baseline_ids {
                    self.pairing_errors.insert((table.clone(), id), reason.clone());
                }
                for id in current_ids {
                    current.pairing_errors.insert((table.clone(), id), reason.clone());
                }
                continue;
            }
            let identity = match current_ids.first() {
                Some(id) => current.identity(&table, *id)?,
                None => format!("{table}/removed:{name:?}/{variant:?}"),
            };
            self.resolved.insert((table, baseline_ids[0]), identity);
        }
        Ok(())
    }

    pub fn normalized_cell(
        &mut self,
        table: &str,
        column: &str,
        value: &SqlValue,
        fields: &RowFields,
    ) -> Result<String> {
        let SqlValue::Integer(id) = value else { return Ok(scalar_text(value)) };
        if table == "entity_identity" && column == "entity_id" {
            fields.get("kind").and_then(sql_text).context("identity row has no kind")?;
        }
        if matches!(table, "citations" | "removed_entities" | "entity_notes") && column == "owner_id" {
            fields.get("owner_kind").and_then(sql_text).context("annotation row has no owner kind")?;
        }
        if column == "id" && self.rows.contains_key(&(table.to_string(), *id)) {
            return self.identity(table, *id);
        }
        if column == "id" && table.starts_with("effect_vocabulary_") {
            return self.identity("effects", *id);
        }
        let target = self.reference_target(table, column, fields);
        if let Some(target) = target {
            if target == "standalone" {
                return Ok("standalone".to_string());
            }
            return self.identity(&target, *id);
        }
        Ok(scalar_text(value))
    }

    pub fn reference_target(&self, table: &str, column: &str, fields: &RowFields) -> Option<String> {
        if table == "entity_identity" && column == "entity_id" {
            return fields.get("kind").and_then(sql_text).map(str::to_string);
        }
        if matches!(table, "citations" | "removed_entities" | "entity_notes") && column == "owner_id" {
            return fields.get("owner_kind").and_then(sql_text).map(str::to_string);
        }
        self.foreign_keys.get(&(table.to_string(), column.to_string())).cloned().or_else(|| {
            if table == "item_augment_slot_options" && column == "item_id" {
                return Some("items".to_string());
            }
            if table == "class_spells" && column == "spell_id" {
                return Some("spells".to_string());
            }
            if column == "ingredient_id" && table == "vendor_offer_costs" {
                return fields.get("ingredient_kind").and_then(sql_text).map(|kind| {
                    if kind == "vendor_currency" { "vendor_currencies" } else { "crafting_ingredients" }.to_string()
                });
            }
            if column == "source_id" {
                return fields.get("source_kind").and_then(sql_text).and_then(polymorphic_table).map(str::to_string);
            }
            if column == "owner_id" {
                return fields.get("owner_kind").and_then(sql_text).and_then(polymorphic_table).map(str::to_string);
            }
            None
        })
    }

    pub fn row(&self, table: &str, id: i64) -> Option<&RowFields> {
        self.rows.get(&(table.to_string(), id))
    }

    pub fn remove_row(&mut self, table: &str, id: i64) {
        let key = (table.to_string(), id);
        self.rows.remove(&key);
        self.resolved.clear();
        self.pairing_errors.clear();
    }

    pub fn normalized_rows(&mut self, db: &Connection, table: &str) -> Result<Vec<Vec<String>>> {
        let columns = table_columns(db, table)?;
        let mut rows = Vec::new();
        for values in table_rows(db, table)? {
            let fields: RowFields = columns.iter().cloned().zip(values).collect();
            let mut row = Vec::new();
            for column in &columns {
                if (table == "schema_version" && column == "applied_at")
                    || (table == "dataset_version" && column == "built_at")
                {
                    row.push("build-time".to_string());
                } else {
                    row.push(self.normalized_cell(table, column, &fields[column], &fields)?);
                }
            }
            rows.push(row);
        }
        Ok(rows)
    }
}

pub fn sql_text(value: &SqlValue) -> Option<&str> {
    if let SqlValue::Text(text) = value {
        Some(text)
    } else {
        None
    }
}

pub fn scalar_text(value: &SqlValue) -> String {
    match value {
        SqlValue::Null => "null".to_string(),
        SqlValue::Integer(number) => format!("integer:{number}"),
        SqlValue::Real(number) => format!("real:{number}"),
        SqlValue::Text(text) => format!("text:{text:?}"),
        SqlValue::Blob(bytes) => format!("blob:{bytes:?}"),
    }
}

fn identity_qualifiers(table: &str) -> &'static [&'static str] {
    match table {
        "augments" => &["family", "min_level", "description"],
        "crafting_ingredients" => &["system_id", "tier"],
        "dcs" | "stances" => &["owner_kind", "owner_id"],
        "enhancement_selections" => &["enhancement_id"],
        "enhancements" => &["tree_id"],
        "feats" => &["source_kind", "source_id", "description", "acquire"],
        "item_augment_slot_options" => &["item_id", "slot_order", "option_order"],
        _ => &[],
    }
}

pub fn polymorphic_table(kind: &str) -> Option<&'static str> {
    let base = kind.strip_suffix("_follow_on").or_else(|| kind.strip_suffix("_this_attack")).unwrap_or(kind);
    let base = base.strip_suffix("_auto_acquire").unwrap_or(base);
    Some(match base {
        "augment" => "augments",
        "clickie" => "clickies",
        "enhancement" => "enhancements",
        "enhancement_selection" => "enhancement_selections",
        "enhancement_tree" => "enhancement_trees",
        "feat" => "feats",
        "feat_conditional_group" => "feat_conditional_groups",
        "filigree" => "filigrees",
        "guild_buff" => "guild_buffs",
        "item" => "items",
        "item_augment_slot_option" => "item_augment_slot_options",
        "modifier" => "modifiers",
        "optional_buff" => "optional_buffs",
        "set_bonus_tier" => "set_bonus_tiers",
        "spell" => "spells",
        "stance" => "stances",
        "class" => "classes",
        "race" => "races",
        "adventure_pack" => "adventure_packs",
        "challenge" | "quest" => "quests",
        "crafting_system" => "crafting_systems",
        "event" => "events",
        "quest_chain" => "quest_chains",
        "saga" => "sagas",
        "vendor" => "vendors",
        "starter" => return None,
        "standard" | "standalone" => return None,
        _ => return None,
    })
}
