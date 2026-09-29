#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AugmentSlotType {
    pub label: String,
    pub family: String,
    pub variant: String,
    pub qualifier: Option<String>,
}

const STANDARD_COLORS: &[&str] = &["Red", "Green", "Blue", "Orange", "Purple", "Colorless", "Yellow", "Sun", "Moon"];
const LAMORDIA_VARIANTS: &[&str] = &["Melancholic", "Dolorous", "Miserable", "Woeful"];

impl AugmentSlotType {
    pub fn parse(upstream_name: &str) -> Self {
        let upstream_name = upstream_name.trim();

        if STANDARD_COLORS.contains(&upstream_name) {
            let color = upstream_name.to_lowercase();
            return Self { label: color.clone(), family: "standard".into(), variant: color, qualifier: None };
        }

        if let Some(tier) = upstream_name.strip_prefix("Tier ").and_then(|n| n.parse::<u32>().ok()) {
            let variant = format!("tier {tier}");
            return Self { label: format!("upgrade: {variant}"), family: "upgrade".into(), variant, qualifier: None };
        }

        if let Some((head, tail)) = upstream_name.split_once(" Slot (") {
            if LAMORDIA_VARIANTS.contains(&head) {
                if let Some(qualifier) = tail.strip_suffix(')') {
                    let (variant, qualifier) = (head.to_lowercase(), qualifier.to_lowercase());
                    return Self {
                        label: format!("lamordia: {variant} ({qualifier})"),
                        family: "lamordia".into(),
                        variant,
                        qualifier: Some(qualifier),
                    };
                }
            }
        }

        if let Some(rest) = upstream_name.strip_prefix("IoD: ") {
            if rest == "Set Bonus Slot" {
                return Self {
                    label: "isle of dread: set bonus".into(),
                    family: "dino".into(),
                    variant: "set".into(),
                    qualifier: None,
                };
            }
            if let Some((qualifier, part)) = rest.split_once(": ") {
                if let Some(variant) = part.strip_suffix(" Slot") {
                    let (variant, qualifier) = (variant.to_lowercase(), qualifier.to_lowercase());
                    return Self {
                        label: format!("isle of dread: {variant} ({qualifier})"),
                        family: "dino".into(),
                        variant,
                        qualifier: Some(qualifier),
                    };
                }
            }
        }

        let variant = upstream_name.to_lowercase();
        Self { label: format!("crafting: {variant}"), family: "crafting".into(), variant, qualifier: None }
    }
}
