//! `Items/*.item`: one `<Items><Item>…</Item></Items>` per file.
//!
//! An `<Item>` is read as a sequence of child elements ([`ItemChild`]) and then folded into
//! [`Item`]. Reading children as a sequence rather than as struct fields makes element order
//! irrelevant, which matters because upstream occasionally interleaves repeated elements
//! (`<Buff>`, `<Effect>`, `<Buff>`), and serde's field-based collection only accepts adjacent
//! repeats. An element with no [`ItemChild`] variant is still a hard error: the 2026-09-20 survey
//! enumerated every element the 8,779 upstream files use, and a new one should stop the build so
//! a person decides whether it maps to a column.

use super::Empty;
use anyhow::{bail, Result};
use serde::de::IgnoredAny;
use serde::Deserialize;
use std::path::Path;

pub fn parse_item_file(path: &Path) -> Result<ItemFile> {
    let raw: RawItemFile = super::read_xml(path)?;
    let items = raw.items.into_iter().map(Item::try_from).collect::<Result<Vec<_>>>()?;
    Ok(ItemFile { items })
}

#[derive(Debug)]
pub struct ItemFile {
    pub items: Vec<Item>,
}

#[derive(Debug, Deserialize)]
#[serde(rename = "Items")]
struct RawItemFile {
    #[serde(rename = "Item", default)]
    items: Vec<RawItem>,
}

#[derive(Debug, Deserialize)]
struct RawItem {
    #[serde(rename = "$value", default)]
    children: Vec<ItemChild>,
}

/// Every element an `<Item>` may contain, named exactly as upstream spells it.
#[derive(Debug, Deserialize)]
enum ItemChild {
    Name(String),
    Icon(String),
    Description(String),
    DropLocation(String),
    MinLevel(i64),
    EquipmentSlot(EquipmentSlots),
    Weapon(String),
    Armor(String),
    AttackModifier(String),
    DamageModifier(String),
    #[serde(rename = "DRBypass")]
    DrBypass(String),
    WeaponDamage(f64),
    BaseDice(Dice),
    CriticalMultiplier(i64),
    CriticalThreatRange(i64),
    Material(String),
    Buff(Buff),
    ItemAugment(ItemAugment),
    SetBonus(String),
    Requirements(Requirements),
    ArmorBonus(i64),
    MaximumDexterityBonus(i64),
    ArcaneSpellFailure(i64),
    ArmorCheckPenalty(i64),
    ShieldBonus(i64),
    DamageReduction(i64),
    MithralBody(i64),
    AdamantineBody(i64),
    IsAcceptsSentience(Empty),
    MinorArtifact(Empty),
    IsGreensteel(Empty),
    // Upstream bookkeeping and rarities the schema does not model yet.
    NoAutoUpdate(Empty),
    UserSetsLevel(Empty),
    Effect(IgnoredAny),
    RestrictedSlots(IgnoredAny),
    SlotUpgrade(IgnoredAny),
}

#[derive(Debug, Default)]
pub struct Item {
    pub name: String,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub drop_location: Option<String>,
    pub min_level: Option<i64>,
    pub equipment_slot: EquipmentSlots,
    pub weapon: Option<String>,
    pub armor: Option<String>,
    pub attack_modifier: Vec<String>,
    pub damage_modifier: Vec<String>,
    pub dr_bypass: Vec<String>,
    pub weapon_damage: Option<f64>,
    pub base_dice: Option<Dice>,
    pub critical_multiplier: Option<i64>,
    pub critical_threat_range: Option<i64>,
    pub material: Option<String>,
    pub buffs: Vec<Buff>,
    pub augments: Vec<ItemAugment>,
    pub set_bonus: Vec<String>,
    pub requirements: Option<Requirements>,
    pub armor_bonus: Option<i64>,
    pub maximum_dexterity_bonus: Option<i64>,
    pub arcane_spell_failure: Option<i64>,
    pub armor_check_penalty: Option<i64>,
    pub shield_bonus: Option<i64>,
    pub damage_reduction: Option<i64>,
    pub mithral_body: Option<i64>,
    pub adamantine_body: Option<i64>,
    pub accepts_sentience: bool,
    pub minor_artifact: bool,
    pub is_greensteel: bool,
}

impl TryFrom<RawItem> for Item {
    type Error = anyhow::Error;

    fn try_from(raw: RawItem) -> Result<Self> {
        let mut item = Item::default();
        let mut named = false;
        for child in raw.children {
            match child {
                ItemChild::Name(v) => {
                    item.name = v.trim().to_string();
                    named = true;
                }
                ItemChild::Icon(v) => item.icon = Some(v.trim().to_string()),
                ItemChild::Description(v) => item.description = Some(v),
                ItemChild::DropLocation(v) => item.drop_location = Some(v),
                ItemChild::MinLevel(v) => item.min_level = Some(v),
                ItemChild::EquipmentSlot(v) => item.equipment_slot = v,
                ItemChild::Weapon(v) => item.weapon = Some(v.trim().to_string()),
                ItemChild::Armor(v) => item.armor = Some(v.trim().to_string()),
                ItemChild::AttackModifier(v) => item.attack_modifier.push(v),
                ItemChild::DamageModifier(v) => item.damage_modifier.push(v),
                ItemChild::DrBypass(v) => item.dr_bypass.push(v),
                ItemChild::WeaponDamage(v) => item.weapon_damage = Some(v),
                ItemChild::BaseDice(v) => item.base_dice = Some(v),
                ItemChild::CriticalMultiplier(v) => item.critical_multiplier = Some(v),
                ItemChild::CriticalThreatRange(v) => item.critical_threat_range = Some(v),
                ItemChild::Material(v) => item.material = Some(v),
                ItemChild::Buff(v) => item.buffs.push(v),
                ItemChild::ItemAugment(v) => item.augments.push(v),
                ItemChild::SetBonus(v) => item.set_bonus.push(v),
                ItemChild::Requirements(v) => item.requirements = Some(v),
                ItemChild::ArmorBonus(v) => item.armor_bonus = Some(v),
                ItemChild::MaximumDexterityBonus(v) => item.maximum_dexterity_bonus = Some(v),
                ItemChild::ArcaneSpellFailure(v) => item.arcane_spell_failure = Some(v),
                ItemChild::ArmorCheckPenalty(v) => item.armor_check_penalty = Some(v),
                ItemChild::ShieldBonus(v) => item.shield_bonus = Some(v),
                ItemChild::DamageReduction(v) => item.damage_reduction = Some(v),
                ItemChild::MithralBody(v) => item.mithral_body = Some(v),
                ItemChild::AdamantineBody(v) => item.adamantine_body = Some(v),
                ItemChild::IsAcceptsSentience(_) => item.accepts_sentience = true,
                ItemChild::MinorArtifact(_) => item.minor_artifact = true,
                ItemChild::IsGreensteel(_) => item.is_greensteel = true,
                ItemChild::NoAutoUpdate(_)
                | ItemChild::UserSetsLevel(_)
                | ItemChild::Effect(_)
                | ItemChild::RestrictedSlots(_)
                | ItemChild::SlotUpgrade(_) => {}
            }
        }
        if !named {
            bail!("<Item> without a <Name>");
        }
        Ok(item)
    }
}

/// The slot tags a `<EquipmentSlot>` element lists. Upstream uses one empty child element per
/// slot the item can occupy, so a one-handed weapon lists both `Weapon1` and `Weapon2`.
#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
pub struct EquipmentSlots {
    #[serde(rename = "$value", default)]
    pub tags: Vec<SlotTag>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum SlotTag {
    Weapon1,
    Weapon2,
    Armor,
    Ring,
    Helmet,
    Cloak,
    Trinket,
    Necklace,
    Bracers,
    Gloves,
    Belt,
    Boots,
    Goggles,
    Quiver,
    CosmeticHelm,
    CosmeticCloak,
    CosmeticArmor,
    CosmeticWeapon1,
}

impl SlotTag {
    pub const fn is_cosmetic(self) -> bool {
        matches!(self, Self::CosmeticHelm | Self::CosmeticCloak | Self::CosmeticArmor | Self::CosmeticWeapon1)
    }
}

/// `<BaseDice>`; shields carry an empty element, so every part is optional.
#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct Dice {
    #[serde(rename = "Number")]
    pub number: Option<i64>,
    #[serde(rename = "Sides")]
    pub sides: Option<i64>,
    #[serde(rename = "Bonus")]
    pub bonus: Option<i64>,
}

/// One property of an item. `kind` is upstream's `<Type>`; the display template for it lives in
/// `ItemBuffs.xml`, keyed by the same string.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Buff {
    #[serde(rename = "Type")]
    pub kind: String,
    #[serde(rename = "Item")]
    pub item: Option<String>,
    #[serde(rename = "Item2")]
    pub item2: Option<String>,
    #[serde(rename = "Value1")]
    pub value1: Option<i64>,
    #[serde(rename = "Value2")]
    pub value2: Option<i64>,
    #[serde(rename = "BonusType")]
    pub bonus_type: Option<String>,
    #[serde(rename = "Description1")]
    pub description1: Option<String>,
}

/// A socket on an item. `options` lists the content upstream has fixed for it: one entry is a
/// crafted upgrade already applied ("Tier 2: Seeker 11 and a Green augment slot"), several are the
/// choices a crafting step offers ("Commendation Upgrade" with fourteen blessings). Empty means an
/// ordinary open socket.
#[derive(Debug, Deserialize)]
pub struct ItemAugment {
    #[serde(rename = "Type")]
    pub kind: String,
    #[serde(rename = "Augment", default)]
    pub options: Vec<AugmentOption>,
}

#[derive(Debug, Deserialize)]
pub struct AugmentOption {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Description", default)]
    pub description: String,
    #[serde(rename = "MinLevel")]
    pub min_level: Option<i64>,
    #[serde(rename = "Icon")]
    pub icon: Option<String>,
    #[serde(rename = "GrantAugment", default)]
    pub grant_augment: Vec<String>,
    #[serde(rename = "SetBonus", default)]
    pub set_bonus: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Requirements {
    #[serde(rename = "Requirement", default)]
    pub requirement: Vec<Requirement>,
    #[serde(rename = "RequiresOneOf", default)]
    pub requires_one_of: Vec<RequirementGroup>,
    #[serde(rename = "RequiresNoneOf", default)]
    pub requires_none_of: Vec<RequirementGroup>,
}

#[derive(Debug, Deserialize)]
pub struct RequirementGroup {
    #[serde(rename = "Requirement", default)]
    pub requirement: Vec<Requirement>,
}

#[derive(Debug, Deserialize)]
pub struct Requirement {
    #[serde(rename = "Type")]
    pub kind: String,
    #[serde(rename = "Item")]
    pub item: Option<String>,
    #[serde(rename = "Value")]
    pub value: Option<String>,
}

impl Item {
    /// The race restriction, if any: `Race`/`Item` requirements joined, `RaceConstruct` as
    /// "Construct". Restrictions (`NotConstruct`) and feat requirements are not race requirements.
    pub fn race_required(&self) -> Option<String> {
        let reqs = self.requirements.as_ref()?;
        let all = reqs.requirement.iter().chain(reqs.requires_one_of.iter().flat_map(|g| g.requirement.iter()));
        let races: Vec<String> = all
            .filter_map(|r| match r.kind.as_str() {
                "Race" => r.item.clone(),
                "RaceConstruct" => Some("Construct".to_string()),
                _ => None,
            })
            .collect();
        if races.is_empty() {
            None
        } else {
            Some(races.join(", "))
        }
    }
}
