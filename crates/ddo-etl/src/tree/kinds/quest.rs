use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use anyhow::{ensure, Context, Result};
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::path::Path;

pub(crate) type QuestCitation = super::super::Citation;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct QuestRun {
    pub(crate) tier: String,
    pub(crate) level: i64,
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct QuestFile in "quests" {
        context { levels: &[Option<i64>; 3] }
        keys { id: i64, slug: &str }
        values { level => levels[0], epic_level => levels[1], legendary_level => levels[2] }
        fields {
            name: String => name,
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            is_wiki_only: bool => provenance [wiki_only],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            bestowed_by: Option<String> => bestowed_by,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            variant: Option<String> => variant,
            #[serde(default)]
            difficulties: Vec<String> => difficulties [json_required],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            epic_name: Option<String> => epic_name,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            favor: Option<i64> => favor,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            flagging: Option<String> => flagging,
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            is_challenge: bool => is_challenge,
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            is_free_to_play: bool => is_free_to_play,
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            is_raid: bool => is_raid,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            max_level: Option<i64> => max_level,
            pack: String => pack_id [reference "adventure_packs"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            patron: Option<String> => patron_id [optional_reference "patrons"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            zone: Option<String> => zone,
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            citation: Vec<QuestCitation>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            ddobuilder_correction: Vec<super::super::DdobuilderCorrection>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            run: Vec<QuestRun>,
        }
    }
}

impl QuestFile {
    pub(in crate::tree) fn all_from_rows(
        db: &Connection,
        annotations: &super::super::annotations::EntityAnnotations,
        notes: &super::super::export::CorrectionNoteIndex,
    ) -> Result<BTreeMap<i64, Self>> {
        let mut files = BTreeMap::new();
        let mut select = db.prepare(&format!("SELECT {} FROM quests ORDER BY id", Self::select_columns()))?;
        for row in select.query_map([], |row| {
            Ok((
                row.get::<_, i64>("id")?,
                Self::from_row(row)?,
                row.get::<_, Option<i64>>("level")?,
                row.get::<_, Option<i64>>("epic_level")?,
                row.get::<_, Option<i64>>("legendary_level")?,
            ))
        })? {
            let (id, mut file, heroic, epic, legendary) = row?;
            for (tier, level) in [("heroic", heroic), ("epic", epic), ("legendary", legendary)] {
                if let Some(level) = level {
                    file.run.push(QuestRun { tier: tier.to_string(), level });
                }
            }
            if let Some(annotation) = annotations.entity("quests", id) {
                file.citation = annotation.citation.clone();
            }
            file.ddobuilder_correction = notes.for_entity("quests", &file.name);
            files.insert(id, file);
        }
        Ok(files)
    }

    pub(in crate::tree) fn file_text(&self) -> anyhow::Result<String> {
        super::super::typed_file_text(self)
    }

    pub(in crate::tree) fn write_quest(&self, db: &Connection, path: &Path, id: i64, identity: &str) -> Result<()> {
        ensure!(
            id == super::super::ids::stable_hash(&["quests", identity]),
            "{}: quest id does not match its slug",
            path.display()
        );
        let mut heroic_level = None;
        let mut epic_level = None;
        let mut legendary_level = None;
        for run in &self.run {
            let level = match run.tier.as_str() {
                "heroic" => &mut heroic_level,
                "epic" => &mut epic_level,
                "legendary" => &mut legendary_level,
                other => anyhow::bail!("{}: unknown quest run tier {other:?}", path.display()),
            };
            ensure!(level.replace(run.level).is_none(), "{}: duplicate quest run tier {:?}", path.display(), run.tier);
        }
        self.insert(db, &[heroic_level, epic_level, legendary_level], id, identity)
            .with_context(|| format!("{}: inserting quest {:?}", path.display(), self.name))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quest_rows_round_trip_run_order() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        let pack_id = super::super::super::ids::stable_hash(&["adventure_packs", "test-pack"]);
        db.execute(
            "INSERT INTO adventure_packs (id, slug, name, is_free_to_play) VALUES (?1, 'test-pack', 'Test Pack', 0)",
            [pack_id],
        )
        .unwrap();
        let file: QuestFile = toml::from_str(
            "name = 'Test Quest'\npack = 'Test Pack'\ndifficulties = ['Normal', 'Hard']\n[[run]]\ntier = 'epic'\nlevel = 21\n[[run]]\ntier = 'heroic'\nlevel = 9\n",
        )
        .unwrap();
        let id = super::super::super::ids::stable_hash(&["quests", "test-quest"]);
        file.write_quest(&db, Path::new("test-quest/quest.toml"), id, "test-quest").unwrap();
        let notes = super::super::super::export::CorrectionNoteIndex::from_database(&db).unwrap();
        let annotations = super::super::super::annotations::EntityAnnotations::from_database(&db).unwrap();
        let all = QuestFile::all_from_rows(&db, &annotations, &notes).unwrap();
        let restored = &all[&id];
        assert_eq!(
            restored.run.iter().map(|run| (run.tier.as_str(), run.level)).collect::<Vec<_>>(),
            vec![("heroic", 9), ("epic", 21)]
        );
        assert_eq!(restored.difficulties, vec!["Normal", "Hard"]);
        assert_eq!(restored.pack, "Test Pack");
    }
}
