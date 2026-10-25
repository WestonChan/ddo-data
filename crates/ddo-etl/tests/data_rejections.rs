mod fixture;

const CHAIN: &str = "packs/free-to-play/chains/the-lost-seekers.toml";
const SAGA_ITEM: &str = "packs/shared/gear/alabaster-of-the-twelve.toml";
const CHAIN_ITEM: &str = "packs/free-to-play/gear/acrobats-ring.toml";
const CRAFTING: &str = "packs/shared/crafting/heroic-green-steel/crafting.toml";
const VENDOR_ITEM: &str = "packs/free-to-play/vendors/morten-edgewright/gear/ethereal-great-crossbow.toml";

fn edited(file: &'static str, from: &'static str, to: &'static str) -> impl Fn(&std::path::Path) {
    move |root| {
        let path = root.join(file);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(from), "{file} lacks {from:?}");
        std::fs::write(path, text.replacen(from, to, 1)).unwrap();
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
        ("an ingredient tier of mythic", CRAFTING, Box::new(edited(CRAFTING, "tier = \"heroic\"", "tier = \"mythic\""))),
        ("a recipe tier of mythic", CRAFTING, Box::new(edited(CRAFTING, "option = \"Minor Fire Guard\"\nslot = \"crafting: accessory invasion\"\ntier = \"heroic\"", "option = \"Minor Fire Guard\"\nslot = \"crafting: accessory invasion\"\ntier = \"mythic\""))),
        ("a cost naming an undeclared ingredient", CRAFTING, Box::new(edited(CRAFTING, "ingredient = \"Small Shard of Power\"", "ingredient = \"Undeclared Shard\""))),
        (
            "an ingredient declared twice",
            CRAFTING,
            Box::new(edited(CRAFTING, "[[ingredient]]\nname = \"Small Focus of Earth\"\ntier = \"heroic\"\n", "[[ingredient]]\nname = \"Small Focus of Earth\"\ntier = \"heroic\"\n\n[[ingredient]]\nname = \"Small Focus of Earth\"\ntier = \"heroic\"\n")),
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
        ("packs/shared/augments/dinosaurbone/silverscale.toml", "effect"),
        ("characters/feats/other.toml", "row.effect"),
        ("packs/shared/sets/perfect-silence.toml", "tier.effect"),
        ("packs/unsourced/gear/epic-ring-of-master-artifice.toml", "socket.option.effect"),
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
