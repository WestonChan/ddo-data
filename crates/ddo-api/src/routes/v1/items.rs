use super::crafting::crafting_systems_making;
use super::quest_series::{quest_chains_rewarding, sagas_rewarding};
use super::quests::{
    adventure_packs_dropping_via, challenge_packs_rewarding, quests_dropping_via, sources_via, starter_rewards_of,
};
use super::vendors_and_events::{events_rewarding, vendors_offering};
use crate::db::{
    bonuses_via, convert_to_booleans, json_row, json_rows, like_escaped_text, modifiers_for, paged_query,
    paged_table_json, TableListSource, WhereClause,
};
use crate::error::ApiError;
use crate::query::{declare_list_parameters, declare_query_parameters, repeated_key_values, ApiQuery};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use ddo_model::enums::ItemCategory;
use rusqlite::Connection;
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(items))
        .routes(routes!(item_detail))
        .routes(routes!(equipment_slots))
        .routes(routes!(weapon_types))
        .routes(routes!(damage_types))
        .routes(routes!(augment_slot_types))
}

declare_query_parameters! {
    pub(super) struct ItemFilters {
        pub slot: Option<String>,
        pub category: Option<String>,
        pub min_level: Option<i64>,
        pub max_level: Option<i64>,
        pub pack: Option<String>,
        pub raid: Option<bool>,
        pub rare: Option<bool>,
        pub quest: Option<i64>,
        pub quest_chain: Option<i64>,
        pub saga: Option<i64>,
        #[serde(default, deserialize_with = "repeated_key_values")]
        pub enchantment: Vec<String>,
        pub include_set_bonuses: Option<bool>,
        pub include_legacy: Option<bool>,
    }
    repeatable: ["enchantment"]
}

const ITEM_LIST_COLUMNS: &str =
    "i.id, i.name, es.name AS slot, i.item_category AS category, i.item_type, i.minimum_level,
     i.enhancement_bonus, i.icon, i.is_legacy,
     (SELECT MIN(ap.name) FROM sources ql LEFT JOIN quests q ON q.id = ql.quest_id
      JOIN adventure_packs ap ON ap.id = COALESCE(ql.pack_id, q.pack_id) WHERE ql.item_id = i.id) AS pack,
     EXISTS (SELECT 1 FROM sources ql
             WHERE ql.item_id = i.id AND ql.kind = 'quest' AND ql.loot_type = 'raid') AS is_raid,
     EXISTS (SELECT 1 FROM sources ql
             WHERE ql.item_id = i.id AND ql.kind IN ('quest', 'adventure_pack') AND ql.is_rare) AS is_rare";

const ITEMS_SORT_FIELDS: &[(&str, &str)] = &[
    ("name", "i.name"),
    ("id", "i.id"),
    ("minimum_level", "i.minimum_level"),
    ("slot", "es.name"),
    ("category", "i.item_category"),
    ("pack", "pack"),
    ("enhancement_bonus", "i.enhancement_bonus"),
];

declare_list_parameters!(
    ItemsParameters,
    ITEMS_SORT_FIELDS,
    "",
    Some(
        "Search text, trimmed and matched ignoring case: keeps items whose name contains it, \
         or whose slot, category or any adventure pack it drops in is named exactly it. Without \
         `sort`, ranks an exact name first, then names starting with it, then the rest, each group by name. \
         Blank applies no search."
    )
);

#[utoipa::path(
    get,
    path = "/v1/items",
    tag = "items",
    summary = "List items",
    description = "One page of equipment matching every filter given, so a client needs no matching of its own. \
                   Filters: `q` (search text against the name, or exactly a slot, category or pack name), `slot`, \
                   `category`, `min_level` and `max_level`, `pack`, `raid`, `rare`, `quest`, `quest_chain` and `saga` \
                   (ids of what drops or rewards the item), `enchantment` (a name from /v1/enchantments, \
                   or several as repeated `enchantment` keys, any of which the item must carry, as a stat bonus or as \
                   a named effect) and \
                   `include_set_bonuses` (let the stat names in `enchantment` also match the item's set tiers) and \
                   `include_legacy` (also list legacy items, which are left out by default); \
                   `limit` and `offset` page the matches; `sort` chooses the ordering. \
                   Without `sort`, ordered by name; with `q`, an exact name match comes first, then names starting \
                   with the text, then the rest, each group by name. Each row carries what a picker needs: id, name, \
                   slot, category, item type, minimum level, enhancement bonus, icon name, the alphabetically first \
                   adventure pack it drops in, whether any of its sources is a raid, whether it is rare loot from \
                   at least one quest (marked rare in Maetrim's drop text or on ddowiki), `is_legacy` (an old version \
                   kept beside the current one, such as a name ending `(legacy)` or `(historic)`, or an item the \
                   wiki says no longer drops; always false unless `include_legacy=true`). The items his files lack \
                   are read whole from ddowiki and listed like his, dropped as soon as his files carry an item of \
                   that name. Use the detail endpoint for \
                   bonuses, sockets and quests. `total` counts every match, not just this page.",
    params(
        ItemsParameters,
        ("slot" = Option<String>, Query, description = "Equipment slot name exactly as /v1/equipment-slots lists it, e.g. `Main Hand`"),
        ("category" = Option<String>, Query, description = "One of `Armor`, `Shield`, `Weapon`, `Jewelry`, `Clothing`; anything else is a 400"),
        ("min_level" = Option<i64>, Query, description = "Only items whose minimum level is at least this"),
        ("max_level" = Option<i64>, Query, description = "Only items whose minimum level is at most this"),
        ("pack" = Option<String>, Query, description = "Adventure pack name as /v1/adventure-packs lists it; matches items dropping from a quest in it or credited to any quest of the whole pack"),
        ("raid" = Option<bool>, Query, description = "`true` keeps only items that drop from a raid; `false` and unset apply no filter"),
        ("rare" = Option<bool>, Query, description = "`true` keeps only items that are rare loot from at least one quest or from any quest of a pack, per Maetrim's drop text or ddowiki; `false` and unset apply no filter"),
        ("quest" = Option<i64>, Query, description = "Quest id as /v1/quests lists it; keeps the items its /v1/quests/{id} `items` lists, dropped from any chest, as raid loot or as an end reward (loot his drop text credits to the whole pack is matched by `pack` instead); an id no quest has matches nothing rather than a 400, as an unknown `pack` does"),
        ("quest_chain" = Option<i64>, Query, description = "Quest chain id as /v1/quest-chains lists it; keeps items its end reward offers; an id no chain has matches nothing"),
        ("saga" = Option<i64>, Query, description = "Saga id as /v1/sagas lists it; keeps items its end reward offers in any tier; an id no saga has matches nothing"),
        ("enchantment" = Option<String>, Query, description = "An enchantment name exactly as /v1/enchantments lists it (a stat name from /v1/stats or an effect name as an item's `effects` give it); several are given as repeated keys (`enchantment=Strength&enchantment=Vorpal`) and keep items carrying any of them. A name may contain commas and is matched whole (`enchantment=Constitution%20Poison%2C%20Lesser`), so a comma never separates names. A stat name matches an item with at least one bonus of its own to that stat, and with `include_set_bonuses=true` also an item whose set has a tier with a bonus to it; an effect name matches an item whose `effects` (its `item_effects` rows) name it; a name that is both matches either way. Matching is case-sensitive; a name that is neither a stat nor an effect is a 400 naming it"),
        ("include_set_bonuses" = Option<bool>, Query, description = "`true` widens the stat names in `enchantment` to also match an item when any tier of its set (see /v1/sets/{id}) carries a bonus to one of them; the item's own bonuses and effects match either way; `false` and unset match the item's own bonuses only; no effect without `enchantment`"),
        ("include_legacy" = Option<bool>, Query, description = "`true` also lists legacy items (`is_legacy`: old versions such as names ending `(legacy)` or `(historic)`, and items the wiki says no longer drop) and counts them in `total`; `false` and unset leave them out. /v1/items/{id} serves a legacy item either way"),
    ),
    responses(
        (status = 200, description = "`total`, `limit`, `offset` and the `items` page", body = Value),
        (status = 400, description = "Unknown category or enchantment, or an unknown or malformed query parameter", body = crate::error::ErrorBody)
    )
)]
async fn items(
    State(state): State<AppState>,
    ApiQuery(query, filters): ApiQuery<ItemFilters>,
) -> Result<Json<Value>, ApiError> {
    if let Some(category) = &filters.category {
        if !ItemCategory::ALL.iter().any(|known| known.as_str() == category) {
            return Err(ApiError::BadRequest(format!("unknown category {category:?}")));
        }
    }
    state
        .read_db(move |db| {
            let mut where_clause = WhereClause::default();
            if filters.include_legacy != Some(true) {
                where_clause.add_condition("NOT i.is_legacy");
            }
            let mut default_order = "i.name".to_string();
            if let Some(search_text) = query.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
                let search_placeholder =
                    where_clause.add_bound_condition(ITEMS_MATCHING_SEARCH_TEXT_SQL, like_escaped_text(search_text));
                default_order = format!(
                    "CASE WHEN i.name LIKE {search_placeholder} ESCAPE '\\' THEN 0 \
                          WHEN i.name LIKE {search_placeholder} || '%' ESCAPE '\\' THEN 1 ELSE 2 END, i.name"
                );
            }
            if let Some(slot) = &filters.slot {
                where_clause.add_bound_condition("es.name = ?", slot.clone());
            }
            if let Some(category) = &filters.category {
                where_clause.add_bound_condition("i.item_category = ?", category.clone());
            }
            if let Some(min_level) = filters.min_level {
                where_clause.add_bound_condition("i.minimum_level >= ?", min_level);
            }
            if let Some(max_level) = filters.max_level {
                where_clause.add_bound_condition("i.minimum_level <= ?", max_level);
            }
            if let Some(pack) = &filters.pack {
                where_clause.add_bound_condition("EXISTS (SELECT 1 FROM sources ql LEFT JOIN quests q ON q.id = ql.quest_id JOIN adventure_packs ap ON ap.id = COALESCE(ql.pack_id, q.pack_id) WHERE ql.item_id = i.id AND ap.name = ?)",
                    pack.clone(),
                );
            }
            if filters.raid == Some(true) {
                where_clause.add_condition("EXISTS (SELECT 1 FROM sources ql WHERE ql.item_id = i.id AND ql.kind = 'quest' AND ql.loot_type = 'raid')");
            }
            if filters.rare == Some(true) {
                where_clause.add_condition("EXISTS (SELECT 1 FROM sources ql WHERE ql.item_id = i.id AND ql.kind IN ('quest', 'adventure_pack') AND ql.is_rare)");
            }
            if let Some(quest_id) = filters.quest {
                where_clause.add_bound_condition("i.id IN (SELECT d.item_id FROM sources d WHERE d.kind = 'quest' AND d.quest_id = ?)", quest_id);
            }
            if let Some(chain_id) = filters.quest_chain {
                where_clause.add_bound_condition("i.id IN (SELECT d.item_id FROM sources d WHERE d.kind = 'quest_chain' AND d.chain_id = ?)", chain_id);
            }
            if let Some(saga_id) = filters.saga {
                where_clause.add_bound_condition("i.id IN (SELECT d.item_id FROM sources d WHERE d.kind = 'saga' AND d.saga_id = ?)", saga_id);
            }
            let enchantment_names = filters.enchantment;
            if let Some(unknown_enchantment_name) = first_unknown_enchantment_name(db, &enchantment_names)? {
                return Err(ApiError::BadRequest(format!("unknown enchantment {unknown_enchantment_name:?}")));
            }
            if !enchantment_names.is_empty() {
                let set_tier_match_sql = if filters.include_set_bonuses == Some(true) {
                    format!(" OR {ITEMS_WITH_SET_TIER_BONUS_TO_STATS_SQL}")
                } else {
                    String::new()
                };
                where_clause.add_bound_list_condition(
                    &format!(
                        "({ITEMS_WITH_OWN_BONUS_TO_STATS_SQL}{set_tier_match_sql} OR {ITEMS_WITH_EFFECTS_SQL})"
                    ),
                    enchantment_names,
                );
            }
            let mut page = paged_query(
                db,
                ITEM_LIST_COLUMNS,
                "items i JOIN equipment_slots es ON es.id = i.slot_id",
                &query,
                &default_order,
                ITEMS_SORT_FIELDS,
                &where_clause,
            )?;
            for item in &mut page.rows {
                convert_to_booleans(item, &["is_raid", "is_rare", "is_legacy"]);
            }
            Ok(Json(page.into_json("items")))
        })
        .await
}

const ITEMS_MATCHING_SEARCH_TEXT_SQL: &str = "(i.name LIKE '%' || ? || '%' ESCAPE '\\' \
     OR es.name LIKE ? ESCAPE '\\' OR i.item_category LIKE ? ESCAPE '\\' \
     OR i.id IN (SELECT d.item_id FROM sources d LEFT JOIN quests q ON q.id = d.quest_id \
                 JOIN adventure_packs ap ON ap.id = COALESCE(d.pack_id, q.pack_id) WHERE ap.name LIKE ? ESCAPE '\\'))";

const ITEMS_WITH_OWN_BONUS_TO_STATS_SQL: &str =
    "i.id IN (SELECT ib.item_id FROM stats s JOIN bonuses b ON b.stat_id = s.id \
     JOIN item_bonuses ib ON ib.bonus_id = b.id WHERE s.name IN (?))";

const ITEMS_WITH_SET_TIER_BONUS_TO_STATS_SQL: &str =
    "i.id IN (SELECT sbi.item_id FROM stats s JOIN bonuses b ON b.stat_id = s.id \
     JOIN set_bonus_tier_bonuses tb ON tb.bonus_id = b.id JOIN set_bonus_tiers t ON t.id = tb.tier_id \
     JOIN set_bonus_items sbi ON sbi.set_id = t.set_id WHERE s.name IN (?))";

const ITEMS_WITH_EFFECTS_SQL: &str =
    "i.id IN (SELECT ie.item_id FROM effects e JOIN item_effects ie ON ie.effect_id = e.id WHERE e.name IN (?))";

fn first_unknown_enchantment_name(
    db: &rusqlite::Connection,
    enchantment_names: &[String],
) -> Result<Option<String>, ApiError> {
    let mut statement = db.prepare_cached(
        "SELECT EXISTS (SELECT 1 FROM stats WHERE name = ?1) OR EXISTS (SELECT 1 FROM effects WHERE name = ?1)",
    )?;
    for enchantment_name in enchantment_names {
        if !statement.query_row([enchantment_name], |row| row.get::<_, bool>(0))? {
            return Ok(Some(enchantment_name.clone()));
        }
    }
    Ok(None)
}

#[utoipa::path(
    get,
    path = "/v1/items/{id}",
    tag = "items",
    summary = "Get an item",
    description = "One item with everything the dataset knows about it: the core row (slot, category, type, minimum \
                   level, enhancement bonus, material, race restriction, description, drop location text, set name, \
                   sentience and minor-artifact flags, `is_legacy` (served whatever its value; the list hides legacy \
                   items by default) and wiki URL), \
                   then `weapon` (dice, threat range, multipliers, \
                   proficiency, `dr_bypass`) or `armor` (AC, max Dex, spell failure, check penalty) when the item is \
                   one, `bonuses` (stat, bonus type, value), `effects` (named effects with value and target), \
                   `augment_slots` (sockets in order, each with its `label` and the fixed `options` upstream gives \
                   it: the upgrade tiers a player unlocks on Quenched, Smoldering, Thunder-Forged, Attuned to Heroism \
                   and other upgradeable items, or the choices a crafting step offers; an open socket has none. Each \
                   option has `id`, `name`, `description`, `min_level`, `icon` (an icon name like the item's), \
                   `grants_slot` (the label, as /v1/augment-slot-types lists it, of the socket the option adds, or \
                   null), `sets` (each set the option makes the item count toward, with `id` and `name`; see \
                   /v1/sets/{id}), `bonuses` (stat, bonus type and value, as the item's) and `modifiers` (the raw \
                   effects those bonuses come from, as the item's). What an option gives is the option's until the \
                   player unlocks or picks it, so it is never among the item's own `bonuses`, `augment_slots` or \
                   `set`), `clickies`, `set`, `quests` it drops \
                   from (once per loot type, so a quest that both drops it and gives it as an end reward appears twice) with loot type, raid flag, `is_rare` (rare loot in that quest, per Maetrim's drop text or ddowiki), \
                   `chest` (the chest his drop text names for that quest, lower-cased, such as `end chest` or \
                   `optional chest`; null when it names none, and always null on a `reward` row), the \
                   `difficulties` each offers, and ddowiki's `is_free_to_play` for each \
                   (see /v1/quests for the rest of the quest), `quest_chains` and `sagas` whose end reward offers the item (each with `id`, `name`, `is_rare` and the ddowiki page it was read from as `wiki_url`, a saga also with its reward `tier`; see /v1/quest-chains and /v1/sagas), \
                   `adventure_packs` any of whose quests drops it, as his drop text credits a whole pack (`Magic of \
                   Myth Drannor, any end chest`; each with `id`, `name`, `loot_type`, `chest`, `is_rare` and the ddowiki \
                   page named after the pack as `wiki_url`, once per loot type; see /v1/adventure-packs/{id}), `challenge_packs` whose challenges' ingredients or \
                   commendations are turned in for it (`Vaults of the Artificers, Turn in various challenge \
                   ingredients`; each with the pack's `id` and `name`, `is_rare` and `wiki_url`), \
                   `crafting_systems` whose station crafts or upgrades it, as \
                   his drop text names the system or its station (`Magma Forge, Crafted from various ingredients`; \
                   each with `id`, `name`, `is_rare` and the ddowiki page as `wiki_url`; see \
                   /v1/crafting-systems/{id}), `vendors` that sell or trade it (each with `id`, `name`, `location`, \
                   `cost`, `is_rare` and `wiki_url`; see /v1/vendors/{id}), `events` that reward it (each with `id`, \
                   `name`, `is_rare` and `wiki_url`; see /v1/events/{id}), as a wiki vendors or events file lists it \
                   or his drop text names one, `starter_rewards`, the `character_level` an iconic hero reaches to \
                   be given it as starter gear (`Advance to level 15, End reward`), \
                   `sources`, every one of those sources in one array, each \
                   with `kind` (`quest`, `quest_chain`, `saga`, `adventure_pack`, `challenge`, `crafting_system`, \
                   `vendor`, `event` or `starter`), the source's \
                   `id` and `name` (a starter row has no `id`, and `Advance to level N` as its name), \
                   `loot_type` (set only on a quest or pack drop), `chest`, `is_rare`, `tier` (a saga reward's list, \
                   null otherwise), `character_level` (a starter row's, null otherwise), `cost` (a vendor row's, null \
                   otherwise) and the source's ddowiki page as `wiki_url` (the page read for a chain, saga, crafting \
                   system, vendor or event, null on a starter row, the page \
                   named after a quest or pack otherwise), sorted by kind in that order and then by name, and \
                   the raw `modifiers` the ETL derived the bonuses from.",
    params(("id" = i64, Path, description = "The item's numeric id from the list endpoint")), responses((status = 200, description = "The item with its child collections", body = Value), (status = 404, description = "No item has this id", body = crate::error::ErrorBody))
)]
async fn item_detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut item = json_row(
                db,
                "SELECT i.id, i.name, es.name AS slot, i.item_category AS category, i.item_type, i.minimum_level, i.enhancement_bonus,
                        m.name AS material, i.race_required, i.icon, i.description, i.drop_location, i.set_bonus AS set_name,
                        i.accepts_sentience, i.is_minor_artifact, i.is_legacy, i.wiki_url
                   FROM items i JOIN equipment_slots es ON es.id = i.slot_id LEFT JOIN item_materials m ON m.id = i.material_id
                  WHERE i.id = ?1",
                [id],
            )?;
            convert_to_booleans(&mut item, &["accepts_sentience", "is_minor_artifact", "is_legacy"]);

            let weapon = json_rows(
                db,
                "SELECT wt.name AS weapon_type, p.name AS proficiency, w.handedness, w.damage, w.critical, w.base_dice_count, w.base_dice_sides,
                        w.base_dice_bonus, w.damage_multiplier, w.critical_threat_range, w.critical_multiplier, w.attack_modifier, w.damage_modifier
                   FROM item_weapon_stats w JOIN weapon_types wt ON wt.id = w.weapon_type_id
                   LEFT JOIN weapon_proficiencies p ON p.id = wt.proficiency_id WHERE w.item_id = ?1",
                [id],
            )?
            .pop();
            item["weapon"] = match weapon {
                Some(mut weapon) => {
                    let dr_bypasses: Vec<Value> = json_rows(db, "SELECT bypass FROM item_dr_bypass WHERE item_id = ?1 ORDER BY bypass", [id])?
                        .into_iter()
                        .map(|row| row["bypass"].clone())
                        .collect();
                    weapon["dr_bypass"] = Value::Array(dr_bypasses);
                    weapon
                }
                None => Value::Null,
            };
            item["armor"] = json_rows(
                db,
                "SELECT armor_type, armor_bonus, max_dex_bonus, arcane_spell_failure, armor_check_penalty, shield_bonus, damage_reduction, mithral_body, adamantine_body
                   FROM item_armor_stats WHERE item_id = ?1",
                [id],
            )?
            .pop()
            .unwrap_or(Value::Null);

            item["bonuses"] = Value::Array(bonuses_via(db, "item_bonuses", "item_id", id)?);
            item["effects"] = Value::Array(json_rows(
                db,
                "SELECT e.id, e.name, e.description, ie.value, ie.target FROM item_effects ie JOIN effects e ON e.id = ie.effect_id
                  WHERE ie.item_id = ?1 ORDER BY ie.sort_order",
                [id],
            )?);

            let mut augment_slots = json_rows(
                db,
                "SELECT s.sort_order, t.id AS slot_type_id, t.label, t.family, t.variant, t.qualifier FROM item_augment_slots s
                   JOIN augment_slot_types t ON t.id = s.slot_id WHERE s.item_id = ?1 ORDER BY s.sort_order",
                [id],
            )?;
            for augment_slot in &mut augment_slots {
                let slot_order = augment_slot["sort_order"].as_i64().unwrap_or(0);
                augment_slot["options"] = Value::Array(augment_slot_options(db, id, slot_order)?);
            }
            item["augment_slots"] = Value::Array(augment_slots);

            item["clickies"] = Value::Array(json_rows(
                db,
                "SELECT ic.name, ic.clickie_id, ic.spell_id, c.description, c.icon FROM item_clickies ic
                   LEFT JOIN clickies c ON c.id = ic.clickie_id WHERE ic.item_id = ?1 ORDER BY ic.sort_order",
                [id],
            )?);
            item["set"] = json_rows(
                db,
                "SELECT s.id, s.name, s.icon FROM set_bonus_items sbi JOIN set_bonuses s ON s.id = sbi.set_id WHERE sbi.item_id = ?1",
                [id],
            )?
            .pop()
            .unwrap_or(Value::Null);
            item["quests"] = Value::Array(quests_dropping_via(db, "item_id", id)?);
            item["quest_chains"] = Value::Array(quest_chains_rewarding(db, id)?);
            item["sagas"] = Value::Array(sagas_rewarding(db, id)?);
            item["adventure_packs"] = Value::Array(adventure_packs_dropping_via(db, "item_id", id)?);
            item["challenge_packs"] = Value::Array(challenge_packs_rewarding(db, id)?);
            item["crafting_systems"] = Value::Array(crafting_systems_making(db, id)?);
            item["vendors"] = Value::Array(vendors_offering(db, id)?);
            item["events"] = Value::Array(events_rewarding(db, id)?);
            item["starter_rewards"] = Value::Array(starter_rewards_of(db, id)?);
            item["sources"] = Value::Array(sources_via(db, "item_id", id)?);
            item["modifiers"] = Value::Array(modifiers_for(db, "item", id)?);
            Ok(Json(item))
        })
        .await
}

fn augment_slot_options(db: &Connection, item_id: i64, slot_order: i64) -> Result<Vec<Value>, ApiError> {
    let mut options = json_rows(
        db,
        "SELECT o.id, o.name, o.description, o.min_level, o.icon,
                (SELECT t.label FROM item_augment_slot_option_grants g JOIN augment_slot_types t ON t.id = g.slot_id
                  WHERE g.option_id = o.id ORDER BY g.sort_order LIMIT 1) AS grants_slot
           FROM item_augment_slot_options o WHERE o.item_id = ?1 AND o.slot_order = ?2 ORDER BY o.option_order",
        (item_id, slot_order),
    )?;
    for option in &mut options {
        let option_id = option["id"].as_i64().unwrap_or(0);
        option["sets"] = Value::Array(json_rows(
            db,
            "SELECT s.id, s.name FROM item_augment_slot_option_sets os JOIN set_bonuses s ON s.id = os.set_id
              WHERE os.option_id = ?1 ORDER BY s.name",
            [option_id],
        )?);
        option["bonuses"] = Value::Array(bonuses_via(db, "item_augment_slot_option_bonuses", "option_id", option_id)?);
        option["modifiers"] = Value::Array(modifiers_for(db, "item_augment_slot_option", option_id)?);
    }
    Ok(options)
}

const EQUIPMENT_SLOTS_SORT_FIELDS: &[(&str, &str)] = &[
    ("name", "listed.name"),
    ("id", "listed.id"),
    ("sort_order", "listed.sort_order"),
    ("category", "listed.category"),
];

declare_list_parameters!(EquipmentSlotsParameters, EQUIPMENT_SLOTS_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/equipment-slots",
    tag = "items",
    summary = "List equipment slots",
    description = "The equipment slots an item can occupy, in display order, with a category (weapon, armor, \
                   accessory). /v1/items accepts these names in `slot`.",
    params(
        EquipmentSlotsParameters,
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `equipment_slots` page", body = Value),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn equipment_slots(State(state): State<AppState>, ApiQuery(query, _): ApiQuery) -> Result<Json<Value>, ApiError> {
    paged_table_json(
        state,
        query,
        TableListSource {
            select_sql: "SELECT id, name, sort_order, category FROM equipment_slots",
            rows_key: "equipment_slots",
            name_column: "listed.name",
            default_order: "listed.sort_order",
            sortable_fields: EQUIPMENT_SLOTS_SORT_FIELDS,
            flag_columns: &[],
        },
    )
    .await
}

const WEAPON_TYPES_SORT_FIELDS: &[(&str, &str)] =
    &[("name", "listed.name"), ("id", "listed.id"), ("proficiency", "listed.proficiency")];

declare_list_parameters!(WeaponTypesParameters, WEAPON_TYPES_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/weapon-types",
    tag = "items",
    summary = "List weapon types",
    description = "Every weapon and shield type with the proficiency it needs and whether it is a shield. Item \
                   `weapon` blocks name their type from this list.",
    params(
        WeaponTypesParameters,
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `weapon_types` page", body = Value),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn weapon_types(State(state): State<AppState>, ApiQuery(query, _): ApiQuery) -> Result<Json<Value>, ApiError> {
    paged_table_json(
        state,
        query,
        TableListSource {
            select_sql: "SELECT wt.id, wt.name, p.name AS proficiency, wt.is_shield FROM weapon_types wt
                         LEFT JOIN weapon_proficiencies p ON p.id = wt.proficiency_id",
            rows_key: "weapon_types",
            name_column: "listed.name",
            default_order: "listed.id",
            sortable_fields: WEAPON_TYPES_SORT_FIELDS,
            flag_columns: &["is_shield"],
        },
    )
    .await
}

const DAMAGE_TYPES_SORT_FIELDS: &[(&str, &str)] =
    &[("name", "listed.name"), ("id", "listed.id"), ("category", "listed.category")];

declare_list_parameters!(DamageTypesParameters, DAMAGE_TYPES_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/damage-types",
    tag = "items",
    summary = "List damage types",
    description = "Every damage type (physical, elemental, alignment, special) with its category. Spell damage \
                   lines and DR bypass entries use these names.",
    params(
        DamageTypesParameters,
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `damage_types` page", body = Value),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn damage_types(State(state): State<AppState>, ApiQuery(query, _): ApiQuery) -> Result<Json<Value>, ApiError> {
    paged_table_json(
        state,
        query,
        TableListSource {
            select_sql: "SELECT id, name, category FROM damage_types",
            rows_key: "damage_types",
            name_column: "listed.name",
            default_order: "listed.id",
            sortable_fields: DAMAGE_TYPES_SORT_FIELDS,
            flag_columns: &[],
        },
    )
    .await
}

const AUGMENT_SLOT_TYPES_SORT_FIELDS: &[(&str, &str)] = &[
    ("name", "listed.label"),
    ("id", "listed.id"),
    ("family", "listed.family"),
    ("variant", "listed.variant"),
    ("qualifier", "listed.qualifier"),
];

declare_list_parameters!(
    AugmentSlotTypesParameters,
    AUGMENT_SLOT_TYPES_SORT_FIELDS,
    "`name` sorts by the display `label`.",
    Some(
        "Case-insensitive substring of the socket's display `label`; \
         surrounding whitespace is trimmed and blank applies no search."
    )
);

#[utoipa::path(
    get,
    path = "/v1/augment-slot-types",
    tag = "items",
    summary = "List augment slot types",
    description = "Every socket an item can carry: gem colours (`red`, `colorless`, `sun`, ...) and crafting-family \
                   sockets (`lamordia: melancholic (accessory)`, `isle of dread: set bonus`, ...). `family` says which \
                   kind it is; `label` is what /v1/augments accepts in `slot`.",
    params(
        AugmentSlotTypesParameters,
    ),
    responses(
        (status = 200, description = "`total`, `limit`, `offset` and the `augment_slot_types` page", body = Value),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn augment_slot_types(
    State(state): State<AppState>,
    ApiQuery(query, _): ApiQuery,
) -> Result<Json<Value>, ApiError> {
    paged_table_json(
        state,
        query,
        TableListSource {
            select_sql: "SELECT id, label, family, variant, qualifier FROM augment_slot_types",
            rows_key: "augment_slot_types",
            name_column: "listed.label",
            default_order: "listed.family, label",
            sortable_fields: AUGMENT_SLOT_TYPES_SORT_FIELDS,
            flag_columns: &[],
        },
    )
    .await
}
