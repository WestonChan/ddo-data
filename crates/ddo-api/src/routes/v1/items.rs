use super::quest_series::{quest_chains_rewarding, sagas_rewarding};
use super::quests::{adventure_packs_dropping_via, quests_dropping_via, sources_via};
use crate::db::{
    bonuses_via, clamped_page, convert_to_booleans, json_row, json_rows, like_escaped_text, modifiers_for, row_count,
    whole_table_json, WhereClause,
};
use crate::error::ApiError;
use crate::query::{comma_separated_values, ApiQuery, QueryParameters};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use ddo_model::enums::ItemCategory;
use serde::Deserialize;
use serde_json::{json, Value};
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ItemListQuery {
    pub q: Option<String>,
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
    pub enchantment: Option<String>,
    pub include_set_bonuses: Option<bool>,
    pub include_legacy: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl QueryParameters for ItemListQuery {
    const REPEATABLE_KEYS: &'static [&'static str] = &["enchantment"];
}

#[utoipa::path(
    get,
    path = "/v1/items",
    tag = "items",
    summary = "List items",
    description = "One page of equipment matching every filter given, so a client needs no matching of its own. \
                   Filters: `q` (search text against the name, or exactly a slot, category or pack name), `slot`, \
                   `category`, `min_level` and `max_level`, `pack`, `raid`, `rare`, `quest`, `quest_chain` and `saga` \
                   (ids of what drops or rewards the item), `enchantment` (one or more names from \
                   /v1/enchantments, any of which the item must carry, as a stat bonus or as a named effect) and \
                   `include_set_bonuses` (let the stat names in `enchantment` also match the item's set tiers) and \
                   `include_legacy` (also list legacy items, which are left out by default); \
                   `limit` and `offset` page the matches. \
                   Ordered by name; with `q`, an exact name match comes first, then names starting with the text, \
                   then the rest, each group by name. Each row carries what a picker needs: id, name, slot, \
                   category, item type, minimum level, enhancement bonus, icon name, the alphabetically first \
                   adventure pack it drops in, whether any of its sources is a raid, whether it is rare loot from \
                   at least one quest (marked rare in Maetrim's drop text or on ddowiki), `is_legacy` (an old version \
                   kept beside the current one, such as a name ending `(legacy)` or `(historic)`, or an item the \
                   wiki says no longer drops; always false unless `include_legacy=true`), and `provenance`: `maetrim` \
                   for an item from DDOBuilderV2's files, `wiki` for one read from ddowiki because his files lack \
                   it (dropped as soon as his files carry an item of that name). Use the detail endpoint for \
                   bonuses, sockets and quests. `total` counts every match, not just this page.",
    params(
        ("q" = Option<String>, Query, description = "Search text, trimmed and matched ignoring case: keeps items whose name contains it, or whose slot, category or any adventure pack it drops in is named exactly it (`q=feet`, `q=jewelry`, `q=vault of night`); ranks an exact name first, then names starting with it, then the rest, each group by name"),
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
        ("enchantment" = Option<String>, Query, description = "One or more enchantment names exactly as /v1/enchantments lists them (a stat name from /v1/stats or an effect name as an item's `effects` give it), comma-separated (`enchantment=Strength,Vorpal`) or as repeated keys (`enchantment=Strength&enchantment=Vorpal`); keeps items carrying any of them. A stat name matches an item with at least one bonus of its own to that stat, and with `include_set_bonuses=true` also an item whose set has a tier with a bonus to it; an effect name matches an item whose `effects` (its `item_effects` rows) name it; a name that is both matches either way. Matching is case-sensitive; a name that is neither a stat nor an effect is a 400 naming it"),
        ("include_set_bonuses" = Option<bool>, Query, description = "`true` widens the stat names in `enchantment` to also match an item when any tier of its set (see /v1/sets/{id}) carries a bonus to one of them; the item's own bonuses and effects match either way; `false` and unset match the item's own bonuses only; no effect without `enchantment`"),
        ("include_legacy" = Option<bool>, Query, description = "`true` also lists legacy items (`is_legacy`: old versions such as names ending `(legacy)` or `(historic)`, and items the wiki says no longer drop) and counts them in `total`; `false` and unset leave them out. /v1/items/{id} serves a legacy item either way"),
        ("limit" = Option<i64>, Query, description = "Page size, 1 to 10000; defaults to 100; out-of-range values are clamped rather than rejected"),
        ("offset" = Option<i64>, Query, description = "Rows to skip before the first returned row; defaults to 0; negative values are clamped to 0 rather than rejected")
    ),
    responses(
        (status = 200, description = "`total`, `limit`, `offset` and the `items` page", body = Value),
        (status = 400, description = "Unknown category or enchantment, or an unknown or malformed query parameter", body = crate::error::ErrorBody)
    )
)]
async fn items(
    State(state): State<AppState>,
    ApiQuery(query): ApiQuery<ItemListQuery>,
) -> Result<Json<Value>, ApiError> {
    if let Some(category) = &query.category {
        if !ItemCategory::ALL.iter().any(|known| known.as_str() == category) {
            return Err(ApiError::BadRequest(format!("unknown category {category:?}")));
        }
    }
    let (limit, offset) = clamped_page(query.limit, query.offset);
    state
        .read_db(move |db| {
            let mut where_clause = WhereClause::default();
            if query.include_legacy != Some(true) {
                where_clause.add_condition("NOT i.is_legacy");
            }
            let mut order_sql = "i.name".to_string();
            if let Some(search_text) = query.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
                let search_placeholder =
                    where_clause.add_bound_condition(ITEMS_MATCHING_SEARCH_TEXT_SQL, like_escaped_text(search_text));
                order_sql = format!(
                    "CASE WHEN i.name LIKE {search_placeholder} ESCAPE '\\' THEN 0 \
                          WHEN i.name LIKE {search_placeholder} || '%' ESCAPE '\\' THEN 1 ELSE 2 END, i.name"
                );
            }
            if let Some(slot) = &query.slot {
                where_clause.add_bound_condition("es.name = ?", slot.clone());
            }
            if let Some(category) = &query.category {
                where_clause.add_bound_condition("i.item_category = ?", category.clone());
            }
            if let Some(min_level) = query.min_level {
                where_clause.add_bound_condition("i.minimum_level >= ?", min_level);
            }
            if let Some(max_level) = query.max_level {
                where_clause.add_bound_condition("i.minimum_level <= ?", max_level);
            }
            if let Some(pack) = &query.pack {
                where_clause.add_bound_condition("EXISTS (SELECT 1 FROM sources ql LEFT JOIN quests q ON q.id = ql.quest_id JOIN adventure_packs ap ON ap.id = COALESCE(ql.pack_id, q.pack_id) WHERE ql.item_id = i.id AND ap.name = ?)",
                    pack.clone(),
                );
            }
            if query.raid == Some(true) {
                where_clause.add_condition("EXISTS (SELECT 1 FROM sources ql WHERE ql.item_id = i.id AND ql.kind = 'quest' AND ql.loot_type = 'raid')");
            }
            if query.rare == Some(true) {
                where_clause.add_condition("EXISTS (SELECT 1 FROM sources ql WHERE ql.item_id = i.id AND ql.kind IN ('quest', 'adventure_pack') AND ql.is_rare)");
            }
            if let Some(quest_id) = query.quest {
                where_clause.add_bound_condition("i.id IN (SELECT d.item_id FROM sources d WHERE d.kind = 'quest' AND d.quest_id = ?)", quest_id);
            }
            if let Some(chain_id) = query.quest_chain {
                where_clause.add_bound_condition("i.id IN (SELECT d.item_id FROM sources d WHERE d.kind = 'quest_chain' AND d.chain_id = ?)", chain_id);
            }
            if let Some(saga_id) = query.saga {
                where_clause.add_bound_condition("i.id IN (SELECT d.item_id FROM sources d WHERE d.kind = 'saga' AND d.saga_id = ?)", saga_id);
            }
            let enchantment_names = query.enchantment.as_deref().map(comma_separated_values).unwrap_or_default();
            if let Some(unknown_enchantment_name) = first_unknown_enchantment_name(db, &enchantment_names)? {
                return Err(ApiError::BadRequest(format!("unknown enchantment {unknown_enchantment_name:?}")));
            }
            if !enchantment_names.is_empty() {
                let set_tier_match_sql = if query.include_set_bonuses == Some(true) {
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
            let where_sql = where_clause.to_sql();
            let from_sql = format!("FROM items i JOIN equipment_slots es ON es.id = i.slot_id {where_sql}");
            let total = row_count(db, &format!("SELECT COUNT(*) {from_sql}"), where_clause.params())?;
            let page_sql = format!(
                "SELECT i.id, i.name, es.name AS slot, i.item_category AS category, i.item_type, i.minimum_level, i.enhancement_bonus, i.icon, i.is_legacy, i.provenance,
                        (SELECT MIN(ap.name) FROM sources ql LEFT JOIN quests q ON q.id = ql.quest_id JOIN adventure_packs ap ON ap.id = COALESCE(ql.pack_id, q.pack_id) WHERE ql.item_id = i.id) AS pack,
                        EXISTS (SELECT 1 FROM sources ql WHERE ql.item_id = i.id AND ql.kind = 'quest' AND ql.loot_type = 'raid') AS is_raid,
                        EXISTS (SELECT 1 FROM sources ql WHERE ql.item_id = i.id AND ql.kind IN ('quest', 'adventure_pack') AND ql.is_rare) AS is_rare
                 {from_sql} ORDER BY {order_sql} LIMIT {limit} OFFSET {offset}"
            );
            let mut items = json_rows(db, &page_sql, where_clause.params())?;
            for item in &mut items {
                convert_to_booleans(item, &["is_raid", "is_rare", "is_legacy"]);
            }
            Ok(Json(json!({ "total": total, "limit": limit, "offset": offset, "items": items })))
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
                   items by default), wiki URL, and `provenance`, `maetrim` or `wiki` as in the list), \
                   then `weapon` (dice, threat range, multipliers, \
                   proficiency, `dr_bypass`) or `armor` (AC, max Dex, spell failure, check penalty) when the item is \
                   one, `bonuses` (stat, bonus type, value), `effects` (named effects with value and target), \
                   `augment_slots` (sockets in order with their fixed `options`), `clickies`, `set`, `quests` it drops \
                   from (once per loot type, so a quest that both drops it and gives it as an end reward appears twice) with loot type, raid flag, `is_rare` (rare loot in that quest, per Maetrim's drop text or ddowiki), \
                   `chest` (the chest his drop text names for that quest, lower-cased, such as `end chest` or \
                   `optional chest`; null when it names none, and always null on a `reward` row), the \
                   `difficulties` each offers, ddowiki's `is_free_to_play` for each and its `provenance` (`maetrim`, or `wiki` \
                   for a quest read from ddowiki because his files lack it; see /v1/quests for the rest of the quest), `quest_chains` and `sagas` whose end reward offers the item (each with `id`, `name`, `is_rare` and the ddowiki page it was read from as `wiki_url`, a saga also with its reward `tier`; see /v1/quest-chains and /v1/sagas), \
                   `adventure_packs` any of whose quests drops it, as his drop text credits a whole pack (`Magic of \
                   Myth Drannor, any end chest`; each with `id`, `name`, `loot_type`, `chest` and `is_rare`, once per \
                   loot type; see /v1/adventure-packs/{id}), `sources`, every one of those sources in one array, each \
                   with `kind` (`quest`, `quest_chain`, `saga` or `adventure_pack`), the source's `id` and `name`, \
                   `loot_type` (null on a chain or saga reward), `chest`, `is_rare`, `tier` (a saga reward's list, \
                   null otherwise) and the source's ddowiki page as `wiki_url` (the page read for a chain or saga, the \
                   page named after a quest or pack otherwise), sorted by kind in that order and then by name, and \
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
                        i.accepts_sentience, i.is_minor_artifact, i.is_legacy, i.wiki_url, i.provenance
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
                augment_slot["options"] = Value::Array(json_rows(
                    db,
                    "SELECT name, description, min_level FROM item_augment_slot_options WHERE item_id = ?1 AND slot_order = ?2 ORDER BY option_order",
                    (id, slot_order),
                )?);
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
            item["sources"] = Value::Array(sources_via(db, "item_id", id)?);
            item["modifiers"] = Value::Array(modifiers_for(db, "item", id)?);
            Ok(Json(item))
        })
        .await
}

#[utoipa::path(
    get,
    path = "/v1/equipment-slots",
    tag = "items",
    summary = "List equipment slots",
    description = "The equipment slots an item can occupy, in display order, with a category (weapon, armor, \
                   accessory). /v1/items accepts these names in `slot`.",
    responses((status = 200, description = "The whole table", body = Vec<Value>))
)]
async fn equipment_slots(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    whole_table_json(state, "SELECT id, name, sort_order, category FROM equipment_slots ORDER BY sort_order", &[]).await
}

#[utoipa::path(
    get,
    path = "/v1/weapon-types",
    tag = "items",
    summary = "List weapon types",
    description = "Every weapon and shield type with the proficiency it needs and whether it is a shield. Item \
                   `weapon` blocks name their type from this list.",
    responses((status = 200, description = "The whole table", body = Vec<Value>))
)]
async fn weapon_types(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    whole_table_json(
        state,
        "SELECT wt.id, wt.name, p.name AS proficiency, wt.is_shield FROM weapon_types wt
           LEFT JOIN weapon_proficiencies p ON p.id = wt.proficiency_id ORDER BY wt.id",
        &["is_shield"],
    )
    .await
}

#[utoipa::path(
    get,
    path = "/v1/damage-types",
    tag = "items",
    summary = "List damage types",
    description = "Every damage type (physical, elemental, alignment, special) with its category. Spell damage \
                   lines and DR bypass entries use these names.",
    responses((status = 200, description = "The whole table", body = Vec<Value>))
)]
async fn damage_types(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    whole_table_json(state, "SELECT id, name, category FROM damage_types ORDER BY id", &[]).await
}

#[utoipa::path(
    get,
    path = "/v1/augment-slot-types",
    tag = "items",
    summary = "List augment slot types",
    description = "Every socket an item can carry: gem colours (`red`, `colorless`, `sun`, ...) and crafting-family \
                   sockets (`lamordia: melancholic (accessory)`, `isle of dread: set bonus`, ...). `family` says which \
                   kind it is; `label` is what /v1/augments accepts in `slot`.",
    responses((status = 200, description = "The whole table", body = Vec<Value>))
)]
async fn augment_slot_types(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    whole_table_json(
        state,
        "SELECT id, label, family, variant, qualifier FROM augment_slot_types ORDER BY family, label",
        &[],
    )
    .await
}
