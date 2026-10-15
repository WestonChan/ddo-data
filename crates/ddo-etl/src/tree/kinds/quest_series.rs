use anyhow::{Context, Result};
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::path::Path;

database_record! {
    #[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct QuestSeriesFile in (table: &str) {
        context { wiki_url: &str }
        keys { id: i64, slug: &str, provenance: &str }
        fields {
            name: String => name,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pack: Option<String> => pack_id [optional_reference "adventure_packs"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            wiki_url: Option<String> => wiki_url [fallback wiki_url],
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            #[schemars(extend("x-reference-kind" = "quests"))]
            quests: Vec<String>,
            citation: Vec<super::super::Citation>,
        }
    }
}

database_record! {
    #[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    pub(crate) struct SeriesQuest in (table: &str, owner_column: &str) {
        keys { owner_id: i64 => owner_column, sort_order: i64 }
        fields { name: String => quest_id [reference "quests"] }
    }
}

impl QuestSeriesFile {
    pub(in crate::tree) fn all_from_rows(
        db: &Connection,
        annotations: &super::super::annotations::EntityAnnotations,
        table: &str,
        member_table: &str,
        owner_column: &str,
    ) -> Result<BTreeMap<i64, Self>> {
        let mut files = BTreeMap::new();
        let mut select = db.prepare(&format!("SELECT {} FROM {table} ORDER BY id", Self::select_columns(table)))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("id")?, Self::from_row(row)?)))? {
            let (id, mut file) = row?;
            let annotation =
                annotations.entity(table, id).with_context(|| format!("{table} {:?} has no citation", file.name))?;
            file.citation = annotation.citation.clone();
            let default_url = super::super::wiki_url_for_name(table, &file.name);
            file.wiki_url = file.wiki_url.filter(|url| Some(url) != default_url.as_ref());
            files.insert(id, file);
        }
        let mut members = db.prepare(&format!(
            "SELECT {} FROM {member_table} ORDER BY {owner_column}, sort_order",
            SeriesQuest::select_columns(member_table, owner_column)
        ))?;
        for row in members.query_map([], |row| Ok((row.get::<_, i64>("owner_id")?, SeriesQuest::from_row(row)?)))? {
            let (id, quest) = row?;
            files.get_mut(&id).context("quest series member has no parent")?.quests.push(quest.name);
        }
        Ok(files)
    }

    pub(in crate::tree) fn write_quest_series(
        &self,
        db: &Connection,
        path: &Path,
        ids: &mut super::super::ids::StableIds,
        table: &str,
    ) -> Result<i64> {
        let identity = super::super::slug(&self.name);
        let id = ids.issue(table, &identity, path)?;
        let wiki_url = super::super::wiki_url_for_name(table, &self.name).context("quest series has no wiki URL")?;
        self.insert(db, table, &wiki_url, id, &identity, "wiki")?;
        Ok(id)
    }

    pub(in crate::tree) fn insert_quests(
        &self,
        db: &Connection,
        path: &Path,
        id: i64,
        member_table: &str,
        owner_column: &str,
    ) -> Result<usize> {
        for (position, name) in self.quests.iter().enumerate() {
            SeriesQuest { name: name.clone() }
                .insert(db, member_table, owner_column, id, position as i64)
                .with_context(|| format!("{}: quest series member {name:?}", path.display()))?;
        }
        Ok(self.quests.len())
    }

    pub(in crate::tree) fn file_text(&self) -> Result<String> {
        super::super::typed_file_text(self)
    }
}
