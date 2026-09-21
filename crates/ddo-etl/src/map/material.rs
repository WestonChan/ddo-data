//! `<Material>` carries wiki artefacts upstream never cleaned: "Gem (material)",
//! "Category:No Material items", "Feysteel (page does not exist)". Normalise to a plain name or
//! nothing.

pub fn normalize(raw: Option<&str>) -> Option<String> {
    let raw = raw?.trim();
    if raw.is_empty()
        || raw.starts_with("Category:")
        || raw.starts_with("Special:")
        || raw.contains("(page does not exist)")
        || matches!(raw, "Enhancement bonus" | "Unknown Material" | "Sentient Weapon")
    {
        return None;
    }
    Some(raw.strip_suffix(" (material)").unwrap_or(raw).to_string())
}
