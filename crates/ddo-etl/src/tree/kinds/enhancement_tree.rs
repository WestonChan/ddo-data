use super::super::file_types::*;
use crate::tree::tables::{Attack, DifficultyClass, Modifier, Requirements, Stance};
use anyhow::{ensure, Context, Result};
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct EnhancementSelection in "enhancement_selections" {
        keys { id: i64, slug: &str, enhancement_id: i64, sort_order: i64 }
        fields {
            name: String => name,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            cooldown_seconds: Option<i64> => cooldown_seconds,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            cost_per_rank: Vec<i64> => cost_per_rank [json],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            description: Option<String> => description,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            duration_seconds: Option<i64> => duration_seconds,
            icon: String => icon [image ""],
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            is_clickie: bool => is_clickie,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            min_spent: Option<i64> => min_spent,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            ranks: Option<i64> => ranks,
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            citation: Vec<crate::tree::Citation>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            ddobuilder_correction: Vec<super::super::DdobuilderCorrection>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            attack: Vec<Attack>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            dc: Vec<DifficultyClass>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            modifier: Vec<Modifier>,
            #[serde(flatten)]
            requirements: Requirements,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            stance: Vec<Stance>,
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct SelectorExclusion in "enhancement_selector_exclusions" {
        keys { enhancement_id: i64 }
        fields {
            internal_name: String => internal_name,
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct Enhancement in "enhancements" {
        keys { id: i64, slug: &str, tree_id: i64 }
        fields {
            name: String => name,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            arrows: Vec<String> => arrows [json],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            cooldown_seconds: Option<i64> => cooldown_seconds,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            cost_per_rank: Vec<i64> => cost_per_rank [json],
            description: String => description [correctable],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            variant: Option<String> => variant,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            duration_seconds: Option<i64> => duration_seconds,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            icon: Option<String> => icon [correctable] [image ""],
            internal_name: String => internal_name,
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            is_clickie: bool => is_clickie,
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            is_tier5: bool => is_tier5,
            min_spent: i64 => min_spent,
            ranks: i64 => ranks,
            x: i64 => x,
            y: i64 => y,
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            citation: Vec<super::super::Citation>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            attack: Vec<Attack>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            dc: Vec<DifficultyClass>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            ddobuilder_correction: Vec<super::super::DdobuilderCorrection>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            modifier: Vec<Modifier>,
            #[serde(flatten)]
            requirements: Requirements,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            selection: Vec<EnhancementSelection>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            selector_exclusion: Vec<SelectorExclusion>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            stance: Vec<Stance>,
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct EnhancementTreeFile in "enhancement_trees" {
        keys { id: i64, slug: &str }
        fields {
            name: String => name,
            background: String => background [image "ui"],
            icon: String => icon [correctable] [image ""],
            #[serde(default, skip_serializing_if = "is_false")]
            is_legacy: bool => is_legacy,
            kind: String => kind,
            version: i64 => version,
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            citation: Vec<crate::tree::Citation>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            ddobuilder_correction: Vec<super::super::DdobuilderCorrection>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            enhancement: Vec<Enhancement>,
            #[serde(flatten)]
            requirements: Requirements,
        }
    }
}

impl EnhancementTreeFile {
    pub(in crate::tree) fn write_enhancement_tree(
        &self,
        db: &Connection,
        path: &Path,
        id: i64,
        identity: &str,
    ) -> Result<usize> {
        ensure!(
            id == super::super::ids::stable_hash(&["enhancement_trees", identity]),
            "{}: enhancement tree id does not match its slug",
            path.display()
        );
        self.insert(db, id, identity)
            .with_context(|| format!("{}: enhancement tree {:?}", path.display(), self.name))?;
        let requirements = &self.requirements;
        let mut count = 1 + requirements.insert(db, "enhancement_tree", id)?;
        for enhancement in &self.enhancement {
            let mut enhancement_slug = super::super::slug(&enhancement.internal_name);
            if let Some(variant) = &enhancement.variant {
                enhancement_slug.push('-');
                enhancement_slug.push_str(&super::super::slug(variant));
            }
            let enhancement_id = super::super::ids::stable_hash(&["enhancements", &enhancement_slug]);
            enhancement
                .insert(db, enhancement_id, &enhancement_slug, id)
                .with_context(|| format!("{}: enhancement {:?}", path.display(), enhancement.name))?;
            count += 1 + enhancement.insert_abilities(db, enhancement_id, &enhancement_slug)?;
            for (position, selection) in enhancement.selection.iter().enumerate() {
                let selection_slug = format!("{}-{}", enhancement_slug, super::super::slug(&selection.name));
                let selection_id = super::super::ids::stable_hash(&["enhancement_selections", &selection_slug]);
                selection
                    .insert(db, selection_id, &selection_slug, enhancement_id, position as i64)
                    .with_context(|| format!("{}: selection {:?}", path.display(), selection.name))?;
                count += 1 + selection.insert_abilities(db, selection_id, &selection_slug)?;
            }
            for exclusion in &enhancement.selector_exclusion {
                exclusion.insert(db, enhancement_id)?;
                count += 1;
            }
        }
        Ok(count)
    }
}

impl EnhancementTreeFile {
    pub(in crate::tree) fn file_text(&self) -> anyhow::Result<String> {
        super::super::typed_file_text(self)
    }

    pub(in crate::tree) fn all_from_rows(
        db: &Connection,
        annotations: &super::super::annotations::EntityAnnotations,
        notes: &super::super::export::CorrectionNoteIndex,
        modifiers: &mut super::super::ModifierRowsByOwner,
    ) -> Result<BTreeMap<i64, Self>> {
        let mut files = BTreeMap::new();
        let mut requirements = super::super::requirement_rows_by_owner(db)?;
        let mut stances = super::super::stances_by_owner(db, annotations, &mut requirements)?;
        let mut select = db.prepare(&format!(
            "SELECT {} FROM enhancement_trees ORDER BY name, id",
            EnhancementTreeFile::select_columns()
        ))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("id")?, EnhancementTreeFile::from_row(row)?)))? {
            let (id, mut file) = row?;
            file.ddobuilder_correction = notes.for_entity("enhancement_trees", &file.name);
            file.requirements = requirements.remove(&("enhancement_tree".to_string(), id)).unwrap_or_default();
            files.insert(id, file);
        }
        let mut enhancements = BTreeMap::new();
        let mut select = db.prepare(&format!(
            "SELECT {} FROM enhancements ORDER BY tree_id, y, x, internal_name",
            Enhancement::select_columns()
        ))?;
        for row in select.query_map([], |row| {
            Ok((row.get::<_, i64>("id")?, row.get::<_, i64>("tree_id")?, Enhancement::from_row(row)?))
        })? {
            let (id, parent_id, mut file) = row?;
            file.requirements = requirements.remove(&("enhancement".to_string(), id)).unwrap_or_default();
            for phase in [None, Some("follow_on"), Some("this_attack")] {
                let kind = phase.map_or_else(|| "enhancement".to_string(), |phase| format!("enhancement_{phase}"));
                for modifier in modifiers.remove(&(kind, id)).unwrap_or_default() {
                    let mut modifier = modifier;
                    modifier.phase = phase.map(str::to_string);
                    file.modifier.push(modifier);
                }
            }
            file.attach_citations(annotations, id);
            file.ddobuilder_correction = notes.for_entity("enhancements", &file.name);
            let tree = files.get_mut(&parent_id).context("enhancement has no tree")?;
            enhancements.insert(id, (parent_id, tree.enhancement.len()));
            tree.enhancement.push(file);
        }
        let mut selections = BTreeMap::new();
        let mut select = db.prepare(&format!(
            "SELECT {} FROM enhancement_selections ORDER BY enhancement_id, sort_order",
            EnhancementSelection::select_columns()
        ))?;
        for row in select.query_map([], |row| {
            Ok((row.get::<_, i64>("id")?, row.get::<_, i64>("enhancement_id")?, EnhancementSelection::from_row(row)?))
        })? {
            let (id, parent_id, mut file) = row?;
            file.attach_citations(annotations, id);
            file.requirements = requirements.remove(&("enhancement_selection".to_string(), id)).unwrap_or_default();
            for phase in [None, Some("follow_on"), Some("this_attack")] {
                let kind = phase.map_or_else(
                    || "enhancement_selection".to_string(),
                    |phase| format!("enhancement_selection_{phase}"),
                );
                for modifier in modifiers.remove(&(kind, id)).unwrap_or_default() {
                    let mut modifier = modifier;
                    modifier.phase = phase.map(str::to_string);
                    file.modifier.push(modifier);
                }
            }
            let &(tree_id, position) = enhancements.get(&parent_id).context("selection has no enhancement")?;
            let enhancement = &mut files.get_mut(&tree_id).unwrap().enhancement[position];
            selections.insert(id, (tree_id, position, enhancement.selection.len()));
            enhancement.selection.push(file);
        }
        let mut exclusions = db.prepare(&format!(
            "SELECT {} FROM enhancement_selector_exclusions ORDER BY rowid",
            SelectorExclusion::select_columns()
        ))?;
        for row in exclusions.query_map([], |row| Ok((row.get::<_, i64>(0)?, SelectorExclusion::from_row(row)?)))? {
            let (id, exclusion) = row?;
            let &(tree_id, position) = enhancements.get(&id).context("exclusion has no enhancement")?;
            files.get_mut(&tree_id).unwrap().enhancement[position].selector_exclusion.push(exclusion);
        }
        let mut select = db.prepare(&format!(
            "SELECT {} FROM attacks WHERE owner_kind = 'enhancement' ORDER BY owner_id, id",
            Attack::select_columns()
        ))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("owner_id")?, Attack::from_row(row)?)))? {
            let (id, ability) = row?;
            let &(tree_id, position) = enhancements.get(&id).context("attack has no enhancement")?;
            files.get_mut(&tree_id).unwrap().enhancement[position].attack.push(ability);
        }
        let mut select = db.prepare(&format!(
            "SELECT {} FROM dcs WHERE owner_kind = 'enhancement' ORDER BY owner_id, sort_order",
            DifficultyClass::select_columns()
        ))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("owner_id")?, DifficultyClass::from_row(row)?)))? {
            let (id, ability) = row?;
            let &(tree_id, position) = enhancements.get(&id).context("dc has no enhancement")?;
            files.get_mut(&tree_id).unwrap().enhancement[position].dc.push(ability);
        }
        for ((kind, id), abilities) in &mut stances {
            if kind != "enhancement" {
                continue;
            }
            for ability in std::mem::take(abilities) {
                let &(tree_id, position) = enhancements.get(id).context("stance has no enhancement")?;
                files.get_mut(&tree_id).unwrap().enhancement[position].stance.push(ability);
            }
        }
        let mut select = db.prepare(&format!(
            "SELECT {} FROM attacks WHERE owner_kind = 'enhancement_selection' ORDER BY owner_id, id",
            Attack::select_columns()
        ))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("owner_id")?, Attack::from_row(row)?)))? {
            let (id, ability) = row?;
            let &(tree_id, position, choice) = selections.get(&id).context("attack has no selection")?;
            files.get_mut(&tree_id).unwrap().enhancement[position].selection[choice].attack.push(ability);
        }
        let mut select = db.prepare(&format!(
            "SELECT {} FROM dcs WHERE owner_kind = 'enhancement_selection' ORDER BY owner_id, sort_order",
            DifficultyClass::select_columns()
        ))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("owner_id")?, DifficultyClass::from_row(row)?)))? {
            let (id, ability) = row?;
            let &(tree_id, position, choice) = selections.get(&id).context("dc has no selection")?;
            files.get_mut(&tree_id).unwrap().enhancement[position].selection[choice].dc.push(ability);
        }
        for ((kind, id), abilities) in &mut stances {
            if kind != "enhancement_selection" {
                continue;
            }
            for ability in std::mem::take(abilities) {
                let &(tree_id, position, choice) = selections.get(id).context("stance has no selection")?;
                files.get_mut(&tree_id).unwrap().enhancement[position].selection[choice].stance.push(ability);
            }
        }
        for tree in files.values_mut() {
            for enhancement in &mut tree.enhancement {
                for dc in &mut enhancement.dc {
                    dc.ddobuilder_correction =
                        notes.for_named_child("enhancements", &enhancement.name, &[("dc", &dc.name)]);
                }
                for stance in &mut enhancement.stance {
                    stance.ddobuilder_correction =
                        notes.for_named_child("enhancements", &enhancement.name, &[("stance", &stance.name)]);
                }
                for selection in &mut enhancement.selection {
                    selection.ddobuilder_correction =
                        notes.for_named_child("enhancements", &enhancement.name, &[("selection", &selection.name)]);
                    for stance in &mut selection.stance {
                        stance.ddobuilder_correction = notes.for_named_child(
                            "enhancements",
                            &enhancement.name,
                            &[("selection", &selection.name), ("stance", &stance.name)],
                        );
                    }
                    for dc in &mut selection.dc {
                        dc.ddobuilder_correction = notes.for_named_child(
                            "enhancements",
                            &enhancement.name,
                            &[("selection", &selection.name), ("dc", &dc.name)],
                        );
                    }
                }
            }
        }
        Ok(files)
    }
}

impl Enhancement {
    fn insert_abilities(&self, db: &Connection, id: i64, owner_slug: &str) -> Result<usize> {
        let requirements = &self.requirements;
        let mut count = requirements.insert(db, "enhancement", id)?;
        let mut positions = BTreeMap::<String, i64>::new();
        for modifier in &self.modifier {
            let kind = modifier
                .phase
                .as_ref()
                .map_or_else(|| "enhancement".to_string(), |phase| format!("enhancement_{phase}"));
            let position = positions.entry(kind.clone()).or_default();
            count += modifier.write_modifier(db, &kind, id, *position)?;
            *position += 1;
        }
        for ability in &self.attack {
            ability.insert(db, "enhancement", id)?;
            count += 1;
        }
        for (position, ability) in self.dc.iter().enumerate() {
            ability.insert(db, "enhancement", id, position as i64)?;
            count += 1;
        }
        for (position, ability) in self.stance.iter().enumerate() {
            count += ability.write_stance(
                db,
                None,
                &ddo_model::identity::nested_slug(owner_slug, &ability.name),
                "enhancement",
                id,
                position as i64,
            )?;
        }
        Ok(count)
    }
}

impl EnhancementSelection {
    fn insert_abilities(&self, db: &Connection, id: i64, owner_slug: &str) -> Result<usize> {
        let requirements = &self.requirements;
        let mut count = requirements.insert(db, "enhancement_selection", id)?;
        let mut positions = BTreeMap::<String, i64>::new();
        for modifier in &self.modifier {
            let kind = modifier
                .phase
                .as_ref()
                .map_or_else(|| "enhancement_selection".to_string(), |phase| format!("enhancement_selection_{phase}"));
            let position = positions.entry(kind.clone()).or_default();
            count += modifier.write_modifier(db, &kind, id, *position)?;
            *position += 1;
        }
        for ability in &self.attack {
            ability.insert(db, "enhancement_selection", id)?;
            count += 1;
        }
        for (position, ability) in self.dc.iter().enumerate() {
            ability.insert(db, "enhancement_selection", id, position as i64)?;
            count += 1;
        }
        for (position, ability) in self.stance.iter().enumerate() {
            count += ability.write_stance(
                db,
                None,
                &ddo_model::identity::nested_slug(owner_slug, &ability.name),
                "enhancement_selection",
                id,
                position as i64,
            )?;
        }
        Ok(count)
    }
}
