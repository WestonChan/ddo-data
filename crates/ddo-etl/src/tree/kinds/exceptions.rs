use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct PackException in "pack_exceptions" {
        keys { pack_name: &str, rule: &str, row_index: i64, entity_kind: &str }
        fields { name: String => entity_name, url: String => url }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct QuestPackException in "pack_exceptions" {
        keys { pack_name: &str, rule: &str, row_index: i64, entity_kind: &str }
        fields { name: String => entity_name, reason: String => reason, url: String => url, read: String => read }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExceptionsFile {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub item: Vec<PackException>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub patron: Vec<PackException>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quest: Vec<QuestPackException>,
}

impl ExceptionsFile {
    pub(in crate::tree) fn all_from_rows(
        db: &rusqlite::Connection,
    ) -> anyhow::Result<std::collections::BTreeMap<String, Self>> {
        let mut files = std::collections::BTreeMap::new();
        let mut select = db.prepare(&format!(
            "SELECT {} FROM pack_exceptions WHERE rule IN ('item', 'patron') ORDER BY pack_name, rule, row_index",
            PackException::select_columns()
        ))?;
        for row in select.query_map([], |row| {
            Ok((row.get::<_, String>("pack_name")?, row.get::<_, String>("rule")?, PackException::from_row(row)?))
        })? {
            let (pack, rule, exception) = row?;
            let file =
                files.entry(pack).or_insert_with(|| Self { item: Vec::new(), patron: Vec::new(), quest: Vec::new() });
            if rule == "item" {
                file.item.push(exception);
            } else {
                file.patron.push(exception);
            }
        }
        let mut select = db.prepare(&format!(
            "SELECT {} FROM pack_exceptions WHERE rule = 'quest' ORDER BY pack_name, row_index",
            QuestPackException::select_columns()
        ))?;
        for row in
            select.query_map([], |row| Ok((row.get::<_, String>("pack_name")?, QuestPackException::from_row(row)?)))?
        {
            let (pack, exception) = row?;
            files
                .entry(pack)
                .or_insert_with(|| Self { item: Vec::new(), patron: Vec::new(), quest: Vec::new() })
                .quest
                .push(exception);
        }
        Ok(files)
    }

    pub(in crate::tree) fn write_pack_exceptions(
        &self,
        db: &rusqlite::Connection,
        pack: &str,
    ) -> anyhow::Result<usize> {
        for (kind, exceptions) in [("item", &self.item), ("patron", &self.patron)] {
            for (position, exception) in exceptions.iter().enumerate() {
                exception.insert(db, pack, kind, position as i64, kind)?;
            }
        }
        for (position, quest) in self.quest.iter().enumerate() {
            quest.insert(db, pack, "quest", position as i64, "quest")?;
        }
        Ok(self.item.len() + self.patron.len() + self.quest.len())
    }
}
