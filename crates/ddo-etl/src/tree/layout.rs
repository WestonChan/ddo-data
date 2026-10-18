use super::*;

pub(crate) fn qualified_slug(name: &str, variant: Option<&str>) -> String {
    let mut identity = slug(name);
    if let Some(variant) = variant {
        identity.push('-');
        identity.push_str(&slug(variant));
    }
    identity
}

pub(super) fn augment_name_slug(name: &str) -> String {
    let mut result = String::new();
    let mut needs_separator = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            if needs_separator && !result.is_empty() {
                result.push('-');
            }
            result.push(character.to_ascii_lowercase());
            needs_separator = false;
        } else if character == ':' {
            if !result.is_empty() && !result.ends_with('-') {
                result.push('-');
            }
            result.push('-');
            needs_separator = false;
        } else if !matches!(character, '\'' | '’') && !result.ends_with('-') {
            needs_separator = true;
        }
    }
    result.trim_end_matches('-').to_string()
}

pub(super) fn augment_variant(description: &str) -> Option<&str> {
    let variant = description.lines().last()?.trim();
    let mut parts = variant.split(':');
    let element = parts.next()?;
    let opposition = parts.next()?;
    let plane = parts.next()?;
    (parts.next().is_none()
        && !element.is_empty()
        && !opposition.is_empty()
        && !plane.is_empty()
        && [element, opposition, plane]
            .iter()
            .all(|part| part.chars().all(|character| character.is_ascii_alphabetic())))
    .then_some(variant)
}

pub(super) fn item_level_family(name: &str) -> Option<(&str, i64)> {
    let (family_name, level_suffix) = name.rsplit_once(" (")?;
    let level_text = level_suffix.strip_suffix(')')?.to_ascii_lowercase();
    let level = level_text.strip_prefix("level ")?.parse::<i64>().ok()?;
    Some((family_name, level))
}

pub(super) fn item_family_tier(name: &str) -> (&str, String) {
    if let Some((family_name, level)) = item_level_family(name) {
        return (item_family_tier(family_name).0, format!("level-{level}"));
    }
    if let Some(family_name) = name.strip_prefix("Epic ") {
        return (family_name, "epic".to_string());
    }
    if let Some(family_name) = name.strip_prefix("Legendary ") {
        return (family_name, "legendary".to_string());
    }
    (name, "heroic".to_string())
}

pub(super) fn pack_folder_name(identity: &str, update: Option<&str>) -> Result<String> {
    if identity == "free-to-play" || update.is_none() {
        return Ok(identity.to_string());
    }
    let update = update.unwrap();
    let prefix = if update == "Public Beta Test" {
        "m0".to_string()
    } else if let Some(number) = update.strip_prefix("Update ") {
        ensure!(
            !number.is_empty() && number.bytes().all(|digit| digit.is_ascii_digit()),
            "invalid pack update {update:?}"
        );
        format!("u{:03}", number.parse::<u32>()?)
    } else if let Some(version) = update.strip_prefix("Module ") {
        let parts = version.split('.').collect::<Vec<_>>();
        ensure!(
            parts.iter().all(|part| !part.is_empty() && part.bytes().all(|digit| digit.is_ascii_digit())),
            "invalid pack update {update:?}"
        );
        format!("m{}", parts[0].parse::<u32>()?)
    } else {
        anyhow::bail!("invalid pack update {update:?}");
    };
    Ok(format!("{prefix}-{identity}"))
}

pub(super) fn pack_folders(db: &Connection) -> Result<BTreeMap<String, PathBuf>> {
    let mut folders = BTreeMap::new();
    let mut select = db.prepare("SELECT name, slug, [update] FROM adventure_packs")?;
    for row in select.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<String>>(2)?))
    })? {
        let (name, identity, update) = row?;
        folders.insert(name, PathBuf::from("packs").join(pack_folder_name(&identity, update.as_deref())?));
    }
    Ok(folders)
}

pub(crate) struct ExportLayout {
    paths: BTreeMap<(String, i64), PathBuf>,
    pub(super) warnings: Vec<String>,
}

struct ItemSourceFolder {
    kind: String,
    quest_id: Option<i64>,
    pack: Option<String>,
    folder: Option<PathBuf>,
}

enum FolderPlacement<'a> {
    Pack(&'a str),
    Quest {
        pack: Option<&'a str>,
        patron: Option<&'a str>,
        identity: &'a str,
    },
    Source {
        kind: &'a str,
        identity: &'a str,
        pack: Option<&'a str>,
        patron: Option<&'a str>,
    },
    Item {
        sources: &'a [ItemSourceFolder],
        drop_text: Option<&'a str>,
        family_pack: Option<&'a str>,
        patron: Option<&'a str>,
    },
    Augment {
        systems: Option<&'a BTreeSet<i64>>,
        family: &'a str,
    },
    Filigrees(&'a [(i64, String)]),
    Character {
        kind: &'a str,
        identity: &'a str,
    },
    Effect(&'a str),
}

fn narrowest_folder(
    placement: FolderPlacement<'_>,
    folders: &BTreeMap<String, PathBuf>,
    paths: &BTreeMap<(String, i64), PathBuf>,
    aliases: &kinds::SourceAliasesFile,
    packs: &[String],
) -> Result<PathBuf> {
    match placement {
        FolderPlacement::Pack(name) => Ok(folders[name].clone()),
        FolderPlacement::Quest { pack, patron, identity } => {
            let mut parent = pack.map_or_else(|| PathBuf::from("packs/shared"), |pack| folders[pack].clone());
            if pack == Some("Free to Play") {
                if let Some(patron) = patron {
                    parent.push(slug(patron));
                }
            }
            Ok(parent.join("quests").join(identity))
        }
        FolderPlacement::Source { kind, identity, pack, patron } => {
            let mut parent = pack.map_or_else(|| PathBuf::from("packs/shared"), |pack| folders[pack].clone());
            if pack == Some("Free to Play") {
                if let Some(patron) = patron {
                    parent.push(slug(patron));
                }
            }
            let (category, owns_folder) = match kind {
                "crafting_systems" => ("crafting", true),
                "vendors" => ("vendors", true),
                "events" => ("events", true),
                "quest_chains" => ("chains", false),
                "sagas" => ("sagas", false),
                "set_bonuses" => ("sets", false),
                "patrons" => ("patrons", false),
                "challenges" => ("challenges", false),
                "sentient_gems" => ("sentient-gems", false),
                "items" => ("gear", false),
                "augments" => ("augments", false),
                _ => anyhow::bail!("{kind} has no source folder category"),
            };
            let folder = parent.join(category);
            Ok(if owns_folder { folder.join(identity) } else { folder })
        }
        FolderPlacement::Item { sources, drop_text, family_pack, patron } => {
            let pack_names = sources.iter().filter_map(|source| source.pack.as_ref()).collect::<BTreeSet<_>>();
            let quest_ids = sources.iter().filter_map(|source| source.quest_id).collect::<BTreeSet<_>>();
            let first = sources.first();
            let same_kind_folder = first
                .filter(|first| sources.iter().all(|source| source.kind == first.kind && source.folder == first.folder))
                .and_then(|source| source.folder.as_ref());
            let mut folder = if quest_ids.len() == 1 {
                paths[&("quests".to_string(), *quest_ids.first().unwrap())].parent().unwrap().join("gear")
            } else if pack_names.len() > 1 {
                PathBuf::from("packs/shared/gear")
            } else if let Some(folder) = same_kind_folder {
                folder.clone()
            } else if let Some(pack) = pack_names.first() {
                folders[pack.as_str()].join("gear")
            } else if !sources.is_empty() {
                PathBuf::from("packs/shared/gear")
            } else {
                let normalized = drop_text.unwrap_or_default().to_ascii_lowercase();
                if normalized.starts_with("ddo store") || normalized.starts_with("random") {
                    PathBuf::from("packs/general/gear")
                } else {
                    let alias = aliases
                        .adventure_packs
                        .iter()
                        .find(|alias| normalized.contains(&alias.text.to_ascii_lowercase()));
                    if let Some(alias) = alias {
                        ensure!(
                            packs.contains(&alias.pack),
                            "adventure pack alias {:?} names absent pack {:?}",
                            alias.text,
                            alias.pack
                        );
                    }
                    let pack = alias
                        .map(|alias| &alias.pack)
                        .or_else(|| packs.iter().find(|pack| normalized.contains(&pack.to_ascii_lowercase())));
                    pack.map_or_else(
                        || PathBuf::from("packs/unsourced/gear"),
                        |pack| folders[pack.as_str()].join("gear"),
                    )
                }
            };
            if folder == Path::new("packs/unsourced/gear") {
                if let Some(pack) = family_pack {
                    folder = folders[pack].join("gear");
                }
            }
            if folder == Path::new("packs/free-to-play/gear") {
                if let Some(patron) = patron {
                    folder = folders["Free to Play"].join(slug(patron)).join("gear");
                }
            }
            Ok(folder)
        }
        FolderPlacement::Augment { systems, family } => {
            Ok(if let Some(system) = systems.filter(|systems| systems.len() == 1).and_then(BTreeSet::first) {
                paths[&("crafting_systems".to_string(), *system)].parent().unwrap().join("augments")
            } else {
                PathBuf::from("packs/shared/augments").join(slug(family))
            })
        }
        FolderPlacement::Filigrees(members) => {
            let member_packs = members
                .iter()
                .map(|(_, menu)| {
                    let pack = aliases
                        .adventure_packs
                        .iter()
                        .find(|alias| alias.text == *menu)
                        .map_or(menu.as_str(), |alias| alias.pack.as_str());
                    folders.get(pack)
                })
                .collect::<Vec<_>>();
            let parent = member_packs
                .first()
                .copied()
                .flatten()
                .filter(|pack| member_packs.iter().all(|member| *member == Some(*pack)));
            Ok(parent.cloned().unwrap_or_else(|| PathBuf::from("packs/shared")).join("filigrees"))
        }
        FolderPlacement::Character { kind, identity } => Ok(if kind == "enhancement_trees" {
            PathBuf::from("characters/enhancements").join(identity)
        } else {
            PathBuf::from("characters").join(kind)
        }),
        FolderPlacement::Effect(kind) => Ok(PathBuf::from("effects").join(kind)),
    }
}

pub(super) fn expected_reference_path(table: &str, name: &str, referring: &Path) -> Result<PathBuf> {
    let identity = slug(name);
    let segments = referring.iter().collect::<Vec<_>>();
    let mut pack = PathBuf::from("packs/shared");
    if segments.first().is_some_and(|segment| *segment == "packs") && segments.len() > 1 {
        pack = segments[..2].iter().collect();
        if segments[1] == "free-to-play"
            && segments.len() > 3
            && !matches!(
                segments[2].to_str(),
                Some(
                    "quests"
                        | "crafting"
                        | "vendors"
                        | "events"
                        | "chains"
                        | "sagas"
                        | "sets"
                        | "patrons"
                        | "filigrees"
                        | "challenges"
                        | "gear"
                        | "augments"
                )
            )
        {
            pack.push(segments[2]);
        }
    }
    let folders = BTreeMap::from([("referencing pack".to_string(), pack)]);
    let source = match table {
        "quests" => Some((
            FolderPlacement::Quest { pack: Some("referencing pack"), patron: None, identity: &identity },
            "quest.toml".to_string(),
        )),
        "crafting_systems" | "vendors" | "events" | "quest_chains" | "sagas" | "set_bonuses" | "patrons"
        | "sentient_gems" | "items" | "augments" => {
            let record = match table {
                "crafting_systems" => "crafting",
                "vendors" => "vendor",
                "events" => "event",
                _ => &identity,
            };
            Some((
                FolderPlacement::Source {
                    kind: table,
                    identity: &identity,
                    pack: Some("referencing pack"),
                    patron: None,
                },
                format!("{record}.toml"),
            ))
        }
        "effects" => Some((FolderPlacement::Effect(""), format!("{identity}.toml"))),
        "effect_tier_groups" => Some((FolderPlacement::Effect("tiers"), format!("{identity}.toml"))),
        "classes" | "races" => {
            Some((FolderPlacement::Character { kind: table, identity: &identity }, format!("{identity}.toml")))
        }
        "feats" => Some((FolderPlacement::Character { kind: table, identity: "" }, "<category>.toml".into())),
        "spells" => Some((FolderPlacement::Character { kind: table, identity: "" }, "<class>.toml".into())),
        "crafting_ingredients" if referring.file_name().is_some_and(|name| name == "crafting.toml") => {
            return Ok(referring.to_path_buf());
        }
        "crafting_ingredients" => Some((
            FolderPlacement::Source {
                kind: "crafting_systems",
                identity: "<system>",
                pack: Some("referencing pack"),
                patron: None,
            },
            "crafting.toml".into(),
        )),
        "vendor_currencies" => return Ok(PathBuf::from("packs/shared/vendors/currencies.toml")),
        "enhancement_trees" => {
            Some((FolderPlacement::Character { kind: table, identity: &identity }, "tree.toml".into()))
        }
        "adventure_packs" => return Ok(PathBuf::from("packs").join(format!("<update>-{identity}")).join("pack.toml")),
        "bonus_types"
        | "augment_slot_types"
        | "equipment_slots"
        | "weapon_types"
        | "damage_types"
        | "item_materials"
        | "triggers"
        | "weapon_proficiencies"
        | "clickies" => Some((FolderPlacement::Effect(""), format!("{}.toml", table.replace('_', "-")))),
        _ => None,
    };
    if let Some((placement, filename)) = source {
        Ok(narrowest_folder(placement, &folders, &BTreeMap::new(), &kinds::SourceAliasesFile::default(), &[])?
            .join(filename))
    } else {
        anyhow::bail!("{table} has no expected reference path")
    }
}

fn quest_paths(
    db: &Connection,
    assignments: &super::pack_layout::PackAssignments,
    folders: &BTreeMap<String, PathBuf>,
    aliases: &kinds::SourceAliasesFile,
    packs: &[String],
) -> Result<BTreeMap<(String, i64), PathBuf>> {
    let mut paths = BTreeMap::new();
    let mut select = db.prepare("SELECT id, slug, is_challenge FROM quests")?;
    for row in
        select.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, bool>(2)?)))?
    {
        let (id, identity, is_challenge) = row?;
        let pack = assignments.pack("quests", id);
        let patron = assignments.patron("quests", id);
        let placement = if is_challenge {
            FolderPlacement::Source { kind: "challenges", identity: &identity, pack, patron }
        } else {
            FolderPlacement::Quest { pack, patron, identity: &identity }
        };
        let folder = narrowest_folder(placement, folders, &paths, aliases, packs)?;
        paths.insert(
            ("quests".to_string(), id),
            folder.join(if is_challenge { format!("{identity}.toml") } else { "quest.toml".to_string() }),
        );
    }
    Ok(paths)
}

impl ExportLayout {
    pub(super) fn from_database(db: &Connection) -> Result<Self> {
        let assignments = super::pack_layout::PackAssignments::from_database(db)?;
        let folders = pack_folders(db)?;
        let aliases = kinds::SourceAliasesFile::from_rows(db)?;
        let mut select = db.prepare("SELECT name FROM adventure_packs ORDER BY LENGTH(name) DESC, name")?;
        let packs = select.query_map([], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut paths = quest_paths(db, &assignments, &folders, &aliases, &packs)?;
        let mut warnings = Vec::new();
        let mut select =
            db.prepare(&format!("SELECT {} FROM entity_identity", annotations::EntityIdentity::select_columns()))?;
        for row in select.query_map([], annotations::EntityIdentity::from_row)? {
            let annotations::EntityIdentity { kind, entity_id: id, slug: identity, name, .. } = row?;
            if let Some((authored_pack, derived_pack)) = assignments.disagreements.get(&(kind.clone(), id)) {
                warnings.push(format!(
                    "{kind} {name:?}: authored pack {authored_pack:?} disagrees with members' pack {derived_pack:?}"
                ));
            }
            let source_record = match kind.as_str() {
                "crafting_systems" => Some(Some("crafting.toml")),
                "vendors" => Some(Some("vendor.toml")),
                "events" => Some(Some("event.toml")),
                "quest_chains" | "sagas" | "set_bonuses" | "patrons" | "sentient_gems" => Some(None),
                _ => None,
            };
            let placement = if source_record.is_some() {
                Some(FolderPlacement::Source {
                    kind: &kind,
                    identity: &identity,
                    pack: assignments.pack(&kind, id),
                    patron: assignments.patron(&kind, id),
                })
            } else {
                match kind.as_str() {
                    "adventure_packs" => Some(FolderPlacement::Pack(&name)),
                    "classes" | "races" | "enhancement_trees" => {
                        Some(FolderPlacement::Character { kind: &kind, identity: &identity })
                    }
                    _ => None,
                }
            };
            let parent = placement
                .map(|placement| {
                    let folder = narrowest_folder(placement, &folders, &paths, &aliases, &packs)?;
                    let file_name = match kind.as_str() {
                        "adventure_packs" => "pack.toml".to_string(),
                        "enhancement_trees" => "tree.toml".to_string(),
                        _ => source_record.flatten().map(str::to_string).unwrap_or_else(|| format!("{identity}.toml")),
                    };
                    Ok::<_, anyhow::Error>(folder.join(file_name))
                })
                .transpose()?;
            if let Some(path) = parent {
                paths.insert((kind.clone(), id), path);
            }
        }
        let mut item_sources = BTreeMap::<i64, Vec<ItemSourceFolder>>::new();
        let mut select = db.prepare(
            "SELECT source.item_id, source.kind, source.quest_id, pack.name,
            COALESCE(source.crafting_system_id, source.vendor_id, source.event_id, source.chain_id, source.saga_id)
            FROM sources source LEFT JOIN quests quest ON quest.id = source.quest_id
            LEFT JOIN quest_chains chain ON chain.id = source.chain_id LEFT JOIN sagas saga ON saga.id = source.saga_id
            LEFT JOIN crafting_systems system ON system.id = source.crafting_system_id
            LEFT JOIN vendors vendor ON vendor.id = source.vendor_id
            LEFT JOIN adventure_packs pack ON pack.id = COALESCE(quest.pack_id, chain.pack_id, saga.pack_id,
                source.pack_id, system.pack_id, vendor.pack_id)
            WHERE source.item_id IS NOT NULL ORDER BY source.id",
        )?;
        for row in select.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<i64>>(4)?,
            ))
        })? {
            let (id, kind, quest_id, pack, source_id) = row?;
            let owner = match kind.as_str() {
                "crafting_system" => Some("crafting_systems"),
                "vendor" => Some("vendors"),
                "event" => Some("events"),
                _ => None,
            };
            let folder = if let Some((table, source_id)) = owner.zip(source_id) {
                Some(paths[&(table.to_string(), source_id)].parent().unwrap().join("gear"))
            } else if kind == "starter" {
                Some(PathBuf::from("packs/general/gear"))
            } else {
                None
            };
            item_sources.entry(id).or_default().push(ItemSourceFolder { kind, quest_id, pack, folder });
        }
        let mut select = db.prepare("SELECT id, slug, drop_location FROM items")?;
        for row in select.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<String>>(2)?))
        })? {
            let (id, identity, drop_text) = row?;
            let sources = item_sources.remove(&id).unwrap_or_default();
            let folder = narrowest_folder(
                FolderPlacement::Item {
                    sources: &sources,
                    drop_text: drop_text.as_deref(),
                    family_pack: assignments.family_packs.get(&id).map(String::as_str),
                    patron: assignments.patron("items", id),
                },
                &folders,
                &paths,
                &aliases,
                &packs,
            )?;
            paths.insert(("items".to_string(), id), folder.join(format!("{identity}.toml")));
        }
        let mut systems_by_family = BTreeMap::<String, BTreeSet<i64>>::new();
        let mut select = db.prepare("SELECT family.family, system.id FROM crafting_system_families family JOIN crafting_systems system ON system.id = family.system_id")?;
        for row in select.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))? {
            let (family, system) = row?;
            systems_by_family.entry(family).or_default().insert(system);
        }
        let mut select = db.prepare("SELECT id, slug, family FROM augments")?;
        for row in select
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)))?
        {
            let (id, identity, family) = row?;
            let folder = narrowest_folder(
                FolderPlacement::Augment { systems: systems_by_family.get(&family), family: &family },
                &folders,
                &paths,
                &aliases,
                &packs,
            )?;
            paths.insert(("augments".to_string(), id), folder.join(format!("{identity}.toml")));
        }
        let mut select = db.prepare("SELECT id, slug, is_stat, is_group FROM effects")?;
        for row in select.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, bool>(2)?, row.get::<_, bool>(3)?))
        })? {
            let (id, identity, is_stat, is_group) = row?;
            let folder = if is_stat {
                "stats"
            } else if is_group {
                "stat-groups"
            } else {
                ""
            };
            paths.insert(
                ("effects".to_string(), id),
                narrowest_folder(FolderPlacement::Effect(folder), &folders, &paths, &aliases, &packs)?
                    .join(format!("{identity}.toml")),
            );
        }
        let mut select = db.prepare("SELECT id, name FROM effect_tier_groups")?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))? {
            let (id, name) = row?;
            paths.insert(
                ("effect_tier_groups".to_string(), id),
                narrowest_folder(FolderPlacement::Effect("tiers"), &folders, &paths, &aliases, &packs)?
                    .join(format!("{}.toml", slug(&name))),
            );
        }
        let mut select = db.prepare("SELECT feat.id, COALESCE((SELECT group_name FROM feat_groups WHERE feat_id = feat.id ORDER BY rowid LIMIT 1), 'other') FROM feats feat")?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))? {
            let (id, category) = row?;
            paths.insert(
                ("feats".to_string(), id),
                narrowest_folder(
                    FolderPlacement::Character { kind: "feats", identity: "" },
                    &folders,
                    &paths,
                    &aliases,
                    &packs,
                )?
                .join(format!("{}.toml", slug(&category))),
            );
        }
        let mut select = db.prepare("SELECT spell.id, COALESCE((SELECT class.name FROM class_spells member JOIN classes class ON class.id = member.class_id WHERE member.spell_id = spell.id ORDER BY class.name LIMIT 1), 'unassigned') FROM spells spell")?;
        for row in select.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))? {
            let (id, class) = row?;
            paths.insert(
                ("spells".to_string(), id),
                narrowest_folder(
                    FolderPlacement::Character { kind: "spells", identity: "" },
                    &folders,
                    &paths,
                    &aliases,
                    &packs,
                )?
                .join(format!("{}.toml", slug(&class))),
            );
        }
        let mut filigrees_by_set = BTreeMap::<String, Vec<(i64, String)>>::new();
        let mut select = db.prepare("SELECT filigree.id, COALESCE(set_bonus.name, 'Unassigned'), filigree.menu FROM filigrees filigree LEFT JOIN set_bonuses set_bonus ON set_bonus.id = filigree.set_id")?;
        for row in select
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)))?
        {
            let (id, set, menu) = row?;
            filigrees_by_set.entry(set).or_default().push((id, menu));
        }
        for (set, members) in filigrees_by_set {
            let folder = narrowest_folder(FolderPlacement::Filigrees(&members), &folders, &paths, &aliases, &packs)?;
            for (id, _) in members {
                paths.insert(("filigrees".to_string(), id), folder.join(format!("{}.toml", slug(&set))));
            }
        }
        Ok(Self { paths, warnings })
    }

    pub(super) fn path(&self, table: &str, id: i64) -> Result<&Path> {
        self.paths
            .get(&(table.to_string(), id))
            .map(PathBuf::as_path)
            .with_context(|| format!("{table} {id} has no export path"))
    }
}

pub(super) fn reviewed_pack_names_for_item(db: &Connection, item_name: &str) -> Result<Vec<String>> {
    let mut statement = db.prepare(
        "SELECT DISTINCT p.name FROM items i
         JOIN sources s ON s.item_id = i.id
         LEFT JOIN quests q ON q.id = s.quest_id
         LEFT JOIN quest_chains c ON c.id = s.chain_id
         LEFT JOIN sagas g ON g.id = s.saga_id
         JOIN adventure_packs p ON p.id = COALESCE(s.pack_id, q.pack_id, c.pack_id, g.pack_id)
         WHERE i.name = ?1 ORDER BY p.name",
    )?;
    let pack_names =
        statement.query_map([item_name], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<_>>()?;
    Ok(pack_names)
}

pub fn pack_exceptions(root: &Path) -> Result<kinds::ExceptionsFile> {
    let mut exceptions = kinds::ExceptionsFile { item: Vec::new(), patron: Vec::new(), quest: Vec::new() };
    for entry in std::fs::read_dir(root.join("packs"))? {
        let path = entry?.path().join("exceptions.toml");
        if !path.is_file() {
            continue;
        }
        let file: kinds::ExceptionsFile =
            toml::from_str(&std::fs::read_to_string(&path)?).with_context(|| format!("reading {}", path.display()))?;
        exceptions.item.extend(file.item);
        exceptions.patron.extend(file.patron);
        exceptions.quest.extend(file.quest);
    }
    Ok(exceptions)
}

pub(super) fn write_pack_exceptions(db: &Connection, root: &Path, report: &mut DatasetFileReport) -> Result<()> {
    let folders = pack_folders(db)?;
    for (pack, file) in kinds::ExceptionsFile::all_from_rows(db)? {
        let path = root.join(&folders[&pack]).join("exceptions.toml");
        write_data_file(&path, &toml::to_string_pretty(&file)?)?;
        bump_file_count(report, &path, root);
    }
    Ok(())
}

pub(super) fn bump_file_count(report: &mut DatasetFileReport, path: &Path, root: &Path) {
    report.written_files.insert(path.to_path_buf());
    let folder = path.strip_prefix(root).unwrap().components().next().unwrap().as_os_str().to_string_lossy();
    *report.file_count_by_folder.entry(folder.into_owned()).or_default() += 1;
}

pub(super) fn remove_unwritten_data_files(root: &Path, report: &DatasetFileReport) -> Result<()> {
    for entry in WalkDir::new(root) {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type().is_file()
            && path.extension().is_some_and(|extension| extension == "toml")
            && !path.components().any(|component| component.as_os_str() == ".schemas")
            && !report.written_files.contains(path)
        {
            std::fs::remove_file(path).with_context(|| format!("removing stale data file {}", path.display()))?;
        }
    }
    for entry in WalkDir::new(root).contents_first(true) {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type().is_dir() && path != root && std::fs::read_dir(path)?.next().is_none() {
            std::fs::remove_dir(path).with_context(|| format!("removing empty data folder {}", path.display()))?;
        }
    }
    Ok(())
}

pub(super) fn folder_quest_name(root: &Path, item_path: &Path) -> Result<Option<String>> {
    for folder in item_path.ancestors().skip(1) {
        if folder == root {
            break;
        }
        let quest_path = folder.join("quest.toml");
        if quest_path.is_file() {
            let quest: toml::Value = toml::from_str(&std::fs::read_to_string(&quest_path)?)
                .with_context(|| format!("reading {}", quest_path.display()))?;
            let name = quest
                .get("name")
                .and_then(toml::Value::as_str)
                .with_context(|| format!("{}: quest has no name", quest_path.display()))?;
            return Ok(Some(name.to_string()));
        }
    }
    Ok(None)
}

pub(super) const FIXED_DATA_FILES: &[(&str, &str)] = &[
    ("augment_slot_types", "effects/augment-slot-types.toml"),
    ("bonus_types", "effects/bonus-types.toml"),
    ("clickies", "effects/clickies.toml"),
    ("damage_types", "effects/damage-types.toml"),
    ("dataset_version", "upstream/commit.toml"),
    ("equipment_slots", "effects/equipment-slots.toml"),
    ("excluded_items", "upstream/excluded-items.toml"),
    ("guild_buffs", "characters/guild-buffs.toml"),
    ("item_materials", "effects/item-materials.toml"),
    ("optional_buffs", "characters/optional-buffs.toml"),
    ("redirects", "redirects.toml"),
    ("retired_slugs", "redirects.toml"),
    ("source_aliases", "upstream/source-aliases.toml"),
    ("stances", "characters/stances.toml"),
    ("triggers", "effects/triggers.toml"),
    ("vendor_currencies", "packs/shared/vendors/currencies.toml"),
    ("weapon_proficiencies", "effects/weapon-proficiencies.toml"),
    ("weapon_types", "effects/weapon-types.toml"),
];

pub(super) fn fixed_data_file(table: &str) -> Result<&'static Path> {
    FIXED_DATA_FILES
        .iter()
        .find(|(name, _)| *name == table)
        .map(|(_, path)| Path::new(path))
        .with_context(|| format!("{table} has no fixed data file"))
}

#[derive(Debug)]
pub struct DataFilePaths {
    paths: BTreeMap<(String, i64), PathBuf>,
    dataset_files: BTreeMap<String, BTreeSet<PathBuf>>,
    pack_exception_rows: BTreeMap<String, i64>,
}

impl DataFilePaths {
    pub fn from_database(db: &Connection) -> Result<Self> {
        use crate::database_tables::{table_columns, table_rows};
        use crate::delta::{table_merge, IdentityIndex, TableMerge};
        use rusqlite::types::Value;
        let layout = ExportLayout::from_database(db)?;
        let mut paths = layout.paths;
        let index = IdentityIndex::from_db(db)?;
        let mut rows = BTreeMap::new();
        for table in table_names(db)? {
            let columns = table_columns(db, &table)?;
            for row in table_rows(db, &table)? {
                let fields: crate::delta::RowFields = columns.iter().cloned().zip(row).collect();
                if let Some(Value::Integer(id)) = fields.get("id") {
                    rows.insert((table.clone(), *id), fields);
                }
            }
        }
        loop {
            let before = paths.len();
            for ((table, id), fields) in &rows {
                if paths.contains_key(&(table.clone(), *id)) {
                    continue;
                }
                let owner_column = match table_merge(table) {
                    Some(TableMerge::Entity { owner_column, .. }) => *owner_column,
                    Some(TableMerge::Array { owner_column, .. }) => Some(*owner_column),
                    _ => None,
                };
                if let Some(column) = owner_column {
                    if let (Some(target), Some(Value::Integer(owner))) =
                        (index.reference_target(table, column, fields), fields.get(column))
                    {
                        if let Some(path) = paths.get(&(target, *owner)).cloned() {
                            paths.insert((table.clone(), *id), path);
                        }
                        continue;
                    }
                }
                if let Ok(path) = fixed_data_file(table) {
                    paths.insert((table.clone(), *id), path.to_path_buf());
                }
            }
            if before == paths.len() {
                break;
            }
        }
        let correction_ids: BTreeMap<_, _> = rows
            .iter()
            .filter(|((table, _), _)| table == "corrections")
            .map(|((_, id), fields)| {
                let key = ["kind", "name", "qualifier", "field"]
                    .map(|column| fields.get(column).and_then(crate::delta::sql_text).unwrap_or("").to_string());
                (key, *id)
            })
            .collect();
        let notes = super::export::CorrectionNoteIndex::from_database(db)?;
        for ((table, id), fields) in &rows {
            let name_column = if table == "augment_slot_types" { "label" } else { "name" };
            let Some(name) = fields.get(name_column).and_then(crate::delta::sql_text) else {
                continue;
            };
            let Some(path) = paths.get(&(table.clone(), *id)).cloned() else {
                continue;
            };
            for correction in notes.corrections_for_owner(table, name) {
                let key = [
                    correction.ddobuilder_correction.kind.as_str().to_string(),
                    correction.name.clone(),
                    correction.ddobuilder_correction.qualifier.clone(),
                    correction.ddobuilder_correction.field.clone(),
                ];
                if let Some(id) = correction_ids.get(&key) {
                    paths.insert(("corrections".into(), *id), path.clone());
                }
            }
        }
        let mut removed = db.prepare(&format!("SELECT {} FROM removed_entities", RemovedEntry::select_columns()))?;
        for row in removed.query_map([], |row| {
            Ok((row.get::<_, String>("owner_kind")?, row.get::<_, i64>("owner_id")?, RemovedEntry::from_row(row)?))
        })? {
            let (table, owner, removal) = row?;
            let key = [removal.kind, removal.name, String::new(), "remove".to_string()];
            if let (Some(id), Some(path)) = (correction_ids.get(&key), paths.get(&(table, owner)).cloned()) {
                paths.insert(("corrections".into(), *id), path);
            }
        }
        let mut dataset_files: BTreeMap<String, BTreeSet<PathBuf>> = FIXED_DATA_FILES
            .iter()
            .map(|(table, path)| ((*table).to_string(), BTreeSet::from([PathBuf::from(path)])))
            .collect();
        let folders = pack_folders(db)?;
        let mut pack_exception_rows = BTreeMap::new();
        let columns = crate::database_tables::table_columns(db, "pack_exceptions")?;
        let pack_column =
            columns.iter().position(|name| name == "pack_name").context("pack exceptions lack pack_name")?;
        for (id, row) in crate::database_tables::table_rows_by_rowid(db, "pack_exceptions")? {
            let pack = crate::delta::sql_text(&row[pack_column]).context("pack exception has no pack name")?;
            let path = folders
                .get(pack)
                .with_context(|| format!("pack exception has no folder: {pack}"))?
                .join("exceptions.toml");
            dataset_files.entry("pack_exceptions".into()).or_default().insert(path.clone());
            paths.insert(("pack_exceptions".into(), id), path);
            pack_exception_rows.entry(pack.to_string()).or_insert(id);
        }
        Ok(Self { paths, dataset_files, pack_exception_rows })
    }

    pub(crate) fn dataset_paths(&self, table: &str) -> BTreeSet<PathBuf> {
        self.dataset_files.get(table).cloned().unwrap_or_default()
    }

    pub(crate) fn pack_exception_path(&self, name: &str) -> Option<&PathBuf> {
        self.pack_exception_rows.get(name).and_then(|id| self.paths.get(&("pack_exceptions".into(), *id)))
    }

    pub fn path(&self, table: &str, id: i64) -> Result<PathBuf> {
        self.paths
            .get(&(table.to_string(), id))
            .cloned()
            .or_else(|| fixed_data_file(table).ok().map(Path::to_path_buf))
            .with_context(|| format!("{table} {id} has no data file path"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filigree_sets_keep_all_members_together_and_resolve_existing_aliases() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        for name in ["First Pack", "Second Pack"] {
            let pack: kinds::PackFile = toml::from_str(&format!("name = {name:?}\nupdate = 'Update 39'\n")).unwrap();
            let identity = slug(name);
            pack.write_pack(&db, Path::new("pack.toml"), ids::stable_hash(&["adventure_packs", &identity]), &identity)
                .unwrap();
        }
        let aliases: kinds::SourceAliasesFile = toml::from_str("[[adventure_pack]]\ntext = 'Alias'\npack = 'First Pack'\nreason = 'Fixture alias'\nurl = 'https://ddowiki.com/page/Example'\nread = '2026-10-14'\n").unwrap();
        aliases.write_source_aliases(&db).unwrap();
        for (name, menus, folder) in [
            ("Direct", vec!["First Pack", "First Pack"], "packs/u039-first-pack"),
            ("Aliased", vec!["First Pack", "Alias"], "packs/u039-first-pack"),
            ("Unresolved", vec!["First Pack", "Unknown"], "packs/shared"),
            ("Conflicting", vec!["First Pack", "Second Pack"], "packs/shared"),
        ] {
            let set: kinds::SetFile = toml::from_str(&format!("name = {name:?}\n")).unwrap();
            let identity = slug(name);
            set.write_set(&db, Path::new("set.toml"), ids::stable_hash(&["set_bonuses", &identity]), &identity)
                .unwrap();
            let mut contents = String::new();
            for (index, menu) in menus.iter().enumerate() {
                contents.push_str(&format!(
                    "[[row]]\nname = '{name} {index}'\ndescription = ''\nicon = ''\nmenu = {menu:?}\nset = {name:?}\n"
                ));
            }
            let file: kinds::FiligreeSetFile = toml::from_str(&contents).unwrap();
            file.write_filigree_set(&db, Path::new("filigrees.toml")).unwrap();
            let layout = ExportLayout::from_database(&db).unwrap();
            for index in 0..menus.len() {
                let id = ids::stable_hash(&["filigrees", &slug(&format!("{name} {index}"))]);
                assert_eq!(
                    layout.path("filigrees", id).unwrap(),
                    Path::new(folder).join("filigrees").join(format!("{identity}.toml"))
                );
            }
        }
    }

    #[test]
    fn source_records_and_rewards_share_their_resolved_folders() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        ddo_model::seeds::insert_all(&db).unwrap();
        for name in ["First Pack", "Second Pack", "Free to Play"] {
            let file: kinds::PackFile = toml::from_str(&format!("name = {name:?}")).unwrap();
            let identity = slug(name);
            file.write_pack(&db, Path::new("pack.toml"), ids::stable_hash(&["adventure_packs", &identity]), &identity)
                .unwrap();
        }
        for name in ["First Patron", "Second Patron"] {
            let file: kinds::PatronFile = toml::from_str(&format!("name = {name:?}")).unwrap();
            let identity = slug(name);
            file.write_patron(&db, Path::new("patron.toml"), ids::stable_hash(&["patrons", &identity]), &identity)
                .unwrap();
        }
        for (name, pack, patron) in [
            ("First Quest", "First Pack", None),
            ("Other Quest", "First Pack", None),
            ("Unchained Quest", "First Pack", None),
            ("Many Chains Quest", "First Pack", None),
            ("Cross Pack Quest", "Second Pack", None),
            ("Cross Patron Quest", "Free to Play", Some("Second Patron")),
            ("Free Quest", "Free to Play", Some("First Patron")),
            ("Other Free Quest", "Free to Play", Some("First Patron")),
            ("Patronless Quest", "Free to Play", None),
            ("Disagreeing Quest", "First Pack", None),
        ] {
            let mut contents = format!("name = {name:?}\npack = {pack:?}\n");
            if let Some(patron) = patron {
                contents.push_str(&format!("patron = {patron:?}\n"));
            }
            let file: kinds::QuestFile = toml::from_str(&contents).unwrap();
            let identity = slug(name);
            file.write_quest(&db, Path::new("quest.toml"), ids::stable_hash(&["quests", &identity]), &identity)
                .unwrap();
        }
        for (name, pack, quests) in [
            ("First Chain", Some("First Pack"), vec!["First Quest", "Many Chains Quest"]),
            ("Second Chain", None, vec!["Other Quest", "Many Chains Quest"]),
            ("Empty Chain", Some("First Pack"), vec![]),
            ("Cross Pack Chain", Some("First Pack"), vec!["Other Quest", "Cross Pack Quest"]),
            ("Cross Patron Chain", None, vec!["Other Free Quest", "Cross Patron Quest"]),
            ("Free Chain", None, vec!["Free Quest"]),
            ("Patronless Chain", None, vec!["Patronless Quest"]),
            ("Disagreeing Chain", Some("Second Pack"), vec!["Disagreeing Quest"]),
        ] {
            let mut contents = format!("name = {name:?}\nquests = {quests:?}\n");
            if let Some(pack) = pack {
                contents.push_str(&format!("pack = {pack:?}\n"));
            }
            contents.push_str("citation = []\n");
            let file: kinds::QuestSeriesFile = toml::from_str(&contents).unwrap();
            let id = file
                .write_quest_series(&db, Path::new("chain.toml"), &mut ids::StableIds::default(), "quest_chains")
                .unwrap();
            file.insert_quests(&db, Path::new("chain.toml"), id, "quest_chain_quests", "chain_id").unwrap();
        }
        for (name, pack) in
            [("Pack Vendor", Some("First Pack")), ("Shared Vendor", None), ("Other Shared Vendor", None)]
        {
            let mut contents = format!("name = {name:?}\nlocation = 'Somewhere'\ncitation = []\n");
            if let Some(pack) = pack {
                contents.push_str(&format!("pack = {pack:?}\n"));
            }
            let file: kinds::VendorFile = toml::from_str(&contents).unwrap();
            let identity = slug(name);
            file.write_vendor(&db, Path::new("vendor.toml"), ids::stable_hash(&["vendors", &identity]), &identity)
                .unwrap();
        }
        for (name, pack, families) in
            [("Pack System", Some("First Pack"), vec!["Unique", "Multiple"]), ("Shared System", None, vec!["Multiple"])]
        {
            let mut contents = format!("name = {name:?}\npage = 'https://ddowiki.com/page/Example'\ncitation = []\n");
            if let Some(pack) = pack {
                contents.push_str(&format!("pack = {pack:?}\n"));
            }
            for family in families {
                contents.push_str(&format!("[[family]]\nfamily = {family:?}\n"));
            }
            let file: kinds::CraftingSystemFile = toml::from_str(&contents).unwrap();
            file.write_crafting_system(&db, Path::new("crafting.toml"), &mut ids::StableIds::default()).unwrap();
        }
        let file: kinds::EventFile = toml::from_str("name = 'Shared Event'\ncitation = []\n").unwrap();
        file.write_event(&db, Path::new("event.toml"), ids::stable_hash(&["events", "shared-event"]), "shared-event")
            .unwrap();
        for (name, quests) in
            [("Pack Saga", vec!["First Quest"]), ("Shared Saga", vec!["First Quest", "Cross Pack Quest"])]
        {
            let file: kinds::QuestSeriesFile =
                toml::from_str(&format!("name = {name:?}\nquests = {quests:?}\ncitation = []\n")).unwrap();
            let id =
                file.write_quest_series(&db, Path::new("saga.toml"), &mut ids::StableIds::default(), "sagas").unwrap();
            file.insert_quests(&db, Path::new("saga.toml"), id, "saga_quests", "saga_id").unwrap();
        }
        for family in ["Unique", "Multiple", "Unknown"] {
            let file: kinds::AugmentFile =
                toml::from_str(&format!("name = {family:?}\nfamily = {family:?}\ndescription = ''\n")).unwrap();
            let identity = slug(family);
            file.write_augment(&db, Path::new("augment.toml"), ids::stable_hash(&["augments", &identity]), &identity)
                .unwrap();
        }
        for (name, sources) in [
            ("Quest Drop", vec![("quest", "First Quest")]),
            ("Distinct Chests Drop", vec![("quest", "First Quest"), ("quest", "First Quest")]),
            ("Several Shared Vendors Drop", vec![("vendor", "Shared Vendor"), ("vendor", "Other Shared Vendor")]),
            ("Mixed Packs Drop", vec![("quest", "First Quest"), ("adventure_pack", "Second Pack")]),
            ("Several Quests Drop", vec![("quest", "First Quest"), ("quest", "Other Quest")]),
            ("Cross Packs Drop", vec![("quest", "First Quest"), ("quest", "Cross Pack Quest")]),
            ("Vendor Drop", vec![("vendor", "Pack Vendor")]),
            ("Shared Vendor Drop", vec![("vendor", "Shared Vendor")]),
            ("Several Vendors Drop", vec![("vendor", "Pack Vendor"), ("vendor", "Shared Vendor")]),
            ("Crafted Drop", vec![("crafting_system", "Pack System")]),
            ("Shared Crafted Drop", vec![("crafting_system", "Shared System")]),
            ("Event Drop", vec![("event", "Shared Event")]),
            ("Saga Drop", vec![("saga", "Pack Saga")]),
            ("Shared Saga Drop", vec![("saga", "Shared Saga")]),
            ("Challenge Drop", vec![("challenge", "First Pack")]),
            ("Starter Drop", vec![("starter", "Advance to level 15")]),
            ("Unknown Drop", vec![]),
            ("Chain Reward", vec![("quest_chain", "First Chain")]),
            ("Shared Chain Reward", vec![("quest_chain", "Cross Pack Chain")]),
            ("Several Chain Reward", vec![("quest_chain", "First Chain"), ("quest_chain", "Second Chain")]),
            ("Mixed Drop", vec![("quest", "First Quest"), ("quest_chain", "First Chain")]),
        ] {
            let mut contents =
                format!("name = {name:?}\nslot = 'Head'\nitem_category = 'Clothing'\nminimum_level = 1\n");
            for (source_index, (kind, source)) in sources.into_iter().enumerate() {
                contents.push_str(&format!("[[source]]\nkind = {kind:?}\nname = {source:?}\n"));
                if kind == "saga" {
                    contents.push_str("tier = 'any'\n");
                }
                if matches!(kind, "quest" | "adventure_pack") {
                    contents.push_str("loot_type = 'chest'\n");
                    if name == "Distinct Chests Drop" {
                        contents.push_str(&format!("chest = 'Chest {source_index}'\n"));
                    }
                }
            }
            let file: kinds::ItemFile = toml::from_str(&contents).unwrap();
            let identity = slug(name);
            let id = ids::stable_hash(&["items", &identity]);
            file.write_item(&db, Path::new("item.toml"), id, &identity).unwrap();
            file.insert_sources(&db, id, &BTreeMap::new(), &SourceAliases::default()).unwrap();
        }
        insert_entity_identities(&db).unwrap();
        let layout = ExportLayout::from_database(&db).unwrap();
        for (kind, name, expected) in [
            ("vendors", "Pack Vendor", "packs/first-pack/vendors/pack-vendor/vendor.toml"),
            ("vendors", "Shared Vendor", "packs/shared/vendors/shared-vendor/vendor.toml"),
            ("crafting_systems", "Pack System", "packs/first-pack/crafting/pack-system/crafting.toml"),
            ("crafting_systems", "Shared System", "packs/shared/crafting/shared-system/crafting.toml"),
            ("events", "Shared Event", "packs/shared/events/shared-event/event.toml"),
            ("sagas", "Pack Saga", "packs/first-pack/sagas/pack-saga.toml"),
            ("sagas", "Shared Saga", "packs/shared/sagas/shared-saga.toml"),
            ("augments", "Unique", "packs/first-pack/crafting/pack-system/augments/unique.toml"),
            ("augments", "Multiple", "packs/shared/augments/multiple/multiple.toml"),
            ("augments", "Unknown", "packs/shared/augments/unknown/unknown.toml"),
            ("items", "Mixed Packs Drop", "packs/first-pack/quests/first-quest/gear/mixed-packs-drop.toml"),
            ("items", "Several Quests Drop", "packs/first-pack/gear/several-quests-drop.toml"),
            ("items", "Cross Packs Drop", "packs/shared/gear/cross-packs-drop.toml"),
            ("items", "Vendor Drop", "packs/first-pack/vendors/pack-vendor/gear/vendor-drop.toml"),
            ("items", "Shared Vendor Drop", "packs/shared/vendors/shared-vendor/gear/shared-vendor-drop.toml"),
            ("items", "Several Vendors Drop", "packs/first-pack/gear/several-vendors-drop.toml"),
            ("items", "Crafted Drop", "packs/first-pack/crafting/pack-system/gear/crafted-drop.toml"),
            ("items", "Shared Crafted Drop", "packs/shared/crafting/shared-system/gear/shared-crafted-drop.toml"),
            ("items", "Event Drop", "packs/shared/events/shared-event/gear/event-drop.toml"),
            ("items", "Saga Drop", "packs/shared/gear/saga-drop.toml"),
            ("items", "Shared Saga Drop", "packs/shared/gear/shared-saga-drop.toml"),
            ("items", "Challenge Drop", "packs/first-pack/gear/challenge-drop.toml"),
            ("items", "Starter Drop", "packs/general/gear/starter-drop.toml"),
            ("items", "Unknown Drop", "packs/unsourced/gear/unknown-drop.toml"),
            ("quests", "First Quest", "packs/first-pack/quests/first-quest/quest.toml"),
            ("quests", "Unchained Quest", "packs/first-pack/quests/unchained-quest/quest.toml"),
            ("quests", "Many Chains Quest", "packs/first-pack/quests/many-chains-quest/quest.toml"),
            ("quests", "Other Quest", "packs/first-pack/quests/other-quest/quest.toml"),
            ("quests", "Cross Pack Quest", "packs/second-pack/quests/cross-pack-quest/quest.toml"),
            ("quests", "Cross Patron Quest", "packs/free-to-play/second-patron/quests/cross-patron-quest/quest.toml"),
            ("quests", "Other Free Quest", "packs/free-to-play/first-patron/quests/other-free-quest/quest.toml"),
            ("quests", "Free Quest", "packs/free-to-play/first-patron/quests/free-quest/quest.toml"),
            ("quests", "Patronless Quest", "packs/free-to-play/quests/patronless-quest/quest.toml"),
            ("quests", "Disagreeing Quest", "packs/first-pack/quests/disagreeing-quest/quest.toml"),
            ("quest_chains", "First Chain", "packs/first-pack/chains/first-chain.toml"),
            ("quest_chains", "Second Chain", "packs/first-pack/chains/second-chain.toml"),
            ("quest_chains", "Empty Chain", "packs/first-pack/chains/empty-chain.toml"),
            ("quest_chains", "Cross Pack Chain", "packs/first-pack/chains/cross-pack-chain.toml"),
            ("quest_chains", "Cross Patron Chain", "packs/free-to-play/chains/cross-patron-chain.toml"),
            ("quest_chains", "Disagreeing Chain", "packs/second-pack/chains/disagreeing-chain.toml"),
            ("items", "Distinct Chests Drop", "packs/first-pack/quests/first-quest/gear/distinct-chests-drop.toml"),
            ("items", "Several Shared Vendors Drop", "packs/shared/gear/several-shared-vendors-drop.toml"),
            ("items", "Quest Drop", "packs/first-pack/quests/first-quest/gear/quest-drop.toml"),
            ("items", "Chain Reward", "packs/first-pack/gear/chain-reward.toml"),
            ("items", "Shared Chain Reward", "packs/first-pack/gear/shared-chain-reward.toml"),
            ("items", "Several Chain Reward", "packs/first-pack/gear/several-chain-reward.toml"),
            ("items", "Mixed Drop", "packs/first-pack/quests/first-quest/gear/mixed-drop.toml"),
        ] {
            let id = db.query_row(&format!("SELECT id FROM {kind} WHERE name = ?1"), [name], |row| row.get(0)).unwrap();
            assert_eq!(layout.path(kind, id).unwrap(), Path::new(expected), "{name}");
        }
    }
}
