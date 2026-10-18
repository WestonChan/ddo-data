use super::{wiki_citations, Citation, RemovedEntry, NAMED_ENTITY_TABLES};
use crate::wiki::WikiOverrides;
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

database_record! {
    pub(crate) struct AppliedCorrection in "corrections" as "ddobuilder_correction" {
        keys {}
        fields {
            kind: String => kind,
            name: String => name,
            qualifier: String => qualifier,
            field: String => field,
            from_value: String => from_value,
            to_value: String => to_value,
            reason: String => reason,
            url: String => source,
            read: String => read,
        }
    }
}

database_record! {
    pub(super) struct EntityIdentity in "entity_identity" {
        keys {}
        fields { kind: String => kind, entity_id: i64 => entity_id, name: String => name, slug: String => slug }
    }
}

pub(super) fn wiki_url_for_name(table: &str, name: &str) -> Option<String> {
    let page = name.replace(' ', "_").replace('+', "%2B");
    match table {
        "items" => Some(format!("https://ddowiki.com/page/Item:{page}")),
        "events" | "vendors" | "quest_chains" | "sagas" => Some(format!("https://ddowiki.com/page/{page}")),
        _ => None,
    }
}

#[derive(Default, Serialize, Deserialize)]
pub(super) struct EntityAnnotations {
    pub(super) files: BTreeMap<String, String>,
    pub(super) entities: BTreeMap<String, EntityAnnotation>,
}

#[derive(Default, Serialize, Deserialize)]
pub(super) struct EntityAnnotation {
    #[serde(default)]
    pub(super) expected_name: Option<String>,
    #[serde(default)]
    pub(super) expected_description: Option<String>,
    pub(super) citation: Vec<Citation>,
    pub(super) removed: Vec<RemovedEntry>,
    pub(super) variant: Option<String>,
}

impl EntityAnnotation {
    fn is_empty(&self) -> bool {
        self.citation.is_empty() && self.removed.is_empty() && self.variant.is_none()
    }
}

fn entity_key(table: &str, id: i64) -> String {
    format!("{table}/{id}")
}

impl EntityAnnotations {
    pub(super) fn from_xml_inputs(db: &Connection) -> Result<Self> {
        let catalog = super::xml_slugs::xml_identities();
        let mut metadata = Self::default();
        metadata.files.insert(
            "upstream/source-aliases.toml".to_string(),
            toml::to_string(&super::kinds::SourceAliasesFile::from_xml()?)?,
        );
        let mut matching_entities = BTreeMap::new();
        for identity in &catalog.entity {
            let table = identity.table.clone();
            let old_id = identity.upstream_id;
            let expected_name = identity.name.clone();
            let annotation = EntityAnnotation {
                expected_name: Some(expected_name.clone()),
                expected_description: identity.description.clone(),
                citation: identity.citation.clone(),
                removed: identity.removed.clone(),
                variant: identity.variant.clone(),
            };
            let exact = if let Some(description) = annotation.expected_description.as_deref() {
                db.query_row(
                    &format!("SELECT id FROM {table} WHERE id = ?1 AND name = ?2 AND description = ?3"),
                    rusqlite::params![old_id, expected_name, description],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
            } else {
                db.query_row(
                    &format!("SELECT id FROM {table} WHERE id = ?1 AND name = ?2"),
                    rusqlite::params![old_id, expected_name],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
            };
            let id = if let Some(id) = exact {
                Some(id)
            } else if let Some(description) = annotation.expected_description.as_deref() {
                let mut statement =
                    db.prepare(&format!("SELECT id FROM {table} WHERE name = ?1 AND description = ?2 LIMIT 2"))?;
                let matches = statement
                    .query_map([expected_name.as_str(), description], |row| row.get::<_, i64>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                (matches.len() == 1).then(|| matches[0])
            } else {
                None
            };
            if let Some(id) = id {
                matching_entities.insert(entity_key(&table, id), annotation);
            }
        }
        metadata.entities = matching_entities;
        Ok(metadata)
    }

    pub(super) fn entity(&self, table: &str, id: i64) -> Option<&EntityAnnotation> {
        self.entities.get(&entity_key(table, id))
    }

    pub(super) fn from_database(db: &Connection) -> Result<Self> {
        let mut metadata = Self::default();
        for &table in NAMED_ENTITY_TABLES {
            if table == "augment_slot_types"
                || !super::table_columns(db, table)?.iter().any(|column| column == "variant")
            {
                continue;
            }
            let mut statement = db.prepare(&format!("SELECT id, variant FROM {table} WHERE variant IS NOT NULL"))?;
            for row in statement.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))? {
                let (id, variant) = row?;
                metadata.entities.entry(entity_key(table, id)).or_default().variant = Some(variant);
            }
        }
        let mut citations =
            db.prepare(&format!("SELECT {} FROM citations ORDER BY rowid", Citation::select_columns()))?;
        for row in citations.query_map([], |row| {
            Ok((row.get::<_, String>("owner_kind")?, row.get::<_, i64>("owner_id")?, Citation::from_row(row)?))
        })? {
            let (kind, id, citation) = row?;
            metadata.entities.entry(entity_key(&kind, id)).or_default().citation.push(citation);
        }
        let mut removed =
            db.prepare(&format!("SELECT {} FROM removed_entities ORDER BY rowid", RemovedEntry::select_columns()))?;
        for row in removed.query_map([], |row| {
            Ok((row.get::<_, String>("owner_kind")?, row.get::<_, i64>("owner_id")?, RemovedEntry::from_row(row)?))
        })? {
            let (kind, id, removed) = row?;
            metadata.entities.entry(entity_key(&kind, id)).or_default().removed.push(removed);
        }
        metadata.files = files_from_database(db)?;
        Ok(metadata)
    }

    pub(super) fn store(&self, db: &Connection) -> Result<()> {
        for (key, annotation) in &self.entities {
            let (table, id) = key.split_once('/').with_context(|| format!("entity annotation {key:?} has no table"))?;
            let id = id.parse::<i64>()?;
            if let Some(variant) = &annotation.variant {
                db.execute(&format!("UPDATE {table} SET variant = ?1 WHERE id = ?2"), params![variant, id])?;
            }
            insert_citations(db, table, id, &annotation.citation)?;
            for removed in &annotation.removed {
                removed.insert(db, table, id)?;
            }
        }
        store_files(db, &self.files)?;
        Ok(())
    }

    pub(super) fn add_data_file(&mut self, root: &Path, path: &Path, file: &super::schemas::SourceFile) -> Result<()> {
        use super::schemas::SourceFile;
        let mut entries = Vec::new();
        match file {
            SourceFile::Item(file) => {
                let identity = super::qualified_slug(&file.name, None);
                let id = super::ids::stable_hash(&["items", &identity]);
                let annotation = EntityAnnotation { removed: file.removed.clone(), ..Default::default() };
                entries.push((entity_key("items", id), annotation));
            }
            SourceFile::Augment(file) => {
                let identity = {
                    let base = super::augment_name_slug(&file.name);
                    file.variant.as_deref().map_or(base.clone(), |variant| format!("{base}-{}", super::slug(variant)))
                };
                let id = super::ids::stable_hash(&["augments", &identity]);
                let annotation = EntityAnnotation { variant: file.variant.clone(), ..Default::default() };
                entries.push((entity_key("augments", id), annotation));
            }
            SourceFile::Quest(file) => {
                let identity = super::qualified_slug(&file.name, file.variant.as_deref());
                let id = super::ids::stable_hash(&["quests", &identity]);
                let annotation = EntityAnnotation { variant: file.variant.clone(), ..Default::default() };
                entries.push((entity_key("quests", id), annotation));
            }
            SourceFile::Effect(file) => {
                let identity = super::qualified_slug(&file.name, file.variant.as_deref());
                let id = super::ids::stable_hash(&["effects", &identity]);
                let annotation = EntityAnnotation { variant: file.variant.clone(), ..Default::default() };
                entries.push((entity_key("effects", id), annotation));
            }
            SourceFile::CraftingSystem(file) => {
                let identity = super::qualified_slug(&file.name, None);
                let id = super::ids::stable_hash(&["crafting_systems", &identity]);
                let annotation = EntityAnnotation { removed: file.removed.clone(), ..Default::default() };
                entries.push((entity_key("crafting_systems", id), annotation));
            }
            SourceFile::FeatCategory(file) => {
                for row in &file.row {
                    let identity = super::qualified_slug(&row.name, row.variant.as_deref());
                    let id = super::ids::stable_hash(&["feats", &identity]);
                    let annotation = EntityAnnotation { variant: row.variant.clone(), ..Default::default() };
                    entries.push((entity_key("feats", id), annotation));
                }
            }
            SourceFile::EnhancementTree(file) => {
                for row in &file.enhancement {
                    let identity = super::qualified_slug(&row.internal_name, row.variant.as_deref());
                    let id = super::ids::stable_hash(&["enhancements", &identity]);
                    let annotation = EntityAnnotation { variant: row.variant.clone(), ..Default::default() };
                    entries.push((entity_key("enhancements", id), annotation));
                }
            }
            SourceFile::SourceAliases(file) => {
                self.files.insert("upstream/source-aliases.toml".to_string(), toml::to_string(file)?);
            }
            SourceFile::Exceptions(file) => {
                self.files.insert(path.strip_prefix(root)?.to_string_lossy().into_owned(), toml::to_string(file)?);
            }
            _ => {}
        }
        self.entities.extend(entries.into_iter().filter(|(_, annotation)| !annotation.is_empty()));
        Ok(())
    }

    pub(super) fn add_wiki_citations(&mut self, db: &Connection, wiki: &WikiOverrides) -> Result<()> {
        for &table in NAMED_ENTITY_TABLES {
            let name = if table == "augment_slot_types" { "label" } else { "name" };
            let provenance =
                if matches!(table, "items" | "augments" | "quests" | "quest_chains" | "sagas" | "vendors" | "events") {
                    "provenance"
                } else {
                    "NULL"
                };
            let family = if table == "augments" { "family" } else { "NULL" };
            let mut select =
                db.prepare(&format!("SELECT id, {name}, {provenance}, {family} FROM {table} ORDER BY {name}, id"))?;
            for row in select.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })? {
                let (id, name, provenance, family) = row?;
                let mut citations =
                    wiki_citations(wiki, table, &name, provenance.as_deref() == Some("wiki"), family.as_deref());
                let annotation = self.entities.entry(entity_key(table, id)).or_default();
                citations.append(&mut annotation.citation);
                annotation.citation = citations;
            }
        }
        self.entities.retain(|_, annotation| !annotation.is_empty());
        Ok(())
    }
}

pub(super) fn insert_citations(db: &Connection, table: &str, id: i64, citations: &[Citation]) -> Result<()> {
    for citation in citations {
        citation.insert(db, table, id)?;
    }
    Ok(())
}

fn store_files(db: &Connection, files: &BTreeMap<String, String>) -> Result<()> {
    let packs_by_folder =
        super::pack_folders(db)?.into_iter().map(|(name, folder)| (folder, name)).collect::<BTreeMap<_, _>>();
    for (relative_path, contents) in files {
        if relative_path == "upstream/source-aliases.toml" {
            let file: super::kinds::SourceAliasesFile = toml::from_str(contents)?;
            file.write_source_aliases(db)?;
        } else if relative_path.ends_with("/exceptions.toml") {
            let folder = Path::new(relative_path).parent().context("exception has no pack folder")?;
            let pack = packs_by_folder
                .get(folder)
                .with_context(|| format!("{relative_path}: exception has no matching pack record"))?;
            let file: super::kinds::ExceptionsFile = toml::from_str(contents)?;
            file.write_pack_exceptions(db, pack)?;
        }
    }
    Ok(())
}

fn files_from_database(db: &Connection) -> Result<BTreeMap<String, String>> {
    let mut files = BTreeMap::new();
    let aliases = super::kinds::SourceAliasesFile::from_rows(db)?;
    files.insert("upstream/source-aliases.toml".to_string(), super::typed_file_text(&aliases)?);
    let folders = super::pack_folders(db)?;
    for (pack, exceptions) in super::kinds::ExceptionsFile::all_from_rows(db)? {
        files.insert(
            folders[&pack].join("exceptions.toml").to_string_lossy().into_owned(),
            super::typed_file_text(&exceptions)?,
        );
    }
    Ok(files)
}

pub(super) fn store_xml_pack_exceptions(db: &Connection) -> Result<()> {
    let review: serde_json::Value = serde_json::from_str(include_str!("../../../../xtask/data/source_review.json"))?;
    let item_reviews = review["items_spanning_packs"].as_object().context("source review has no items")?;
    let mut item_index_by_pack = BTreeMap::<String, i64>::new();
    for (name, url) in item_reviews {
        let url = url.as_str().with_context(|| format!("item {name:?} has no citation"))?;
        let mut statement = db.prepare(
            "SELECT DISTINCT p.name FROM loot_adventure_packs loot JOIN adventure_packs p ON p.id = loot.pack_id JOIN items i ON i.id = loot.item_id WHERE i.name = ?1 ORDER BY p.name",
        )?;
        for pack_name in statement.query_map([name], |row| row.get::<_, String>(0))? {
            let pack_name = pack_name?;
            let row_index = item_index_by_pack.entry(pack_name.clone()).or_default();
            super::kinds::PackException { name: name.clone(), url: url.to_string() }
                .insert(db, &pack_name, "item", *row_index, "item")?;
            *row_index += 1;
        }
    }
    let patron_reviews = review["packs_with_several_patrons"].as_object().context("source review has no patrons")?;
    for (pack_name, url) in patron_reviews {
        let url = url.as_str().with_context(|| format!("pack {pack_name:?} has no citation"))?;
        let present: bool =
            db.query_row("SELECT EXISTS(SELECT 1 FROM adventure_packs WHERE name = ?1)", [pack_name], |row| {
                row.get(0)
            })?;
        if !present {
            continue;
        }
        super::kinds::PackException { name: pack_name.clone(), url: url.to_string() }
            .insert(db, pack_name, "patron", 0, "patron")?;
    }
    let quest_reviews =
        review["quest_versions_pending_wiki_read"].as_object().context("source review has no quests")?;
    for (name, fields) in quest_reviews {
        let pack_name: Option<String> = db
            .query_row(
                "SELECT p.name FROM quests q JOIN adventure_packs p ON p.id = q.pack_id WHERE q.name = ?1",
                [name],
                |row| row.get(0),
            )
            .optional()?;
        let Some(pack_name) = pack_name else { continue };
        let text = |key: &str| fields.get(key).and_then(serde_json::Value::as_str);
        super::kinds::QuestPackException {
            name: name.clone(),
            reason: text("reason").context("quest review has no reason")?.to_string(),
            url: text("source").context("quest review has no source")?.to_string(),
            read: text("read").context("quest review has no read date")?.to_string(),
        }
        .insert(db, &pack_name, "quest", 0, "quest")?;
    }
    Ok(())
}
