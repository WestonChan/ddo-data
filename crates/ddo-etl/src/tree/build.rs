use super::*;
use schemas::SourceFile;

pub(super) fn validate_data_icons(db: &Connection, icon_index: &crate::icons::IconIndex) -> Result<()> {
    for &(table, kind) in crate::icons::ICON_TABLE_KINDS {
        let mut statement =
            db.prepare(&format!("SELECT name, icon FROM {table} WHERE icon IS NOT NULL AND icon <> '' ORDER BY id"))?;
        let icons = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
        for icon in icons {
            let (name, icon) = icon?;
            ensure!(
                !icon.contains('/') && !icon.contains('\\'),
                "{table} {name:?} has icon name {icon:?} with a path separator"
            );
            let file_name = format!("{icon}.png");
            ensure!(
                icon_index.has_database_icon(kind, &file_name),
                "{table} {name:?} names missing data icon {kind}/{file_name}"
            );
        }
    }
    Ok(())
}

pub(crate) struct DataIconReference {
    pub(crate) kind: String,
    pub(crate) tree: Option<String>,
    pub(crate) icon: String,
    pub(crate) field: &'static str,
}

pub(crate) fn data_file_icon_references(root: &Path, path: &Path, document: &SourceFile) -> Vec<DataIconReference> {
    let relative_path = path.strip_prefix(root).unwrap_or(path);
    let kind = crate::icons::icon_kind_for_data_path(relative_path).map_or("", |(_, kind)| kind);
    let tree = if kind == "enhancements" {
        path.parent().and_then(Path::file_name).and_then(|name| name.to_str()).map(str::to_string)
    } else {
        None
    };
    document
        .icons(kind)
        .into_iter()
        .map(|(kind, icon, field)| DataIconReference {
            kind: kind.to_string(),
            tree: tree.clone(),
            icon: icon.to_string(),
            field,
        })
        .collect()
}

pub(crate) fn referenced_data_icons(root: &Path) -> Result<Vec<DataIconReference>> {
    let mut references = Vec::new();
    for entry in WalkDir::new(root).sort_by_file_name() {
        let entry = entry?;
        let path = entry.path();
        if !entry.file_type().is_file()
            || path.extension().is_none_or(|extension| extension != "toml")
            || path.components().any(|component| component.as_os_str() == ".schemas")
        {
            continue;
        }
        schemas::file_kind(root, path)?;
        let contents = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let document =
            schemas::read_file(root, path, &contents).with_context(|| format!("reading {}", path.display()))?;
        references.extend(data_file_icon_references(root, path, &document));
    }
    Ok(references)
}

pub(super) fn validate_file_icons(
    root: &Path,
    path: &Path,
    document: &schemas::SourceFile,
    icon_index: &crate::icons::IconIndex,
    referenced_icons: &mut BTreeSet<(String, String)>,
) -> Result<()> {
    path.strip_prefix(root)?;
    for DataIconReference { kind, tree, icon, field } in data_file_icon_references(root, path, document) {
        let reference = icon_index
            .validate_reference(&kind, tree.as_deref(), &icon)
            .with_context(|| format!("{}: {field} {icon:?}", path.display()))?;
        referenced_icons.insert(reference);
    }
    Ok(())
}

pub(super) fn validate_pack_exceptions(db: &Connection, entries: &[(&Path, &kinds::ExceptionsFile)]) -> Result<()> {
    for (path, exceptions) in entries {
        let pack_path = path.parent().context("exceptions file has no pack folder")?.join("pack.toml");
        let contents =
            std::fs::read_to_string(&pack_path).with_context(|| format!("reading {}", pack_path.display()))?;
        let pack: kinds::PackFile =
            toml::from_str(&contents).with_context(|| format!("reading {}", pack_path.display()))?;
        let pack_name = &pack.name;
        for item in &exceptions.item {
            ensure!(
                reviewed_pack_names_for_item(db, &item.name)?.iter().any(|name| name == pack_name),
                "{}: reviewed item {:?} does not belong to pack {:?}",
                path.display(),
                item.name,
                pack_name
            );
            ensure!(
                item.url.starts_with("https://ddowiki.com/page/"),
                "{}: reviewed item {:?} has no wiki citation",
                path.display(),
                item.name
            );
        }
        for patron in &exceptions.patron {
            ensure!(
                &patron.name == pack_name,
                "{}: reviewed patron pack {:?} belongs in {:?}",
                path.display(),
                patron.name,
                pack_name
            );
            ensure!(
                patron.url.starts_with("https://ddowiki.com/page/"),
                "{}: reviewed patron pack {:?} has no wiki citation",
                path.display(),
                patron.name
            );
        }
        for quest in &exceptions.quest {
            let belongs_to_pack: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM quests q JOIN adventure_packs p ON p.id = q.pack_id WHERE q.name = ?1 AND p.name = ?2)",
                [&quest.name, pack_name],
                |row| row.get(0),
            )?;
            ensure!(
                belongs_to_pack,
                "{}: reviewed quest {:?} does not belong to pack {:?}",
                path.display(),
                quest.name,
                pack_name
            );
            ensure!(
                quest.url.starts_with("https://ddowiki.com/page/")
                    && !quest.read.is_empty()
                    && !quest.reason.is_empty(),
                "{}: reviewed quest {:?} needs a reason and wiki citation",
                path.display(),
                quest.name
            );
        }
    }
    Ok(())
}

pub(super) fn derive_item_set_names(db: &Connection, primary_set_by_item: &BTreeMap<String, String>) -> Result<()> {
    db.execute_batch(
        "UPDATE items SET set_bonus = (
           SELECT MIN(s.name) FROM set_bonus_items member JOIN set_bonuses s ON s.id = member.set_id
            WHERE member.item_id = items.id)",
    )?;
    for (item_name, set_name) in primary_set_by_item {
        let member_count: i64 = db.query_row(
            "SELECT COUNT(*) FROM items i JOIN set_bonus_items member ON member.item_id = i.id
             JOIN set_bonuses s ON s.id = member.set_id WHERE i.name = ?1 AND s.name = ?2",
            [item_name, set_name],
            |row| row.get(0),
        )?;
        ensure!(member_count == 1, "item {item_name:?} marks {set_name:?} primary without belonging to it");
        db.execute("UPDATE items SET set_bonus = ?1 WHERE name = ?2", [set_name, item_name])?;
    }
    Ok(())
}

pub(super) fn validate_entity_file_name(
    path: &Path,
    identity: &str,
    folder_owned: bool,
    item_name: Option<&str>,
) -> Result<()> {
    let actual = if folder_owned {
        path.parent().and_then(Path::file_name).and_then(|name| name.to_str())
    } else {
        path.file_stem().and_then(|name| name.to_str())
    };
    let family_tier_matches = item_name.is_some_and(|name| {
        let (family, tier) = item_family_tier(name);
        path.parent().and_then(Path::file_name).and_then(|name| name.to_str()) == Some(slug(family).as_str())
            && actual == Some(tier.as_str())
    });
    ensure!(actual == Some(identity) || family_tier_matches, "{}: slug must be {identity}", path.display());
    Ok(())
}

fn file_build_order(file: &SourceFile) -> u8 {
    match file {
        SourceFile::BonusTypes(_)
        | SourceFile::ItemMaterials(_)
        | SourceFile::Triggers(_)
        | SourceFile::WeaponProficiencies(_)
        | SourceFile::EquipmentSlots(_)
        | SourceFile::DamageTypes(_)
        | SourceFile::AugmentSlotTypes(_)
        | SourceFile::ExcludedItems(_)
        | SourceFile::Redirects(_)
        | SourceFile::SentientGem(_)
        | SourceFile::Pack(_)
        | SourceFile::Patron(_)
        | SourceFile::EffectTier(_)
        | SourceFile::SourceAliases(_)
        | SourceFile::Exceptions(_)
        | SourceFile::VendorCurrencies(_)
        | SourceFile::UpstreamCommit(_) => 0,
        SourceFile::WeaponTypes(_)
        | SourceFile::Stat(_)
        | SourceFile::StatGroup(_)
        | SourceFile::Effect(_)
        | SourceFile::Quest(_)
        | SourceFile::Race(_)
        | SourceFile::CraftingSystem(_) => 1,
        SourceFile::Class(file) => {
            if file.base_class.is_some() {
                3
            } else {
                2
            }
        }
        SourceFile::Item(_)
        | SourceFile::Augment(_)
        | SourceFile::Set(_)
        | SourceFile::SpellList(_)
        | SourceFile::Stances(_)
        | SourceFile::Clickies(_)
        | SourceFile::GuildBuffs(_)
        | SourceFile::OptionalBuffs(_)
        | SourceFile::QuestChain(_)
        | SourceFile::Saga(_)
        | SourceFile::Vendor(_)
        | SourceFile::Event(_) => 4,
        SourceFile::FeatCategory(_) | SourceFile::EnhancementTree(_) | SourceFile::FiligreeSet(_) => 5,
    }
}

fn write_data_file(
    root: &Path,
    file: &SourceFile,
    db: &Connection,
    path: &Path,
    ids: &mut ids::StableIds,
) -> Result<Option<i64>> {
    let folder_owned = schemas::pack_local_kind(root, path)?.is_none();
    match file {
        SourceFile::BonusTypes(file) => {
            file.write_bonus_types(db, path, ids)?;
        }
        SourceFile::ItemMaterials(file) => {
            file.write_vocabulary(db, path, ids)?;
        }
        SourceFile::Triggers(file) => {
            file.write_vocabulary(db, path, ids)?;
        }
        SourceFile::WeaponProficiencies(file) => {
            file.write_vocabulary(db, path, ids)?;
        }
        SourceFile::EquipmentSlots(file) => {
            file.write_equipment_slots(db, path, ids)?;
        }
        SourceFile::DamageTypes(file) => {
            file.write_damage_types(db, path, ids)?;
        }
        SourceFile::WeaponTypes(file) => {
            file.write_weapon_types(db, path, ids)?;
        }
        SourceFile::SentientGem(file) => {
            return Ok(Some(file.write_sentient_gem(db, path, ids)?));
        }
        SourceFile::AugmentSlotTypes(file) => {
            file.write_augment_slot_types(db, path, ids)?;
        }
        SourceFile::Stances(file) => {
            file.write_stances(db, path, ids)?;
        }
        SourceFile::Clickies(file) => {
            file.write_clickies(db, path, ids)?;
        }
        SourceFile::GuildBuffs(file) => {
            file.write_guild_buffs(db, path, ids)?;
        }
        SourceFile::OptionalBuffs(file) => {
            file.write_optional_buffs(db, path, ids)?;
        }
        SourceFile::Redirects(file) => file.write_redirects(db)?,
        SourceFile::ExcludedItems(file) => {
            file.write_excluded_items(db)?;
        }
        SourceFile::Item(file) => {
            let identity = qualified_slug(&file.name, None);
            validate_entity_file_name(path, &identity, false, Some(&file.name))?;
            let id = ids.issue("items", &identity, path)?;
            file.write_item(db, path, id, &identity)?;
            return Ok(Some(id));
        }
        SourceFile::Augment(file) => {
            let base = augment_name_slug(&file.name);
            let identity = file.variant.as_ref().map_or(base.clone(), |variant| format!("{base}-{}", slug(variant)));
            validate_entity_file_name(path, &identity, false, None)?;
            let id = ids.issue("augments", &identity, path)?;
            file.write_augment(db, path, id, &identity)?;
            return Ok(Some(id));
        }
        SourceFile::Quest(file) => {
            let identity = qualified_slug(&file.name, file.variant.as_deref());
            validate_entity_file_name(path, &identity, folder_owned && !file.is_challenge, None)?;
            let id = ids.issue("quests", &identity, path)?;
            file.write_quest(db, path, id, &identity)?;
            return Ok(Some(id));
        }
        SourceFile::Effect(file) => {
            let identity = qualified_slug(&file.name, file.variant.as_deref());
            validate_entity_file_name(path, &identity, false, None)?;
            let id = ids.issue("effects", &identity, path)?;
            file.write_effect(db, path, id, &identity)?;
            return Ok(Some(id));
        }
        SourceFile::Set(file) => {
            let identity = qualified_slug(&file.name, None);
            validate_entity_file_name(path, &identity, false, None)?;
            let id = ids.issue("set_bonuses", &identity, path)?;
            file.write_set(db, path, id, &identity)?;
            return Ok(Some(id));
        }
        SourceFile::EnhancementTree(file) => {
            let identity = qualified_slug(&file.name, None);
            validate_entity_file_name(path, &identity, folder_owned, None)?;
            let id = ids.issue("enhancement_trees", &identity, path)?;
            for enhancement in &file.enhancement {
                let identity = qualified_slug(&enhancement.internal_name, enhancement.variant.as_deref());
                ids.issue("enhancements", &identity, path)?;
                for selection in &enhancement.selection {
                    ids.issue("enhancement_selections", &format!("{identity}-{}", slug(&selection.name)), path)?;
                }
            }
            file.write_enhancement_tree(db, path, id, &identity)?;
            return Ok(Some(id));
        }
        SourceFile::Class(file) => {
            let identity = qualified_slug(&file.name, None);
            validate_entity_file_name(path, &identity, false, None)?;
            let id = ids.issue("classes", &identity, path)?;
            file.write_class(db, path, id, &identity)?;
            return Ok(Some(id));
        }
        SourceFile::Race(file) => {
            let identity = qualified_slug(&file.name, None);
            validate_entity_file_name(path, &identity, false, None)?;
            let id = ids.issue("races", &identity, path)?;
            file.write_race(db, path, id, &identity)?;
            return Ok(Some(id));
        }
        SourceFile::Pack(file) => {
            let identity = qualified_slug(&file.name, None);
            validate_entity_file_name(path, &pack_folder_name(&identity, file.update.as_deref())?, folder_owned, None)?;
            let id = ids.issue("adventure_packs", &identity, path)?;
            file.write_pack(db, path, id, &identity)?;
            return Ok(Some(id));
        }
        SourceFile::Patron(file) => {
            let identity = qualified_slug(&file.name, None);
            validate_entity_file_name(path, &identity, false, None)?;
            let id = ids.issue("patrons", &identity, path)?;
            file.write_patron(db, path, id, &identity)?;
            return Ok(Some(id));
        }
        SourceFile::EffectTier(file) => {
            let identity = qualified_slug(&file.name, None);
            validate_entity_file_name(path, &identity, false, None)?;
            let id = ids.issue("effect_tier_groups", &identity, path)?;
            file.write_effect_tier(db, path, id)?;
            return Ok(Some(id));
        }
        SourceFile::Event(file) => {
            let identity = qualified_slug(&file.name, None);
            validate_entity_file_name(path, &identity, folder_owned, None)?;
            let id = ids.issue("events", &identity, path)?;
            file.write_event(db, path, id, &identity)?;
            return Ok(Some(id));
        }
        SourceFile::Vendor(file) => {
            let identity = qualified_slug(&file.name, None);
            validate_entity_file_name(path, &identity, folder_owned, None)?;
            let id = ids.issue("vendors", &identity, path)?;
            file.write_vendor(db, path, id, &identity)?;
            return Ok(Some(id));
        }
        SourceFile::Stat(file) => {
            validate_entity_file_name(path, &slug(&file.name), false, None)?;
            return Ok(Some(file.write_stat(db, path, ids)?));
        }
        SourceFile::StatGroup(file) => {
            validate_entity_file_name(path, &slug(&file.name), false, None)?;
            return Ok(Some(file.write_stat_group(db, path, ids)?));
        }
        SourceFile::QuestChain(file) => {
            validate_entity_file_name(path, &slug(&file.name), false, None)?;
            return Ok(Some(file.write_quest_series(db, path, ids, "quest_chains")?));
        }
        SourceFile::Saga(file) => {
            validate_entity_file_name(path, &slug(&file.name), false, None)?;
            return Ok(Some(file.write_quest_series(db, path, ids, "sagas")?));
        }
        SourceFile::CraftingSystem(file) => {
            validate_entity_file_name(path, &slug(&file.name), folder_owned, None)?;
            return Ok(Some(file.write_crafting_system(db, path, ids)?));
        }
        SourceFile::FeatCategory(file) => {
            for row in &file.row {
                let identity = qualified_slug(&row.name, row.variant.as_deref());
                ids.issue("feats", &identity, path)?;
            }
            file.write_feat_category(db, path)?;
        }
        SourceFile::SpellList(file) => {
            for row in &file.row {
                let identity = qualified_slug(&row.name, None);
                ids.issue("spells", &identity, path)?;
            }
            file.write_spell_list(db, path)?;
        }
        SourceFile::FiligreeSet(file) => {
            for row in &file.row {
                let identity = qualified_slug(&row.name, None);
                ids.issue("filigrees", &identity, path)?;
            }
            file.write_filigree_set(db, path)?;
        }
        SourceFile::VendorCurrencies(file) => file.write_vendor_currencies(db, path)?,
        SourceFile::SourceAliases(_) | SourceFile::Exceptions(_) | SourceFile::UpstreamCommit(_) => {}
    }
    Ok(None)
}

pub fn rederive_data_build_tables(db: &Connection) -> Result<()> {
    crate::delta::clear_rederived_tables(db)?;
    crate::build::effect_vocabulary::insert_data_effect_vocabulary(db)?;
    insert_entity_identities(db)?;
    let violations = super::redirects::redirect_violations(db)?;
    ensure!(violations.is_empty(), "redirects.toml: {}", violations.join("\n"));
    Ok(())
}

pub fn build_from_data(root: &Path, db: &mut Connection) -> Result<DatasetFileReport> {
    ensure!(root.is_dir(), "data directory {} does not exist", root.display());
    let icon_index = crate::icons::IconIndex::from_data(root);
    let aliases_path = root.join("upstream/source-aliases.toml");
    let aliases_contents =
        std::fs::read_to_string(&aliases_path).with_context(|| format!("reading {}", aliases_path.display()))?;
    let source_aliases =
        SourceAliases::from_toml_str(&aliases_contents).with_context(|| format!("reading {}", aliases_path.display()));
    db.execute_batch(ddo_model::ddl())?;
    db.execute_batch("PRAGMA foreign_keys = OFF")?;
    let mut stable_ids = ids::StableIds::default();
    let mut report = DatasetFileReport::default();
    let mut referenced_icons = BTreeSet::new();
    let mut files = Vec::new();
    let mut validation_errors = Vec::new();
    if let Err(error) = &icon_index {
        validation_errors.push(format!("{error:#}"));
    }
    if let Err(error) = &source_aliases {
        validation_errors.push(format!("{error:#}"));
    }
    let mut catalog = schemas::Catalog::default();
    for entry in WalkDir::new(root).sort_by_file_name() {
        let entry = entry?;
        let path = entry.path();
        if !entry.file_type().is_file()
            || path.extension().is_none_or(|extension| extension != "toml")
            || path.components().any(|component| component.as_os_str() == ".schemas")
        {
            continue;
        }
        let contents = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let kind = match schemas::file_kind(root, path) {
            Ok(kind) => kind,
            Err(error) => {
                validation_errors.push(format!("{error:#}"));
                continue;
            }
        };
        catalog.add_contents(path.strip_prefix(root)?, kind, &contents)?;
        match schemas::read_file(root, path, &contents) {
            Ok(file) => {
                if let Ok(icon_index) = &icon_index {
                    if let Err(error) = validate_file_icons(root, path, &file, icon_index, &mut referenced_icons) {
                        validation_errors.push(format!("{error:#}"));
                    }
                }
                files.push((path.to_path_buf(), file));
            }
            Err(error) => validation_errors.push(format!("{}: {error:#}", path.display())),
        }
        bump_file_count(&mut report, path, root);
    }
    validation_errors.extend(catalog.reference_errors()?);
    ensure!(validation_errors.is_empty(), "data validation failed:\n{}", validation_errors.join("\n"));
    let icon_index = icon_index?;
    let source_aliases = source_aliases?;
    files.sort_by_key(|(_, file)| file_build_order(file));
    let quest_epic_names = files
        .iter()
        .filter_map(|(_, file)| {
            if let SourceFile::Quest(file) = file {
                file.epic_name.as_ref().map(|epic| (file.name.clone(), epic.clone()))
            } else {
                None
            }
        })
        .collect::<BTreeMap<_, _>>();
    let mut annotations = annotations::EntityAnnotations::default();
    let transaction = db.transaction()?;
    let mut inserted_files = Vec::new();
    let mut entity_locations = Vec::new();
    for (path, file) in &files {
        let id = match write_data_file(root, file, &transaction, path, &mut stable_ids) {
            Ok(id) => id,
            Err(error) => {
                validation_errors.push(format!("{}: {error:#}", path.display()));
                continue;
            }
        };
        if let Some((id, (kind, name))) = id.zip(file.layout_entity()) {
            entity_locations.push((path, kind, name, id));
        }
        if let SourceFile::FiligreeSet(file) = file {
            for row in &file.row {
                entity_locations.push((
                    path,
                    "filigrees",
                    row.name.as_str(),
                    ids::stable_hash(&["filigrees", &slug(&row.name)]),
                ));
            }
        }
        if let Err(error) = annotations.add_data_file(root, path, file) {
            validation_errors.push(format!("{}: annotations: {error:#}", path.display()));
        }
        inserted_files.push((path, file, id));
    }
    let mut notes_by_file = Vec::new();
    let mut removals_by_file = Vec::new();
    let mut tier_descriptions = BTreeMap::new();
    let mut primary_set_by_item = BTreeMap::new();
    for (path, file, id) in inserted_files {
        let mut correction_notes_to_insert = Vec::<(String, DdobuilderCorrection)>::new();
        let mut removed_entries_to_insert = Vec::<RemovedEntry>::new();
        let outcome = (|| -> Result<()> {
            match file {
                SourceFile::Item(file) => {
                    let id = id.context("Item file has no identity")?;
                    file.insert_weapon_stats(&transaction, path, id)?;
                    file.insert_effect_links(&transaction, path, id)?;
                    file.insert_clickies(&transaction, path, id)?;
                    file.insert_sockets(&transaction, path, id)?;
                    file.insert_modifiers(&transaction, path, id)?;
                    file.insert_socket_options(&transaction, id)?;
                    if let Some(quest) = folder_quest_name(root, path)? {
                        if !file.source.iter().any(|source| source.kind == "quest" && source.name == quest) {
                            report.missing_folder_quest_sources.push(path.clone());
                        }
                    }
                    file.insert_sources(&transaction, id, &quest_epic_names, &source_aliases)?;
                    correction_notes_to_insert
                        .extend(file.ddobuilder_correction.iter().cloned().map(|note| (file.name.clone(), note)));
                    removed_entries_to_insert.extend(file.removed.iter().cloned());
                }
                SourceFile::Augment(file) => {
                    let id = id.context("Augment file has no identity")?;
                    file.insert_effect_links(&transaction, path, id)?;
                    file.insert_slots(&transaction, path, id)?;
                    file.insert_modifiers(&transaction, path, id)?;
                    file.insert_sources(&transaction, id, &quest_epic_names, &source_aliases)?;
                    correction_notes_to_insert
                        .extend(file.ddobuilder_correction.iter().cloned().map(|note| (file.name.clone(), note)));
                }
                SourceFile::Effect(file) => {
                    let id = id.context("Effect file has no identity")?;
                    file.insert_bonuses(&transaction, path, id)?;
                    file.insert_damage(&transaction, path, id)?;
                    correction_notes_to_insert
                        .extend(file.ddobuilder_correction.iter().cloned().map(|note| (file.name.clone(), note)));
                }
                SourceFile::StatGroup(file) => {
                    let id = id.context("Group file has no identity")?;
                    file.insert_bonuses(&transaction, path, id)?;
                }
                SourceFile::Set(file) => {
                    let id = id.context("Set file has no identity")?;
                    file.insert_tiers(&transaction, path, id)?;
                    file.insert_members(&transaction, path, id)?;
                    correction_notes_to_insert
                        .extend(file.ddobuilder_correction.iter().cloned().map(|note| (file.name.clone(), note)));
                    for tier in &file.tier {
                        if let Some(description) = &tier.description {
                            tier_descriptions.insert((file.name.clone(), tier.equipped_count), description.clone());
                        }
                    }
                    for item in &file.item {
                        if let Some(primary) = item.primary {
                            ensure!(primary, "{}: set piece primary must be true", path.display());
                            ensure!(
                                primary_set_by_item.insert(item.name.clone(), file.name.clone()).is_none(),
                                "{}: item {:?} has more than one primary set",
                                path.display(),
                                item.name
                            );
                        }
                    }
                }
                SourceFile::Class(file) => {
                    let id = id.context("Class file has no identity")?;
                    file.insert_children(&transaction, path, id)?;
                }
                SourceFile::Race(file) => {
                    let id = id.context("Race file has no identity")?;
                    file.insert_children(&transaction, path, id)?;
                }
                SourceFile::QuestChain(file) => {
                    let id = id.context("QuestChain file has no identity")?;
                    file.insert_quests(&transaction, path, id, "quest_chain_quests", "chain_id")?;
                }
                SourceFile::Saga(file) => {
                    let id = id.context("Saga file has no identity")?;
                    file.insert_quests(&transaction, path, id, "saga_quests", "saga_id")?;
                }
                SourceFile::CraftingSystem(file) => {
                    let id = id.context("CraftingSystem file has no identity")?;
                    file.insert_recipes(&transaction, path, id)?;
                    removed_entries_to_insert.extend(file.removed.iter().cloned());
                }
                SourceFile::OptionalBuffs(file) => {
                    for row in &file.row {
                        correction_notes_to_insert
                            .extend(row.ddobuilder_correction.iter().cloned().map(|note| (row.name.clone(), note)));
                    }
                }
                SourceFile::Clickies(file) => {
                    for row in &file.row {
                        correction_notes_to_insert
                            .extend(row.ddobuilder_correction.iter().cloned().map(|note| (row.name.clone(), note)));
                    }
                }
                SourceFile::Quest(file) => correction_notes_to_insert
                    .extend(file.ddobuilder_correction.iter().cloned().map(|note| (file.name.clone(), note))),
                SourceFile::Pack(file) => correction_notes_to_insert
                    .extend(file.ddobuilder_correction.iter().cloned().map(|note| (file.name.clone(), note))),
                SourceFile::Patron(file) => correction_notes_to_insert
                    .extend(file.ddobuilder_correction.iter().cloned().map(|note| (file.name.clone(), note))),
                SourceFile::AugmentSlotTypes(file) => {
                    for row in &file.row {
                        correction_notes_to_insert
                            .extend(row.ddobuilder_correction.iter().cloned().map(|note| (row.label.clone(), note)));
                    }
                }
                SourceFile::EnhancementTree(file) => {
                    correction_notes_to_insert
                        .extend(file.ddobuilder_correction.iter().cloned().map(|note| (file.name.clone(), note)));
                    for row in &file.enhancement {
                        correction_notes_to_insert
                            .extend(row.ddobuilder_correction.iter().cloned().map(|note| (row.name.clone(), note)));
                        for dc in &row.dc {
                            correction_notes_to_insert.extend(correction_notes::nested_notes(
                                &row.name,
                                &dc.ddobuilder_correction,
                                &[("dc", &dc.name)],
                            )?);
                        }
                        for stance in &row.stance {
                            correction_notes_to_insert.extend(correction_notes::nested_notes(
                                &row.name,
                                &stance.ddobuilder_correction,
                                &[("stance", &stance.name)],
                            )?);
                        }
                        for selection in &row.selection {
                            correction_notes_to_insert.extend(correction_notes::nested_notes(
                                &row.name,
                                &selection.ddobuilder_correction,
                                &[("selection", &selection.name)],
                            )?);
                            for stance in &selection.stance {
                                correction_notes_to_insert.extend(correction_notes::nested_notes(
                                    &row.name,
                                    &stance.ddobuilder_correction,
                                    &[("selection", &selection.name), ("stance", &stance.name)],
                                )?);
                            }
                            for dc in &selection.dc {
                                correction_notes_to_insert.extend(correction_notes::nested_notes(
                                    &row.name,
                                    &dc.ddobuilder_correction,
                                    &[("selection", &selection.name), ("dc", &dc.name)],
                                )?);
                            }
                        }
                    }
                }
                SourceFile::FiligreeSet(file) => {
                    for row in &file.row {
                        correction_notes_to_insert
                            .extend(row.ddobuilder_correction.iter().cloned().map(|note| (row.name.clone(), note)));
                    }
                }
                SourceFile::FeatCategory(file) => {
                    file.insert_children(&transaction, path)?;
                    file.insert_abilities(&transaction)?;
                    for row in &file.row {
                        correction_notes_to_insert
                            .extend(row.ddobuilder_correction.iter().cloned().map(|note| (row.name.clone(), note)));
                        for stance in &row.stance {
                            correction_notes_to_insert.extend(correction_notes::nested_notes(
                                &row.name,
                                &stance.ddobuilder_correction,
                                &[("stance", &stance.name)],
                            )?);
                        }
                    }
                }
                SourceFile::SpellList(file) => {
                    file.insert_damage_and_dcs(&transaction)?;
                    file.insert_modifiers_and_stances(&transaction, path)?;
                    for row in &file.row {
                        correction_notes_to_insert
                            .extend(row.ddobuilder_correction.iter().cloned().map(|note| (row.name.clone(), note)));
                        for stance in &row.stance {
                            correction_notes_to_insert.extend(correction_notes::nested_notes(
                                &row.name,
                                &stance.ddobuilder_correction,
                                &[("stance", &stance.name)],
                            )?);
                        }
                    }
                }
                _ => {}
            }
            Ok(())
        })();
        if let Err(error) = outcome {
            validation_errors.push(format!("{}: {error:#}", path.display()));
        }
        notes_by_file.push((path, correction_notes_to_insert));
        removals_by_file.push((path, removed_entries_to_insert));
    }
    ensure!(validation_errors.is_empty(), "data validation failed:\n{}", validation_errors.join("\n"));
    let selection_renames = notes_by_file
        .iter()
        .flat_map(|(_, notes)| notes)
        .filter_map(|(owner, note)| {
            if note.field != "name" || !note.qualifier.starts_with("selection ") {
                return None;
            }
            note.upstream.as_ref().and_then(serde_json::Value::as_str).map(|old_name| {
                (
                    (note.kind, owner.clone(), note.qualifier.clone()),
                    correction_notes::named_row_qualifier(&[("selection", old_name)]),
                )
            })
        })
        .collect::<BTreeMap<_, _>>();
    let mut recorded_notes = BTreeMap::new();
    for (path, entity_name, note) in notes_by_file
        .into_iter()
        .flat_map(|(path, notes)| notes.into_iter().map(move |(entity_name, note)| (path, entity_name, note)))
    {
        (|| -> Result<()> {
            let correction_name = if let Some(original_name) = &note.original_name {
                original_name.clone()
            } else if note.field == "name" && note.qualifier.is_empty() {
                note.upstream
                    .as_ref()
                    .and_then(serde_json::Value::as_str)
                    .with_context(|| format!("correction for {entity_name:?} has no upstream name"))?
                    .to_string()
            } else {
                entity_name.clone()
            };
            let (from_value, to_value) =
                correction_notes::correction_values(&transaction, &entity_name, &note, &tier_descriptions)
                    .with_context(|| format!("{} {:?}.{}", note.kind, correction_name, note.field))?;
            let recorded_qualifier = correction_notes::renamed_qualifier(
                &note.qualifier,
                selection_renames
                    .iter()
                    .filter(|((kind, owner, _), _)| *kind == note.kind && owner == &entity_name)
                    .map(|((_, _, current), original)| (current.as_str(), original.as_str())),
            );
            let key = (note.kind, correction_name.clone(), recorded_qualifier.clone(), note.field.clone());
            let evidence =
                (from_value.clone(), to_value.clone(), note.reason.clone(), note.source.clone(), note.read.clone());
            if let Some(previous) = recorded_notes.insert(key, evidence.clone()) {
                ensure!(
                    previous == evidence,
                    "{} {:?}.{} has conflicting notes",
                    note.kind,
                    correction_name,
                    note.field
                );
                return Ok(());
            }
            annotations::AppliedCorrection {
                kind: note.kind.as_str().into(),
                name: correction_name,
                qualifier: recorded_qualifier,
                field: note.field,
                from_value,
                to_value,
                reason: note.reason,
                source: note.source,
                read: note.read,
            }
            .insert(&transaction)?;
            Ok(())
        })()
        .with_context(|| format!("{}: correction note", path.display()))?;
    }
    for (path, removed) in
        removals_by_file.into_iter().flat_map(|(path, entries)| entries.into_iter().map(move |entry| (path, entry)))
    {
        annotations::AppliedCorrection {
            kind: removed.kind,
            name: removed.name,
            qualifier: String::new(),
            field: "remove".into(),
            from_value: "0".into(),
            to_value: "1".into(),
            reason: removed.reason,
            source: removed.url.into(),
            read: removed.read,
        }
        .insert(&transaction)
        .with_context(|| format!("{}: removal note", path.display()))?;
    }
    transaction.execute("INSERT INTO schema_version (version) VALUES (?1)", [ddo_model::SCHEMA_VERSION])?;
    files
        .iter()
        .find_map(|(_, file)| match file {
            SourceFile::UpstreamCommit(upstream) => Some(upstream),
            _ => None,
        })
        .context("data directory has no upstream/commit.toml")?
        .write_dataset_version(&transaction, root)
        .with_context(|| format!("{}: dataset", root.join("upstream/commit.toml").display()))?;
    derive_item_set_names(&transaction, &primary_set_by_item)?;
    rederive_data_build_tables(&transaction)?;
    annotations.store(&transaction)?;
    let pack_exceptions = files
        .iter()
        .filter_map(
            |(path, file)| {
                if let SourceFile::Exceptions(file) = file {
                    Some((path.as_path(), file))
                } else {
                    None
                }
            },
        )
        .collect::<Vec<_>>();
    validate_pack_exceptions(&transaction, &pack_exceptions)?;
    crate::loot_sources::reject_missing_source_references(&transaction)?;
    let invalid_reference: Option<(String, i64, String, i64)> = transaction
        .query_row("PRAGMA foreign_key_check", [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
        .optional()?;
    if let Some((table, row_id, parent, key)) = invalid_reference {
        bail!("data files have broken foreign key in {table} row {row_id}, parent {parent}, key {key}");
    }
    validate_data_icons(&transaction, &icon_index)?;
    let layout = ExportLayout::from_database(&transaction)?;
    for (path, kind, name, id) in entity_locations {
        let current = path.strip_prefix(root)?;
        let expected = layout.path(kind, id)?;
        if current != expected {
            report.layout_warnings.push(format!(
                "{kind} {name:?}: move {} -> {}",
                current.display(),
                expected.display()
            ));
        }
    }
    report.layout_warnings.extend(layout.warnings);
    for (kind, count) in icon_index.unreferenced_global_counts(&referenced_icons) {
        eprintln!("WARN icons_unreferenced {kind}: {count}");
    }
    for table in table_names(&transaction)? {
        report.row_count +=
            transaction.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get::<_, i64>(0))? as usize;
    }
    transaction.commit()?;
    db.execute_batch("PRAGMA foreign_keys = ON")?;
    Ok(report)
}

#[cfg(test)]
mod file_name_tests {
    use super::*;

    #[test]
    fn pack_release_prefixes_preserve_identity_and_reject_mismatched_folders() {
        let root = Path::new("data");
        for (name, update, folder) in [
            ("Example", "Update 39", "u039-example"),
            ("Example", "Update 100", "u100-example"),
            ("Example", "Module 1.4", "m1-example"),
            ("Example", "Public Beta Test", "m0-example"),
            ("Free to Play", "Module 9", "free-to-play"),
        ] {
            let contents = format!("name = {name:?}\nupdate = {update:?}\nreleased = '2018-07-18'\n");
            let path = root.join("packs").join(folder).join("pack.toml");
            let file = schemas::read_file(root, &path, &contents).unwrap();
            let db = Connection::open_in_memory().unwrap();
            db.execute_batch(ddo_model::ddl()).unwrap();
            let id = write_data_file(root, &file, &db, &path, &mut ids::StableIds::default()).unwrap().unwrap();
            let stored: (String, String, String) = db
                .query_row("SELECT slug, [update], released FROM adventure_packs WHERE id = ?1", [id], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
                .unwrap();
            assert_eq!(stored, (slug(name), update.to_string(), "2018-07-18".to_string()));
            assert_eq!(id, ids::stable_hash(&["adventure_packs", &slug(name)]));
            let wrong = root.join("packs/wrong/pack.toml");
            let error = schemas::read_file(root, &wrong, &contents).unwrap_err();
            assert!(error.to_string().contains(folder), "{error:#}");
            let error = write_data_file(root, &file, &db, &wrong, &mut ids::StableIds::default()).unwrap_err();
            assert!(error.to_string().contains(folder), "{error:#}");
            assert!(error.to_string().contains("packs/wrong/pack.toml"), "{error:#}");
        }
    }

    #[test]
    fn pack_releases_reject_malformed_updates_and_dates_but_keep_unknown_dates_null() {
        let root = Path::new("data");
        let path = root.join("packs/u039-example/pack.toml");
        for update in ["", "Update ", "Update -1", "Update 39x", "Module ", "Module 1.", "Module x", "Other"] {
            let contents = format!("name = 'Example'\nupdate = {update:?}\n");
            assert!(schemas::read_file(root, &path, &contents).is_err(), "{update}");
        }
        for released in [Some("2024-02-30"), Some("2024-13-01"), Some("2024-1-01"), Some(""), Some("2024-02-29"), None]
        {
            let mut contents = "name = 'Example'\nupdate = 'Update 39'\n".to_string();
            if let Some(released) = released {
                contents.push_str(&format!("released = {released:?}\n"));
            }
            let file = schemas::read_file(root, &path, &contents).unwrap();
            let db = Connection::open_in_memory().unwrap();
            db.execute_batch(ddo_model::ddl()).unwrap();
            let outcome = write_data_file(root, &file, &db, &path, &mut ids::StableIds::default());
            assert_eq!(outcome.is_ok(), released.is_none() || released == Some("2024-02-29"), "{released:?}");
            if outcome.is_ok() {
                let stored: Option<String> =
                    db.query_row("SELECT released FROM adventure_packs", [], |row| row.get(0)).unwrap();
                assert_eq!(stored.as_deref(), released);
            }
        }
    }

    #[test]
    fn reserved_record_names_always_use_the_parent_identity() {
        for (name, contents, path) in [
            ("Pack", "name = 'Pack'\n", "packs/wrong/pack.toml"),
            ("Quest", "name = 'Quest'\npack = 'Pack'\n", "packs/pack/wrong/quest.toml"),
            ("Crafting", "name = 'Crafting'\ncitation = []\n", "packs/pack/crafting/wrong/crafting.toml"),
            ("Vendor", "name = 'Vendor'\nlocation = 'Here'\ncitation = []\n", "packs/pack/vendors/wrong/vendor.toml"),
            ("Event", "name = 'Event'\ncitation = []\n", "packs/pack/events/wrong/event.toml"),
        ] {
            let root = Path::new("data");
            let path = root.join(path);
            let db = Connection::open_in_memory().unwrap();
            db.execute_batch(ddo_model::ddl()).unwrap();
            let error = schemas::read_file(root, &path, contents)
                .and_then(|file| write_data_file(root, &file, &db, &path, &mut ids::StableIds::default()))
                .unwrap_err();
            assert!(error.to_string().contains(&format!("slug must be {}", slug(name))), "{error:#}");
        }
    }

    #[test]
    fn pseudo_pack_names_cannot_be_real_packs() {
        let root = Path::new("data");
        for name in ["Shared", "General", "Unsourced"] {
            let path = root.join("packs").join(slug(name)).join("pack.toml");
            let error = schemas::read_file(root, &path, &format!("name = {name:?}")).unwrap_err();
            assert!(error.to_string().contains("reserved pseudo-pack"), "{error:#}");
        }
    }

    #[test]
    fn pack_local_file_names_cannot_borrow_the_kind_folder_identity() {
        let root = Path::new("data");
        let path = root.join("packs/example/patrons/wrong.toml");
        let file = schemas::read_file(root, &path, "name = 'Patrons'\n").unwrap();
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        let error = write_data_file(root, &file, &db, &path, &mut ids::StableIds::default()).unwrap_err();
        assert!(error.to_string().contains("slug must be patrons"), "{error:#}");
    }
}
