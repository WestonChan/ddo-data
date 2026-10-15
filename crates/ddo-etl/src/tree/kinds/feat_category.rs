use crate::tree::tables::{Attack, DifficultyClass, EffectGrant, Modifier, Requirements, Stance};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use anyhow::{ensure, Context, Result};
use rusqlite::{Connection, OptionalExtension};
use std::collections::BTreeMap;
use std::path::Path;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct ConditionalFeatGroup in "feat_conditional_groups" {
        keys { id: Option<i64>, feat_id: i64 }
        fields {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            groups: Vec<String> => groups [json_required],
        }
        attached {
            #[serde(flatten)]
            requirements: Requirements,
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct FeatGroup in "feat_groups" {
        keys { feat_id: i64 }
        fields {
            group_name: String => group_name,
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct FeatSubItem in "feat_sub_items" {
        keys { feat_id: i64, sort_order: i64 }
        fields {
            name: String => name,
            description: String => description,
            icon: String => icon [image ""],
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct Feat in "feats" {
        keys { id: i64, slug: &str, source_kind: &str, source_id: Option<i64> }
        fields {
            name: String => name,
            acquire: String => acquire,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            variant: Option<String> => variant,
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            auto_acquire_ignores_requirements: bool => auto_acquire_ignores_requirements,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            description: Option<String> => description,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            icon: Option<String> => icon [image ""],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            max_times_acquire: Option<i64> => max_times_acquire,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            sphere: Option<String> => sphere,
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            citation: Vec<super::super::Citation>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            attack: Vec<Attack>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            conditional_group: Vec<ConditionalFeatGroup>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            dc: Vec<DifficultyClass>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            effect: Vec<EffectGrant>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            group: Vec<FeatGroup>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            note: Vec<super::super::EntityNote>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            modifier: Vec<Modifier>,
            #[serde(flatten)]
            requirements: Requirements,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            #[schemars(extend("x-reference-kind" = "classes|races"))]
            source_name: Option<String>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            stance: Vec<Stance>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            sub_item: Vec<FeatSubItem>,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FeatCategoryFile {
    pub(crate) row: Vec<Feat>,
}

impl FeatCategoryFile {
    pub(in crate::tree) fn all_from_rows(
        db: &Connection,
        annotations: &super::super::annotations::EntityAnnotations,
        notes: &super::super::export::CorrectionNoteIndex,
        modifiers: &mut super::super::ModifierRowsByOwner,
    ) -> Result<BTreeMap<i64, Feat>> {
        let mut feats = BTreeMap::new();
        let mut source_names = BTreeMap::new();
        let mut source_rows =
            db.prepare("SELECT 'class', id, name FROM classes UNION ALL SELECT 'race', id, name FROM races")?;
        for row in source_rows
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?)))?
        {
            let (kind, id, name) = row?;
            source_names.insert((kind, id), name);
        }
        let mut select = db.prepare(&format!("SELECT {} FROM feats ORDER BY id", Feat::select_columns()))?;
        for row in select.query_map([], |row| {
            Ok((
                row.get::<_, i64>("id")?,
                row.get::<_, String>("source_kind")?,
                row.get::<_, Option<i64>>("source_id")?,
                Feat::from_row(row)?,
            ))
        })? {
            let (id, source_kind, source_id, mut feat) = row?;
            let name = &feat.name;
            let source_name = match (source_kind.as_str(), source_id) {
                ("class" | "race", Some(source_id)) => Some(
                    source_names
                        .get(&(source_kind.clone(), source_id))
                        .cloned()
                        .with_context(|| format!("feat {name:?} has unknown {source_kind} {source_id}"))?,
                ),
                ("standard", None) => None,
                _ => anyhow::bail!("feat {name:?} has invalid source {source_kind:?} {source_id:?}"),
            };
            let annotation = annotations.entity("feats", id);
            let mut modifier_rows = Vec::new();
            for kind in ["feat", "feat_follow_on", "feat_this_attack"] {
                for modifier in modifiers.remove(&(kind.to_string(), id)).unwrap_or_default() {
                    modifier_rows.push({
                        let mut modifier = modifier;
                        modifier.phase = kind.strip_prefix("feat_").map(str::to_string);
                        modifier
                    });
                }
            }
            feat.source_name = source_name;
            feat.citation = annotation.map_or_else(Vec::new, |annotation| annotation.citation.clone());
            feat.variant = annotation.and_then(|annotation| annotation.variant.clone()).or(feat.variant);
            feat.note = notes.for_entity("feats", &feat.name);
            feat.modifier = modifier_rows;
            feats.insert(id, feat);
        }
        let mut select = db.prepare(&format!(
            "SELECT {} FROM feat_effects ORDER BY feat_id, sort_order",
            EffectGrant::select_columns("feat_effects", "feat_id")
        ))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("owner_id")?, EffectGrant::from_row(row)?)))? {
            let (id, record) = row?;
            feats.get_mut(&id).with_context(|| format!("effect has no feat {id}"))?.effect.push(record);
        }
        let mut select =
            db.prepare(&format!("SELECT {} FROM feat_groups ORDER BY feat_id, rowid", FeatGroup::select_columns()))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("feat_id")?, FeatGroup::from_row(row)?)))? {
            let (id, record) = row?;
            feats.get_mut(&id).with_context(|| format!("group has no feat {id}"))?.group.push(record);
        }
        let mut select = db.prepare(&format!(
            "SELECT {} FROM feat_sub_items ORDER BY feat_id, sort_order",
            FeatSubItem::select_columns()
        ))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("feat_id")?, FeatSubItem::from_row(row)?)))? {
            let (id, record) = row?;
            feats.get_mut(&id).with_context(|| format!("sub_item has no feat {id}"))?.sub_item.push(record);
        }
        let mut conditional_positions = BTreeMap::new();
        let mut conditional_rows = db.prepare(&format!(
            "SELECT {} FROM feat_conditional_groups ORDER BY feat_id, id",
            ConditionalFeatGroup::select_columns()
        ))?;
        for row in conditional_rows.query_map([], |row| {
            Ok((row.get::<_, i64>("id")?, row.get::<_, i64>("feat_id")?, ConditionalFeatGroup::from_row(row)?))
        })? {
            let (id, feat_id, conditional) = row?;
            let feat = feats.get_mut(&feat_id).with_context(|| format!("conditional group has no feat {feat_id}"))?;
            conditional_positions.insert(id, (feat_id, feat.conditional_group.len()));
            feat.conditional_group.push(conditional);
        }
        let mut requirements = super::super::requirement_rows_by_owner(db)?;
        for (&id, feat) in &mut feats {
            for (kind, phase) in [("feat", None), ("feat_auto_acquire", Some("auto_acquire"))] {
                feat.requirements.append(requirements.remove(&(kind.to_string(), id)).unwrap_or_default(), phase);
            }
        }
        for (id, (feat_id, position)) in conditional_positions {
            feats.get_mut(&feat_id).unwrap().conditional_group[position].requirements =
                requirements.remove(&("feat_conditional_group".to_string(), id)).unwrap_or_default();
        }
        let mut select = db.prepare(&format!(
            "SELECT {} FROM attacks WHERE owner_kind = 'feat' ORDER BY owner_id, id",
            Attack::select_columns()
        ))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("owner_id")?, Attack::from_row(row)?)))? {
            let (id, attack) = row?;
            feats.get_mut(&id).with_context(|| format!("attack has no feat {id}"))?.attack.push(attack);
        }
        let mut select = db.prepare(&format!(
            "SELECT {} FROM dcs WHERE owner_kind = 'feat' ORDER BY owner_id, sort_order",
            DifficultyClass::select_columns()
        ))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("owner_id")?, DifficultyClass::from_row(row)?)))? {
            let (id, ability) = row?;
            feats.get_mut(&id).with_context(|| format!("DC has no feat {id}"))?.dc.push(ability);
        }
        for ((kind, id), mut stances) in super::super::stances_by_owner(db, &mut requirements)? {
            if kind != "feat" {
                continue;
            }
            let feat = feats.get_mut(&id).context("stance has no feat")?;
            for stance in &mut stances {
                stance.note = notes.for_named_child("feats", &feat.name, &[("stance", &stance.name)]);
            }
            feat.stance = stances;
        }
        Ok(feats)
    }

    pub(in crate::tree) fn insert_children(&self, db: &Connection, path: &Path) -> Result<usize> {
        let mut count = 0;
        for feat in &self.row {
            let mut identity = super::super::slug(&feat.name);
            if let Some(variant) = &feat.variant {
                identity.push('-');
                identity.push_str(&super::super::slug(variant));
            }
            let feat_id = super::super::ids::stable_hash(&["feats", &identity]);
            for (position, effect) in feat.effect.iter().enumerate() {
                effect
                    .insert(db, "feat_effects", "feat_id", feat_id, position as i64)
                    .with_context(|| format!("{}: feat effect", path.display()))?;
                count += 1;
            }
            for group in &feat.group {
                group.insert(db, feat_id)?;
                count += 1;
            }
            for (position, sub_item) in feat.sub_item.iter().enumerate() {
                sub_item.insert(db, feat_id, position as i64)?;
                count += 1;
            }
            for conditional in &feat.conditional_group {
                conditional.insert(db, None, feat_id)?;
                let conditional_id = db.last_insert_rowid();
                count += 1;
                count += conditional.requirements.insert(db, "feat_conditional_group", conditional_id)?;
            }
        }
        Ok(count)
    }

    pub(in crate::tree) fn write_feat_category(&self, db: &Connection, path: &Path) -> Result<usize> {
        for feat in &self.row {
            let mut identity = super::super::slug(&feat.name);
            if let Some(variant) = &feat.variant {
                identity.push('-');
                identity.push_str(&super::super::slug(variant));
            }
            let id = super::super::ids::stable_hash(&["feats", &identity]);
            let (source_kind, source_id) = if let Some(source_name) = &feat.source_name {
                let class_id: Option<i64> = db
                    .query_row("SELECT id FROM classes WHERE name = ?1", [source_name], |row| row.get(0))
                    .optional()?;
                let race_id: Option<i64> =
                    db.query_row("SELECT id FROM races WHERE name = ?1", [source_name], |row| row.get(0)).optional()?;
                ensure!(
                    class_id.is_some() != race_id.is_some(),
                    "{}: feat {:?} source {source_name:?} must name one class or race",
                    path.display(),
                    feat.name
                );
                if let Some(id) = class_id {
                    ("class", Some(id))
                } else {
                    ("race", race_id)
                }
            } else {
                ("standard", None)
            };
            feat.insert(db, id, &identity, source_kind, source_id)
                .with_context(|| format!("{}: feat {:?}", path.display(), feat.name))?;
        }
        Ok(self.row.len())
    }
}

impl FeatCategoryFile {
    pub(in crate::tree) fn insert_abilities(&self, db: &Connection) -> Result<usize> {
        let mut count = 0;
        for feat in &self.row {
            let identity = super::super::qualified_slug(&feat.name, feat.variant.as_deref());
            let id = super::super::ids::stable_hash(&["feats", &identity]);
            let mut positions = BTreeMap::<String, i64>::new();
            for modifier in &feat.modifier {
                let kind = modifier.phase.as_ref().map_or_else(|| "feat".to_string(), |phase| format!("feat_{phase}"));
                let position = positions.entry(kind.clone()).or_default();
                count += modifier.write_modifier(db, &kind, id, *position)?;
                *position += 1;
            }
            count += feat.requirements.insert(db, "feat", id)?;
            for ability in &feat.attack {
                ability.insert(db, "feat", id)?;
                count += 1;
            }
            for (position, ability) in feat.dc.iter().enumerate() {
                ability.insert(db, "feat", id, position as i64)?;
                count += 1;
            }
            for (position, ability) in feat.stance.iter().enumerate() {
                count += ability.write_stance(
                    db,
                    None,
                    &ddo_model::identity::nested_slug(&identity, &ability.name),
                    "feat",
                    id,
                    position as i64,
                )?;
            }
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::trace::{TraceEvent, TraceEventCodes};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn feat_export_query_count_does_not_grow_with_feats() {
        static QUERY_COUNT: AtomicUsize = AtomicUsize::new(0);
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        db.trace_v2(
            TraceEventCodes::SQLITE_TRACE_STMT,
            Some(|event| {
                if let TraceEvent::Stmt(_, _) = event {
                    QUERY_COUNT.fetch_add(1, Ordering::SeqCst);
                }
            }),
        );
        let mut counts = Vec::new();
        for name in ["First Feat", "Second Feat"] {
            let file: FeatCategoryFile =
                toml::from_str(&format!("[[row]]\nname = {name:?}\nacquire = 'Train'\n")).unwrap();
            file.write_feat_category(&db, Path::new("characters/feats/test.toml")).unwrap();
            let annotations = super::super::super::annotations::EntityAnnotations::from_database(&db).unwrap();
            let notes = super::super::super::export::CorrectionNoteIndex::from_database(&db).unwrap();
            let mut modifiers = super::super::super::modifier_rows_by_owner(&db).unwrap();
            QUERY_COUNT.store(0, Ordering::SeqCst);
            let restored = FeatCategoryFile::all_from_rows(&db, &annotations, &notes, &mut modifiers).unwrap();
            assert_eq!(restored.len(), counts.len() + 1);
            counts.push(QUERY_COUNT.load(Ordering::SeqCst));
        }
        assert_eq!(counts, vec![10, 10]);
    }
}
