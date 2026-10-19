use super::*;
use schemars::JsonSchema;

database_record! {
    #[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct Requirement in "requirements" {
        keys { owner_kind: &str, owner_id: i64, group_kind: &str, group_index: i64, sort_order: i64 }
        fields {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            items: Vec<String> => items [json],
            req_type: String => req_type,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            value: Option<i64> => value [integer_text],
        }
        attached {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            phase: Option<String>,
        }
    }
}

database_record! {
    #[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct Modifier in "modifiers" {
        keys { source_kind: &str, source_id: i64, sort_order: i64 }
        fields {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            amount_type: Option<String> => amount_type,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            amounts: Vec<file_types::FileNumber> => amounts [json],
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            apply_as_item_effect: bool => apply_as_item_effect,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            bonus: Option<String> => bonus [bonus],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            bonus_type: Option<String> => bonus_type_id [optional_reference "bonus_types"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            cap: Option<String> => cap,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            damage: Option<String> => damage,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            dice_bonus: Vec<i64> => dice_bonus [json],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            dice_damage: Option<String> => dice_damage,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            dice_number: Vec<i64> => dice_number [json],
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            dice_sides: Vec<i64> => dice_sides [json],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            display_name: Option<String> => display_name,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            effect: Option<String> => effect_id [optional_reference "effects"],
            effect_type: String => effect_type,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            extra_types: Vec<String> => extra_types [json],
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            is_item_specific: bool => is_item_specific,
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            is_rare: bool => is_rare,
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            percent: bool => percent,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            rank: Option<i64> => rank,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            stack_source: Option<String> => stack_source,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            targets: Vec<String> => targets [json],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            value: Option<String> => value,
        }
        attached {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            phase: Option<String>,
            #[serde(flatten)]
            requirements: Requirements,
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct Attack in "attacks" {
        keys { owner_kind: &str, owner_id: i64 }
        fields {
            name: String => name,
            cooldown_seconds: i64 => cooldown_seconds,
            description: String => description,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            duration_seconds: Option<i64> => duration_seconds,
            icon: String => icon [image ""],
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct DifficultyClass in "dcs" {
        keys { owner_kind: &str, owner_id: i64, sort_order: i64 }
        fields {
            name: String => name,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            amount: Vec<i64> => amount [json],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            base_class_level: Option<String> => base_class_level,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            class_level: Option<String> => class_level,
            dc_type: String => dc_type,
            dc_versus: String => dc_versus,
            description: String => description,
            icon: String => icon [image ""],
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            mod_ability: Vec<String> => mod_ability [json],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            other: Option<String> => other,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            skill: Option<String> => skill,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            tactical: Option<String> => tactical,
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            ddobuilder_correction: Vec<DdobuilderCorrection>,
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct Stance in "stances" {
        keys { id: Option<i64>, slug: &str, owner_kind: &str, owner_id: i64, sort_order: i64 }
        fields {
            name: String => name,
            #[serde(default, skip_serializing_if = "crate::tree::file_types::is_false")]
            auto_controlled: bool => auto_controlled,
            description: String => description,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            group_name: Option<String> => group_name,
            icon: String => icon [image ""],
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            incompatible: Vec<String> => incompatible [json],
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            citation: Vec<crate::tree::Citation>,
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            ddobuilder_correction: Vec<DdobuilderCorrection>,
            #[serde(flatten)]
            requirements: Requirements,
        }
    }
}

impl Stance {
    pub(super) fn write_stance(
        &self,
        db: &Connection,
        id: Option<i64>,
        slug: &str,
        owner_kind: &str,
        owner_id: i64,
        position: i64,
    ) -> Result<usize> {
        let inserted = self.insert(db, id, slug, owner_kind, owner_id, position)?;
        Ok(inserted.row_count + self.requirements.insert(db, "stance", inserted.row_id)?)
    }
}

pub(super) type StancesByOwner = BTreeMap<(String, i64), Vec<Stance>>;

pub(super) fn stances_by_owner(
    db: &Connection,
    annotations: &crate::tree::annotations::EntityAnnotations,
    requirements: &mut RequirementRowsByOwner,
) -> Result<StancesByOwner> {
    let mut stances = StancesByOwner::new();
    let mut select = db.prepare(&format!(
        "SELECT {} FROM stances ORDER BY owner_kind, owner_id, sort_order",
        Stance::select_columns()
    ))?;
    for row in select.query_map([], |row| {
        Ok((
            row.get::<_, i64>("id")?,
            row.get::<_, String>("owner_kind")?,
            row.get::<_, i64>("owner_id")?,
            Stance::from_row(row)?,
        ))
    })? {
        let (id, kind, owner_id, mut stance) = row?;
        stance.attach_citations(annotations, id);
        stance.requirements = requirements.remove(&("stance".to_string(), id)).unwrap_or_default();
        stances.entry((kind, owner_id)).or_default().push(stance);
    }
    Ok(stances)
}

pub(super) fn optional_json<T: Serialize>(rows: &[T]) -> Result<Option<String>> {
    (!rows.is_empty()).then(|| serde_json::to_string(rows)).transpose().map_err(Into::into)
}

impl Modifier {
    pub(super) fn write_modifier(&self, db: &Connection, kind: &str, owner_id: i64, position: i64) -> Result<usize> {
        ensure!(
            self.phase.as_ref().is_none_or(|phase| kind.ends_with(&format!("_{phase}"))),
            "modifier phase does not match its owner kind"
        );
        self.insert(db, kind, owner_id, position)?;
        let modifier_id = db.last_insert_rowid();
        Ok(1 + self.requirements.insert(db, "modifier", modifier_id)?)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub(crate) struct Requirements {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) requirement: Vec<Requirement>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) requirement_group: Vec<RequirementGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RequirementGroup {
    pub(crate) kind: String,
    pub(crate) requirement: Vec<Requirement>,
}

impl Requirements {
    pub(super) fn append(&mut self, mut requirements: Self, phase: Option<&str>) {
        for requirement in requirements
            .requirement
            .iter_mut()
            .chain(requirements.requirement_group.iter_mut().flat_map(|group| &mut group.requirement))
        {
            requirement.phase = phase.map(str::to_string);
        }
        self.requirement.append(&mut requirements.requirement);
        self.requirement_group.append(&mut requirements.requirement_group);
    }

    pub(super) fn insert(&self, db: &Connection, owner_kind: &str, owner_id: i64) -> Result<usize> {
        let mut groups_by_phase = BTreeMap::<Option<&str>, Vec<(&str, Vec<&Requirement>)>>::new();
        for requirement in &self.requirement {
            let groups = groups_by_phase.entry(requirement.phase.as_deref()).or_default();
            if groups.is_empty() {
                groups.push(("all", Vec::new()));
            }
            groups[0].1.push(requirement);
        }
        let shorthand_phases = groups_by_phase.keys().copied().collect::<BTreeSet<_>>();
        for group in &self.requirement_group {
            ensure!(
                ddo_model::enums::RequirementGroupKind::ALL.iter().any(|kind| kind.as_str() == group.kind),
                "unknown requirement group kind {:?}",
                group.kind
            );
            ensure!(!group.requirement.is_empty(), "requirement group must not be empty");
            let phase = group.requirement[0].phase.as_deref();
            ensure!(
                group.requirement.iter().all(|requirement| requirement.phase.as_deref() == phase),
                "requirement group must have one phase"
            );
            ensure!(!shorthand_phases.contains(&phase), "plain requirements are only valid for a single all group");
            groups_by_phase.entry(phase).or_default().push((&group.kind, group.requirement.iter().collect()));
        }
        let mut count = 0;
        for (phase, groups) in groups_by_phase {
            let kind = match (owner_kind, phase) {
                ("feat", Some("auto_acquire")) => "feat_auto_acquire",
                (_, None) => owner_kind,
                _ => bail!("requirement phase {phase:?} is not valid for {owner_kind}"),
            };
            for (group_index, (group_kind, requirements)) in groups.iter().enumerate() {
                for (position, requirement) in requirements.iter().enumerate() {
                    requirement.insert(db, kind, owner_id, group_kind, group_index as i64, position as i64)?;
                    count += 1;
                }
            }
        }
        Ok(count)
    }
}

pub(super) type ModifierRowsByOwner = BTreeMap<(String, i64), Vec<Modifier>>;

pub(super) fn json_list<T: serde::de::DeserializeOwned>(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> rusqlite::Result<Vec<T>> {
    let json: Option<String> = row.get(index)?;
    json.map(|value| {
        serde_json::from_str(&value).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, Box::new(error))
        })
    })
    .transpose()
    .map(Option::unwrap_or_default)
}

pub(super) type RequirementRowsByOwner = BTreeMap<(String, i64), Requirements>;

pub(super) fn requirement_rows_by_owner(db: &Connection) -> Result<RequirementRowsByOwner> {
    let mut owners = BTreeMap::<(String, i64), BTreeMap<i64, RequirementGroup>>::new();
    let mut select = db.prepare(&format!(
        "SELECT {} FROM requirements ORDER BY owner_kind, owner_id, group_index, sort_order",
        Requirement::select_columns()
    ))?;
    for row in select.query_map([], |row| {
        Ok((
            row.get::<_, String>("owner_kind")?,
            row.get::<_, i64>("owner_id")?,
            row.get::<_, i64>("group_index")?,
            row.get::<_, String>("group_kind")?,
            Requirement::from_row(row)?,
        ))
    })? {
        let (kind, id, index, group_kind, requirement) = row?;
        let groups = owners.entry((kind, id)).or_default();
        ensure!(groups.contains_key(&index) || index == groups.len() as i64, "requirement groups must be consecutive");
        let group = groups
            .entry(index)
            .or_insert_with(|| RequirementGroup { kind: group_kind.clone(), requirement: Vec::new() });
        ensure!(group.kind == group_kind, "requirement group must have one kind");
        group.requirement.push(requirement);
    }
    Ok(owners
        .into_iter()
        .map(|(owner, groups)| {
            let mut groups = groups.into_values().collect::<Vec<_>>();
            let requirements = if groups.len() == 1 && groups[0].kind == "all" {
                Requirements { requirement: groups.remove(0).requirement, ..Requirements::default() }
            } else {
                Requirements { requirement_group: groups, ..Requirements::default() }
            };
            (owner, requirements)
        })
        .collect())
}

pub(super) fn modifier_rows_by_owner(db: &Connection) -> Result<ModifierRowsByOwner> {
    let mut modifiers = ModifierRowsByOwner::new();
    let mut locations = BTreeMap::new();
    let mut select = db.prepare(&format!(
        "SELECT id, {} FROM modifiers ORDER BY source_kind, source_id, sort_order",
        Modifier::select_columns()
    ))?;
    for row in select.query_map([], |row| {
        let mut modifier = Modifier::from_row(row)?;
        if modifier.bonus == modifier.bonus_type {
            modifier.bonus = None;
        }
        Ok((row.get::<_, i64>("id")?, row.get::<_, String>("source_kind")?, row.get::<_, i64>("source_id")?, modifier))
    })? {
        let (id, kind, source_id, modifier) = row?;
        let key = (kind, source_id);
        let rows = modifiers.entry(key.clone()).or_default();
        locations.insert(id, (key, rows.len()));
        rows.push(modifier);
    }
    for ((kind, id), requirements) in requirement_rows_by_owner(db)? {
        if kind != "modifier" {
            continue;
        }
        let (key, position) = locations.get(&id).with_context(|| format!("requirement has no modifier {id}"))?;
        modifiers.get_mut(key).unwrap()[*position].requirements = requirements;
    }
    Ok(modifiers)
}

pub(super) fn id_for_name(db: &Connection, table: &str, name: &str) -> Result<i64> {
    db.query_row(&format!("SELECT id FROM \"{table}\" WHERE name = ?1"), [name], |row| row.get(0))
        .with_context(|| format!("unknown {table} name {name:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requirement_shorthand_and_phases_round_trip() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::schema::ddl()).unwrap();
        let text = r#"
[[row]]
name = "Test Feat"
acquire = "Train"
[[row.requirement]]
req_type = "Level"
value = 0
[[row.requirement_group]]
kind = "none_of"
[[row.requirement_group.requirement]]
phase = "auto_acquire"
req_type = "Feat"
items = ["Shield Proficiency"]
"#;
        let file: super::super::kinds::FeatCategoryFile = toml::from_str(text).unwrap();
        file.row[0].requirements.insert(&db, "feat", 1).unwrap();
        let mut restored = requirement_rows_by_owner(&db).unwrap();
        let mut requirements = restored.remove(&("feat".to_string(), 1)).unwrap();
        requirements.append(restored.remove(&("feat_auto_acquire".to_string(), 1)).unwrap(), Some("auto_acquire"));
        assert_eq!(toml::to_string(&requirements).unwrap(), toml::to_string(&file.row[0].requirements).unwrap());
        for invalid in [
            text.replace("req_type = \"Level\"", "group = 'label'\nreq_type = \"Level\""),
            text.replace("acquire = \"Train\"", "unknown = true\nacquire = \"Train\""),
        ] {
            assert!(toml::from_str::<super::super::kinds::FeatCategoryFile>(&invalid).is_err());
        }
    }

    #[test]
    fn invalid_requirement_groups_fail_before_writing() {
        for text in [
            "[[requirement_group]]\nkind = 'all'\nrequirement = []",
            "[[requirement_group]]\nkind = 'invalid'\n[[requirement_group.requirement]]\nreq_type = 'Level'\nvalue = 1",
            "[[requirement]]\nreq_type = 'Level'\nvalue = 1\n[[requirement_group]]\nkind = 'all'\n[[requirement_group.requirement]]\nreq_type = 'Level'\nvalue = 2",
            "[[requirement_group]]\nkind = 'all'\n[[requirement_group.requirement]]\nreq_type = 'Level'\nvalue = 1\n[[requirement_group.requirement]]\nphase = 'auto_acquire'\nreq_type = 'Level'\nvalue = 2",
            "[[requirement]]\nphase = 'invalid'\nreq_type = 'Level'\nvalue = 1",
        ] {
            let db = Connection::open_in_memory().unwrap();
            db.execute_batch(ddo_model::schema::ddl()).unwrap();
            let requirements: Requirements = toml::from_str(text).unwrap();
            assert!(requirements.insert(&db, "feat", 1).is_err(), "{text}");
            assert_eq!(db.query_row("SELECT COUNT(*) FROM requirements", [], |row| row.get::<_, i64>(0)).unwrap(), 0);
        }
    }

    #[test]
    fn nested_requirement_groups_preserve_indexes_and_order() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::schema::ddl()).unwrap();
        let text = r#"
[[requirement_group]]
kind = "all"
[[requirement_group.requirement]]
req_type = "Level"
value = 21
[[requirement_group.requirement]]
req_type = "Ability"
items = ["Wisdom"]
value = 23
[[requirement_group]]
kind = "one_of"
[[requirement_group.requirement]]
req_type = "ClassMinLevel"
items = ["Monk"]
value = 12
[[requirement_group.requirement]]
req_type = "ClassMinLevel"
items = ["Sacred Fist"]
value = 12
[[requirement_group]]
kind = "all"
[[requirement_group.requirement]]
req_type = "Feat"
items = ["Improved Critical: Bludgeoning Weapons"]
"#;
        let requirements: Requirements = toml::from_str(text).unwrap();
        requirements.insert(&db, "feat", 1).unwrap();
        let groups = db.prepare("SELECT group_index, group_kind, req_type, sort_order FROM requirements ORDER BY group_index, sort_order").unwrap()
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, i64>(3)?))).unwrap()
            .collect::<rusqlite::Result<Vec<_>>>().unwrap();
        assert_eq!(
            groups,
            vec![
                (0, "all".into(), "Level".into(), 0),
                (0, "all".into(), "Ability".into(), 1),
                (1, "one_of".into(), "ClassMinLevel".into(), 0),
                (1, "one_of".into(), "ClassMinLevel".into(), 1),
                (2, "all".into(), "Feat".into(), 0)
            ]
        );
        let restored = requirement_rows_by_owner(&db).unwrap().remove(&("feat".to_string(), 1)).unwrap();
        assert_eq!(toml::to_string(&restored).unwrap(), toml::to_string(&requirements).unwrap());
    }
}

database_record! {
    #[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct Skill in (table: &str, owner_column: &str) uses {
        "class_skills" => "class_id",
        "class_auto_buy_skills" => "class_id",
        "race_auto_buy_skills" => "race_id",
    } {
        keys { owner_id: i64 => owner_column }
        fields { skill: String => skill }
    }
}

database_record! {
    #[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct EffectGrant in (table: &str, owner_column: &str) uses {
        "feat_effects" => "feat_id",
        "set_bonus_tier_effects" => "tier_id",
    } {
        keys { owner_id: i64 => owner_column, sort_order: i64 }
        fields {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            bonus_type: Option<String> => bonus_type_id [optional_reference "bonus_types"],
            effect: String => effect_id [reference "effects"],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            value: Option<i64> => value,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            value2: Option<i64> => value2,
        }
        derived_corrections {
            GRANT_CORRECTIONS { value => value [Integer true], bonus_type => bonus_type [BonusTypeName false] }
        }
    }

}
