use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use anyhow::{ensure, Context, Result};
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::path::Path;

database_record! {
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct EffectAlias in "effect_aliases" {
        keys { effect_id: i64, sort_order: i64, slug: Option<&str> }
        fields {
            name: String => name,
            verbose_name_template: Option<String> => verbose_name_template,
        }
    }
}

impl EffectAlias {
    pub(in crate::tree) fn validate_links(db: &Connection, locations: &BTreeMap<(&str, i64), &Path>) -> Result<()> {
        let mut statement = db.prepare(
            "WITH links AS (
                 SELECT 'items' AS owner_kind, item_id AS owner_id, effect_id, alias FROM item_effects
                 UNION ALL SELECT 'augments', augment_id, effect_id, alias FROM augment_effects
                 UNION ALL SELECT 'feats', feat_id, effect_id, alias FROM feat_effects
                 UNION ALL SELECT 'set_bonuses', t.set_id, l.effect_id, l.alias
                   FROM set_bonus_tier_effects l JOIN set_bonus_tiers t ON t.id = l.tier_id
                 UNION ALL SELECT 'items', o.item_id, l.effect_id, l.alias
                   FROM item_augment_slot_option_effects l
                   JOIN item_augment_slot_options o ON o.id = l.option_id
             )
             SELECT l.owner_kind, l.owner_id, e.name, l.alias, a.name IS NOT NULL
               FROM links l JOIN effects e ON e.id = l.effect_id
               LEFT JOIN effect_aliases a ON a.effect_id = l.effect_id AND a.name = l.alias
              WHERE l.alias IS NOT NULL AND (a.name IS NULL OR a.verbose_name_template IS NULL)
              ORDER BY l.owner_kind, l.owner_id, e.name, l.alias",
        )?;
        let mut errors = Vec::new();
        for row in statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, bool>(4)?,
            ))
        })? {
            let (kind, id, effect, alias, exists) = row?;
            let path = locations.get(&(kind.as_str(), id)).context("alias link owner has no file location")?;
            errors.push(if exists {
                format!("{}: effect {effect:?} alias {alias:?} has no line template", path.display())
            } else {
                format!("{}: effect {effect:?} has no alias {alias:?}", path.display())
            });
        }
        ensure!(errors.is_empty(), "data validation failed:\n{}", errors.join("\n"));
        Ok(())
    }

    pub(in crate::tree) fn write_validated_slugs(
        db: &Connection,
        groupings: &crate::groupings::Groupings,
    ) -> Result<()> {
        let mut statement = db.prepare(&format!(
            "WITH redirect_rows AS (SELECT {} FROM redirects), retired_rows AS (SELECT {} FROM retired_slugs)
             SELECT a.effect_id, a.name, a.slug, a.verbose_name_template, e.name,
                    named.name, named.is_stat, routed.name,
                    EXISTS (SELECT 1 FROM redirect_rows r WHERE r.kind = 'effects' AND r.from_slug = a.slug),
                    EXISTS (SELECT 1 FROM retired_rows r WHERE r.kind = 'effects' AND r.slug = a.slug), e.is_stat
               FROM effect_aliases a JOIN effects e ON e.id = a.effect_id
               LEFT JOIN effects named ON named.name = a.name
               LEFT JOIN effects routed ON routed.slug = a.slug
              ORDER BY e.name, a.sort_order",
            super::redirects::Redirect::select_columns(),
            super::redirects::RetiredSlug::select_columns(),
        ))?;
        let aliases = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<bool>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, bool>(8)?,
                    row.get::<_, bool>(9)?,
                    row.get::<_, bool>(10)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (
            effect_id,
            name,
            alias_slug,
            template,
            owner,
            named,
            named_is_stat,
            routed,
            redirected,
            retired,
            owner_is_stat,
        ) in aliases
        {
            let effect = groupings.effect_named(&owner).context("alias owner is missing")?;
            let member_name = named_is_stat == Some(true)
                && effect.stat_ids.iter().any(|id| groupings.by_id.get(id).is_some_and(|member| member.name == name));
            ensure!(named.is_none() || member_name, "effect {owner:?} alias {name:?} collides with effect {named:?}");
            if member_name {
                db.execute(
                    "UPDATE effect_aliases SET slug = NULL WHERE effect_id = ?1 AND name = ?2",
                    rusqlite::params![effect_id, name],
                )?;
            } else {
                ensure!(
                    routed.is_none(),
                    "effect {owner:?} alias {name:?} slug {alias_slug:?} collides with effect {routed:?}"
                );
                ensure!(!redirected, "effect {owner:?} alias {name:?} slug {alias_slug:?} collides with a redirect");
                ensure!(!retired, "effect {owner:?} alias {name:?} slug {alias_slug:?} collides with a retired slug");
            }
            if let Some(template) = template {
                crate::build::validate_templates(
                    &name,
                    &template,
                    effect.description_template.as_deref(),
                    if owner_is_stat {
                        1
                    } else {
                        crate::build::amount_count(
                            &effect.verbose_name_template,
                            effect.description_template.as_deref(),
                        )
                    },
                )
                .with_context(|| format!("effect {owner:?} alias {name:?}"))?;
            }
        }
        Ok(())
    }

    pub(in crate::tree) fn by_effect(db: &Connection) -> Result<BTreeMap<i64, Vec<Self>>> {
        let mut statement = db.prepare(&format!(
            "SELECT {} FROM effect_aliases ORDER BY effect_id, sort_order",
            Self::select_columns()
        ))?;
        let mut by_effect = BTreeMap::<i64, Vec<Self>>::new();
        for row in statement.query_map([], |row| Ok((row.get::<_, i64>("effect_id")?, Self::from_row(row)?)))? {
            let (id, alias) = row?;
            by_effect.entry(id).or_default().push(alias);
        }
        Ok(by_effect)
    }
}

database_record! {
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct EffectBonus in "effect_bonuses" {
        keys { effect_id: i64, sort_order: i64 }
        fields {
            amount_from: i64 => amount_from,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            bonus_type: Option<String> => bonus_type_id [optional_reference "bonus_types"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            constant: Option<i64> => constant,
            rounding: String => rounding,
            scale: f64 => scale,
            target: String => target_effect_id [reference "effects"],
        }
        derived_corrections {
            SCALING_CORRECTIONS { scale => scale [StatScale false], rounding => rounding [StatRounding false] }
        }
    }

}

impl EffectBonus {
    pub(crate) fn from_database(db: &Connection, row_id: i64) -> rusqlite::Result<Self> {
        db.query_row(
            &format!("SELECT {} FROM effect_bonuses WHERE rowid = ?1", Self::select_columns()),
            [row_id],
            Self::from_row,
        )
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct EffectDamage in "effect_damage" {
        keys { effect_id: i64, sort_order: i64 }
        fields {
            amount_from: i64 => amount_from,
            damage_type: String => damage_type_id [reference "damage_types"],
            dice_bonus: i64 => dice_bonus,
            dice_number: i64 => dice_number,
            dice_sides: i64 => dice_sides,
            scale: f64 => scale,
            trigger: String => trigger_id [reference "triggers"],
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct EffectFile in "effects" {
        keys { id: i64, slug: &str, is_stat: bool, is_group: bool }
        fields {
            name: String => name [correctable],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            variant: Option<String> => variant,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            default_value: Option<i64> => default_value [correctable],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            description_template: Option<String> => description_template [correctable],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            home_bonus_type: Option<String> => home_bonus_type_id [optional_reference "bonus_types"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            set_bonus_line_template: Option<String> => set_bonus_line_template,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            stat_set_verdict: Option<String> => stat_set_verdict,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            stat_set_reason: Option<String> => stat_set_reason,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            tier: Option<i64> => tier,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            tier_group: Option<String> => tier_group_id [optional_reference "effect_tier_groups"],
            verbose_name_template: String => verbose_name_template [correctable],
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            alias: Vec<EffectAlias>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            bonus: Vec<EffectBonus>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            damage: Vec<EffectDamage>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            citation: Vec<super::super::Citation>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            ddobuilder_correction: Vec<super::super::DdobuilderCorrection>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            game_bug_correction: Vec<crate::tree::GameBugCorrection>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            verification_request: Vec<crate::tree::VerificationRequest>,
        }
    }
}

impl EffectFile {
    pub(in crate::tree) fn all_from_rows(
        db: &Connection,
        annotations: &crate::tree::annotations::EntityAnnotations,
        notes: &super::super::export::CorrectionNoteIndex,
    ) -> Result<BTreeMap<i64, Self>> {
        let mut files = BTreeMap::new();
        let mut aliases = EffectAlias::by_effect(db)?;
        let mut select = db.prepare(&format!(
            "SELECT {} FROM effects WHERE is_stat = 0 AND is_group = 0 ORDER BY id",
            Self::select_columns()
        ))?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>("id")?, Self::from_row(row)?)))? {
            let (id, mut file) = row?;
            file.alias = aliases.remove(&id).unwrap_or_default();
            file.attach_annotations(annotations, id);
            file.ddobuilder_correction = notes.for_entity("effects", &file.name);
            files.insert(id, file);
        }
        let mut bonuses = db.prepare(&format!(
            "SELECT {} FROM effect_bonuses ORDER BY effect_id, sort_order",
            EffectBonus::select_columns()
        ))?;
        for row in bonuses.query_map([], |row| Ok((row.get::<_, i64>("effect_id")?, EffectBonus::from_row(row)?)))? {
            let (id, bonus) = row?;
            if let Some(file) = files.get_mut(&id) {
                file.bonus.push(bonus);
            }
        }
        let mut damages = db.prepare(&format!(
            "SELECT {} FROM effect_damage ORDER BY effect_id, sort_order",
            EffectDamage::select_columns()
        ))?;
        for row in damages.query_map([], |row| Ok((row.get::<_, i64>("effect_id")?, EffectDamage::from_row(row)?)))? {
            let (id, damage) = row?;
            if let Some(file) = files.get_mut(&id) {
                file.damage.push(damage);
            }
        }
        Ok(files)
    }

    pub(in crate::tree) fn file_text(&self) -> anyhow::Result<String> {
        super::super::typed_file_text(self)
    }

    pub(in crate::tree) fn write_effect(&self, db: &Connection, path: &Path, id: i64, identity: &str) -> Result<()> {
        ensure!(
            id == super::super::ids::stable_hash(&["effects", identity]),
            "{}: effect id does not match its slug",
            path.display()
        );
        self.insert(db, id, identity, false, false)
            .with_context(|| format!("{}: inserting effect {:?}", path.display(), self.name))?;
        Ok(())
    }

    pub(in crate::tree) fn insert_bonuses(&self, db: &Connection, path: &Path, id: i64) -> Result<usize> {
        for (position, bonus) in self.bonus.iter().enumerate() {
            bonus.insert(db, id, position as i64).with_context(|| format!("{}: effect bonus", path.display()))?;
        }
        Ok(self.bonus.len())
    }

    pub(in crate::tree) fn insert_damage(&self, db: &Connection, path: &Path, id: i64) -> Result<usize> {
        for (position, damage) in self.damage.iter().enumerate() {
            damage.insert(db, id, position as i64).with_context(|| format!("{}: effect damage", path.display()))?;
        }
        Ok(self.damage.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::trace::{TraceEvent, TraceEventCodes};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn named_effect_round_trips_through_its_table() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        let file: EffectFile = toml::from_str("name = 'Test Effect'\nverbose_name_template = 'Test Effect'\nstat_set_verdict = 'distinct'\nstat_set_reason = 'Different mechanic'\n").unwrap();
        let id = super::super::super::ids::stable_hash(&["effects", "test-effect"]);
        file.write_effect(&db, Path::new("effects/test-effect.toml"), id, "test-effect").unwrap();
        let notes = super::super::super::export::CorrectionNoteIndex::from_database(&db).unwrap();
        let annotations = crate::tree::annotations::EntityAnnotations::from_database(&db).unwrap();
        let all = EffectFile::all_from_rows(&db, &annotations, &notes).unwrap();
        let restored = &all[&id];
        assert_eq!(restored.name, "Test Effect");
        assert!(restored.bonus.is_empty());
        assert!(restored.damage.is_empty());
        assert_eq!(restored.file_text().unwrap(), "name = \"Test Effect\"\nstat_set_verdict = \"distinct\"\nstat_set_reason = \"Different mechanic\"\nverbose_name_template = \"Test Effect\"\n");
    }

    #[test]
    fn effect_export_query_count_does_not_grow_with_effects() {
        static QUERY_COUNT: AtomicUsize = AtomicUsize::new(0);
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        let notes = super::super::super::export::CorrectionNoteIndex::from_database(&db).unwrap();
        let annotations = crate::tree::annotations::EntityAnnotations::from_database(&db).unwrap();
        db.trace_v2(
            TraceEventCodes::SQLITE_TRACE_STMT,
            Some(|event| {
                if let TraceEvent::Stmt(_, _) = event {
                    QUERY_COUNT.fetch_add(1, Ordering::SeqCst);
                }
            }),
        );
        let mut counts = Vec::new();
        for index in 0..50 {
            let name = format!("Example Effect {index}");
            let file: EffectFile =
                toml::from_str(&format!("name = {name:?}\nverbose_name_template = {name:?}\n")).unwrap();
            let identity = super::super::super::slug(&name);
            let id = super::super::super::ids::stable_hash(&["effects", &identity]);
            file.write_effect(&db, Path::new("effects/test.toml"), id, &identity).unwrap();
            let alias = EffectAlias {
                name: format!("Example Alias {index}"),
                verbose_name_template: Some(format!("Example Alias {index}")),
            };
            alias.insert(&db, id, 0, Some(&super::super::super::slug(&alias.name))).unwrap();
            if index != 0 && index != 49 {
                continue;
            }
            QUERY_COUNT.store(0, Ordering::SeqCst);
            let exported = EffectFile::all_from_rows(&db, &annotations, &notes).unwrap();
            assert_eq!(exported.len(), index + 1);
            assert!(exported.values().all(|file| file.alias.len() == 1));
            counts.push(QUERY_COUNT.load(Ordering::SeqCst));
        }
        assert_eq!(counts, vec![4, 4]);
    }
}
