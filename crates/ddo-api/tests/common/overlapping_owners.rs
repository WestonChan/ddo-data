use std::path::{Path, PathBuf};

pub(crate) fn write_overlapping_owners(root: &Path, true_owner: &str) -> PathBuf {
    for owner in ["foo", "foo-bar"] {
        let path = root.join(format!("packs/shared/crafting/{owner}/crafting.toml"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let ingredient = if owner == true_owner {
            format!(
                "\n[[ingredient]]\nname = {:?}\ntier = \"heroic\"\n",
                if owner == "foo" { "Bar Baz" } else { "Baz" }
            )
        } else {
            String::new()
        };
        std::fs::write(path, format!("name = {owner:?}\n[[citation]]\nurl = \"https://ddowiki.com/page/Foo\"\nread = \"2026-10-01\"\n{ingredient}")).unwrap();
    }
    let tree_path = walkdir::WalkDir::new(root.join("characters/enhancements"))
        .into_iter()
        .map(Result::unwrap)
        .find(|entry| {
            entry.file_name() == "tree.toml"
                && std::fs::read_to_string(entry.path()).unwrap().contains("[[enhancement.selection]]")
        })
        .unwrap()
        .into_path();
    let mut tree: toml::Value = toml::from_str(&std::fs::read_to_string(&tree_path).unwrap()).unwrap();
    let enhancements = tree["enhancement"].as_array_mut().unwrap();
    let template = enhancements.iter().find(|enhancement| enhancement.get("selection").is_some()).unwrap().clone();
    for owner in ["foo", "foo-bar"] {
        let mut enhancement = template.clone();
        enhancement["name"] = owner.into();
        enhancement["internal_name"] = owner.into();
        enhancement.as_table_mut().unwrap().remove("variant");
        enhancement.as_table_mut().unwrap().remove("selector_exclusion");
        let selections = enhancement["selection"].as_array_mut().unwrap();
        selections.truncate(1);
        if owner == true_owner {
            selections[0]["name"] = if owner == "foo" { "Bar Baz" } else { "Baz" }.into();
        } else {
            selections.clear();
        }
        enhancements.push(enhancement);
    }
    std::fs::write(&tree_path, toml::to_string_pretty(&tree).unwrap()).unwrap();
    tree_path
}
