mod fixture;

const CHAIN: &str = "packs/free-to-play/chains/the-lost-seekers.toml";
const SAGA_ITEM: &str = "packs/shared/gear/alabaster-of-the-twelve.toml";
const CHAIN_ITEM: &str = "packs/free-to-play/gear/acrobats-ring.toml";
const CRAFTING: &str = "packs/shared/crafting/heroic-green-steel/crafting.toml";
const VENDOR_ITEM: &str = "packs/free-to-play/vendors/morten-edgewright/gear/ethereal-great-crossbow.toml";

fn crafting_row_error(rows: &str) -> String {
    fixture::data_build_error(|root| {
        let path = root.join("packs/shared/crafting/catalyst-crafting/crafting.toml");
        let original = std::fs::read_to_string(&path).unwrap();
        std::fs::write(path, format!("{original}\n{rows}")).unwrap();
    })
}

#[test]
fn crafting_system_files_reject_the_removed_npc_field() {
    let path = "packs/shared/crafting/catalyst-crafting/crafting.toml";
    let error = fixture::data_build_error(|root| {
        let file = root.join(path);
        let mut system: toml::Value = toml::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        system.as_table_mut().unwrap().remove("station");
        system.as_table_mut().unwrap().insert("npc".into(), "Fixture trader".into());
        std::fs::write(file, toml::to_string(&system).unwrap()).unwrap();
    });
    assert!(
        error.contains(path) && error.contains("unknown field") && error.contains("npc"),
        "expected npc rejection naming {path}, got {error:?}"
    );
}

#[test]
fn recipe_costs_cannot_repeat_a_step_ingredient_or_item() {
    for reference in ["ingredient='Thread of Fate'", "item=\"Acrobat's Ring\""] {
        for step_tier in ["epic", "any"] {
            for quantity in [1, 2] {
                let error = crafting_row_error(&format!(
                    "[[step]]\nname='Shared cost'\n[[step.cost]]\ntier='{step_tier}'\n{reference}\nquantity=1\n[[recipe]]\noption='Repeated cost'\ntier='epic'\nstep='Shared cost'\n[[recipe.ingredient]]\n{reference}\nquantity={quantity}\n"
                ));
                assert!(
                    error.contains("packs/shared/crafting/catalyst-crafting/crafting.toml"),
                    "expected a rejection naming the crafting file, got {error:?}"
                );
                assert!(error.contains("Repeated cost") && error.contains("already charged by step"), "{error}");
            }
        }
    }
}

#[test]
fn item_socket_label_notes_require_one_present_socket_with_a_changed_label() {
    let path = "packs/unsourced/gear/celestial-emerald-ring.toml";
    for (option, ambiguous, expected) in [
        ("Absent option", false, "correction target is absent"),
        ("+8 Constitution", false, "must change the socket label"),
        ("+8 Constitution", true, "2 sockets"),
    ] {
        let error = fixture::data_build_error(|root| {
            if ambiguous {
                let file = root.join(path);
                let original = std::fs::read_to_string(&file).unwrap();
                let start = original.find("[[socket]]").unwrap();
                let end = original[start + 1..].find("[[socket]]").unwrap() + start + 1;
                let duplicate = &original[start..end];
                std::fs::write(file, format!("{original}\n{duplicate}")).unwrap();
            }
            let qualifier = format!("option {option:?}");
            fixture::append_to_file(root.join(path), format!(
                "\n[[ddobuilder_correction]]\nkind='item_socket'\nfield='label'\nqualifier={qualifier:?}\nupstream='crafting: nearly finished'\nreason='Fixture socket label.'\nin_game=true\nread='2026-11-11'\n"
            ));
        });
        assert!(error.contains(path) && error.contains(expected), "{error}");
        assert!(error.contains(option), "{error}");
    }
}

#[test]
fn a_correction_whose_qualifier_matches_no_line_is_rejected() {
    let qualifier = "Strength / Enhancement / 999";
    let error = fixture::data_build_error(|root| {
        fixture::append_to_file(root.join(CHAIN_ITEM), format!("\n[[ddobuilder_correction]]\nkind='item_bonus'\nfield='bonus_type'\nqualifier={qualifier:?}\nupstream='Equipment'\nreason='Fixture target probe.'\nurl='https://ddowiki.com/page/Item:Acrobat%27s_Ring'\nread='2026-11-11'\n"));
    });
    assert!(error.contains("correction target is absent"), "{error}");
    assert!(error.contains(CHAIN_ITEM), "{error}");
    assert!(error.contains(qualifier), "{error}");
}

#[test]
fn missing_correction_targets_agree_between_xml_and_tree_builds() {
    let qualifier = "Strength / Enhancement / 999";
    for (upstream, recorded_stale) in [("Strength", false), ("no such bonus", true)] {
        let correction = fixture::new_qualified_correction(
            "item_bonus",
            "Acrobat's Ring",
            "stat='Strength'\nbonus_type='Enhancement'\nbonus_value=999",
            "alias",
            &format!("{upstream:?}"),
            "\"Fixture Strength\"",
        );
        let corrections = fixture::corrections_from(&[(CHAIN_ITEM, &correction)]).unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        let report = ddo_etl::build::build_database(
            fixture::groupings(),
            fixture::xml_source().path(),
            &ddo_etl::wiki::WikiOverrides::default(),
            &corrections,
            &mut db,
            &ddo_model::DatasetVersion { upstream_sha: "test".into(), built_at: "2026-11-11T00:00:00Z".into() },
        )
        .unwrap();
        assert_eq!(report.correction_applied_count, usize::from(recorded_stale));
        assert_eq!(report.stale_corrections.len(), usize::from(!recorded_stale));
        if !recorded_stale {
            assert_eq!(report.stale_corrections[0].file_name, CHAIN_ITEM);
        }
        let error = fixture::data_build_error(|root| {
            fixture::append_to_file(root.join(CHAIN_ITEM), format!("\n[[ddobuilder_correction]]\nkind='item_bonus'\nfield='alias'\nqualifier={qualifier:?}\nupstream={upstream:?}\nreason='Fixture target probe.'\nurl='https://ddowiki.com/page/Test'\nread='2026-11-11'\n"));
            if recorded_stale {
                let mut rebuilt = rusqlite::Connection::open_in_memory().unwrap();
                ddo_etl::tree::build_from_data(root, &mut rebuilt).unwrap();
                ddo_etl::corrections::Corrections::from_database(&rebuilt).unwrap();
            }
        });
        if recorded_stale {
            assert!(error.is_empty(), "{error}");
        } else {
            assert!(error.contains("correction target is absent"), "{error}");
            assert!(error.contains(CHAIN_ITEM) && error.contains(qualifier), "{error}");
        }
    }
}

#[test]
fn a_recipe_rejects_the_unqualified_variant_field() {
    let error = crafting_row_error("[[recipe]]\noption='Invalid variant'\ntier='any'\nvariant='legendary'\n");
    assert!(error.contains("unknown field") && error.contains("variant"), "{error}");
}

#[test]
fn a_cost_row_naming_both_an_ingredient_and_an_item_is_rejected() {
    let error = crafting_row_error("[[recipe]]\noption='Invalid cost'\ntier='any'\n[[recipe.ingredient]]\ningredient='Small Shard of Power'\nitem=\"Acrobat's Ring\"\nquantity=1\n");
    assert!(error.contains("exactly one ingredient or item"), "{error}");
}

#[test]
fn a_cost_row_naming_neither_an_ingredient_nor_an_item_is_rejected() {
    let error =
        crafting_row_error("[[recipe]]\noption='Invalid cost'\ntier='any'\n[[recipe.ingredient]]\nquantity=1\n");
    assert!(error.contains("exactly one ingredient or item"), "{error}");
}

#[test]
fn a_cost_row_naming_an_unknown_item_is_rejected_with_nearest_names() {
    let error = crafting_row_error(
        "[[recipe]]\noption='Invalid cost'\ntier='any'\n[[recipe.ingredient]]\nitem=\"Acrobat's Rin\"\nquantity=1\n",
    );
    assert!(error.contains("Acrobat's Ring") && error.contains("nearest"), "{error}");
}

#[test]
fn an_unlock_whose_kind_and_reference_disagree_is_rejected() {
    let error =
        crafting_row_error("[[unlock]]\nkind='level'\nvalue=10\nitem=\"Acrobat's Ring\"\ntext='Invalid level gate'\n");
    assert!(error.contains("Invalid level gate") && error.contains("CHECK constraint"), "{error}");
}

#[test]
fn an_other_unlock_with_a_reference_or_value_is_rejected() {
    for extra in ["value=10", "item=\"Acrobat's Ring\""] {
        let error = crafting_row_error(&format!("[[unlock]]\nkind='other'\ntext='Invalid other gate'\n{extra}\n"));
        assert!(error.contains("Invalid other gate") && error.contains("CHECK constraint"), "{error}");
    }
}

#[test]
fn an_unlock_naming_an_unknown_quest_is_rejected_with_nearest_names() {
    let error = crafting_row_error("[[unlock]]\nkind='quest'\nquest='Book Burnin'\ntext='Complete the quest'\n");
    assert!(error.contains("Book Burning") && error.contains("nearest"), "{error}");
}

#[test]
fn a_recipe_set_option_on_a_recipe_without_a_socketed_step_is_rejected() {
    let error = crafting_row_error("[[recipe]]\noption='Invalid price'\ntier='any'\n[[recipe.set_option]]\nset='Test options'\nname='Test option'\n");
    assert!(error.contains("requires a socketed step"), "{error}");
    let error = crafting_row_error("[[step]]\nname='Pure step'\n[[recipe]]\noption='Invalid price'\ntier='any'\nstep='Pure step'\n[[recipe.set_option]]\nset='Test options'\nname='Test option'\n");
    assert!(error.contains("requires a socketed step"), "{error}");
}

#[test]
fn a_step_cost_naming_an_unknown_ingredient_is_rejected_with_nearest_names() {
    let error = crafting_row_error(
        "[[step]]\nname='Create item'\n[[step.cost]]\ntier='any'\ningredient='Small Shard of Powe'\nquantity=1\n",
    );
    assert!(error.contains("Small Shard of Power") && error.contains("nearest"), "{error}");
}

fn station_reference_error(stations: &str, reference: &str) -> String {
    fixture::data_build_error(|root| {
        fixture::append_to_file(root.join("packs/shared/crafting/stations.toml"), stations);
        let path = root.join("packs/shared/crafting/catalyst-crafting/crafting.toml");
        let file: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        std::fs::write(path, format!("{}\n[[station]]\n{reference}\n", toml::to_string(&file).unwrap())).unwrap();
    })
}

#[test]
fn a_device_station_without_a_place_is_rejected() {
    let error =
        station_reference_error("[[row]]\nname='Test Forge'\nkind='device'\ncitation=[]\n", "name='Test Forge'");
    assert!(error.contains("Test Forge") && error.contains("CHECK constraint"), "{error}");
}

#[test]
fn a_station_reference_without_a_variant_is_ambiguous() {
    let error = station_reference_error("[[row]]\nname='Test Forge'\nvariant='hall'\nkind='device'\nplace='Hall'\ncitation=[]\n[[row]]\nname='Test Forge'\nvariant='airship'\nkind='device'\nplace='Airship'\ncitation=[]\n","name='Test Forge'");
    assert!(error.contains("matched 2 rows") && error.contains("hall") && error.contains("airship"), "{error}");
}

#[test]
fn a_set_reference_without_an_unqualified_identity_lists_its_variants() {
    let error = fixture::data_build_error(|root| {
        let directory = root.join("packs/shared/sockets/crafting-nearly-finished");
        std::fs::create_dir_all(&directory).unwrap();
        for variant in ["2", "3"] {
            std::fs::write(directory.join(format!("test-choices-{variant}.toml")), format!("label='crafting: nearly finished'\nname='Test Choices'\nvariant='{variant}'\n[[option]]\nname='Choice'\n")).unwrap();
        }
        let path = root.join("packs/unsourced/gear/celestial-emerald-ring.toml");
        let mut record: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        record["socket"] =
            toml::from_str::<toml::Value>("[[socket]]\nslot='crafting: nearly finished'\nset='Test Choices'\n")
                .unwrap()["socket"]
                .clone();
        std::fs::write(path, toml::to_string(&record).unwrap()).unwrap();
    });
    assert!(
        error.contains("matched 2 rows") && error.contains("Some(\"2\")") && error.contains("Some(\"3\")"),
        "{error}"
    );
}

#[test]
fn a_system_naming_a_station_the_vocabulary_lacks_is_rejected_with_nearest_names() {
    let error =
        station_reference_error("[[row]]\nname='Test Forge'\nkind='interface'\ncitation=[]\n", "name='Test Forg'");
    assert!(error.contains("Test Forge") && error.contains("nearest"), "{error}");
}

#[test]
fn an_ingredient_reference_without_a_variant_is_ambiguous() {
    let error = fixture::data_build_error(|root| {
        let directory = root.join("packs/shared/crafting/ingredients");
        std::fs::create_dir_all(&directory).unwrap();
        for variant in ["heroic", "legendary"] {
            std::fs::write(
                directory.join(format!("test-key-{variant}.toml")),
                format!("name='Test Key'\nvariant='{variant}'\ntier='any'\ncitation=[]\n"),
            )
            .unwrap();
        }
        let path = root.join("packs/shared/crafting/catalyst-crafting/crafting.toml");
        let original = std::fs::read_to_string(&path).unwrap();
        std::fs::write(path,format!("{original}\n[[recipe]]\noption='Ambiguous cost'\ntier='any'\n[[recipe.ingredient]]\ningredient='Test Key'\nquantity=1\n")).unwrap();
    });
    assert!(error.contains("matched 2 rows") && error.contains("heroic") && error.contains("legendary"), "{error}");
}

#[test]
fn an_unused_shared_ingredient_is_rejected_with_its_file() {
    let error = fixture::data_build_error(|root| {
        let directory = root.join("packs/shared/crafting/ingredients");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("unused-key.toml"), "name='Unused Key'\ntier='any'\ncitation=[]\n").unwrap();
    });
    assert!(error.contains("unused-key.toml") && error.contains("no using system"), "{error}");
}

#[test]
fn a_recipe_with_slot_in_a_file_that_declares_steps_is_rejected() {
    let error =
        fixture::data_build_error(edited(CRAFTING, "step = \"Invasion\"", "slot = \"crafting: accessory invasion\""));
    assert!(error.contains(CRAFTING) && error.contains("cannot name slot"), "{error}");
}

#[test]
fn a_recipe_naming_no_step_of_its_file_is_rejected_with_nearest_names() {
    let error = fixture::data_build_error(edited(CRAFTING, "step = \"Invasion\"", "step = \"Invasio\""));
    assert!(error.contains(CRAFTING) && error.contains("Invasio") && error.contains("nearest: Invasion"), "{error}");
}

#[test]
fn two_steps_sharing_a_socket_are_rejected() {
    let error = fixture::data_build_error(|root| {
        let path = root.join(CRAFTING);
        let original = std::fs::read_to_string(&path).unwrap();
        std::fs::write(
            path,
            format!("{original}\n[[step]]\nname='Other invasion'\nslot='crafting: accessory invasion'\n"),
        )
        .unwrap();
    });
    assert!(
        error.contains(CRAFTING) && error.contains("Other invasion") && error.contains("UNIQUE constraint"),
        "{error}"
    );
}

#[test]
fn an_adds_augment_line_naming_no_socket_label_fails_the_build() {
    let error = fixture::data_build_error(edited(
        "packs/shared/crafting/heroic-green-steel/augments/minor-fire-guard.toml",
        "adds_augment = \"\"\"\nAccessory Subjugation\nColorless\"\"\"",
        "adds_augment = \"Unknown Socket\"",
    ));
    assert!(error.contains("Minor Fire Guard") && error.contains("Unknown Socket"), "{error}");
}

fn edited(file: &'static str, from: &'static str, to: &'static str) -> impl Fn(&std::path::Path) {
    move |root| {
        let path = root.join(file);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(from), "{file} lacks {from:?}");
        std::fs::write(path, text.replacen(from, to, 1)).unwrap();
    }
}

#[test]
fn socket_removal_requires_the_socket_to_be_absent_from_our_file() {
    let file = "packs/menace-of-the-underdark/quests/caught-in-the-web/gear/sireth-spear-of-the-sky.toml";
    let error = fixture::data_build_error(|root| {
        let path = root.join(file);
        let mut text = std::fs::read_to_string(&path).unwrap();
        text.push_str("\n[[ddobuilder_correction]]\nkind = 'item_socket'\nfield = 'remove'\nqualifier = 'socket \"crafting: attuned to heroism 2\"'\nupstream = 0\nreason = 'The screenshot has no upgrade socket.'\nurl = 'https://ddowiki.com/page/Item:Sireth,_Spear_of_the_Sky'\nread = '2026-10-31'\n");
        std::fs::write(path, text).unwrap();
    });
    assert!(error.contains(file) && error.contains("removed socket is still present"), "{error}");
}

#[test]
fn unequal_member_amounts_are_valid_links_but_not_a_lines_split() {
    let file = "packs/shared/crafting/dinosaur-bone-crafting/augments/voidscale.toml";
    let members = "\n[[effect]]\nname = 'Doublestrike'\nbonus_type = 'Profane'\nvalue = 3\n\n[[effect]]\nname = 'Doubleshot'\nbonus_type = 'Profane'\nvalue = 4\n";
    let db = fixture::database(|root| {
        let path = root.join(file);
        let original = std::fs::read_to_string(&path).unwrap();
        std::fs::write(path, format!("{original}{members}")).unwrap();
    });
    let amounts = db.prepare("SELECT e.name,l.value FROM augment_effects l JOIN augments a ON a.id=l.augment_id JOIN effects e ON e.id=l.effect_id WHERE a.name='Voidscale' AND e.name IN ('Doublestrike','Doubleshot') ORDER BY e.name").unwrap()
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))).unwrap()
        .collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(amounts, [("Doubleshot".into(), 4), ("Doublestrike".into(), 3)]);
    let error = fixture::data_build_error(|root| {
        let path = root.join(file);
        let original = std::fs::read_to_string(&path).unwrap();
        std::fs::write(path, format!("{original}{members}\n[[ddobuilder_correction]]\nkind = 'augment_bonus'\nfield = 'lines'\nqualifier = 'Doublestrike, Doubleshot / Profane'\nupstream = [['Doublestrike', 'Doubleshot']]\nreason = 'Separate member lines'\nurl = 'https://ddowiki.com/page/Test'\nread = '2026-11-01'\n")).unwrap();
    });
    assert!(error.contains(file) && error.contains("split lines have different stats, types or values"), "{error}");
}

#[test]
fn files_cannot_redeclare_always_unnamed_home_types() {
    for file in ["effects/riposte.toml", "effects/stat-groups/charisma-skills.toml"] {
        for home in ["Enhancement", "Equipment", "Feat"] {
            let error = fixture::data_build_error(|root| {
                let path = root.join(file);
                let original = std::fs::read_to_string(&path).unwrap();
                let original = original
                    .lines()
                    .filter(|line| !line.starts_with("home_bonus_type = "))
                    .collect::<Vec<_>>()
                    .join("\n");
                std::fs::write(path, format!("home_bonus_type = {home:?}\n{original}\n")).unwrap();
            });
            assert!(error.contains(file) && error.contains("always unnamed"), "{error}");
        }
    }
}

fn duplicate_grouping_file(root: &std::path::Path, shape: &str, verdict: Option<&str>) -> String {
    let (path, name, bonuses) = match shape {
        "group" => {
            ("effects/stat-groups/repeated-strikes.toml", "Repeated Strikes", vec!["Doublestrike", "Doubleshot"])
        }
        "wrapper" => ("effects/repeated-strikes.toml", "Repeated Strikes", vec!["Doublestrike and Doubleshot"]),
        "ladder" => (
            "effects/doublestrike-and-doubleshot-ii.toml",
            "Doublestrike and Doubleshot II",
            vec!["Doublestrike", "Doubleshot"],
        ),
        _ => unreachable!(),
    };
    let declaration = verdict
        .map(|verdict| {
            format!("stat_set_verdict = {verdict:?}\nstat_set_reason = 'Verified separate rank in the fixture'\n")
        })
        .unwrap_or_default();
    let mut contents = format!("name = {name:?}\nverbose_name_template = '%b1 {name} +{{1}}'\ndescription_template = '+{{1}} %b1 bonus to Doublestrike and Doubleshot.'\n{declaration}");
    for target in bonuses {
        contents
            .push_str(&format!("\n[[bonus]]\ntarget = {target:?}\namount_from = 1\nscale = 1.0\nrounding = 'down'\n"));
    }
    contents.push_str("\n[[citation]]\nurl = 'https://ddowiki.com/page/Doublestrike'\nread = '2026-10-23'\n");
    std::fs::write(root.join(path), contents).unwrap();
    path.to_string()
}

#[test]
fn undeclared_duplicate_groupings_fail_for_every_shape() {
    let mut failures = Vec::new();
    for shape in ["group", "wrapper", "ladder"] {
        let error = fixture::data_build_error(|root| {
            duplicate_grouping_file(root, shape, None);
        });
        let expected = if shape == "ladder" { "doublestrike-and-doubleshot-ii.toml" } else { "repeated-strikes.toml" };
        if !error.contains("effects sharing a stat set")
            || !error.contains("doublestrike-and-doubleshot.toml")
            || !error.contains(expected)
        {
            failures.push(format!("{shape}: {error:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn declared_duplicate_effects_remain_addressable() {
    for verdict in ["distinct", "composes"] {
        let db = fixture::database(|root| {
            duplicate_grouping_file(root, "ladder", Some(verdict));
        });
        let groupings = ddo_etl::groupings::Groupings::from_database(&db).unwrap();
        assert!(groupings.pairs(ddo_etl::groupings::GroupingPairDomain::LinkValued).is_empty());
        assert_eq!(
            groupings
                .by_id
                .values()
                .find(|effect| effect.name == "Doublestrike and Doubleshot II")
                .unwrap()
                .stat_set_verdict
                .as_deref(),
            Some(verdict)
        );
    }
}

#[test]
fn structural_rejections_fail_the_data_build_naming_their_file_once() {
    type Injection = (&'static str, &'static str, Box<dyn Fn(&std::path::Path)>);
    let injections: Vec<Injection> = vec![
        ("a chain quest listed twice", CHAIN, Box::new(edited(CHAIN, "\"Redemption\",\n", "\"Redemption\",\n    \"The Grotto\",\n"))),
        (
            "a saga reward tier of mythic",
            SAGA_ITEM,
            Box::new(edited(SAGA_ITEM, "name = \"Masterminds of Sharn\"\ntier = \"any\"", "name = \"Masterminds of Sharn\"\ntier = \"mythic\"")),
        ),
        (
            "a saga reward listed twice at one tier",
            SAGA_ITEM,
            Box::new(edited(
                SAGA_ITEM,
                "kind = \"saga\"\nname = \"Masterminds of Sharn\"\ntier = \"any\"\n",
                "kind = \"saga\"\nname = \"Masterminds of Sharn\"\ntier = \"any\"\n\n[[source]]\nkind = \"saga\"\nname = \"Masterminds of Sharn\"\ntier = \"any\"\n",
            )),
        ),
        ("a crafting cost quantity of 0", CRAFTING, Box::new(edited(CRAFTING, "quantity = 1", "quantity = 0"))),
        ("a crafting cost quantity of -1", CRAFTING, Box::new(edited(CRAFTING, "quantity = 1", "quantity = -1"))),
        ("an ingredient tier of mythic", "packs/shared/crafting/heroic-green-steel/ingredients/small-focus-of-earth.toml", Box::new(edited("packs/shared/crafting/heroic-green-steel/ingredients/small-focus-of-earth.toml", "tier = \"heroic\"", "tier = \"mythic\""))),
        ("a recipe tier of mythic", CRAFTING, Box::new(edited(CRAFTING, "option = \"Minor Fire Guard\"\ntier = \"heroic\"", "option = \"Minor Fire Guard\"\ntier = \"mythic\""))),
        ("a cost naming an undeclared ingredient", CRAFTING, Box::new(edited(CRAFTING, "ingredient = \"Small Shard of Power\"", "ingredient = \"Undeclared Shard\""))),
        (
            "an ingredient declared twice",
            CRAFTING,
            Box::new(|root| fixture::append_to_file(root.join(CRAFTING), "[[ingredient]]\nname='Duplicate Test Ingredient'\ntier='heroic'\n[[ingredient]]\nname='Duplicate Test Ingredient'\ntier='heroic'\n")),
        ),
        (
            "a vendor listing an item twice",
            VENDOR_ITEM,
            Box::new(edited(VENDOR_ITEM, "[[source]]\nkind = \"vendor\"\nname = \"Morten Edgewright\"\n", "[[source]]\nkind = \"vendor\"\nname = \"Morten Edgewright\"\n\n[[source]]\nkind = \"vendor\"\nname = \"Morten Edgewright\"\n")),
        ),
        ("a chain reward with a tier", CHAIN_ITEM, Box::new(edited(CHAIN_ITEM, "kind = \"quest_chain\"\nname = \"The Lost Seekers\"\n", "kind = \"quest_chain\"\nname = \"The Lost Seekers\"\ntier = \"epic\"\n"))),
    ];
    let mut failures = Vec::new();
    for (what, file, edit) in injections {
        let message = fixture::data_build_error(edit);
        let file_name = file.rsplit('/').next().unwrap();
        if !message.contains(file_name) {
            failures.push(format!("{what}: expected a failure naming {file_name}, got {message:?}"));
        } else if message.replace("<data>/", "").contains(&format!("{file}: {file}"))
            || message.contains(&format!("{file} and "))
        {
            failures.push(format!("{what}: repeats {file}: {message}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn effect_replacement_requires_the_owner_to_link_the_qualified_destination() {
    let file = "packs/chill-of-ravenloft/quests/ghosts-of-perdition/gear/epic-ethereal-bracers.toml";
    for destination in ["Riposte IX", "Strength"] {
        let error = fixture::data_build_error(|root| {
            let path = root.join(file);
            let original = std::fs::read_to_string(&path).unwrap();
            std::fs::write(path, format!("{original}\n[[ddobuilder_correction]]\nkind = 'item_effect'\nfield = 'effect'\nqualifier = '{destination}'\nupstream = 'Riposte V'\nreason = 'Fixture effect replacement.'\nurl = 'https://ddowiki.com/page/Riposte'\nread = '2026-10-23'\n")).unwrap();
        });
        if destination == "Riposte IX" {
            assert!(error.is_empty(), "{error}");
        } else {
            assert!(
                error.contains(file.rsplit('/').next().unwrap())
                    && error.contains("destination effect")
                    && error.contains(destination),
                "{error}"
            );
        }
    }
}

#[test]
fn an_effect_member_removal_requires_the_member_absent_from_the_file() {
    let file = "effects/power-of-the-frozen-thunderstorm.toml";
    let removed = "\n[[bonus]]\namount_from = 1\nrounding = \"down\"\nscale = 1.0\ntarget = \"Cold Spell Power\"\n";
    let note = "\n[[ddobuilder_correction]]\nkind = 'effect'\nfield = 'remove'\nqualifier = 'stat \"Cold Spell Power\"'\nupstream = { target = 'Cold Spell Power', amount_from = 1, scale = 1.0, rounding = 'down' }\nreason = 'Fixture member removal.'\nurl = 'https://ddowiki.com/page/Test'\nread = '2026-10-23'\n";
    for keep_member in [true, false] {
        let error = fixture::data_build_error(|root| {
            let path = root.join(file);
            let original = std::fs::read_to_string(&path).unwrap();
            assert!(original.contains(removed));
            let effect = if keep_member { original } else { original.replace(removed, "") };
            std::fs::write(path, effect + note).unwrap();
        });
        if keep_member {
            assert!(error.contains(file) && error.contains("removed effect member is still present"), "{error}");
        } else {
            assert!(error.is_empty(), "{error}");
        }
    }
}

#[test]
fn omitted_bonus_type_is_restricted_to_complete_named_effect_titles() {
    for (file, name, is_alias, template, valid) in [
        ("effects/command.toml", "Command", false, "Command +{1}", true),
        ("effects/command.toml", "Command", true, "Quiet Command +{1}", true),
        ("effects/command.toml", "Command", false, "%b1 Command +{1}", false),
        ("effects/command.toml", "Command", true, "%b1 Quiet Command +{1}", false),
        ("effects/stats/strength.toml", "Strength", false, "Strength +{1}", false),
        ("effects/stats/strength.toml", "Strength", true, "Quiet Strength +{1}", false),
        ("effects/stat-groups/charisma-skills.toml", "Charisma Skills", false, "Charisma Skills +{1}", false),
        ("effects/stat-groups/charisma-skills.toml", "Charisma Skills", true, "Quiet Skills +{1}", false),
    ] {
        let error = fixture::data_build_error(|root| {
            let path = root.join(file);
            let original = std::fs::read_to_string(&path).unwrap().replace("omits_bonus_type = true\n", "");
            let modified = if is_alias {
                format!("{original}\n[[alias]]\nname = 'Quiet Title'\nverbose_name_template = {template:?}\nomits_bonus_type = true\n")
            } else {
                let original = original
                    .lines()
                    .filter(|line| !line.starts_with("verbose_name_template ="))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("omits_bonus_type = true\nverbose_name_template = {template:?}\n{original}\n")
            };
            std::fs::write(path, modified).unwrap();
        });
        if valid {
            assert!(error.is_empty(), "{file}: {error}");
        } else {
            assert!(error.contains(file) && error.contains(name) && error.contains("omits_bonus_type"), "{error}");
        }
    }
}

#[test]
fn search_only_aliases_build_but_cannot_be_chosen_by_links() {
    for chosen in [false, true] {
        let error = fixture::data_build_error(|root| {
            let path = root.join("effects/stats/spot.toml");
            let original = std::fs::read_to_string(&path).unwrap();
            std::fs::write(path, format!("{original}\n[[alias]]\nname = 'Awareness'\n")).unwrap();
            if chosen {
                let path = root.join(SAGA_ITEM);
                let original = std::fs::read_to_string(&path).unwrap();
                std::fs::write(path, format!("{original}\n[[effect]]\nname = 'Spot'\nalias = 'Awareness'\nbonus_type = 'Competence'\nvalue = 3\n")).unwrap();
            }
        });
        if chosen {
            assert!(
                error.contains(SAGA_ITEM) && error.contains("Awareness") && error.contains("has no line template"),
                "{error}"
            );
        } else {
            assert_eq!(error, "");
        }
    }
}

#[test]
fn repeated_alias_names_identify_both_effects() {
    let error = fixture::data_build_error(|root| {
        for stat in ["search", "spot"] {
            let path = root.join(format!("effects/stats/{stat}.toml"));
            let original = std::fs::read_to_string(&path).unwrap();
            std::fs::write(
                path,
                format!("{original}\n[[alias]]\nname = 'Awareness'\nverbose_name_template = '%b1 Awareness +{{1}}'\n"),
            )
            .unwrap();
        }
    });
    assert!(error.contains("Search") && error.contains("Spot") && error.contains("Awareness"), "{error}");
}

#[test]
fn missing_aliases_name_the_file_for_every_link_owner() {
    for (file, array) in [
        (CHAIN_ITEM, "effect"),
        ("packs/shared/crafting/dinosaur-bone-crafting/augments/silverscale.toml", "effect"),
        ("characters/feats/other.toml", "row.effect"),
        ("packs/shared/sets/perfect-silence.toml", "tier.effect"),
        ("packs/shared/sockets/upgrade-tier-2/epic-ring-of-master-artifice.toml", "option.effect"),
    ] {
        let error = fixture::data_build_error(|root| {
            let path = root.join(file);
            let original = std::fs::read_to_string(&path).unwrap();
            let header = format!("[[{array}]]");
            assert!(original.contains(&header), "{file}");
            std::fs::write(path, original.replacen(&header, &format!("{header}\nalias = 'Missing Alias'"), 1)).unwrap();
        });
        assert!(error.contains(file) && error.contains("has no alias \"Missing Alias\""), "{file}: {error}");
    }
}

#[test]
fn removing_or_renaming_an_alias_requires_updating_its_links() {
    for replacement in ["", "[[alias]]\nname = 'New Awareness'\nverbose_name_template = '%b1 New Awareness +{1}'\n"] {
        let error = fixture::data_build_error(|root| {
            let stat_path = root.join("effects/stats/spot.toml");
            let original = std::fs::read_to_string(&stat_path).unwrap();
            std::fs::write(
                &stat_path,
                format!("{original}\n[[alias]]\nname = 'Awareness'\nverbose_name_template = '%b1 Awareness +{{1}}'\n"),
            )
            .unwrap();
            let item_path = root.join(SAGA_ITEM);
            let item = std::fs::read_to_string(&item_path).unwrap();
            std::fs::write(
                item_path,
                format!(
                    "{item}\n[[effect]]\nname = 'Spot'\nalias = 'Awareness'\nbonus_type = 'Competence'\nvalue = 3\n"
                ),
            )
            .unwrap();
            let mut db = rusqlite::Connection::open_in_memory().unwrap();
            ddo_etl::tree::build_from_data(root, &mut db).unwrap();
            std::fs::write(stat_path, format!("{original}\n{replacement}")).unwrap();
        });
        assert!(error.contains(SAGA_ITEM) && error.contains("has no alias \"Awareness\""), "{error}");
    }
}

#[test]
fn effect_aliases_validate_names_slugs_templates_and_link_ownership() {
    let group = "effects/stat-groups/doublestrike-and-doubleshot.toml";
    let check = |name: &str, template: &str, linked: &str, chosen: &str, extra: &str| {
        fixture::data_build_error(|root| {
            let path = root.join(group);
            let original = std::fs::read_to_string(&path).unwrap();
            std::fs::write(
                path,
                format!("{original}\n[[alias]]\nname = {name:?}\nverbose_name_template = {template:?}\n"),
            )
            .unwrap();
            let path = root.join(SAGA_ITEM);
            let original = std::fs::read_to_string(&path).unwrap();
            std::fs::write(path, format!("{original}\n[[effect]]\nname = {linked:?}\nalias = {chosen:?}\nbonus_type = 'Enhancement'\nvalue = 3\n")).unwrap();
            if !extra.is_empty() {
                let path = root.join("redirects.toml");
                let original = std::fs::read_to_string(&path).unwrap_or_default();
                std::fs::write(path, format!("{original}\n{extra}")).unwrap();
            }
        })
    };
    let target = "Doublestrike and Doubleshot";
    assert_eq!(check("Combat Grace", "%b1 Combat Grace +{1}", target, "Combat Grace", ""), "");
    assert_eq!(check("Doublestrike", "%b1 Doublestrike +{1}%", target, "Doublestrike", ""), "");
    let mut failures = Vec::new();
    for (name, template, linked, chosen, extra, expected) in [
        ("Strength", "%b1 Strength +{1}", target, "Strength", "", "collides with effect"),
        ("Doublestrike!", "%b1 Doublestrike! +{1}", target, "Doublestrike!", "", "slug"),
        ("Combat Grace", "%b1 Combat Grace +{1}", target, "Absent Alias", "", "Absent Alias"),
        ("Combat Grace", "%b1 Combat Grace +{1}", "Sheltering", "Combat Grace", "", "has no alias"),
        ("Combat Grace", "%b1 Combat Grace {1} %v1", target, "Combat Grace", "", "unconverted amount placeholder"),
        ("Combat Grace", "%b1 Combat Grace +{1}", target, "Combat Grace", "[[redirect]]\nkind = 'effects'\nfrom = 'combat-grace'\nto = 'doublestrike-and-doubleshot'\nsince = '2026-10-23'\n", "redirect"),
        ("Combat Grace", "%b1 Combat Grace +{1}", target, "Combat Grace", "[[retired]]\nkind = 'effects'\nslug = 'combat-grace'\nreason = 'Reserved old route'\nsince = '2026-10-23'\n", "retired"),
    ] {
        let error = check(name, template, linked, chosen, extra);
        if !error.contains(expected) {
            failures.push(format!("{name} / {linked} / {expected}: {error}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn stat_aliases_read_one_amount_without_a_canonical_template() {
    for (template, expected_error) in [("%b1 Rune Arm Focus +{1}", false), ("%b1 Rune Arm Focus +{1}/{2}", true)] {
        let error = fixture::data_build_error(|root| {
            std::fs::write(
                root.join("effects/stats/rune-arm-spell-focus.toml"),
                format!("name = 'Rune Arm Spell Focus'\ncategory = 'magical'\n[[alias]]\nname = 'Rune Arm Focus'\nverbose_name_template = {template:?}\n"),
            )
            .unwrap();
        });
        if expected_error {
            assert!(error.contains("Rune Arm Focus") && error.contains("amount_count 1"), "{error}");
        } else {
            assert!(error.is_empty(), "{error}");
        }
    }
}

#[test]
fn game_bug_targets_and_intended_values_are_checked() {
    let annotation = |field: &str, qualifier: &str, intended: &str| {
        format!(
        "\n[[game_bug_correction]]\nkind = 'item_bonus'\nfield = {field:?}\nqualifier = {qualifier:?}\nintended = {intended}\nreason = 'Observed in game.'\nin_game = true\nread = '2026-10-23'\n"
    )
    };
    let present = annotation("value", "Charisma / Penalty", "-2");
    let absent = annotation("remove", "Intelligence / Penalty", "-2");
    let check = |notes: &str| {
        fixture::data_build_error(|root| {
            let path = root.join(SAGA_ITEM);
            let contents = std::fs::read_to_string(&path).unwrap();
            std::fs::write(
                path,
                format!("{contents}\n[[effect]]\nname = 'Charisma'\nbonus_type = 'Penalty'\nvalue = -4\n{notes}"),
            )
            .unwrap();
        })
    };
    assert_eq!(check(&format!("{present}{absent}")), "");
    assert_eq!(check(&format!("{present}skip_description_check = true\n")), "");
    let typed = annotation("bonus_type", "Charisma / Penalty", "'Enhancement'");
    assert_eq!(check(&format!("{typed}skip_description_check = true\n")), "");
    for (notes, expected) in [
        (annotation("value", "Charisma / Penalty", "'two'"), "does not fit"),
        (annotation("value", "Intelligence / Penalty", "-2"), "target is absent"),
        (annotation("remove", "Charisma / Penalty", "-2"), "target is present"),
        (format!("{absent}skip_description_check = true\n"), "skip_description_check requires a served line"),
        (format!("{present}{present}"), "UNIQUE"),
    ] {
        let error = check(&notes);
        assert!(error.contains("alabaster-of-the-twelve.toml") && error.contains(expected), "{error}");
    }
}

#[test]
fn verification_requests_require_valid_targets_assumptions_and_sources() {
    let source = "\n[[verification_request.source]]\nin_game = true\nread = '2026-10-23'\nsays = 'The tooltip lists this amount.'\n";
    let request = |target: &str, assumed: &str| {
        format!("\n[[verification_request]]\n{target}\nquestion = 'Which value applies?'\nassumed = {assumed}\nraised = '2026-10-23'\n")
    };
    let valid = request("kind = 'item_bonus'\nfield = 'value'\nqualifier = 'Charisma / Penalty'", "-4");
    let whole = request("", "'As recorded'");
    let check = |notes: &str| {
        fixture::data_build_error(|root| {
            let path = root.join(SAGA_ITEM);
            let contents = std::fs::read_to_string(&path).unwrap();
            std::fs::write(
                path,
                format!("{contents}\n[[effect]]\nname = 'Charisma'\nbonus_type = 'Penalty'\nvalue = -4\n{notes}"),
            )
            .unwrap();
        })
    };
    assert_eq!(check(&format!("{valid}{source}{whole}{source}")), "");
    for (notes, expected) in [
        (format!("{}{source}", valid.replace("assumed = -4", "assumed = -2")), "assumed"),
        (format!("{}{source}", valid.replace("Charisma / Penalty", "Intelligence / Penalty")), "absent"),
        (valid.clone(), "source"),
        (format!("{valid}{source}{valid}{source}"), "UNIQUE"),
        (format!("{whole}{source}{whole}{source}"), "UNIQUE"),
        (format!("{}{source}", valid.replace("assumed = -4", "assumed = 'four'")), "does not fit"),
        (format!("{}{source}", valid.replace("kind = 'item_bonus'", "kind = 'feat_bonus'")), "does not address"),
        (format!("{}{source}", valid.replace("kind = 'item_bonus'\n", "")), "kind"),
    ] {
        let error = check(&notes);
        assert!(error.contains("alabaster-of-the-twelve.toml") && error.contains(expected), "{expected}: {error}");
    }
}

#[test]
fn a_citation_field_must_be_empty_or_a_field_of_its_record() {
    const ITEM: &str = "packs/shared/gear/alabaster-of-the-twelve.toml";
    let cite = |field: &'static str| {
        move |root: &std::path::Path| {
            let path = root.join(ITEM);
            let mut text = std::fs::read_to_string(&path).unwrap();
            text.push_str(&format!(
                "\n[[citation]]\nfield = {field:?}\nurl = \"https://ddowiki.com/page/Item:Test\"\nread = \"2026-10-18\"\n"
            ));
            std::fs::write(path, text).unwrap();
        }
    };
    for field in ["description", "minimum_level", "source", "socket"] {
        assert_eq!(fixture::data_build_error(cite(field)), "", "{field}");
    }
    for field in ["quest facts", "sources", "modifier.display_name", "nonsense"] {
        let message = fixture::data_build_error(cite(field));
        assert!(message.contains("alabaster-of-the-twelve.toml") && message.contains(field), "{field}: {message}");
    }
}

#[test]
fn a_file_named_off_its_slug_fails_the_build_naming_the_expected_slug() {
    let message = fixture::data_build_error(|root| {
        std::fs::rename(
            root.join("packs/shared/gear/alabaster-of-the-twelve.toml"),
            root.join("packs/shared/gear/alabaster-twelve.toml"),
        )
        .unwrap();
    });
    assert!(message.contains("alabaster-twelve.toml") && message.contains("alabaster-of-the-twelve"), "{message}");
}

#[test]
fn an_unresolved_reference_fails_the_build_naming_its_file() {
    let message = fixture::data_build_error(edited(CHAIN, "\"Redemption\"", "\"A Quest Nobody Wrote\""));
    assert!(message.contains("the-lost-seekers.toml"), "{message}");
    assert!(message.contains("unknown quest \"A Quest Nobody Wrote\", expected "), "{message}");
    assert!(message.contains("; nearest: "), "{message}");
}
