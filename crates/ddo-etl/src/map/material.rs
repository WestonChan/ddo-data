pub fn normalized_name(upstream_name: Option<&str>) -> Option<String> {
    let upstream_name = upstream_name?.trim();
    if upstream_name.is_empty()
        || upstream_name.starts_with("Category:")
        || upstream_name.starts_with("Special:")
        || upstream_name.contains("(page does not exist)")
        || matches!(upstream_name, "Enhancement bonus" | "Unknown Material" | "Sentient Weapon")
    {
        return None;
    }
    Some(upstream_name.strip_suffix(" (material)").unwrap_or(upstream_name).to_string())
}
