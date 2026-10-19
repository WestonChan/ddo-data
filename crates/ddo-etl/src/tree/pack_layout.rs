use super::*;

type EntityKey = (String, i64);

struct SourcePack {
    source: EntityKey,
    pack: Option<String>,
}

pub(super) struct PackAssignments {
    packs: BTreeMap<EntityKey, String>,
    patrons: BTreeMap<EntityKey, String>,
    pub(super) family_packs: BTreeMap<i64, String>,
    pub(super) disagreements: BTreeMap<EntityKey, (String, String)>,
}

fn unique_pack<'a>(packs: impl IntoIterator<Item = &'a String>) -> Option<String> {
    let mut packs = packs.into_iter();
    let first = packs.next()?;
    packs.all(|pack| pack == first).then(|| first.clone())
}

fn unanimous_pack<'a>(packs: impl IntoIterator<Item = Option<&'a String>>) -> Option<String> {
    let packs = packs.into_iter().collect::<Option<Vec<_>>>()?;
    unique_pack(packs)
}

impl PackAssignments {
    pub(super) fn from_database(db: &Connection) -> Result<Self> {
        let mut authored_packs = BTreeMap::new();
        for table in ["quests", "quest_chains", "sagas", "crafting_systems", "vendors"] {
            let mut statement = db.prepare(&format!(
                "SELECT entity.id, pack.name FROM {table} entity JOIN adventure_packs pack ON pack.id = entity.pack_id"
            ))?;
            for row in statement.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))? {
                let (id, pack) = row?;
                authored_packs.insert((table.to_string(), id), pack);
            }
        }
        let mut sources_by_owner = BTreeMap::<EntityKey, Vec<SourcePack>>::new();
        let mut statement = db.prepare(
            "SELECT CASE WHEN source.item_id IS NOT NULL THEN 'items' ELSE 'augments' END,
                    COALESCE(source.item_id, source.augment_id),
                    CASE source.kind WHEN 'quest' THEN 'quests' WHEN 'quest_chain' THEN 'quest_chains'
                        WHEN 'saga' THEN 'sagas' WHEN 'crafting_system' THEN 'crafting_systems'
                        WHEN 'vendor' THEN 'vendors' WHEN 'event' THEN 'events' ELSE source.kind END,
                    COALESCE(source.quest_id, source.chain_id, source.saga_id, source.crafting_system_id,
                        source.vendor_id, source.event_id, source.pack_id, 0), pack.name
             FROM sources source
             LEFT JOIN quests quest ON quest.id = source.quest_id
             LEFT JOIN quest_chains chain ON chain.id = source.chain_id
             LEFT JOIN sagas saga ON saga.id = source.saga_id
             LEFT JOIN crafting_systems system ON system.id = source.crafting_system_id
             LEFT JOIN vendors vendor ON vendor.id = source.vendor_id
             LEFT JOIN adventure_packs pack ON pack.id = COALESCE(source.pack_id, quest.pack_id,
                 chain.pack_id, saga.pack_id, system.pack_id, vendor.pack_id)",
        )?;
        for row in statement.query_map([], |row| {
            Ok(((row.get(0)?, row.get(1)?), SourcePack { source: (row.get(2)?, row.get(3)?), pack: row.get(4)? }))
        })? {
            let (owner, source) = row?;
            sources_by_owner.entry(owner).or_default().push(source);
        }
        let mut members_by_owner = BTreeMap::<EntityKey, Vec<EntityKey>>::new();
        let mut statement = db.prepare(
            "SELECT 'set_bonuses', set_id, 'items', item_id FROM set_bonus_items
             UNION SELECT 'set_bonuses', set_id, 'augments', augment_id FROM set_bonus_augments
             UNION SELECT 'quest_chains', chain_id, 'quests', quest_id FROM quest_chain_quests
             UNION SELECT 'sagas', saga_id, 'quests', quest_id FROM saga_quests
             UNION SELECT 'patrons', patron_id, 'quests', id FROM quests WHERE patron_id IS NOT NULL
             UNION SELECT 'crafting_systems', crafting_system_id, owner_kind || 's', owner_id FROM crafting_outputs
             UNION SELECT 'crafting_systems', recipe.system_id, 'augments', output.augment_id
                 FROM crafting_recipe_augments output JOIN crafting_recipes recipe ON recipe.id = output.recipe_id
             UNION SELECT 'vendors', vendor_id, 'items', item_id FROM vendor_offers
             UNION SELECT 'events', event_id, owner_kind || 's', owner_id FROM event_rewards",
        )?;
        for row in statement.query_map([], |row| Ok(((row.get(0)?, row.get(1)?), (row.get(2)?, row.get(3)?))))? {
            let (owner, member) = row?;
            members_by_owner.entry(owner).or_default().push(member);
        }
        let mut packs = authored_packs.clone();
        let mut disagreements = BTreeMap::new();
        for (owner, members) in &members_by_owner {
            let member_packs = members
                .iter()
                .map(|member| {
                    if member.0 == "quests" {
                        authored_packs.get(member).cloned()
                    } else {
                        unique_pack(
                            sources_by_owner
                                .get(member)
                                .into_iter()
                                .flatten()
                                .filter(|source| &source.source != owner)
                                .filter_map(|source| source.pack.as_ref()),
                        )
                    }
                })
                .collect::<Vec<_>>();
            let derived_pack = unanimous_pack(member_packs.iter().map(Option::as_ref));
            if let Some(authored_pack) = authored_packs.get(owner) {
                if let Some(derived_pack) = derived_pack.filter(|pack| pack != authored_pack) {
                    disagreements.insert(owner.clone(), (authored_pack.clone(), derived_pack));
                }
            } else if let Some(pack) = derived_pack {
                packs.insert(owner.clone(), pack);
            }
        }
        let mut items_by_family = BTreeMap::<String, Vec<i64>>::new();
        let mut statement = db.prepare("SELECT id, name FROM items")?;
        for row in statement.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))? {
            let (id, name) = row?;
            items_by_family.entry(item_family_tier(&name).0.to_string()).or_default().push(id);
        }
        let mut family_packs = BTreeMap::new();
        for members in items_by_family.into_values() {
            let sourced_packs = members
                .iter()
                .filter_map(|id| {
                    sources_by_owner
                        .get(&("items".to_string(), *id))
                        .map(|sources| unique_pack(sources.iter().filter_map(|source| source.pack.as_ref())))
                })
                .collect::<Vec<_>>();
            if let Some(pack) = unanimous_pack(sourced_packs.iter().map(Option::as_ref)) {
                for id in members {
                    if !sources_by_owner.contains_key(&("items".to_string(), id)) {
                        family_packs.insert(id, pack.clone());
                    }
                }
            }
        }
        let mut patrons = BTreeMap::new();
        let mut statement = db.prepare("SELECT quest.id, patron.name FROM quests quest JOIN patrons patron ON patron.id = quest.patron_id JOIN adventure_packs pack ON pack.id = quest.pack_id WHERE pack.name = 'Free to Play'")?;
        for row in statement.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))? {
            let (id, patron) = row?;
            patrons.insert(("quests".to_string(), id), patron);
        }
        for (owner, members) in &members_by_owner {
            if matches!(owner.0.as_str(), "quest_chains" | "sagas" | "patrons")
                && packs.get(owner).is_some_and(|pack| pack == "Free to Play")
            {
                if let Some(patron) = unanimous_pack(members.iter().map(|member| patrons.get(member))) {
                    patrons.insert(owner.clone(), patron);
                }
            }
        }
        let authored_patrons = patrons.clone();
        for (owner, members) in &members_by_owner {
            if !matches!(owner.0.as_str(), "quest_chains" | "sagas" | "patrons")
                && packs.get(owner).is_some_and(|pack| pack == "Free to Play")
            {
                let member_patrons = members
                    .iter()
                    .map(|member| {
                        unanimous_pack(
                            sources_by_owner
                                .get(member)
                                .into_iter()
                                .flatten()
                                .filter(|source| &source.source != owner)
                                .map(|source| authored_patrons.get(&source.source)),
                        )
                    })
                    .collect::<Vec<_>>();
                if let Some(patron) = unanimous_pack(member_patrons.iter().map(Option::as_ref)) {
                    patrons.insert(owner.clone(), patron);
                }
            }
        }
        for (owner, sources) in &sources_by_owner {
            if let Some(patron) = unanimous_pack(sources.iter().map(|source| authored_patrons.get(&source.source))) {
                patrons.insert(owner.clone(), patron);
            }
        }
        Ok(Self { packs, patrons, family_packs, disagreements })
    }

    pub(super) fn patron(&self, kind: &str, id: i64) -> Option<&str> {
        self.patrons.get(&(kind.to_string(), id)).map(String::as_str)
    }

    pub(super) fn pack(&self, kind: &str, id: i64) -> Option<&str> {
        self.packs.get(&(kind.to_string(), id)).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_pack_layout_database() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        ddo_model::seeds::insert_all(&db).unwrap();
        for name in ["First Pack", "Second Pack"] {
            let pack: kinds::PackFile = toml::from_str(&format!("name = {name:?}")).unwrap();
            let identity = slug(name);
            pack.write_pack(&db, Path::new("pack.toml"), ids::stable_hash(&["adventure_packs", &identity]), &identity)
                .unwrap();
        }
        db
    }

    fn insert_test_item(db: &Connection, name: &str, sources: &[(&str, &str)]) -> i64 {
        let mut contents = format!("name = {name:?}\nslot = 'Head'\nitem_category = 'Clothing'\nminimum_level = 1\n");
        for (kind, source) in sources {
            contents.push_str(&format!("[[source]]\nkind = {kind:?}\nname = {source:?}\n"));
            if *kind == "adventure_pack" {
                contents.push_str("loot_type = 'chest'\n");
            }
        }
        let item: kinds::ItemFile = toml::from_str(&contents).unwrap();
        let identity = slug(name);
        let id = ids::stable_hash(&["items", &identity]);
        item.write_item(db, Path::new("items/test.toml"), id, &identity).unwrap();
        item.insert_sources(db, id, &BTreeMap::new(), &SourceAliases::default()).unwrap();
        id
    }

    #[test]
    fn pack_derivation_uses_real_sources_without_following_derived_owners() {
        let db = build_pack_layout_database();
        for (name, pack) in
            [("Derived Vendor", None), ("Dependent Vendor", None), ("Authored Vendor", Some("Second Pack"))]
        {
            let mut contents = format!("name = {name:?}\nlocation = 'Somewhere'\n");
            if let Some(pack) = pack {
                contents.push_str(&format!("pack = {pack:?}\n"));
            }
            contents.push_str("[[citation]]\nurl = 'https://ddowiki.com/page/Example'\nread = '2026-10-14'\n");
            let vendor: kinds::VendorFile = toml::from_str(&contents).unwrap();
            let identity = slug(name);
            vendor
                .write_vendor(&db, Path::new("vendor.toml"), ids::stable_hash(&["vendors", &identity]), &identity)
                .unwrap();
        }
        let system: kinds::CraftingSystemFile = toml::from_str(
            "name = 'Derived System'\npage = 'https://ddowiki.com/page/Example'\n[[citation]]\nurl = 'https://ddowiki.com/page/Example'\nread = '2026-10-14'\n",
        )
        .unwrap();
        system.write_crafting_system(&db, Path::new("crafting.toml"), &mut ids::StableIds::default()).unwrap();
        let event: kinds::EventFile = toml::from_str(
            "name = 'Derived Event'\n[[citation]]\nurl = 'https://ddowiki.com/page/Example'\nread = '2026-10-14'\n",
        )
        .unwrap();
        event
            .write_event(&db, Path::new("event.toml"), ids::stable_hash(&["events", "derived-event"]), "derived-event")
            .unwrap();
        insert_test_item(
            &db,
            "First Item",
            &[
                ("adventure_pack", "First Pack"),
                ("vendor", "Derived Vendor"),
                ("crafting_system", "Derived System"),
                ("event", "Derived Event"),
            ],
        );
        assert_eq!(
            PackAssignments::from_database(&db)
                .unwrap()
                .pack("vendors", ids::stable_hash(&["vendors", "derived-vendor"])),
            Some("First Pack")
        );
        let quest: kinds::QuestFile = toml::from_str("name = 'First Quest'\npack = 'First Pack'\n").unwrap();
        quest
            .write_quest(&db, Path::new("quest.toml"), ids::stable_hash(&["quests", "first-quest"]), "first-quest")
            .unwrap();
        let chain: kinds::QuestSeriesFile = toml::from_str("name = 'Derived Chain'\nquests = ['First Quest']\n[[citation]]\nurl = 'https://ddowiki.com/page/Example'\nread = '2026-10-14'\n").unwrap();
        let chain_id = chain
            .write_quest_series(&db, Path::new("chain.toml"), &mut ids::StableIds::default(), "quest_chains")
            .unwrap();
        chain.insert_quests(&db, chain_id, "quest_chain_quests", "chain_id").unwrap();
        assert_eq!(PackAssignments::from_database(&db).unwrap().pack("quest_chains", chain_id), Some("First Pack"));
        insert_test_item(&db, "Derived Chain Item", &[("quest_chain", "Derived Chain")]);
        insert_test_item(&db, "Second Item", &[("adventure_pack", "Second Pack")]);
        insert_test_item(&db, "Unresolved Item", &[]);
        insert_test_item(&db, "Dependent Item", &[("vendor", "Derived Vendor"), ("vendor", "Dependent Vendor")]);
        insert_test_item(&db, "Disagreeing Item", &[("adventure_pack", "First Pack"), ("vendor", "Authored Vendor")]);
        let augment: kinds::AugmentFile = toml::from_str("name = 'First Augment'\nfamily = 'Named'\ndescription = ''\n[[source]]\nkind = 'adventure_pack'\nname = 'First Pack'\nloot_type = 'chest'\n").unwrap();
        let augment_id = ids::stable_hash(&["augments", "first-augment"]);
        augment.write_augment(&db, Path::new("augment.toml"), augment_id, "first-augment").unwrap();
        augment.insert_sources(&db, augment_id, &BTreeMap::new(), &SourceAliases::default()).unwrap();
        for (name, members, has_augment, expected) in [
            ("Empty Set", vec![], false, None),
            ("Derived Chain Set", vec!["Derived Chain Item"], false, None),
            ("Unknown Set", vec!["First Item", "Unresolved Item"], false, None),
            ("Mixed Set", vec!["First Item", "Second Item"], false, None),
            ("First Set", vec!["First Item"], true, Some("First Pack")),
            ("Augment Set", vec![], true, Some("First Pack")),
        ] {
            let mut contents = format!("name = {name:?}\n");
            for member in members {
                contents.push_str(&format!("[[item]]\nname = {member:?}\n"));
            }
            if has_augment {
                contents.push_str("[[augment]]\nname = 'First Augment'\nfamily = 'Named'\n");
            }
            let set: kinds::SetFile = toml::from_str(&contents).unwrap();
            let identity = slug(name);
            let id = ids::stable_hash(&["set_bonuses", &identity]);
            set.write_set(&db, Path::new("set.toml"), id, &identity).unwrap();
            set.insert_members(&db, Path::new("set.toml"), id).unwrap();
            assert_eq!(PackAssignments::from_database(&db).unwrap().pack("set_bonuses", id), expected, "{name}");
        }
        let assignments = PackAssignments::from_database(&db).unwrap();
        let derived_vendor = ids::stable_hash(&["vendors", "derived-vendor"]);
        assert_eq!(assignments.pack("vendors", derived_vendor), None, "one unresolved offer prevents derivation");
        assert_eq!(assignments.pack("vendors", ids::stable_hash(&["vendors", "dependent-vendor"])), None);
        assert_eq!(
            assignments.pack("crafting_systems", ids::stable_hash(&["crafting_systems", "derived-system"])),
            Some("First Pack")
        );
        assert_eq!(assignments.pack("events", ids::stable_hash(&["events", "derived-event"])), Some("First Pack"));
        let authored_vendor = ids::stable_hash(&["vendors", "authored-vendor"]);
        assert_eq!(assignments.pack("vendors", authored_vendor), Some("Second Pack"));
        assert_eq!(
            assignments.disagreements[&("vendors".to_string(), authored_vendor)],
            ("Second Pack".to_string(), "First Pack".to_string())
        );
    }

    #[test]
    fn unsourced_versions_use_only_unanimous_sourced_siblings() {
        let db = build_pack_layout_database();
        insert_test_item(&db, "Family Ring", &[("adventure_pack", "First Pack")]);
        let epic = insert_test_item(&db, "Epic Family Ring", &[]);
        let level = insert_test_item(&db, "Legendary Family Ring (level 30)", &[]);
        let orphan = insert_test_item(&db, "Legendary Orphan Ring", &[]);
        let assignments = PackAssignments::from_database(&db).unwrap();
        assert_eq!(assignments.family_packs.get(&epic).map(String::as_str), Some("First Pack"));
        assert_eq!(assignments.family_packs.get(&level).map(String::as_str), Some("First Pack"));
        assert!(!assignments.family_packs.contains_key(&orphan));
        insert_test_item(&db, "Family Ring (level 15)", &[("adventure_pack", "Second Pack")]);
        let assignments = PackAssignments::from_database(&db).unwrap();
        assert!(!assignments.family_packs.contains_key(&epic));
        assert!(!assignments.family_packs.contains_key(&level));
    }

    #[test]
    fn crafting_recipe_outputs_participate_in_pack_derivation() {
        let db = build_pack_layout_database();
        for name in ["Known Augment", "Unknown Augment"] {
            let mut contents = format!("name = {name:?}\nfamily = 'Named'\nmin_level = 1\ndescription = ''\n");
            if name == "Known Augment" {
                contents.push_str("[[source]]\nkind = 'adventure_pack'\nname = 'First Pack'\nloot_type = 'chest'\n");
            }
            let augment: kinds::AugmentFile = toml::from_str(&contents).unwrap();
            let identity = slug(name);
            let id = ids::stable_hash(&["augments", &identity]);
            augment.write_augment(&db, Path::new("augment.toml"), id, &identity).unwrap();
            augment.insert_sources(&db, id, &BTreeMap::new(), &SourceAliases::default()).unwrap();
        }
        for (name, outputs, expected) in [
            ("Known System", vec!["Known Augment"], Some("First Pack")),
            ("Partial System", vec!["Known Augment", "Unknown Augment"], None),
        ] {
            let mut contents = format!("name = {name:?}\npage = 'https://ddowiki.com/page/Example'\n[[citation]]\nurl = 'https://ddowiki.com/page/Example'\nread = '2026-10-14'\n[[recipe]]\noption = 'Craft augments'\ntier = 'any'\n");
            for output in outputs {
                contents.push_str(&format!("[[recipe.augment]]\nname = {output:?}\nfamily = 'Named'\nlevel = 1\n"));
            }
            let system: kinds::CraftingSystemFile = toml::from_str(&contents).unwrap();
            let id =
                system.write_crafting_system(&db, Path::new("crafting.toml"), &mut ids::StableIds::default()).unwrap();
            system.insert_recipes(&db, Path::new("crafting.toml"), id).unwrap();
            insert_test_item(
                &db,
                &format!("{name} Product"),
                &[("crafting_system", name), ("adventure_pack", "First Pack")],
            );
            assert_eq!(PackAssignments::from_database(&db).unwrap().pack("crafting_systems", id), expected, "{name}");
        }
    }

    #[test]
    fn every_member_must_resolve_to_one_pack() {
        let first = "First Pack".to_string();
        let second = "Second Pack".to_string();
        assert_eq!(unanimous_pack([]), None);
        assert_eq!(unanimous_pack([None]), None);
        assert_eq!(unanimous_pack([Some(&first), None]), None);
        assert_eq!(unanimous_pack([Some(&first), Some(&second)]), None);
        assert_eq!(unanimous_pack([Some(&first), Some(&first)]), Some(first));
    }
}
