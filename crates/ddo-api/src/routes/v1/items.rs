mod records;

use super::vocabularies::VocabularySource;
use super::ValueVocabulary;

use crate::db::{
    convert_to_booleans, like_escaped_text, paged_query, paged_rows_with_filter, paged_table_json, TableListSource,
    WhereClause,
};
use crate::effect_names::{effect_name_search_sql, resolve_effect_name};
use crate::entity_slugs::EntityPath;
use crate::error::ApiError;
use crate::query::{
    declare_list_parameters, declare_query_parameters, repeated_key_values, ApiQuery, EffectName, EntityFilter,
    EFFECT_NAME_SEARCH,
};
use crate::state::AppState;
use axum::extract::State;
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
        .routes(routes!(weapon_abilities))
        .routes(routes!(item_materials))
        .routes(routes!(damage_types))
        .routes(routes!(augment_slot_types))
        .routes(routes!(augment_slot_type_detail))
}

declare_query_parameters! {
    pub(super) struct ItemFilters {
        #[serde(default, deserialize_with = "repeated_key_values")]
        pub slot: Vec<String>,
        #[serde(default, deserialize_with = "repeated_key_values")]
        pub category: Vec<String>,
        #[serde(default, deserialize_with = "repeated_key_values")]
        pub attack_ability: Vec<String>,
        pub attack_ability_match: Option<String>,
        #[serde(default, deserialize_with = "repeated_key_values")]
        pub damage_ability: Vec<String>,
        pub damage_ability_match: Option<String>,
        #[serde(default, deserialize_with = "repeated_key_values")]
        pub material: Vec<String>,
        #[serde(default, deserialize_with = "repeated_key_values")]
        pub socket: Vec<String>,
        pub socket_match: Option<String>,
        pub min_level: Option<i64>,
        pub max_level: Option<i64>,
        #[serde(default, deserialize_with = "repeated_key_values")]
        pub pack: Vec<String>,
        pub pack_match: Option<String>,
        #[serde(default)]
        pub crafting_system: EntityFilter,
        pub crafting_system_match: Option<String>,
        #[serde(default, deserialize_with = "repeated_key_values")]
        pub set: Vec<String>,
        pub set_match: Option<String>,
        pub raid: Option<bool>,
        pub rare: Option<bool>,
        #[serde(default)]
        pub quest: EntityFilter,
        pub quest_match: Option<String>,
        #[serde(default)]
        pub quest_chain: EntityFilter,
        pub quest_chain_match: Option<String>,
        #[serde(default)]
        pub saga: EntityFilter,
        pub saga_match: Option<String>,
        #[serde(default, deserialize_with = "repeated_key_values")]
        pub bonus: Vec<EffectName>,
        pub bonus_match: Option<String>,
        pub include_set_bonuses: Option<bool>,
        pub include_legacy: Option<bool>,
        pub view: Option<String>,
    }
    vocabularies: {
        slot => EquipmentSlots, "Repeat equipment slot names from /v1/equipment-slots to match any; omitted applies no filter.",
        category => ItemCategories, "Repeat Armor, Shield, Weapon, Jewelry or Clothing to match any; omitted applies no filter.",
        attack_ability => WeaponAbilities, "Repeat exact names from /v1/weapon-abilities to match a weapon's attack abilities; omitted applies no filter.",
        damage_ability => WeaponAbilities, "Repeat exact names from /v1/weapon-abilities to match a weapon's damage abilities; omitted applies no filter.",
        material => ItemMaterials, "Repeat names from /v1/item-materials to match any material; omitted applies no filter.",
        socket => AugmentSlotTypes, "Repeat case-insensitive labels from /v1/augment-slot-types to match item sockets; omitted applies no filter.",
        crafting_system => CraftingSystems, "Repeat current or redirected slugs or numeric ids from /v1/crafting-systems to match items made or upgraded; omitted applies no filter.",
        pack => AdventurePacks, "Repeat names from /v1/adventure-packs reached through item sources; omitted applies no filter.",
        set => Sets, "Repeat set names from /v1/sets to match member items; omitted applies no filter.",
        quest => Quests, "Repeat current or redirected slugs or numeric ids from /v1/quests to match their item drops; omitted applies no filter.",
        quest_chain => QuestChains, "Repeat current or redirected slugs or numeric ids from /v1/quest-chains to match end rewards; omitted applies no filter.",
        saga => Sagas, "Repeat current or redirected slugs or numeric ids from /v1/sagas to match end rewards; omitted applies no filter.",
        view => ItemViews, "Use `summary` (default) for compact rows or `full` for detail records; full pages cap at 50 rows."; example = "full",
    }
    matchable: ["attack_ability", "damage_ability", "socket", "pack", "crafting_system", "set", "quest", "quest_chain", "saga", "bonus"]
    single_value_match: ["slot", "category", "material"]
}

pub(super) fn item_categories_vocabulary() -> VocabularySource {
    VocabularySource::of_names(ItemCategory::ALL.iter().map(|category| category.as_str()))
}

pub(super) fn item_views_vocabulary() -> VocabularySource {
    VocabularySource::of_names(["summary", "full"])
}

const ITEM_LIST_COLUMNS: &str =
    "i.id, i.slug, i.name, es.name AS slot, i.item_category AS category, i.item_type, i.minimum_level,
     i.enhancement_bonus, i.icon, i.is_legacy,
     (SELECT MIN(ap.name) FROM loot_adventure_packs packs
      JOIN adventure_packs ap ON ap.id = packs.pack_id WHERE packs.item_id = i.id) AS pack,
     (SELECT s.name FROM set_bonus_items member JOIN set_bonuses s ON s.id = member.set_id
      WHERE member.item_id = i.id ORDER BY (s.name <> i.set_bonus), s.name LIMIT 1) AS \"set\",
     (SELECT COALESCE(json_group_array(name), '[]') FROM (
        SELECT s.name FROM set_bonus_items member JOIN set_bonuses s ON s.id = member.set_id
         WHERE member.item_id = i.id ORDER BY (s.name <> i.set_bonus), s.name)) AS sets,
     EXISTS (SELECT 1 FROM sources ql
             WHERE ql.item_id = i.id AND ql.kind = 'quest' AND ql.loot_type = 'raid') AS is_raid,
     EXISTS (SELECT 1 FROM sources ql
             WHERE ql.item_id = i.id AND ql.kind IN ('quest', 'adventure_pack') AND ql.is_rare) AS is_rare";

const ITEMS_SORT_FIELDS: &[(&str, &str)] = &[
    ("name", "i.name"),
    ("slug", "i.slug"),
    ("id", "i.id"),
    ("minimum_level", "i.minimum_level"),
    ("slot", "es.name"),
    ("category", "i.item_category"),
    ("attack_ability", "(SELECT w.attack_modifier FROM item_weapon_stats w WHERE w.item_id = i.id)"),
    ("damage_ability", "(SELECT w.damage_modifier FROM item_weapon_stats w WHERE w.item_id = i.id)"),
    ("material", "(SELECT m.name FROM item_materials m WHERE m.id = i.material_id)"),
    ("socket", "(SELECT MIN(t.label) FROM item_augment_slots s JOIN augment_slot_types t ON t.id = s.slot_id WHERE s.item_id = i.id)"),
    ("pack", "pack"),
    ("crafting_system", "(SELECT MIN(crafting_system_id) FROM crafting_system_items WHERE item_id = i.id)"),
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
    EFFECT_NAME_SEARCH.with_description(
        "Case-insensitive item or effect-name substring, including aliases, or exact slot, category or pack name; own names rank first without sort."
    )
);

#[utoipa::path(
    get,
    path = "/v1/items",
    tag = "items",
    summary = "List items",
    description = "Lists equipment matching the declared filters; `view=full` returns complete item details.",
    params(
        ItemsParameters,
        ItemFilters,
        ("min_level" = Option<i64>, Query, description = "Minimum item level to include."),
        ("max_level" = Option<i64>, Query, description = "Maximum item level to include."),
        ("raid" = Option<bool>, Query, description = "Use true to keep items from raid quests."),
        ("rare" = Option<bool>, Query, description = "Use true to keep rare loot from quests or packs."),
        ("include_set_bonuses" = Option<bool>, Query, description = "Use true to match effects through the item's set tiers too."),
        ("include_legacy" = Option<bool>, Query, description = "Use true to include legacy item versions in the list."),
    ),
    responses(
        (status = 200, description = "`total`, `limit`, `offset` and the `items` page", body = crate::routes::v1::response_schemas::ItemsPageResponse),
        (status = 400, description = "Unknown filter value, or an unknown or malformed query parameter", body = crate::error::ErrorBody)
    )
)]
async fn items(
    State(state): State<AppState>,
    ApiQuery(query, filters): ApiQuery<ItemFilters>,
) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let filters = filters.validated(db)?;
            let full_view = filters.view.as_deref() == Some("full");
            let mut query = query;
            if full_view {
                query.limit = Some(query.limit.unwrap_or(50).min(50));
            }
            let mut where_clause = WhereClause::default();
            if filters.include_legacy != Some(true) {
                where_clause.add_condition("NOT i.is_legacy");
            }
            let mut default_order = "i.name".to_string();
            if let Some(search_text) = query.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
                let search_placeholder =
                    where_clause.add_bound_condition(&items_matching_search_text_sql(), like_escaped_text(search_text));
                default_order = format!(
                    "CASE WHEN i.name LIKE {search_placeholder} ESCAPE '\\' THEN 0 \
                          WHEN i.name LIKE {search_placeholder} || '%' ESCAPE '\\' THEN 1 ELSE 2 END, i.name"
                );
            }
            where_clause.add_repeated_filter("es.name IN (?)", "es.name = ?", &filters.slot, None);
            where_clause.add_repeated_filter("i.item_category IN (?)", "i.item_category = ?", &filters.category, None);
            for (role, abilities, mode) in [
                ("attack", &filters.attack_ability, filters.attack_ability_match.as_deref()),
                ("damage", &filters.damage_ability, filters.damage_ability_match.as_deref()),
            ] {
                where_clause.add_repeated_filter(
                    &format!("EXISTS (SELECT 1 FROM item_weapon_abilities wa WHERE wa.item_id = i.id AND wa.role = '{role}' AND wa.ability IN (?))"),
                    &format!("EXISTS (SELECT 1 FROM item_weapon_abilities wa WHERE wa.item_id = i.id AND wa.role = '{role}' AND wa.ability = ?)"),
                    abilities, mode,
                );
            }
            where_clause.add_repeated_filter(
                "i.material_id IN (SELECT id FROM item_materials WHERE name IN (?))",
                "i.material_id IN (SELECT id FROM item_materials WHERE name = ?)",
                &filters.material, None,
            );
            where_clause.add_repeated_filter(
                "EXISTS (SELECT 1 FROM item_augment_slots s JOIN augment_slot_types t ON t.id = s.slot_id WHERE s.item_id = i.id AND t.label IN (?))",
                "EXISTS (SELECT 1 FROM item_augment_slots s JOIN augment_slot_types t ON t.id = s.slot_id WHERE s.item_id = i.id AND t.label = ?)",
                &filters.socket, filters.socket_match.as_deref(),
            );
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
            where_clause.add_repeated_filter(
                "i.id IN (SELECT reach.item_id FROM crafting_system_items reach WHERE reach.crafting_system_id IN (?))",
                "i.id IN (SELECT reach.item_id FROM crafting_system_items reach WHERE reach.crafting_system_id = ?)",
                filters.crafting_system.ids(), filters.crafting_system_match.as_deref(),
            );
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
                filters.quest.ids(), filters.quest_match.as_deref(),
            );
            where_clause.add_repeated_filter(
                "i.id IN (SELECT d.item_id FROM sources d WHERE d.kind = 'quest_chain' AND d.chain_id IN (?))",
                "i.id IN (SELECT d.item_id FROM sources d WHERE d.kind = 'quest_chain' AND d.chain_id = ?)",
                filters.quest_chain.ids(), filters.quest_chain_match.as_deref(),
            );
            where_clause.add_repeated_filter(
                "i.id IN (SELECT d.item_id FROM sources d WHERE d.kind = 'saga' AND d.saga_id IN (?))",
                "i.id IN (SELECT d.item_id FROM sources d WHERE d.kind = 'saga' AND d.saga_id = ?)",
                filters.saga.ids(), filters.saga_match.as_deref(),
            );
            let bonus_conditions = filters.bonus.iter().map(|name| {
                bonus_match_sql(db, &name.0, filters.include_set_bonuses == Some(true))
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
            if full_view {
                let item_ids = page
                    .rows
                    .iter()
                    .map(|item| item["id"].as_i64().ok_or_else(|| anyhow::anyhow!("listed item has no numeric id")))
                    .collect::<Result<Vec<_>, _>>()?;
                let mut records = records::item_records(db, &item_ids)?;
                page.rows = item_ids
                    .into_iter()
                    .map(|id| records.remove(&id).ok_or(ApiError::NotFound))
                    .collect::<Result<Vec<_>, _>>()?;
            }
            Ok(Json(page.into_json("items")))
        })
        .await
}

fn items_matching_search_text_sql() -> String {
    let effect_names = effect_name_search_sql("linked.effect_id", "?");
    format!(
        "(i.name LIKE '%' || ? || '%' ESCAPE '\\' \
     OR es.name LIKE ? ESCAPE '\\' OR i.item_category LIKE ? ESCAPE '\\' \
     OR EXISTS (SELECT 1 FROM item_effects linked \
                WHERE linked.item_id = i.id AND {effect_names}) \
     OR i.id IN (SELECT packs.item_id FROM loot_adventure_packs packs \
                 JOIN adventure_packs ap ON ap.id = packs.pack_id WHERE ap.name LIKE ? ESCAPE '\\'))"
    )
}

fn bonus_match_sql(db: &Connection, name: &str, include_set_bonuses: bool) -> Result<String, ApiError> {
    use rusqlite::OptionalExtension;

    let (stat_name, type_name) = name.split_once(':').map_or((name, None), |(stat, kind)| (stat, Some(kind)));
    let resolved = resolve_effect_name(db, stat_name)?;
    let (stat_id, group_id) = match &resolved {
        Some(effect) => db.query_row(
            "SELECT CASE WHEN is_stat THEN id END, CASE WHEN is_group THEN id END FROM effects WHERE id = ?1",
            [effect.effect_id],
            |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, Option<i64>>(1)?)),
        )?,
        None => (None, None),
    };
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
        let mut statement = db.prepare_cached(
            "SELECT e.id FROM effects e JOIN effect_tier_groups tg ON tg.id = e.tier_group_id WHERE tg.name = ?1",
        )?;
        let mut ids = statement.query_map([name], |row| row.get::<_, i64>(0))?.collect::<Result<Vec<_>, _>>()?;
        if let Some(effect) = resolved {
            ids.push(effect.effect_id);
        }
        ids.sort_unstable();
        ids.dedup();
        ids
    } else {
        Vec::new()
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
    path = "/v1/items/{slug}",
    tag = "items",
    summary = "Get an item",
    description = "Returns an item with rendered effects, sockets, modifiers and drop sources.",
    params(("slug" = String, Path, description = "Current entity slug or numeric id from the corresponding list.")), responses((status = 200, description = "The item with its child collections", body = crate::routes::v1::response_schemas::ItemsDetailResponse), (status = 308, description = "Renamed slug; follow Location to its current slug."), (status = 404, description = "No item has this slug or numeric id", body = crate::error::ErrorBody))
)]
async fn item_detail(State(state): State<AppState>, EntityPath(id): EntityPath) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut records = records::item_records(db, &[id])?;
            Ok(Json(records.remove(&id).ok_or(ApiError::NotFound)?))
        })
        .await
}

pub(super) fn equipment_slots_vocabulary() -> VocabularySource {
    VocabularySource::new("name", &["name"], "SELECT name FROM equipment_slots")
        .served_by("/v1/equipment-slots", "equipment_slots")
}

const EQUIPMENT_SLOTS_SORT_FIELDS: &[(&str, &str)] = &[
    ("name", "listed.name"),
    ("slug", "listed.name"),
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

const WEAPON_TYPES_SORT_FIELDS: &[(&str, &str)] = &[
    ("name", "listed.name"),
    ("slug", "listed.name"),
    ("id", "listed.id"),
    ("proficiency", "listed.proficiency"),
    ("is_shield", "is_shield"),
];

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
            default_order: "listed.name",
            sortable_fields: WEAPON_TYPES_SORT_FIELDS,
            flag_columns: &["is_shield"],
        },
    )
    .await
}

pub(super) fn weapon_abilities_vocabulary() -> VocabularySource {
    VocabularySource::new(
        "name",
        &["sort_order"],
        format!(
            "SELECT e.id, e.slug, e.name, {} AS sort_order FROM effects e
                 WHERE e.is_stat = 1 AND e.category = 'ability'",
            ddo_model::stats::ability_sort_order_sql()
        ),
    )
    .with_counted_listing(
        |vocabulary| {
            format!(
                "{vocabulary}.*, COALESCE(counted.attack_item_count, 0) AS attack_item_count,
             COALESCE(counted.damage_item_count, 0) AS damage_item_count"
            )
        },
        |vocabulary| {
            format!(
                "LEFT JOIN (
                SELECT wa.ability,
                    COUNT(DISTINCT wa.item_id) FILTER (WHERE wa.role = 'attack') AS attack_item_count,
                    COUNT(DISTINCT wa.item_id) FILTER (WHERE wa.role = 'damage') AS damage_item_count
                FROM item_weapon_abilities wa JOIN items i ON i.id = wa.item_id
                WHERE NOT i.is_legacy GROUP BY wa.ability
            ) counted ON counted.ability = {vocabulary}.name"
            )
        },
    )
    .served_by("/v1/weapon-abilities", "weapon_abilities")
}

const WEAPON_ABILITIES_SORT_FIELDS: &[(&str, &str)] = &[
    ("name", "listed.name"),
    ("slug", "listed.slug"),
    ("id", "listed.id"),
    ("sort_order", "listed.sort_order"),
    ("attack_item_count", "listed.attack_item_count"),
    ("damage_item_count", "listed.damage_item_count"),
];

declare_list_parameters!(WeaponAbilitiesParameters, WEAPON_ABILITIES_SORT_FIELDS, "");

#[utoipa::path(
    get, path = "/v1/weapon-abilities", tag = "items", summary = "List weapon abilities",
    description = "Lists the six abilities in game order with non-legacy weapon counts for attack and damage.",
    params(WeaponAbilitiesParameters),
    responses(
        (status = 200, description = "The weapon abilities page with attack and damage item counts", body = super::response_schemas::WeaponAbilitiesPageResponse),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn weapon_abilities(
    State(state): State<AppState>,
    ApiQuery(query, _): ApiQuery,
) -> Result<Json<Value>, ApiError> {
    paged_table_json(
        state,
        query,
        TableListSource {
            select_sql: ValueVocabulary::WeaponAbilities.select_sql(),
            rows_key: "weapon_abilities",
            name_column: "listed.name",
            default_order: ValueVocabulary::WeaponAbilities.default_order(),
            sortable_fields: WEAPON_ABILITIES_SORT_FIELDS,
            flag_columns: &[],
        },
    )
    .await
}

pub(super) fn item_materials_vocabulary() -> VocabularySource {
    VocabularySource::new("name", &["sort_order", "name"], "SELECT id, name, sort_order FROM item_materials")
        .with_counted_listing(
            |vocabulary| {
                format!("{vocabulary}.name, {vocabulary}.sort_order, COALESCE(counted.item_count, 0) AS item_count")
            },
            |vocabulary| {
                format!(
                    "LEFT JOIN (
                SELECT material_id, COUNT(*) AS item_count FROM items WHERE NOT is_legacy GROUP BY material_id
            ) counted ON counted.material_id = {vocabulary}.id"
                )
            },
        )
        .served_by("/v1/item-materials", "item_materials")
}

const ITEM_MATERIALS_SORT_FIELDS: &[(&str, &str)] =
    &[("name", "listed.name"), ("sort_order", "listed.sort_order"), ("item_count", "listed.item_count")];

declare_list_parameters!(ItemMaterialsParameters, ITEM_MATERIALS_SORT_FIELDS, "");

#[utoipa::path(
    get, path = "/v1/item-materials", tag = "items", summary = "List item materials",
    description = "Lists item materials with their display order and non-legacy item counts.",
    params(ItemMaterialsParameters),
    responses(
        (status = 200, description = "The item materials page with item counts", body = super::response_schemas::ItemMaterialsPageResponse),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn item_materials(State(state): State<AppState>, ApiQuery(query, _): ApiQuery) -> Result<Json<Value>, ApiError> {
    paged_table_json(
        state,
        query,
        TableListSource {
            select_sql: ValueVocabulary::ItemMaterials.select_sql(),
            rows_key: "item_materials",
            name_column: "listed.name",
            default_order: ValueVocabulary::ItemMaterials.default_order(),
            sortable_fields: ITEM_MATERIALS_SORT_FIELDS,
            flag_columns: &[],
        },
    )
    .await
}

const DAMAGE_TYPES_SORT_FIELDS: &[(&str, &str)] =
    &[("name", "listed.name"), ("slug", "listed.name"), ("id", "listed.id"), ("category", "listed.category")];

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
            default_order: "listed.name",
            sortable_fields: DAMAGE_TYPES_SORT_FIELDS,
            flag_columns: &[],
        },
    )
    .await
}

pub(super) fn augment_slot_types_vocabulary() -> VocabularySource {
    VocabularySource::new(
        "label",
        &["family", "label"],
        "SELECT id, slug, label, family, variant, qualifier, not_crafting_reason FROM augment_slot_types",
    )
    .with_counted_listing(
        |vocabulary| format!("{vocabulary}.*, COALESCE(counted.item_count, 0) AS item_count"),
        |vocabulary| {
            format!(
                "LEFT JOIN (
                SELECT s.slot_id, COUNT(DISTINCT s.item_id) AS item_count
                FROM item_augment_slots s JOIN items i ON i.id = s.item_id
                WHERE NOT i.is_legacy GROUP BY s.slot_id
            ) counted ON counted.slot_id = {vocabulary}.id"
            )
        },
    )
    .served_by("/v1/augment-slot-types", "augment_slot_types")
    .case_insensitive()
}

declare_query_parameters! {
    struct AugmentSlotTypeFilters {
        #[serde(default, deserialize_with = "repeated_key_values")]
        family: Vec<String>,
    }
    vocabularies: {
        family => AugmentSlotFamilies, "Repeat family names present in /v1/augment-slot-types to match any; omitted includes every family.",
    }
}

pub(super) fn augment_slot_families_vocabulary() -> VocabularySource {
    VocabularySource::new("family", &["family"], "SELECT DISTINCT family FROM augment_slot_types")
        .served_by("/v1/augment-slot-types", "augment_slot_types")
}

const AUGMENT_SLOT_TYPES_SORT_FIELDS: &[(&str, &str)] = &[
    ("name", "listed.label"),
    ("slug", "listed.label"),
    ("id", "listed.id"),
    ("family", "listed.family"),
    ("variant", "listed.variant"),
    ("qualifier", "listed.qualifier"),
    ("label", "label"),
    ("not_crafting_reason", "not_crafting_reason"),
    ("item_count", "listed.item_count"),
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
    description = "Lists socket labels, their family, variant and qualifier, and non-legacy item counts.",
    params(
        AugmentSlotTypesParameters,
        AugmentSlotTypeFilters,
    ),
    responses(
        (status = 200, description = "`total`, `limit`, `offset` and the `augment_slot_types` page", body = crate::routes::v1::response_schemas::AugmentSlotTypesPageResponse),
        (status = 400, description = "Unknown family, invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn augment_slot_types(
    State(state): State<AppState>,
    ApiQuery(query, filters): ApiQuery<AugmentSlotTypeFilters>,
) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let filters = filters.validated(db)?;
            let mut where_clause = WhereClause::default();
            where_clause.add_repeated_filter("listed.family IN (?)", "listed.family = ?", &filters.family, None);
            let page = paged_rows_with_filter(
                db,
                ValueVocabulary::AugmentSlotTypes.select_sql(),
                &query,
                "listed.label",
                ValueVocabulary::AugmentSlotTypes.default_order(),
                AUGMENT_SLOT_TYPES_SORT_FIELDS,
                where_clause,
            )?;
            Ok(Json(page.into_json("augment_slot_types")))
        })
        .await
}

#[utoipa::path(get,path="/v1/augment-slot-types/{slug}",tag="items",summary="Get a socket's candidates",
    description="Returns the socket, crafting steps, fitting augments, fixed option sets and recipes that yield no augment.",
    params(("slug"=String,Path,description="Socket label slug or numeric id from /v1/augment-slot-types.")),
    responses((status=200,description="The socket and every candidate pool",body=super::response_schemas::SocketCandidatesResponse),
        (status=308,description="Renamed slug; follow Location to its current slug."),
        (status=404,description="No socket label has this slug or numeric id",body=crate::error::ErrorBody)))]
async fn augment_slot_type_detail(
    State(state): State<AppState>,
    EntityPath(id): EntityPath,
) -> Result<Json<Value>, ApiError> {
    state.read_db(move |db| {
        let row = crate::db::json_row(db,"SELECT id,slug,label,family,variant,qualifier,not_crafting_reason FROM augment_slot_types WHERE id=?1",[id])?;
        let mut socket = crate::db::socket_json(&row);
        socket["not_crafting_reason"] = row["not_crafting_reason"].clone();
        socket["augments"] = Value::Array(super::augments::augments_fitting_socket(db,id)?);
        let mut steps = crate::db::json_rows(db,"SELECT step.id,system.name AS system,system.id AS system_id,system.slug AS system_slug FROM crafting_steps step JOIN crafting_systems system ON system.id=step.system_id WHERE step.slot_id=?1 ORDER BY system.name,step.sort_order",[id])?;
        let step_ids = steps.iter().filter_map(|step| step["id"].as_i64()).collect::<Vec<_>>();
        let mut details = super::crafting::steps_by_id(db,&step_ids)?;
        for step in &mut steps {
            let id = step["id"].as_i64().unwrap();
            let fields = step.as_object_mut().unwrap();
            fields.remove("id");
            fields.extend(details.remove(&id).unwrap().as_object().unwrap().clone());
        }
        socket["crafting_steps"] = Value::Array(steps);
        let recipe_ids = crate::db::json_rows(db,"SELECT r.id FROM crafting_recipes r JOIN crafting_steps step ON step.id=r.step_id JOIN crafting_systems system ON system.id=r.system_id WHERE step.slot_id=?1 AND NOT EXISTS (SELECT 1 FROM crafting_recipe_augments yielded WHERE yielded.recipe_id=r.id) ORDER BY system.name,r.sort_order",[id])?.iter().filter_map(|row| row["id"].as_i64()).collect::<Vec<_>>();
        let mut recipes = super::crafting::recipes_by_id(db,&recipe_ids)?;
        socket["recipes"] = Value::Array(recipe_ids.into_iter().filter_map(|id| recipes.remove(&id)).collect());
        socket["option_sets"] = Value::Array(crate::db::json_rows(db,"SELECT s.id,s.slug,s.name,s.variant,t.label FROM socket_option_sets s JOIN augment_slot_types t ON t.id=s.slot_id WHERE s.slot_id=?1 ORDER BY s.name,s.variant,s.slug",[id])?);
        Ok(Json(socket))
    }).await
}
