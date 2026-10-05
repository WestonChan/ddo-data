use super::crafting::crafting_systems_making;
use super::quest_series::{quest_chains_rewarding, sagas_rewarding};
use super::quests::{
    adventure_packs_dropping_via, challenge_packs_rewarding, quests_dropping_via, sources_via, starter_rewards_of,
};
use super::vendors_and_events::{events_rewarding, vendors_offering};
use crate::db::{
    convert_to_booleans, effects_for_owner, effects_via, json_row, json_rows, like_escaped_text, modifiers_for,
    paged_query, paged_table_json, TableListSource, WhereClause,
};
use crate::error::ApiError;
use crate::query::{
    declare_list_parameters, declare_query_parameters, repeated_integer_values, repeated_key_values, ApiQuery,
};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use ddo_model::enums::{EquipmentSlot, ItemCategory};
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
        #[serde(default, deserialize_with = "repeated_key_values")]
        pub slot: Vec<String>,
        #[serde(default, deserialize_with = "repeated_key_values")]
        pub category: Vec<String>,
        pub min_level: Option<i64>,
        pub max_level: Option<i64>,
        #[serde(default, deserialize_with = "repeated_key_values")]
        pub pack: Vec<String>,
        pub pack_match: Option<String>,
        #[serde(default, deserialize_with = "repeated_key_values")]
        pub set: Vec<String>,
        pub set_match: Option<String>,
        pub raid: Option<bool>,
        pub rare: Option<bool>,
        #[serde(default, deserialize_with = "repeated_integer_values")]
        pub quest: Vec<i64>,
        pub quest_match: Option<String>,
        #[serde(default, deserialize_with = "repeated_integer_values")]
        pub quest_chain: Vec<i64>,
        pub quest_chain_match: Option<String>,
        #[serde(default, deserialize_with = "repeated_integer_values")]
        pub saga: Vec<i64>,
        pub saga_match: Option<String>,
        #[serde(default, deserialize_with = "repeated_key_values")]
        pub bonus: Vec<String>,
        pub bonus_match: Option<String>,
        pub include_set_bonuses: Option<bool>,
        pub include_legacy: Option<bool>,
    }
    repeatable: ["slot", "category", "pack", "set", "quest", "quest_chain", "saga", "bonus"]
    matchable: ["pack", "set", "quest", "quest_chain", "saga", "bonus"]
    single_value_match: ["slot", "category"]
}

const ITEM_LIST_COLUMNS: &str =
    "i.id, i.name, es.name AS slot, i.item_category AS category, i.item_type, i.minimum_level,
     i.enhancement_bonus, i.icon, i.is_legacy,
     (SELECT MIN(ap.name) FROM loot_adventure_packs packs
      JOIN adventure_packs ap ON ap.id = packs.pack_id WHERE packs.item_id = i.id) AS pack,
     (SELECT MIN(s.name) FROM set_bonus_items member JOIN set_bonuses s ON s.id = member.set_id
      WHERE member.item_id = i.id) AS \"set\",
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
    ("set", "\"set\""),
    ("enhancement_bonus", "i.enhancement_bonus"),
    ("icon", "icon"),
    ("is_legacy", "is_legacy"),
    ("is_raid", "is_raid"),
    ("is_rare", "is_rare"),
    ("item_type", "item_type"),
    ("min_level", "i.minimum_level"),
    ("max_level", "i.minimum_level"),
    ("raid", "is_raid"),
    ("rare", "is_rare"),
    ("quest", "(SELECT MIN(quest_id) FROM sources WHERE item_id = i.id AND kind = 'quest')"),
    ("quest_chain", "(SELECT MIN(chain_id) FROM sources WHERE item_id = i.id AND kind = 'quest_chain')"),
    ("saga", "(SELECT MIN(saga_id) FROM sources WHERE item_id = i.id AND kind = 'saga')"),
    (
        "bonus",
        "(SELECT MIN(e.name) FROM item_effects ie JOIN effects e ON e.id = ie.effect_id WHERE ie.item_id = i.id)",
    ),
];

declare_list_parameters!(
    ItemsParameters,
    ITEMS_SORT_FIELDS,
    "",
    Some(
        "Case-insensitive item-name substring or exact slot, category or pack name; exact and prefix names rank first without sort."
    )
);

#[utoipa::path(
    get,
    path = "/v1/items",
    tag = "items",
    summary = "List items",
    description = "Lists equipment matching the declared filters, with slot, level, pack, set and source flags.",
    params(
        ItemsParameters,
        ("slot" = Option<Vec<String>>, Query, style = Form, explode = true, description = "Repeat equipment slot names from /v1/equipment-slots to match any."),
        ("slot_match" = Option<String>, Query, description = "Unsupported for a single-slot item; this parameter returns 400."),
        ("category" = Option<Vec<String>>, Query, style = Form, explode = true, description = "Repeat Armor, Shield, Weapon, Jewelry or Clothing to match any."),
        ("category_match" = Option<String>, Query, description = "Unsupported for a single-category item; this parameter returns 400."),
        ("min_level" = Option<i64>, Query, description = "Minimum item level to include."),
        ("max_level" = Option<i64>, Query, description = "Maximum item level to include."),
        ("pack" = Option<Vec<String>>, Query, style = Form, explode = true, description = "Repeat adventure pack names reached through item sources."),
        ("pack_match" = Option<String>, Query, description = "Use `all` to require every named pack, or `any` by default."),
        ("set" = Option<Vec<String>>, Query, style = Form, explode = true, description = "Repeat set names from /v1/sets to match member items."),
        ("set_match" = Option<String>, Query, description = "Use `all` to require every named set, or `any` by default."),
        ("raid" = Option<bool>, Query, description = "Use true to keep items from raid quests."),
        ("rare" = Option<bool>, Query, description = "Use true to keep rare loot from quests or packs."),
        ("quest" = Option<Vec<i64>>, Query, style = Form, explode = true, description = "Repeat quest ids from /v1/quests to match their item drops."),
        ("quest_match" = Option<String>, Query, description = "Use `all` to require every quest, or `any` by default."),
        ("quest_chain" = Option<Vec<i64>>, Query, style = Form, explode = true, description = "Repeat quest chain ids to match end rewards."),
        ("quest_chain_match" = Option<String>, Query, description = "Use `all` to require every quest chain, or `any` by default."),
        ("saga" = Option<Vec<i64>>, Query, style = Form, explode = true, description = "Repeat saga ids to match end rewards."),
        ("saga_match" = Option<String>, Query, description = "Use `all` to require every saga, or `any` by default."),
        ("bonus" = Option<Vec<String>>, Query, style = Form, explode = true, description = "Repeat effect, ladder or stat names; `Constitution:Insight` narrows by type, and `Constitution:Insightful` also works."),
        ("bonus_match" = Option<String>, Query, description = "Use `all` to require every bonus, or `any` by default."),
        ("include_set_bonuses" = Option<bool>, Query, description = "Use true to match effects through the item's set tiers too."),
        ("include_legacy" = Option<bool>, Query, description = "Use true to include legacy item versions in the list."),
    ),
    responses(
        (status = 200, description = "`total`, `limit`, `offset` and the `items` page", body = crate::routes::v1::response_schemas::ItemsPageResponse),
        (status = 400, description = "Unknown category or bonus, or an unknown or malformed query parameter", body = crate::error::ErrorBody)
    )
)]
async fn items(
    State(state): State<AppState>,
    ApiQuery(query, filters): ApiQuery<ItemFilters>,
) -> Result<Json<Value>, ApiError> {
    for slot in &filters.slot {
        if !EquipmentSlot::ALL.iter().any(|known| known.name() == slot) {
            return Err(ApiError::BadRequest(format!("unknown slot {slot:?}")));
        }
    }
    for category in &filters.category {
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
            where_clause.add_repeated_filter("es.name IN (?)", "es.name = ?", &filters.slot, None);
            where_clause.add_repeated_filter("i.item_category IN (?)", "i.item_category = ?", &filters.category, None);
            if let Some(min_level) = filters.min_level {
                where_clause.add_bound_condition("i.minimum_level >= ?", min_level);
            }
            if let Some(max_level) = filters.max_level {
                where_clause.add_bound_condition("i.minimum_level <= ?", max_level);
            }
            where_clause.add_repeated_filter(
                "EXISTS (SELECT 1 FROM loot_adventure_packs packs JOIN adventure_packs ap ON ap.id = packs.pack_id WHERE packs.item_id = i.id AND ap.name IN (?))",
                "EXISTS (SELECT 1 FROM loot_adventure_packs packs JOIN adventure_packs ap ON ap.id = packs.pack_id WHERE packs.item_id = i.id AND ap.name = ?)",
                &filters.pack, filters.pack_match.as_deref(),
            );
            for set_name in &filters.set {
                let known_set: bool = db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM set_bonuses WHERE name = ?1)",
                    [set_name],
                    |row| row.get(0),
                )?;
                if !known_set {
                    return Err(ApiError::BadRequest(format!("unknown set {set_name:?}")));
                }
            }
            where_clause.add_repeated_filter(
                "EXISTS (SELECT 1 FROM set_bonus_items member JOIN set_bonuses s ON s.id = member.set_id WHERE member.item_id = i.id AND s.name IN (?))",
                "EXISTS (SELECT 1 FROM set_bonus_items member JOIN set_bonuses s ON s.id = member.set_id WHERE member.item_id = i.id AND s.name = ?)",
                &filters.set, filters.set_match.as_deref(),
            );
            if filters.raid == Some(true) {
                where_clause.add_condition("EXISTS (SELECT 1 FROM sources ql WHERE ql.item_id = i.id AND ql.kind = 'quest' AND ql.loot_type = 'raid')");
            }
            if filters.rare == Some(true) {
                where_clause.add_condition("EXISTS (SELECT 1 FROM sources ql WHERE ql.item_id = i.id AND ql.kind IN ('quest', 'adventure_pack') AND ql.is_rare)");
            }
            where_clause.add_repeated_filter(
                "i.id IN (SELECT d.item_id FROM sources d WHERE d.kind = 'quest' AND d.quest_id IN (?))",
                "i.id IN (SELECT d.item_id FROM sources d WHERE d.kind = 'quest' AND d.quest_id = ?)",
                &filters.quest, filters.quest_match.as_deref(),
            );
            where_clause.add_repeated_filter(
                "i.id IN (SELECT d.item_id FROM sources d WHERE d.kind = 'quest_chain' AND d.chain_id IN (?))",
                "i.id IN (SELECT d.item_id FROM sources d WHERE d.kind = 'quest_chain' AND d.chain_id = ?)",
                &filters.quest_chain, filters.quest_chain_match.as_deref(),
            );
            where_clause.add_repeated_filter(
                "i.id IN (SELECT d.item_id FROM sources d WHERE d.kind = 'saga' AND d.saga_id IN (?))",
                "i.id IN (SELECT d.item_id FROM sources d WHERE d.kind = 'saga' AND d.saga_id = ?)",
                &filters.saga, filters.saga_match.as_deref(),
            );
            let bonus_conditions = filters.bonus.iter().map(|name| {
                bonus_match_sql(db, name, filters.include_set_bonuses == Some(true))
            }).collect::<Result<Vec<_>, _>>()?;
            if !bonus_conditions.is_empty() {
                let separator = if filters.bonus_match.as_deref() == Some("all") { " AND " } else { " OR " };
                where_clause.add_condition(&format!("({})", bonus_conditions.join(separator)));
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
     OR i.id IN (SELECT packs.item_id FROM loot_adventure_packs packs \
                 JOIN adventure_packs ap ON ap.id = packs.pack_id WHERE ap.name LIKE ? ESCAPE '\\'))";

fn bonus_match_sql(db: &Connection, name: &str, include_set_bonuses: bool) -> Result<String, ApiError> {
    use rusqlite::OptionalExtension;

    let (stat_name, type_name) = name.split_once(':').map_or((name, None), |(stat, kind)| (stat, Some(kind)));
    let stat_id = db
        .query_row("SELECT id FROM effects WHERE name = ?1 AND is_stat = 1", [stat_name], |row| row.get::<_, i64>(0))
        .optional()?;
    let type_id = match type_name {
        Some(type_name) => {
            if stat_id.is_none() {
                return Err(ApiError::BadRequest(format!("unknown bonus {name:?}")));
            }
            Some(
                db.query_row("SELECT id FROM bonus_types WHERE name = ?1 UNION SELECT bonus_type_id FROM bonus_type_aliases WHERE name = ?1", [type_name], |row| row.get::<_, i64>(0))
                    .optional()?
                    .ok_or_else(|| ApiError::BadRequest(format!("unknown bonus type {type_name:?}")))?,
            )
        }
        None => None,
    };
    let family_ids = if type_name.is_none() {
        let mut statement = db.prepare_cached("SELECT e.id FROM effects e LEFT JOIN effect_tier_groups tg ON tg.id = e.tier_group_id WHERE e.name = ?1 OR tg.name = ?1")?;
        let ids = statement.query_map([name], |row| row.get::<_, i64>(0))?.collect::<Result<Vec<_>, _>>()?;
        ids
    } else {
        Vec::new()
    };
    let group_id = if type_name.is_none() {
        db.query_row("SELECT id FROM effects WHERE name = ?1 AND is_group = 1", [name], |row| row.get::<_, i64>(0))
            .optional()?
    } else {
        None
    };
    if stat_id.is_none() && family_ids.is_empty() {
        return Err(ApiError::BadRequest(format!("unknown bonus {name:?}")));
    }
    let matches_link = |link: &str, owner_kind: &str, owner_column: &str| {
        let mut alternatives = Vec::new();
        if !family_ids.is_empty() {
            alternatives.push(format!(
                "{link}.effect_id IN ({})",
                family_ids.iter().map(ToString::to_string).collect::<Vec<_>>().join(",")
            ));
        }
        if let Some(stat_id) = stat_id {
            let type_condition = type_id.map_or(String::new(), |type_id| format!(" AND ob.bonus_type_id = {type_id}"));
            alternatives.push(format!("EXISTS (SELECT 1 FROM owner_bonuses ob WHERE ob.owner_kind = '{owner_kind}' AND ob.owner_id = {link}.{owner_column} AND ob.effect_link_order = {link}.sort_order AND ob.stat_id = {stat_id}{type_condition})"));
        }
        if let Some(group_id) = group_id {
            alternatives.push(format!("EXISTS (SELECT 1 FROM owner_bonuses ob WHERE ob.owner_kind = '{owner_kind}' AND ob.owner_id = {link}.{owner_column} AND ob.effect_link_order = {link}.sort_order AND ob.group_effect_id = {group_id})"));
        }
        format!("({})", alternatives.join(" OR "))
    };
    let own = format!(
        "EXISTS (SELECT 1 FROM item_effects ie WHERE ie.item_id = i.id AND {})",
        matches_link("ie", "item", "item_id")
    );
    if include_set_bonuses {
        let set = format!("EXISTS (SELECT 1 FROM set_bonus_items sbi JOIN set_bonus_tiers t ON t.set_id = sbi.set_id JOIN set_bonus_tier_effects te ON te.tier_id = t.id WHERE sbi.item_id = i.id AND {})", matches_link("te", "set_bonus_tier", "tier_id"));
        Ok(format!("({own} OR {set})"))
    } else {
        Ok(own)
    }
}

#[utoipa::path(
    get,
    path = "/v1/items/{id}",
    tag = "items",
    summary = "Get an item",
    description = "Returns an item with rendered effects, sockets, modifiers and drop sources.",
    params(("id" = i64, Path, description = "The item's numeric id from the list endpoint")), responses((status = 200, description = "The item with its child collections", body = crate::routes::v1::response_schemas::ItemsDetailResponse), (status = 404, description = "No item has this id", body = crate::error::ErrorBody))
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

            item["effects"] = Value::Array(effects_for_owner(db, "item_effects", "item_id", id)?);

            let mut augment_slots = json_rows(
                db,
                "SELECT s.sort_order, t.id AS slot_type_id, t.label, t.family, t.variant, t.qualifier FROM item_augment_slots s
                   JOIN augment_slot_types t ON t.id = s.slot_id WHERE s.item_id = ?1 ORDER BY s.sort_order",
                [id],
            )?;
            let mut option_effects = effects_via(
                db,
                "item_augment_slot_option_effects",
                "option_id",
                "j.option_id IN (SELECT id FROM item_augment_slot_options WHERE item_id = ?1)",
                [id],
            )?;
            for augment_slot in &mut augment_slots {
                let slot_order = augment_slot["sort_order"].as_i64().unwrap_or(0);
                augment_slot["options"] = Value::Array(augment_slot_options(db, id, slot_order, &mut option_effects)?);
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

fn augment_slot_options(
    db: &Connection,
    item_id: i64,
    slot_order: i64,
    option_effects: &mut std::collections::BTreeMap<i64, Vec<Value>>,
) -> Result<Vec<Value>, ApiError> {
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
        option["effects"] = Value::Array(option_effects.remove(&option_id).unwrap_or_default());
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
    description = "Lists equipment slots, their categories and display order.",
    params(
        EquipmentSlotsParameters,
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `equipment_slots` page", body = crate::routes::v1::response_schemas::EquipmentSlotsPageResponse),
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
    &[("name", "listed.name"), ("id", "listed.id"), ("proficiency", "listed.proficiency"), ("is_shield", "is_shield")];

declare_list_parameters!(WeaponTypesParameters, WEAPON_TYPES_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/weapon-types",
    tag = "items",
    summary = "List weapon types",
    description = "Lists weapon and shield types with proficiency and shield status.",
    params(
        WeaponTypesParameters,
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `weapon_types` page", body = crate::routes::v1::response_schemas::WeaponTypesPageResponse),
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
    description = "Lists damage types and their categories.",
    params(
        DamageTypesParameters,
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `damage_types` page", body = crate::routes::v1::response_schemas::DamageTypesPageResponse),
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
    ("label", "label"),
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
    description = "Lists socket labels and their family, variant and qualifier.",
    params(
        AugmentSlotTypesParameters,
    ),
    responses(
        (status = 200, description = "`total`, `limit`, `offset` and the `augment_slot_types` page", body = crate::routes::v1::response_schemas::AugmentSlotTypesPageResponse),
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
