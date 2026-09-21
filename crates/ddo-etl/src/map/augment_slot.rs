//! `<ItemAugment><Type>` → an `augment_slot_types` row. Labels follow the legacy database's
//! conventions (`lamordia: melancholic (accessory)`, `isle of dread: scale (weapon)`) so the
//! frontend's existing display keeps working.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotSpec {
    pub label: String,
    pub family: String,
    pub variant: String,
    pub qualifier: Option<String>,
}

const COLOURS: &[&str] = &["Red", "Green", "Blue", "Orange", "Purple", "Colorless", "Yellow", "Sun", "Moon"];
const LAMORDIA: &[&str] = &["Melancholic", "Dolorous", "Miserable", "Woeful"];

pub fn decode(raw: &str) -> SlotSpec {
    let raw = raw.trim();

    if COLOURS.contains(&raw) {
        let v = raw.to_lowercase();
        return SlotSpec { label: v.clone(), family: "standard".into(), variant: v, qualifier: None };
    }

    if let Some(n) = raw.strip_prefix("Tier ").and_then(|n| n.parse::<u32>().ok()) {
        let v = format!("tier {n}");
        return SlotSpec { label: format!("upgrade: {v}"), family: "upgrade".into(), variant: v, qualifier: None };
    }

    // "Melancholic Slot (Accessory)"
    if let Some((head, tail)) = raw.split_once(" Slot (") {
        if LAMORDIA.contains(&head) {
            if let Some(q) = tail.strip_suffix(')') {
                let (v, q) = (head.to_lowercase(), q.to_lowercase());
                return SlotSpec {
                    label: format!("lamordia: {v} ({q})"),
                    family: "lamordia".into(),
                    variant: v,
                    qualifier: Some(q),
                };
            }
        }
    }

    // "IoD: Weapon: Scale Slot" / "IoD: Set Bonus Slot"
    if let Some(rest) = raw.strip_prefix("IoD: ") {
        if rest == "Set Bonus Slot" {
            return SlotSpec {
                label: "isle of dread: set bonus".into(),
                family: "dino".into(),
                variant: "set".into(),
                qualifier: None,
            };
        }
        if let Some((q, part)) = rest.split_once(": ") {
            if let Some(v) = part.strip_suffix(" Slot") {
                let (v, q) = (v.to_lowercase(), q.to_lowercase());
                return SlotSpec {
                    label: format!("isle of dread: {v} ({q})"),
                    family: "dino".into(),
                    variant: v,
                    qualifier: Some(q),
                };
            }
        }
    }

    let v = raw.to_lowercase();
    SlotSpec { label: format!("crafting: {v}"), family: "crafting".into(), variant: v, qualifier: None }
}
