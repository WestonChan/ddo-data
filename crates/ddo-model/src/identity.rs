pub fn slug(name: &str) -> String {
    let mut slug = String::new();
    let mut needs_separator = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            if needs_separator && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(character.to_ascii_lowercase());
            needs_separator = false;
        } else if !matches!(character, '\'' | '’') {
            needs_separator = true;
        }
    }
    slug
}

pub fn nested_slug(owner_slug: &str, name: &str) -> String {
    if owner_slug.is_empty() {
        slug(name)
    } else {
        format!("{owner_slug}-{}", slug(name))
    }
}
