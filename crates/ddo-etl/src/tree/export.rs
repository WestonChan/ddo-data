use super::*;

pub fn data_file_bytes(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| match entry {
            Ok(entry)
                if entry.file_type().is_file()
                    && entry.path().extension().is_some_and(|extension| extension == "toml") =>
            {
                Some(Ok(entry))
            }
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .map(|entry| {
            let entry = entry?;
            Ok((entry.path().strip_prefix(root)?.to_path_buf(), std::fs::read(entry.path())?))
        })
        .collect()
}

struct CorrectionNote {
    note: EntityNote,
    name: String,
    applied: serde_json::Value,
}

pub(super) struct CorrectionNoteIndex {
    rows: BTreeMap<(String, String), Vec<CorrectionNote>>,
    renames: BTreeMap<(String, String), Vec<String>>,
}

impl CorrectionNoteIndex {
    pub(super) fn from_database(db: &Connection) -> Result<Self> {
        let mut rows: BTreeMap<(String, String), Vec<CorrectionNote>> = BTreeMap::new();
        let mut renames: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
        let mut select = db.prepare(&format!(
            "SELECT {} FROM corrections ORDER BY id",
            super::annotations::AppliedCorrection::select_columns()
        ))?;
        for correction in select.query_map([], super::annotations::AppliedCorrection::from_row)? {
            let super::annotations::AppliedCorrection {
                kind,
                name,
                field,
                from_value: upstream_json,
                reason,
                url: source,
                read,
                qualifier,
                to_value: applied_json,
            } = correction?;
            let upstream_value: serde_json::Value = serde_json::from_str(&upstream_json)?;
            let upstream = if upstream_value.is_null() {
                None
            } else if field == "is_legacy" {
                Some(serde_json::Value::Bool(upstream_value.as_i64() == Some(1)))
            } else {
                Some(upstream_value)
            };
            let applied: serde_json::Value = serde_json::from_str(&applied_json)?;
            if field == "name" {
                if let Some(new_name) = applied.as_str() {
                    if new_name != name {
                        renames.entry((kind.clone(), new_name.to_string())).or_default().push(name.clone());
                    }
                }
            }
            let note = EntityNote {
                kind: kind.clone(),
                original_name: None,
                added_as_name: (kind == "item_effect" && field == "add" && applied.is_string()).then_some(true),
                field,
                upstream,
                reason,
                url: source,
                read,
                qualifier,
            };
            rows.entry((kind, name.clone())).or_default().push(CorrectionNote { note, name, applied });
        }
        Ok(Self { rows, renames })
    }

    pub(super) fn for_entity(&self, table: &str, name: &str) -> Vec<EntityNote> {
        self.notes_for_owner(table, name)
            .into_iter()
            .filter(|note| {
                !note.qualifier.starts_with("selection ")
                    && !note.qualifier.starts_with("stance ")
                    && !note.qualifier.starts_with("dc ")
            })
            .collect()
    }

    pub(super) fn for_named_child(&self, table: &str, owner_name: &str, keys: &[(&str, &str)]) -> Vec<EntityNote> {
        let qualifier = super::correction_notes::named_row_qualifier(keys);
        self.notes_for_owner(table, owner_name)
            .into_iter()
            .filter(|note| note.qualifier == qualifier)
            .map(|mut note| {
                note.qualifier.clear();
                note
            })
            .collect()
    }

    fn notes_for_owner(&self, table: &str, name: &str) -> Vec<EntityNote> {
        let kinds: &[&str] = match table {
            "items" => &["item", "item_bonus", "item_effect", "item_socket"],
            "augments" => &["augment", "augment_bonus"],
            "quests" => &["quest"],
            "set_bonuses" => &["set_bonus", "set_tier", "set_tier_bonus"],
            "effects" => &["effect", "effect_damage"],
            "feats" => &["feat"],
            "races" => &["race"],
            "classes" => &["class"],
            "adventure_packs" => &["adventure_pack"],
            "patrons" => &["patron"],
            "augment_slot_types" => &["socket_label"],
            "enhancements" => &["enhancement"],
            "enhancement_trees" => &["enhancement_tree"],
            "clickies" => &["clickie"],
            "optional_buffs" => &["optional_buff"],
            "filigrees" => &["filigree"],
            "spells" => &["spell"],
            _ => &[],
        };
        let mut source_names = vec![name.to_string()];
        if let Some(rename_kind) = kinds.first() {
            source_names
                .extend(self.renames.get(&(rename_kind.to_string(), name.to_string())).into_iter().flatten().cloned());
        }
        let mut notes = Vec::new();
        for kind in kinds {
            for source_name in &source_names {
                if let Some(rows) = self.rows.get(&((*kind).to_string(), source_name.clone())) {
                    notes.extend(rows.iter().map(|row| {
                        let mut note = row.note.clone();
                        note.original_name = (row.name != name && note.field != "name").then(|| row.name.clone());
                        note.added_as_name =
                            (note.kind == "item_effect" && note.field == "add" && row.applied.is_string())
                                .then_some(true);
                        note
                    }));
                }
            }
        }
        notes
    }
}

#[cfg(test)]
pub(super) fn correction_notes(db: &Connection, table: &str, name: &str) -> Result<Vec<EntityNote>> {
    Ok(CorrectionNoteIndex::from_database(db)?.for_entity(table, name))
}

pub(super) fn write_data_file(path: &Path, text: &str) -> Result<()> {
    std::fs::create_dir_all(path.parent().context("data file has no parent")?)?;
    std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

pub(super) fn typed_file_text<T: serde::Serialize>(file: &T) -> Result<String> {
    escape_trailing_space_multiline_literals(&toml::to_string_pretty(file)?)
}

#[derive(Deserialize)]
struct LiteralText {
    value: String,
}

pub(super) fn escape_trailing_space_multiline_literals(contents: &str) -> Result<String> {
    if !contents.lines().any(|line| line.ends_with([' ', '\t'])) {
        return Ok(contents.to_string());
    }
    let lines = contents.lines().collect::<Vec<_>>();
    let mut output = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        let Some((field, first_literal_line)) = line.split_once(" = ") else {
            output.push(line.to_string());
            index += 1;
            continue;
        };
        if !first_literal_line.starts_with("\"\"\"") {
            output.push(line.to_string());
            index += 1;
            continue;
        }
        let mut literal = first_literal_line.to_string();
        let mut ending_index = index;
        let source_text = loop {
            if let Ok(parsed) = toml::from_str::<LiteralText>(&format!("value = {literal}")) {
                break parsed.value;
            }
            ending_index += 1;
            ensure!(ending_index < lines.len(), "multiline TOML literal has no closing quote");
            literal.push('\n');
            literal.push_str(lines[ending_index]);
        };
        if lines[index..=ending_index].iter().any(|line| line.ends_with([' ', '\t'])) {
            output.push(format!("{field} = {}", serde_json::to_string(&source_text)?));
        } else {
            output.extend(lines[index..=ending_index].iter().map(|line| (*line).to_string()));
        }
        index = ending_index + 1;
    }
    let mut result = output.join("\n");
    if contents.ends_with('\n') {
        result.push('\n');
    }
    ensure!(!result.lines().any(|line| line.ends_with([' ', '\t'])), "generated TOML still has trailing whitespace");
    Ok(result)
}

pub(super) fn wiki_citations(
    wiki: &WikiOverrides,
    table: &str,
    name: &str,
    is_wiki_row: bool,
    family: Option<&str>,
) -> Vec<Citation> {
    let mut source = None;
    let mut read = None;
    let mut citations = Vec::new();
    if table == "items" && is_wiki_row {
        if let Some(item) = wiki.items.iter().find(|item| item.name == name) {
            source = Some(item.page.clone());
            read = Some(item.read.clone());
            if !item.quests.is_empty() {
                citations.push(Citation {
                    field: "sources".to_string(),
                    url: item.page.clone(),
                    read: item.read.clone(),
                });
            }
        }
    }
    if table == "augments" && is_wiki_row {
        if let Some(augment) =
            wiki.augments.iter().find(|augment| augment.name == name && Some(augment.family.as_str()) == family)
        {
            source = Some(augment.page.clone());
            read = Some(augment.read.clone());
        }
    }
    if table == "quests" {
        if let Some(quest) = wiki.quests.iter().find(|quest| quest.name == name) {
            if is_wiki_row {
                source = Some(quest.page.clone());
                read = Some(quest.read.clone());
            } else {
                citations.push(Citation {
                    field: "quest facts".to_string(),
                    url: quest.page.clone(),
                    read: quest.read.clone(),
                });
            }
        }
    }
    if table == "crafting_systems" {
        if let Some(system) = wiki.crafting_systems.iter().find(|system| system.name == name) {
            source = Some(system.page.clone());
            read = Some(system.read.clone());
        }
    }
    if table == "vendors" {
        if let Some(vendor) = wiki.vendors.iter().find(|vendor| vendor.name == name) {
            source = Some(vendor.page.clone());
            read = Some(vendor.read.clone());
        }
    }
    if table == "events" {
        if let Some(event) = wiki.events.iter().find(|event| event.name == name) {
            source = Some(event.page.clone());
            read = Some(event.read.clone());
        }
    }
    if table == "quest_chains" {
        if let Some(chain) = wiki.quest_chains.iter().find(|chain| chain.name == name) {
            source = Some(chain.page.clone());
            read = Some(chain.read.clone());
        }
    }
    if table == "sagas" {
        if let Some(saga) = wiki.sagas.iter().find(|saga| saga.name == name) {
            source = Some(saga.page.clone());
            read = Some(saga.read.clone());
        }
    }
    for description in wiki
        .descriptions
        .iter()
        .filter(|description| description.kind.table_name() == table && description.name == name)
    {
        citations.push(Citation {
            field: "description".to_string(),
            url: description.page.clone(),
            read: description.read.clone(),
        });
    }
    if let (Some(url), Some(read)) = (source, read) {
        citations.insert(0, Citation { field: String::new(), url, read });
    }
    citations
}

pub(super) fn write_export_file(
    root: &Path,
    relative: &Path,
    contents: &str,
    report: &mut DatasetFileReport,
) -> Result<()> {
    let path = root.join(relative);
    ensure!(!report.written_files.contains(&path), "duplicate export path {}", relative.display());
    write_data_file(&path, &escape_trailing_space_multiline_literals(contents)?)?;
    bump_file_count(report, &path, root);
    Ok(())
}

pub fn export_ddobuilder_tree(db: &Connection, data_files_dir: &Path, root: &Path) -> Result<DatasetFileReport> {
    let report = export_database(db, root)?;
    crate::icons::export_icons(data_files_dir, &root.join("icons"))?;
    let icon_report = crate::icons::complete_referenced_icons(root)?;
    ensure!(icon_report.missing_icons.is_empty(), "missing image files: {:?}", icon_report.missing_icons);
    ensure!(
        icon_report.unresolved_case_aliases.is_empty(),
        "icon references differ from upstream filenames; correct the references with notes: {:?}",
        icon_report.unresolved_case_aliases
    );
    crate::icons::place_enhancement_icons_beside_trees(root)?;
    ensure!(report.icon_actions.is_empty(), "export has HARD image layout failures");
    Ok(report)
}

pub fn export_database(db: &Connection, root: &Path) -> Result<DatasetFileReport> {
    let mut report = DatasetFileReport::default();
    let annotations = annotations::EntityAnnotations::from_database(db)?;
    let notes = CorrectionNoteIndex::from_database(db)?;
    let layout = ExportLayout::from_database(db)?;
    report.layout_warnings.extend(layout.warnings.iter().cloned());
    let mut sources = sources::sources_by_owner(db)?;
    let mut modifiers = modifier_rows_by_owner(db)?;
    for (id, file) in kinds::ItemFile::all_from_rows(db, &annotations, &notes, &mut sources, &mut modifiers)? {
        let relative = layout.path("items", id)?;
        write_export_file(root, relative, &file.file_text()?, &mut report)?;
    }
    for (id, file) in kinds::AugmentFile::all_from_rows(db, &annotations, &notes, &mut sources, &mut modifiers)? {
        let relative = layout.path("augments", id)?;
        write_export_file(root, relative, &file.file_text()?, &mut report)?;
    }
    for (id, file) in kinds::SetFile::all_from_rows(db, &notes, &mut modifiers)? {
        let relative = layout.path("set_bonuses", id)?;
        write_export_file(root, relative, &file.file_text()?, &mut report)?;
    }
    for (id, mut file) in kinds::QuestFile::all_from_rows(db, &annotations, &notes)? {
        let relative = layout.path("quests", id)?;
        if let Some(annotation) = annotations.entity("quests", id) {
            file.note.extend(annotation.reviewed_notes.iter().cloned());
        }
        write_export_file(root, relative, &file.file_text()?, &mut report)?;
    }
    for (id, mut file) in kinds::EffectFile::all_from_rows(db, &notes)? {
        let relative = layout.path("effects", id)?;
        if let Some(annotation) = annotations.entity("effects", id) {
            file.note.extend(annotation.reviewed_notes.iter().cloned());
        }
        write_export_file(root, relative, &file.file_text()?, &mut report)?;
    }
    for (id, file) in kinds::StatFile::all_from_rows(db)? {
        let relative = layout.path("effects", id)?;
        write_export_file(root, relative, &file.file_text()?, &mut report)?;
    }
    for (id, file) in kinds::StatGroupFile::all_from_rows(db)? {
        let relative = layout.path("effects", id)?;
        write_export_file(root, relative, &file.file_text()?, &mut report)?;
    }
    for (id, file) in
        kinds::QuestSeriesFile::all_from_rows(db, &annotations, "quest_chains", "quest_chain_quests", "chain_id")?
    {
        let relative = layout.path("quest_chains", id)?;
        write_export_file(root, relative, &file.file_text()?, &mut report)?;
    }
    for (id, file) in kinds::QuestSeriesFile::all_from_rows(db, &annotations, "sagas", "saga_quests", "saga_id")? {
        let relative = layout.path("sagas", id)?;
        write_export_file(root, relative, &file.file_text()?, &mut report)?;
    }
    for (id, file) in kinds::PackFile::all_from_rows(db, &annotations, &notes)? {
        let relative = layout.path("adventure_packs", id)?;
        write_export_file(root, relative, &toml::to_string_pretty(&file)?, &mut report)?;
    }
    for (id, file) in kinds::PatronFile::all_from_rows(db, &notes)? {
        let relative = layout.path("patrons", id)?;
        write_export_file(root, relative, &toml::to_string_pretty(&file)?, &mut report)?;
    }
    for (id, file) in kinds::EffectTierFile::all_from_rows(db)? {
        let relative = layout.path("effect_tier_groups", id)?;
        write_export_file(root, relative, &toml::to_string_pretty(&file)?, &mut report)?;
    }
    for (id, file) in kinds::EventFile::all_from_rows(db, &annotations)? {
        let relative = layout.path("events", id)?;
        write_export_file(root, relative, &toml::to_string_pretty(&file)?, &mut report)?;
    }
    for (id, file) in kinds::VendorFile::all_from_rows(db, &annotations)? {
        let relative = layout.path("vendors", id)?;
        write_export_file(root, relative, &toml::to_string_pretty(&file)?, &mut report)?;
    }
    for (id, file) in kinds::RaceFile::all_from_rows(db, &annotations)? {
        let relative = layout.path("races", id)?;
        write_export_file(root, relative, &file.file_text()?, &mut report)?;
    }
    for (id, file) in kinds::ClassFile::all_from_rows(db)? {
        let relative = layout.path("classes", id)?;
        write_export_file(root, relative, &file.file_text()?, &mut report)?;
    }
    for (id, file) in kinds::CraftingSystemFile::all_from_rows(db, &annotations)? {
        let relative = layout.path("crafting_systems", id)?;
        write_export_file(root, relative, &file.file_text()?, &mut report)?;
    }
    for (id, file) in kinds::EnhancementTreeFile::all_from_rows(db, &annotations, &notes, &mut modifiers)? {
        let relative = layout.path("enhancement_trees", id)?;
        write_export_file(root, relative, &file.file_text()?, &mut report)?;
    }
    let mut groups = BTreeMap::<PathBuf, kinds::SpellListFile>::new();
    for (id, file) in kinds::SpellListFile::all_from_rows(db, &notes, &mut modifiers)? {
        groups
            .entry(layout.path("spells", id)?.to_path_buf())
            .or_insert_with(|| kinds::SpellListFile { row: Vec::new() })
            .row
            .push(file);
    }
    for (relative, mut file) in groups {
        file.row.sort_by_key(|row| row.name.clone());
        write_export_file(root, &relative, &typed_file_text(&file)?, &mut report)?;
    }
    let mut groups = BTreeMap::<PathBuf, kinds::FeatCategoryFile>::new();
    for (id, file) in kinds::FeatCategoryFile::all_from_rows(db, &annotations, &notes, &mut modifiers)? {
        groups
            .entry(layout.path("feats", id)?.to_path_buf())
            .or_insert_with(|| kinds::FeatCategoryFile { row: Vec::new() })
            .row
            .push(file);
    }
    for (relative, mut file) in groups {
        file.row.sort_by_key(|row| (row.name.clone(), row.variant.clone()));
        write_export_file(root, &relative, &typed_file_text(&file)?, &mut report)?;
    }
    let mut groups = BTreeMap::<PathBuf, kinds::FiligreeSetFile>::new();
    for (id, file) in kinds::FiligreeSetFile::all_from_rows(db, &mut modifiers, &notes)? {
        groups
            .entry(layout.path("filigrees", id)?.to_path_buf())
            .or_insert_with(|| kinds::FiligreeSetFile { row: Vec::new() })
            .row
            .push(file);
    }
    for (relative, mut file) in groups {
        file.row.sort_by_key(|row| row.name.clone());
        write_export_file(root, &relative, &typed_file_text(&file)?, &mut report)?;
    }
    let file = kinds::StancesFile::from_rows(db)?;
    write_export_file(root, Path::new("characters/stances.toml"), &toml::to_string_pretty(&file)?, &mut report)?;
    let file = kinds::ClickiesFile::from_rows(db, &mut modifiers, &notes)?;
    write_export_file(root, Path::new("effects/clickies.toml"), &typed_file_text(&file)?, &mut report)?;
    let file = kinds::GuildBuffsFile::from_rows(db, &mut modifiers)?;
    write_export_file(root, Path::new("characters/guild-buffs.toml"), &typed_file_text(&file)?, &mut report)?;
    let file = kinds::OptionalBuffsFile::from_rows(db, &mut modifiers, &notes)?;
    write_export_file(root, Path::new("characters/optional-buffs.toml"), &typed_file_text(&file)?, &mut report)?;
    let file = kinds::BonusTypesFile::from_rows(db)?;
    write_export_file(root, Path::new("effects/bonus-types.toml"), &toml::to_string_pretty(&file)?, &mut report)?;
    let file = kinds::ItemMaterialsFile::from_rows(db)?;
    write_export_file(root, Path::new("effects/item-materials.toml"), &toml::to_string_pretty(&file)?, &mut report)?;
    let file = kinds::TriggersFile::from_rows(db)?;
    write_export_file(root, Path::new("effects/triggers.toml"), &toml::to_string_pretty(&file)?, &mut report)?;
    let file = kinds::WeaponProficienciesFile::from_rows(db)?;
    write_export_file(
        root,
        Path::new("effects/weapon-proficiencies.toml"),
        &toml::to_string_pretty(&file)?,
        &mut report,
    )?;
    let file = kinds::EquipmentSlotsFile::from_rows(db)?;
    write_export_file(root, Path::new("effects/equipment-slots.toml"), &toml::to_string_pretty(&file)?, &mut report)?;
    let file = kinds::DamageTypesFile::from_rows(db)?;
    write_export_file(root, Path::new("effects/damage-types.toml"), &toml::to_string_pretty(&file)?, &mut report)?;
    let file = kinds::WeaponTypesFile::from_rows(db)?;
    write_export_file(root, Path::new("effects/weapon-types.toml"), &toml::to_string_pretty(&file)?, &mut report)?;
    let file = kinds::ExcludedItemsFile::from_rows(db)?;
    write_export_file(root, Path::new("upstream/excluded-items.toml"), &toml::to_string_pretty(&file)?, &mut report)?;
    for (id, file) in kinds::SentientGemFile::all_from_rows(db)? {
        write_export_file(root, layout.path("sentient_gems", id)?, &typed_file_text(&file)?, &mut report)?;
    }
    let file = kinds::AugmentSlotTypesFile::from_rows(db)?;
    write_export_file(root, Path::new("effects/augment-slot-types.toml"), &typed_file_text(&file)?, &mut report)?;
    ensure!(sources.items.is_empty() && sources.augments.is_empty(), "sources remain without an item or augment file");
    ensure!(modifiers.is_empty(), "modifiers remain without an owner file: {:?}", modifiers.keys().collect::<Vec<_>>());
    for table in table_names(db)? {
        report.row_count +=
            db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get::<_, i64>(0))? as usize;
    }
    let schema_files = schemas::schema_files()?;
    let schema_root = root.join(".schemas");
    if schema_root.exists() {
        for entry in std::fs::read_dir(&schema_root)? {
            let path = entry?.path();
            if path.extension().is_some_and(|extension| extension == "json")
                && path.file_name().and_then(|name| name.to_str()).is_some_and(|name| !schema_files.contains_key(name))
            {
                std::fs::remove_file(&path).with_context(|| format!("removing stale schema {}", path.display()))?;
            }
        }
    }
    for (file_name, contents) in schema_files {
        write_data_file(&schema_root.join(file_name), &contents)?;
    }
    let currencies = kinds::VendorCurrenciesFile::from_rows(db, &annotations)?;
    write_export_file(
        root,
        Path::new("packs/shared/vendors/currencies.toml"),
        &toml::to_string_pretty(&currencies)?,
        &mut report,
    )?;
    write_pack_exceptions(db, root, &mut report)?;
    let alias_path = root.join("upstream/source-aliases.toml");
    let alias_contents =
        annotations.files.get("upstream/source-aliases.toml").context("database has no source aliases")?;
    write_data_file(&alias_path, alias_contents)?;
    bump_file_count(&mut report, &alias_path, root);
    let upstream = UpstreamCommitFile::from_rows(db)?;
    let upstream_path = root.join("upstream/commit.toml");
    write_data_file(&upstream_path, &toml::to_string(&upstream)?)?;
    bump_file_count(&mut report, &upstream_path, root);
    remove_unwritten_data_files(root, &report)?;
    report.icon_actions = crate::icons::enhancement_icon_actions(root)?;
    Ok(report)
}
