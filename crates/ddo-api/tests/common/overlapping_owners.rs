use std::path::{Path, PathBuf};

pub(crate) fn write_overlapping_enhancements(root: &Path, true_owner: &str) -> PathBuf {
    let tree_path = root.join("characters/enhancements/kensei/tree.toml");
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
