use super::*;
use crate::tree::DataFilePaths;
use anyhow::Result;
use rusqlite::{Connection, Row};
use std::collections::BTreeMap;

fn rows<T>(db: &Connection, sql: &str, read: impl FnMut(&Row<'_>) -> rusqlite::Result<T>) -> Result<Vec<T>> {
    Ok(db.prepare(sql)?.query_map([], read)?.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn groups<T>(
    db: &Connection,
    sql: &str,
    read: impl FnMut(&Row<'_>) -> rusqlite::Result<(i64, T)>,
) -> Result<BTreeMap<i64, Vec<T>>> {
    let mut grouped = BTreeMap::<i64, Vec<T>>::new();
    for (owner, value) in rows(db, sql, read)? {
        grouped.entry(owner).or_default().push(value);
    }
    Ok(grouped)
}

impl WikiOverrides {
    pub fn from_database(db: &Connection) -> Result<Self> {
        let paths = DataFilePaths::from_database(db)?;
        Ok(Self {
            quests: quests(db, &paths)?,
            quest_loot: quest_loot(db, &paths)?,
            descriptions: descriptions(db, &paths)?,
            crafting_systems: crafting_systems(db, &paths)?,
            quest_chains: quest_chains(db, &paths)?,
            sagas: sagas(db, &paths)?,
            vendors: vendors(db, &paths)?,
            events: events(db, &paths)?,
            items: items(db, &paths)?,
            augments: augments(db, &paths)?,
        })
    }
}

fn quests(db: &Connection, paths: &DataFilePaths) -> Result<Vec<WikiQuest>> {
    rows(db, "SELECT q.*, p.name AS pack, patron.name AS patron FROM quests q LEFT JOIN adventure_packs p ON p.id=q.pack_id LEFT JOIN patrons patron ON patron.id=q.patron_id WHERE q.provenance='wiki' OR EXISTS (SELECT 1 FROM citations c WHERE c.owner_kind='quests' AND c.owner_id=q.id AND c.field='quest facts') ORDER BY q.id", |row| {
        let wiki_only = row.get::<_, String>("provenance")? == "wiki";
        Ok((row.get::<_, i64>("id")?, WikiQuest {
            name: row.get("name")?, free_to_play: row.get("is_free_to_play")?,
            legendary_level: row.get("legendary_level")?, zone: row.get("zone")?, bestowed_by: row.get("bestowed_by")?, flagging: row.get("flagging")?,
            pack: if wiki_only { row.get("pack")? } else { None },
            patron: if wiki_only { row.get("patron")? } else { None },
            level: if wiki_only { row.get("level")? } else { None },
            epic_level: if wiki_only { row.get("epic_level")? } else { None },
            favor: if wiki_only { row.get("favor")? } else { None },
            is_raid: if wiki_only { Some(row.get("is_raid")?) } else { None },
            difficulties: if wiki_only { Some(serde_json::from_str(&row.get::<_, String>("difficulties")?).map_err(|error| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error)))?) } else { None },
            file_name: String::new(),
        }))
    })?.into_iter().map(|(id, mut quest)| { quest.file_name=paths.path("quests",id)?.display().to_string(); Ok(quest) }).collect()
}

fn quest_loot(db: &Connection, paths: &DataFilePaths) -> Result<Vec<QuestLoot>> {
    let mut quests = BTreeMap::new();
    for (id, name, owner_kind, owner_name, loot_type, chest, rare, listed) in rows(db,
        "SELECT q.id,q.name,d.owner_kind,COALESCE(i.name,a.name),d.loot_type,d.chest,d.is_rare,(d.drop_text_segment IS NULL OR COALESCE(i.provenance, a.provenance) = 'wiki') FROM quest_drops d JOIN quests q ON q.id=d.quest_id LEFT JOIN items i ON d.owner_kind='item' AND i.id=d.owner_id LEFT JOIN augments a ON d.owner_kind='augment' AND a.id=d.owner_id WHERE d.is_rare=1 OR (d.drop_text_segment IS NULL OR COALESCE(i.provenance, a.provenance) = 'wiki') ORDER BY q.id,d.id",
        |r| Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,Option<String>>(5)?,r.get::<_,bool>(6)?,r.get::<_,bool>(7)?)))? {
        let quest = quests.entry(id).or_insert_with(|| QuestLoot { name, rare:Vec::new(),rare_augments:Vec::new(),items:Vec::new(),augments:Vec::new(),file_name:String::new() });
        quest.file_name=paths.path("quests",id)?.display().to_string();
        if listed {
            let drop = ListedDrop::Described(DescribedListedDrop { name:owner_name.clone(),loot_type:Some(loot_type),chest:chest.clone() });
            if owner_kind=="item" { quest.items.push(drop); } else { quest.augments.push(drop); }
        }
        if rare {
            let drop = match chest { Some(chest)=>RareDrop::InChest(RareDropInChest {name:owner_name,chest}), None=>RareDrop::Named(owner_name) };
            if owner_kind=="item" {quest.rare.push(drop);} else {quest.rare_augments.push(drop);}
        }
    }
    Ok(quests.into_values().collect())
}

fn descriptions(db: &Connection, paths: &DataFilePaths) -> Result<Vec<WikiDescription>> {
    let mut descriptions = Vec::new();
    for kind in DescriptionKind::ALL {
        let table = kind.table_name();
        for (id,name,description) in rows(db,&format!("SELECT id,name,description FROM {table} e WHERE EXISTS (SELECT 1 FROM citations c WHERE c.owner_kind='{table}' AND c.owner_id=e.id AND c.field='description') AND COALESCE(description,'')<>'' ORDER BY id"),|r| Ok((r.get::<_,i64>(0)?,r.get(1)?,r.get(2)?)))? {
            descriptions.push(WikiDescription {kind,name,description,file_name:paths.path(table,id)?.display().to_string()});
        }
    }
    Ok(descriptions)
}

fn quest_chains(db: &Connection, paths: &DataFilePaths) -> Result<Vec<WikiQuestChain>> {
    let mut quests=groups(db,"SELECT l.chain_id,q.name FROM quest_chain_quests l JOIN quests q ON q.id=l.quest_id ORDER BY l.chain_id,l.sort_order",|r| Ok((r.get(0)?,r.get::<_,String>(1)?)))?;
    let mut rewards=groups(db,"SELECT l.chain_id,i.name,l.is_rare FROM quest_chain_rewards l JOIN items i ON l.owner_kind='item' AND i.id=l.owner_id ORDER BY l.chain_id,l.id",|r| Ok((r.get(0)?,QuestChainReward::Described(DescribedQuestChainReward {name:r.get(1)?,rare:r.get(2)?}))))?;
    rows(db,"SELECT s.id,s.name,s.wiki_url,p.name FROM quest_chains s LEFT JOIN adventure_packs p ON p.id=s.pack_id ORDER BY s.id",|r|Ok((r.get::<_,i64>(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?.into_iter().map(|(id,name,wiki_url,pack)| Ok(WikiQuestChain {name,wiki_url,pack,quests:quests.remove(&id).unwrap_or_default(),rewards:rewards.remove(&id).unwrap_or_default(),file_name:paths.path("quest_chains",id)?.display().to_string()})).collect()
}

fn sagas(db: &Connection, paths: &DataFilePaths) -> Result<Vec<WikiSaga>> {
    let mut quests = groups(
        db,
        "SELECT l.saga_id,q.name FROM saga_quests l JOIN quests q ON q.id=l.quest_id ORDER BY l.saga_id,l.sort_order",
        |r| Ok((r.get(0)?, r.get::<_, String>(1)?)),
    )?;
    let mut rewards=groups(db,"SELECT l.saga_id,i.name,l.is_rare,l.tier FROM saga_rewards l JOIN items i ON l.owner_kind='item' AND i.id=l.owner_id ORDER BY l.saga_id,l.id",|r| Ok((r.get(0)?,SagaReward::Described(DescribedSagaReward {name:r.get(1)?,rare:r.get(2)?,tier:r.get(3)?}))))?;
    rows(
        db,
        "SELECT s.id,s.name,s.wiki_url,p.name FROM sagas s LEFT JOIN adventure_packs p ON p.id=s.pack_id ORDER BY s.id",
        |r| Ok((r.get::<_, i64>(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?
    .into_iter()
    .map(|(id, name, wiki_url, pack)| {
        Ok(WikiSaga {
            name,
            wiki_url,
            pack,
            quests: quests.remove(&id).unwrap_or_default(),
            rewards: rewards.remove(&id).unwrap_or_default(),
            file_name: paths.path("sagas", id)?.display().to_string(),
        })
    })
    .collect()
}

fn vendors(db: &Connection, paths: &DataFilePaths) -> Result<Vec<WikiVendor>> {
    let mut items=groups(db,"SELECT s.vendor_id,i.name,s.cost FROM sources s JOIN items i ON i.id=s.item_id WHERE s.kind='vendor' ORDER BY s.vendor_id,s.id",|r| Ok((r.get(0)?,VendorItem::Priced(PricedVendorItem {name:r.get(1)?,cost:r.get(2)?}))))?;
    rows(db,"SELECT v.id,v.name,v.wiki_url,v.location,p.name FROM vendors v LEFT JOIN adventure_packs p ON p.id=v.pack_id ORDER BY v.id",|r|Ok((r.get::<_,i64>(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?.into_iter().map(|(id,name,wiki_url,location,pack)| Ok(WikiVendor {name,wiki_url,location,pack,items:items.remove(&id).unwrap_or_default(),file_name:paths.path("vendors",id)?.display().to_string()})).collect()
}

fn events(db: &Connection, paths: &DataFilePaths) -> Result<Vec<WikiEvent>> {
    let mut items=groups(db,"SELECT e.event_id,i.name FROM event_rewards e JOIN items i ON e.owner_kind='item' AND i.id=e.owner_id ORDER BY e.event_id,e.id",|r| Ok((r.get(0)?,r.get::<_,String>(1)?)))?;
    rows(db, "SELECT id,name,wiki_url FROM events ORDER BY id", |r| Ok((r.get::<_, i64>(0)?, r.get(1)?, r.get(2)?)))?
        .into_iter()
        .map(|(id, name, wiki_url)| {
            Ok(WikiEvent {
                name,
                wiki_url,
                items: items.remove(&id).unwrap_or_default(),
                file_name: paths.path("events", id)?.display().to_string(),
            })
        })
        .collect()
}

fn crafting_systems(db: &Connection, paths: &DataFilePaths) -> Result<Vec<CraftingSystem>> {
    let mut families =
        groups(db, "SELECT system_id,family FROM crafting_system_families ORDER BY system_id,rowid", |r| {
            Ok((r.get(0)?, r.get::<_, String>(1)?))
        })?;
    let mut ingredients = groups(
        db,
        "SELECT system_id,name,tier,bind,source FROM crafting_ingredients ORDER BY system_id,sort_order",
        |r| {
            Ok((r.get(0)?, CraftingIngredient { name: r.get(1)?, tier: r.get(2)?, bind: r.get(3)?, source: r.get(4)? }))
        },
    )?;
    let mut augments=groups(db,"SELECT r.recipe_id,a.name FROM crafting_recipe_augments r JOIN augments a ON a.id=r.augment_id ORDER BY r.recipe_id,r.sort_order",|r|Ok((r.get(0)?,r.get::<_,String>(1)?)))?;
    let mut costs=groups(db,"SELECT c.recipe_id,i.name,c.quantity FROM crafting_recipe_ingredients c JOIN crafting_ingredients i ON i.id=c.ingredient_id ORDER BY c.recipe_id,c.sort_order",|r|Ok((r.get(0)?,IngredientCost{ingredient:r.get(1)?,quantity:r.get(2)?})))?;
    let mut recipes = BTreeMap::<i64, Vec<CraftingRecipe>>::new();
    for (id,system,tier,slot,grants_slot,option,note) in rows(db,"SELECT r.id,r.system_id,r.tier,s.label,g.label,r.option,r.note FROM crafting_recipes r LEFT JOIN augment_slot_types s ON s.id=r.slot_id LEFT JOIN augment_slot_types g ON g.id=r.grants_slot_id ORDER BY r.system_id,r.sort_order",|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?)))? {
        recipes.entry(system).or_default().push(CraftingRecipe{tier,slot,grants_slot,option,note,augments:augments.remove(&id).unwrap_or_default(),cost:costs.remove(&id).unwrap_or_default()});
    }
    rows(db,"SELECT s.id,s.name,s.page,s.npc,p.name FROM crafting_systems s LEFT JOIN adventure_packs p ON p.id=s.pack_id ORDER BY s.id",|r|Ok((r.get::<_,i64>(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?.into_iter().map(|(id,name,wiki_url,npc,pack)|Ok(CraftingSystem{name,wiki_url,npc,pack,families:families.remove(&id).unwrap_or_default(),ingredients:ingredients.remove(&id).unwrap_or_default(),recipes:recipes.remove(&id).unwrap_or_default(),file_name:paths.path("crafting_systems",id)?.display().to_string()})).collect()
}

#[derive(Default)]
struct WikiEffects {
    bonuses: Vec<WikiBonus>,
    effects: Vec<WikiItemEffect>,
}

fn effects(db: &Connection, table: &str, owner: &str) -> Result<BTreeMap<i64, WikiEffects>> {
    let mut owners = BTreeMap::<i64, WikiEffects>::new();
    let sql=format!("SELECT l.{owner}_id,e.name,e.is_stat,e.is_group,l.value,l.value2,b.name,e.description_template,CASE WHEN EXISTS (SELECT 1 FROM effect_bonuses WHERE effect_id=e.id) THEN e.name ELSE e.verbose_name_template END FROM {owner}_effects l JOIN {table} o ON o.id=l.{owner}_id JOIN effects e ON e.id=l.effect_id LEFT JOIN bonus_types b ON b.id=l.bonus_type_id WHERE o.provenance='wiki' ORDER BY l.{owner}_id,l.sort_order");
    for (id, name, is_stat, is_group, value, value2, bonus_type, description, title) in rows(db, &sql, |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, bool>(2)?,
            r.get::<_, bool>(3)?,
            r.get::<_, Option<i64>>(4)?,
            r.get::<_, Option<i64>>(5)?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, Option<String>>(7)?,
            r.get::<_, String>(8)?,
        ))
    })? {
        let owner = owners.entry(id).or_default();
        if let (Some(value), Some(bonus_type)) = (value, bonus_type) {
            owner.bonuses.push(WikiBonus {
                stat: is_stat.then(|| name.clone()),
                group: is_group.then(|| name.clone()),
                effect: (!is_stat && !is_group).then_some(name),
                bonus_type,
                value,
                value2,
            });
        } else {
            anyhow::ensure!(
                !is_stat && !is_group && value2.is_none(),
                "wiki effect {name:?} cannot be reconstructed from its amounts and type"
            );
            owner.effects.push(WikiItemEffect { name: title, description, value, target: None });
        }
    }
    Ok(owners)
}

fn items(db: &Connection, paths: &DataFilePaths) -> Result<Vec<WikiItem>> {
    let mut effects = effects(db, "items", "item")?;
    let mut sockets=groups(db,"SELECT l.item_id,s.label FROM item_augment_slots l JOIN augment_slot_types s ON s.id=l.slot_id ORDER BY l.item_id,l.sort_order",|r|Ok((r.get(0)?,r.get::<_,String>(1)?)))?;
    let mut bypass = groups(db, "SELECT item_id,bypass FROM item_dr_bypass ORDER BY item_id,rowid", |r| {
        Ok((r.get(0)?, r.get::<_, String>(1)?))
    })?;
    let mut weapons = rows(db, "SELECT * FROM item_weapon_stats", |r| {
        Ok((
            r.get::<_, i64>("item_id")?,
            WikiWeaponStats {
                damage_dice_count: r.get("base_dice_count")?,
                damage_dice_sides: r.get("base_dice_sides")?,
                damage_dice_bonus: r.get("base_dice_bonus")?,
                damage_multiplier: r.get("damage_multiplier")?,
                critical_threat_range: r.get("critical_threat_range")?,
                critical_multiplier: r.get("critical_multiplier")?,
                handedness: r.get("handedness")?,
                dr_bypass: Vec::new(),
            },
        ))
    })?
    .into_iter()
    .collect::<BTreeMap<_, _>>();
    let mut armor = rows(db, "SELECT * FROM item_armor_stats", |r| {
        Ok((
            r.get::<_, i64>("item_id")?,
            WikiArmorStats {
                armor_type: r.get("armor_type")?,
                armor_bonus: r.get("armor_bonus")?,
                max_dex_bonus: r.get("max_dex_bonus")?,
                arcane_spell_failure: r.get("arcane_spell_failure")?,
                armor_check_penalty: r.get("armor_check_penalty")?,
                shield_bonus: r.get("shield_bonus")?,
            },
        ))
    })?
    .into_iter()
    .collect::<BTreeMap<_, _>>();
    rows(db,"SELECT i.*,s.name AS slot,m.name AS material FROM items i JOIN equipment_slots s ON s.id=i.slot_id LEFT JOIN item_materials m ON m.id=i.material_id WHERE i.provenance='wiki' ORDER BY i.id",|r|Ok((r.get::<_,i64>("id")?,WikiItem{
        name:r.get("name")?,wiki_url:r.get("wiki_url")?,slot:r.get("slot")?,category:r.get("item_category")?,item_type:r.get("item_type")?,minimum_level:r.get("minimum_level")?,enhancement_bonus:r.get("enhancement_bonus")?,material:r.get("material")?,race_required:r.get("race_required")?,description:r.get("description")?,drop_location:r.get("drop_location")?,set:r.get("set_bonus")?,accepts_sentience:r.get("accepts_sentience")?,is_minor_artifact:r.get("is_minor_artifact")?,
        augment_slots:Vec::new(),bonuses:Vec::new(),effects:Vec::new(),weapon:None,armor:None,file_name:String::new()
    })))?.into_iter().map(|(id,mut item)|{
        item.file_name=paths.path("items",id)?.display().to_string();
        let grants=effects.remove(&id).unwrap_or_default();
        item.bonuses=grants.bonuses;item.effects=grants.effects;
        item.augment_slots=sockets.remove(&id).unwrap_or_default();
        item.weapon=weapons.remove(&id).map(|mut weapon|{weapon.dr_bypass=bypass.remove(&id).unwrap_or_default();weapon});
        item.armor=armor.remove(&id);
        Ok(item)
    }).collect()
}

fn augments(db: &Connection, paths: &DataFilePaths) -> Result<Vec<WikiAugment>> {
    let mut effects = effects(db, "augments", "augment")?;
    let mut slots=groups(db,"SELECT l.augment_id,s.label FROM augment_slots l JOIN augment_slot_types s ON s.id=l.slot_id ORDER BY l.augment_id,l.rowid",|r|Ok((r.get(0)?,r.get::<_,String>(1)?)))?;
    rows(db, "SELECT * FROM augments WHERE provenance='wiki' ORDER BY id", |r| {
        Ok((
            r.get::<_, i64>("id")?,
            WikiAugment {
                name: r.get("name")?,
                family: r.get("family")?,
                min_level: r.get("min_level")?,
                description: r.get("description")?,
                effect_description: r.get("effect_description")?,
                set: r.get("set_bonus")?,
                slots: Vec::new(),
                bonuses: Vec::new(),
                file_name: String::new(),
            },
        ))
    })?
    .into_iter()
    .map(|(id, mut augment)| {
        augment.file_name = paths.path("augments", id)?.display().to_string();
        let grants = effects.remove(&id).unwrap_or_default();
        anyhow::ensure!(
            grants.effects.is_empty(),
            "{}: wiki augment contains non-bonus effect links",
            augment.file_name
        );
        augment.bonuses = grants.bonuses;
        augment.slots = slots.remove(&id).unwrap_or_default();
        Ok(augment)
    })
    .collect()
}
