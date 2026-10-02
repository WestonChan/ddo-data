pub fn names_legacy_version(item_name: &str) -> bool {
    let lowercase_name = item_name.to_lowercase();
    ["(legacy)", "(historic)"].iter().any(|marker| lowercase_name.contains(marker))
}
