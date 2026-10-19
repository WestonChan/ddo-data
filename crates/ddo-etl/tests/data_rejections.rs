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
