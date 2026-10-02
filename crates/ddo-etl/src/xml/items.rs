use super::effect::Effect;
use super::requirements::Requirements;
use super::{Dice, EmptyElement};
use anyhow::{bail, Result};
use serde::de::IgnoredAny;
use serde::Deserialize;
use std::path::Path;

pub fn parse_item_file(path: &Path) -> Result<ItemFile> {
    let raw_file: RawItemFile = super::parse_xml_file(path)?;
    let items = raw_file.items.into_iter().map(Item::try_from).collect::<Result<Vec<_>>>()?;
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

#[allow(clippy::large_enum_variant)]
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
    ItemAugment(ItemAugmentSlot),
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
    IsAcceptsSentience(EmptyElement),
    MinorArtifact(EmptyElement),
    IsGreensteel(EmptyElement),
    NoAutoUpdate(EmptyElement),
    UserSetsLevel(EmptyElement),
    Effect(Effect),
    RestrictedSlots(IgnoredAny),
    SlotUpgrade(IgnoredAny),
}

#[derive(Debug, Default)]
pub struct Item {
    pub name: String,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub drop_location: Option<String>,
    pub minimum_level: Option<i64>,
    pub equipment_slots: EquipmentSlots,
    pub weapon: Option<String>,
    pub armor: Option<String>,
    pub attack_modifiers: Vec<String>,
    pub damage_modifiers: Vec<String>,
    pub dr_bypasses: Vec<String>,
    pub damage_multiplier: Option<f64>,
    pub base_dice: Option<Dice>,
    pub critical_multiplier: Option<i64>,
    pub critical_threat_range: Option<i64>,
    pub material: Option<String>,
    pub buffs: Vec<Buff>,
    pub augment_slots: Vec<ItemAugmentSlot>,
    pub set_bonus_names: Vec<String>,
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
    pub is_minor_artifact: bool,
    pub is_greensteel: bool,
    pub effects: Vec<Effect>,
}

impl TryFrom<RawItem> for Item {
    type Error = anyhow::Error;

    fn try_from(raw_item: RawItem) -> Result<Self> {
        let mut item = Item::default();
        let mut has_name = false;
        for child in raw_item.children {
            match child {
                ItemChild::Name(v) => {
                    item.name = v.trim().to_string();
                    has_name = true;
                }
                ItemChild::Icon(v) => item.icon = Some(v.trim().to_string()),
                ItemChild::Description(v) => item.description = Some(v),
                ItemChild::DropLocation(v) => item.drop_location = Some(v),
                ItemChild::MinLevel(v) => item.minimum_level = Some(v),
                ItemChild::EquipmentSlot(v) => item.equipment_slots = v,
                ItemChild::Weapon(v) => item.weapon = Some(v.trim().to_string()),
                ItemChild::Armor(v) => item.armor = Some(v.trim().to_string()),
                ItemChild::AttackModifier(v) => item.attack_modifiers.push(v),
                ItemChild::DamageModifier(v) => item.damage_modifiers.push(v),
                ItemChild::DrBypass(v) => item.dr_bypasses.push(v),
                ItemChild::WeaponDamage(v) => item.damage_multiplier = Some(v),
                ItemChild::BaseDice(v) => item.base_dice = Some(v),
                ItemChild::CriticalMultiplier(v) => item.critical_multiplier = Some(v),
                ItemChild::CriticalThreatRange(v) => item.critical_threat_range = Some(v),
                ItemChild::Material(v) => item.material = Some(v),
                ItemChild::Buff(v) => item.buffs.push(v),
                ItemChild::ItemAugment(v) => item.augment_slots.push(v),
                ItemChild::SetBonus(v) => item.set_bonus_names.push(v),
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
                ItemChild::MinorArtifact(_) => item.is_minor_artifact = true,
                ItemChild::IsGreensteel(_) => item.is_greensteel = true,
                ItemChild::Effect(v) => item.effects.push(v),
                ItemChild::NoAutoUpdate(_)
                | ItemChild::UserSetsLevel(_)
                | ItemChild::RestrictedSlots(_)
                | ItemChild::SlotUpgrade(_) => {}
            }
        }
        if !has_name {
            bail!("<Item> without a <Name>");
        }
        Ok(item)
    }
}

#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
pub struct EquipmentSlots {
    #[serde(rename = "$value", default)]
    pub tags: Vec<EquipmentSlotTag>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum EquipmentSlotTag {
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

impl EquipmentSlotTag {
    pub const fn is_cosmetic(self) -> bool {
        matches!(self, Self::CosmeticHelm | Self::CosmeticCloak | Self::CosmeticArmor | Self::CosmeticWeapon1)
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Buff {
    #[serde(rename = "Type")]
    pub kind: String,
    #[serde(rename = "Item")]
    pub target: Option<String>,
    #[serde(rename = "Item2")]
    pub second_target: Option<String>,
    #[serde(rename = "Value1")]
    pub value: Option<i64>,
    #[serde(rename = "Value2")]
    pub second_value: Option<i64>,
    #[serde(rename = "BonusType")]
    pub bonus_type: Option<String>,
    #[serde(rename = "Description1")]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ItemAugmentSlot {
    #[serde(rename = "Type")]
    pub kind: String,
    #[serde(rename = "Augment", default)]
    pub options: Vec<AugmentSlotOption>,
}

#[derive(Debug, Default)]
pub struct AugmentSlotOption {
    pub name: String,
    pub description: String,
    pub minimum_level: Option<i64>,
    pub icon: Option<String>,
    pub granted_augments: Vec<String>,
    pub set_bonus_names: Vec<String>,
    pub effects: Vec<Effect>,
}

#[derive(Debug, Deserialize)]
struct RawAugmentSlotOption {
    #[serde(rename = "$value", default)]
    children: Vec<AugmentSlotOptionChild>,
}

#[derive(Debug, Deserialize)]
enum AugmentSlotOptionChild {
    Name(String),
    Description(String),
    MinLevel(i64),
    Icon(String),
    GrantAugment(String),
    AddAugment(String),
    SetBonus(String),
    Effect(Box<Effect>),
    Type(IgnoredAny),
}

impl<'de> Deserialize<'de> for AugmentSlotOption {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let raw_option = RawAugmentSlotOption::deserialize(deserializer)?;
        let mut option = AugmentSlotOption::default();
        let mut has_name = false;
        for child in raw_option.children {
            match child {
                AugmentSlotOptionChild::Name(name) => {
                    option.name = name;
                    has_name = true;
                }
                AugmentSlotOptionChild::Description(description) => option.description = description,
                AugmentSlotOptionChild::MinLevel(minimum_level) => option.minimum_level = Some(minimum_level),
                AugmentSlotOptionChild::Icon(icon) => option.icon = Some(icon),
                AugmentSlotOptionChild::GrantAugment(slot_type_name)
                | AugmentSlotOptionChild::AddAugment(slot_type_name) => option.granted_augments.push(slot_type_name),
                AugmentSlotOptionChild::SetBonus(set_name) => option.set_bonus_names.push(set_name),
                AugmentSlotOptionChild::Effect(effect) => option.effects.push(*effect),
                AugmentSlotOptionChild::Type(_) => {}
            }
        }
        if !has_name {
            return Err(serde::de::Error::missing_field("Name"));
        }
        Ok(option)
    }
}

impl Item {
    pub fn race_required(&self) -> Option<String> {
        let requirements = self.requirements.as_ref()?;
        let required_races: Vec<String> = requirements
            .requirements_to_meet()
            .filter_map(|r| match r.kind.as_str() {
                "Race" => r.items.first().cloned(),
                "RaceConstruct" => Some("Construct".to_string()),
                _ => None,
            })
            .collect();
        if required_races.is_empty() {
            None
        } else {
            Some(required_races.join(", "))
        }
    }
}
