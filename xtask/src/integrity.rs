use anyhow::{Context, Result};
use ddo_etl::map::drop_location::prerequisite_spans;
use ddo_etl::map::effect_map::EFFECT_MAP;
use rusqlite::{Connection, OptionalExtension};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Hard,
    Warn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Passed,
    Failed,
    Warned,
    Skipped,
}

pub struct IntegrityOptions {
    pub allowed_empty_tables: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offender {
    pub name: String,
    pub id: Option<i64>,
    pub detail: String,
}

enum OffenderQuery {
    Sql(&'static str),
    Built(fn(&Connection, &IntegrityOptions) -> Result<Findings>),
}

#[derive(Default)]
struct Findings {
    offenders: Option<Vec<Offender>>,
    notes: Vec<String>,
}

pub struct IntegrityCheck {
    pub name: &'static str,
    pub severity: Severity,
    pub description: &'static str,
    offender_query: OffenderQuery,
    shown_offender_limit: usize,
    top_detail_limit: usize,
}

const SHOWN_OFFENDER_LIMIT: usize = 10;

const DROP_LOCATION_HEAD_SQL: &str = "COALESCE(NULLIF(TRIM(CASE WHEN instr(i.drop_location, ',') > 0 \
     THEN substr(i.drop_location, 1, instr(i.drop_location, ',') - 1) ELSE i.drop_location END), ''), \
     '(no drop location)')";

const EFFECT_OWNER_LINKS: [(&str, &str); 5] = [
    ("item_effects", "item_id"),
    ("augment_effects", "augment_id"),
    ("set_bonus_tier_effects", "tier_id"),
    ("feat_effects", "feat_id"),
    ("item_augment_slot_option_effects", "option_id"),
];

const REPEATED_LINE_WORD_ALLOWANCES: &[&str] = &[];

impl IntegrityCheck {
    const fn hard(name: &'static str, description: &'static str, offender_query: OffenderQuery) -> Self {
        Self {
            name,
            severity: Severity::Hard,
            description,
            offender_query,
            shown_offender_limit: SHOWN_OFFENDER_LIMIT,
            top_detail_limit: 0,
        }
    }

    const fn warn(name: &'static str, description: &'static str, offender_query: OffenderQuery) -> Self {
        Self { severity: Severity::Warn, ..Self::hard(name, description, offender_query) }
    }

    const fn showing_every_offender(self) -> Self {
        Self { shown_offender_limit: usize::MAX, ..self }
    }

    const fn ranking_top_details(self, top_detail_limit: usize) -> Self {
        Self { top_detail_limit, ..self }
    }
}

pub const INTEGRITY_CHECKS: &[IntegrityCheck] = &[
    IntegrityCheck::hard(
        "served_effect_lines_repeat_words",
        "rendered owner lines have no adjacent repeated words, ignoring case, except explicitly allowed titles",
        OffenderQuery::Built(served_effect_lines_repeat_words),
    ),
    IntegrityCheck::warn(
        "effect_templates_without_their_placeholders",
        "templates use the granted slots, types and units, and amount literals match granted constants",
        OffenderQuery::Built(effect_templates_without_their_placeholders),
    ),
    IntegrityCheck::warn(
        "effect_templates_with_digits_without_amounts",
        "families whose rendered template contains a digit even though owner links carry no amount: \
         decide whether the digit is a fixed rule, an unmodelled amount or prose; \
         listed by cause; rank-named steps are listed separately (ddo-data#56)",
        OffenderQuery::Built(effect_templates_with_digits_without_amounts),
    ),
    IntegrityCheck::warn(
        "effect_descriptions_without_their_amounts",
        "served descriptions or fallback lines state every granted amount with its unit, and line claims match grants",
        OffenderQuery::Built(effect_descriptions_without_their_amounts),
    ),
    IntegrityCheck::hard(
        "effects_sharing_a_stat_set",
        "each link-valued stat set has one grouping unless an effect carries a verified declaration",
        OffenderQuery::Built(effects_sharing_a_stat_set),
    ).showing_every_offender(),
    IntegrityCheck::hard(
        "deferred_line_splits_are_live",
        "every issue-backed deferred split names an existing owner that still needs its exception",
        OffenderQuery::Built(deferred_line_splits_are_live),
    ),
    IntegrityCheck::hard(
        "redirects_agree_with_current_slugs",
        "redirects target current slugs without collisions, chains or retired current entities",
        OffenderQuery::Built(redirect_findings),
    ),
    IntegrityCheck::hard(
        "slugs_are_not_all_digits",
        "current slugs cannot collide with numeric detail addresses",
        OffenderQuery::Sql(
            "SELECT slug, entity_id, kind FROM entity_identity WHERE slug <> '' AND slug NOT GLOB '*[^0-9]*'",
        ),
    ),
    IntegrityCheck::hard(
        "source_references_exist",
        "every source owner and vendor payment names an existing entity of its declared kind",
        OffenderQuery::Sql(ddo_model::sources::REFERENCE_VIOLATIONS),
    ),
    IntegrityCheck::hard(
        "augment_recipes_not_duplicated",
        "augments with the same name, family and level have different recipe descriptions or affixes",
        OffenderQuery::Sql(
            "SELECT name, MIN(id), family || ' level ' || COALESCE(CAST(min_level AS TEXT), 'unknown')
               FROM augments
              GROUP BY name, family, min_level, description
             HAVING COUNT(*) > 1",
        ),
    ),
    IntegrityCheck::hard(
        "legacy_items_hidden",
        "every item flagged is_legacy has a reason: a (legacy) or (historic) name, an is_legacy correction, or no \
         sources row (its only sources are retired)",
        OffenderQuery::Built(legacy_items_without_a_reason),
    ),
    IntegrityCheck::hard(
        "tables_not_empty",
        "every required table has rows; optional annotation and redirect tables and those --allow-empty-table names may be empty",
        OffenderQuery::Built(empty_tables),
    ),
    IntegrityCheck::warn(
        "open_verification_requests",
        "open verification requests are a to-check queue, never a build failure",
        OffenderQuery::Sql(
            "SELECT identity.name, request.owner_id,
                    CASE WHEN request.kind IS NULL THEN 'whole entity'
                         ELSE request.kind || ' ' || request.field ||
                              CASE WHEN request.qualifier = '' THEN '' ELSE ' ' || request.qualifier END END ||
                    ': ' || request.question || '; assumed: ' || request.assumed_value
               FROM verification_requests request
               JOIN entity_identity identity ON identity.kind = request.owner_kind AND identity.entity_id = request.owner_id
              ORDER BY identity.name, request.kind, request.field, request.qualifier",
        ),
    ).showing_every_offender(),
    IntegrityCheck::hard(
        "adventure_pack_names_distinct",
        "adventure packs remain distinct after case folding, dropping a leading The and removing punctuation",
        OffenderQuery::Built(adventure_pack_names_not_distinct),
    ),
    IntegrityCheck::hard(
        "sources_not_from_prerequisite_clauses",
        "a source named only inside a drop-text prerequisite clause is never linked as loot",
        OffenderQuery::Built(sources_from_prerequisite_clauses),
    ),
    IntegrityCheck::hard(
        "quests_not_crafting_systems",
        "a crafting station is not a quest",
        OffenderQuery::Sql("SELECT q.name, q.id, 'also a crafting system' FROM quests q JOIN crafting_systems c ON c.name = q.name"),
    ),
    IntegrityCheck::hard(
        "quest_versions_match_heroic_pack_and_patron",
        "epic and legendary quest rows have the heroic version's pack and patron unless awaiting a named wiki read",
        OffenderQuery::Built(unreviewed_quest_version_differences),
    ),
    IntegrityCheck::hard(
        "effect_link_amount_counts",
        "every effect owner link carries no more amounts than its family allows",
        OffenderQuery::Built(effect_link_amount_counts),
    ),
    IntegrityCheck::hard(
        "owner_bonuses_have_one_stat_type_per_source_line",
        "an owner source line grants each stat and bonus type once after group expansion",
        OffenderQuery::Sql(
            "SELECT ob.owner_kind || ':' || ob.owner_id, ob.owner_id,
                    'line ' || ob.effect_link_order || ' repeats stat ' || ob.stat_id || ' and type ' || ob.bonus_type_id
               FROM owner_bonuses ob
              GROUP BY ob.owner_kind, ob.owner_id, ob.effect_link_order, ob.stat_id, ob.bonus_type_id
             HAVING COUNT(*) > 1",
        ),
    ),
    IntegrityCheck::hard(
        "owner_stat_links_repeat_effect_links",
        "an owner links a stat directly beside an effect or group link that already grants that stat at the same \
         type and amount, which counts one source line twice",
        OffenderQuery::Sql(
            "SELECT direct.owner_kind || ':' || direct.owner_id, direct.owner_id,
                    'direct ' || stat.name || ' repeats ' || via.name
               FROM owner_bonuses direct
               JOIN owner_bonuses effect_link ON effect_link.owner_kind = direct.owner_kind
                AND effect_link.owner_id = direct.owner_id
                AND effect_link.effect_link_order <> direct.effect_link_order
                AND effect_link.via_effect_id IS NOT NULL
                AND effect_link.stat_id = direct.stat_id
                AND effect_link.bonus_type_id IS direct.bonus_type_id
                AND effect_link.amount = direct.amount
               JOIN effects stat ON stat.id = direct.stat_id
               JOIN effects via ON via.id = effect_link.via_effect_id
              WHERE direct.via_effect_id IS NULL
              GROUP BY direct.owner_kind, direct.owner_id, direct.effect_link_order, effect_link.effect_link_order",
        ),
    ),
    IntegrityCheck::warn(
        "owner_bonuses_repeated_across_links",
        "an owner has a link whose resolved stats, all at the same types, another of its links already grants; the \
         lines are separate in the source and the same type does not stack, so each cause is listed once",
        OffenderQuery::Built(owner_bonuses_repeated_across_links),
    )
    .ranking_top_details(40),
    IntegrityCheck::hard(
        "effect_stat_amount_sources",
        "every stat row reads at most the amount slots its family carries",
        OffenderQuery::Sql(
            "SELECT e.name, e.id, 'bonus reads an absent amount slot' FROM effects e
             JOIN effect_bonuses s ON s.effect_id = e.id
             WHERE s.amount_from > CASE WHEN INSTR(COALESCE(e.verbose_name_template, '') || COALESCE(e.description_template, ''), '{2}') > 0 THEN 2
             WHEN INSTR(COALESCE(e.verbose_name_template, '') || COALESCE(e.description_template, ''), '{1}') > 0 THEN 1 ELSE 0 END",
        ),
    ),
    IntegrityCheck::hard(
        "effect_template_placeholders",
        "amount placeholders in a template are contiguous and use only converted tokens",
        OffenderQuery::Sql(
            "SELECT e.name, e.id, 'invalid template amount placeholder' FROM effects e
             WHERE (INSTR(COALESCE(e.verbose_name_template, '') || COALESCE(e.description_template, ''), '{2}') > 0
                    AND INSTR(COALESCE(e.verbose_name_template, '') || COALESCE(e.description_template, ''), '{1}') = 0)
                OR INSTR(COALESCE(e.verbose_name_template, '') || COALESCE(e.description_template, ''), '%v1') > 0
                OR INSTR(COALESCE(e.verbose_name_template, '') || COALESCE(e.description_template, ''), '%v2') > 0",
        ),
    ),
    IntegrityCheck::hard(
        "effect_line_rendering_tokens",
        "effect line templates have no unfilled tokens or repeated bonus-type word where an amount belongs",
        OffenderQuery::Built(effect_line_rendering_tokens),
    ),
    IntegrityCheck::hard(
        "effect_bonus_type_sources",
        "an owner types every bonus row that reads its type from the link, including one-stat effects",
        OffenderQuery::Built(effect_bonus_type_sources),
    ),
    IntegrityCheck::hard(
        "effect_bonuses_have_one_rule_per_stat",
        "a named effect cannot carry multiple bonus-type rules for the same stat",
        OffenderQuery::Sql(
            "SELECT e.name, e.id, s.name || ' has ' || COUNT(*) || ' rules'
             FROM effect_bonuses eb JOIN effects e ON e.id = eb.effect_id
             JOIN effects s ON s.id = eb.target_effect_id
             GROUP BY e.id, eb.target_effect_id HAVING COUNT(*) > 1",
        ),
    ),
    IntegrityCheck::hard(
        "stat_links_have_bonus_types",
        "every direct link to a stat has a bonus type from its owner",
        OffenderQuery::Built(stat_links_without_bonus_types),
    ),
    IntegrityCheck::hard(
        "stat_links_at_routed_types",
        "item, augment and socket-option stat links without an alias use their declared effect at the routed type",
        OffenderQuery::Built(stat_links_at_routed_types),
    ),
    IntegrityCheck::hard(
        "effect_families_have_owners",
        "every effect family has an owner link or is a group granted by an owned effect",
        OffenderQuery::Sql(
            "SELECT e.name, e.id, 'no owner link' FROM effects e WHERE e.is_stat = 0 AND e.id NOT IN
               (SELECT effect_id FROM item_effects UNION SELECT effect_id FROM augment_effects
                UNION SELECT effect_id FROM set_bonus_tier_effects UNION SELECT effect_id FROM feat_effects
                UNION SELECT effect_id FROM item_augment_slot_option_effects
                UNION SELECT eb.target_effect_id FROM effect_bonuses eb WHERE eb.target_effect_id IN
                  (SELECT id FROM effects WHERE is_group = 1))",
        ),
    ),
    IntegrityCheck::hard(
        "effect_groups_have_flat_members",
        "every group has stat members reading slot one from the link with one type source and no nested group",
        OffenderQuery::Sql(
            "SELECT e.name, e.id, 'invalid group members' FROM effects e WHERE e.is_group = 1 AND
              (NOT EXISTS (SELECT 1 FROM effect_bonuses eb WHERE eb.effect_id = e.id)
               OR EXISTS (SELECT 1 FROM effect_bonuses eb LEFT JOIN effects target ON target.id = eb.target_effect_id
                          WHERE eb.effect_id = e.id AND
                            (target.is_stat <> 1 OR eb.amount_from <> 1 OR eb.bonus_type_id IS NOT NULL
                             OR eb.scale <> 1 OR eb.rounding <> 'down' OR eb.trigger_id IS NOT NULL)))",
        ),
    ),
    IntegrityCheck::hard(
        "effect_tier_groups_have_steps",
        "every effect tier group has at least two ordered steps",
        OffenderQuery::Sql(
            "SELECT tg.name, tg.id, 'fewer than two members' FROM effect_tier_groups tg
             LEFT JOIN effects e ON e.tier_group_id = tg.id GROUP BY tg.id HAVING COUNT(e.id) < 2",
        ),
    ),
    IntegrityCheck::hard(
        "effect_names_have_no_em_dash",
        "family names contain no em dash separated prose",
        OffenderQuery::Sql("SELECT name, id, 'em dash in family name' FROM effects WHERE INSTR(name, ' — ') > 0"),
    ),
    IntegrityCheck::hard(
        "set_tier_lines_do_not_repeat_structured_facts",
        "a text-only set tier line does not repeat a structured line's rendered text on the same tier",
        OffenderQuery::Sql(
            "WITH lines AS (
               SELECT t.id AS tier_id, sb.name || ' (' || t.equipped_count || ' pieces)' AS tier_name,
                      (e.is_stat = 1 OR EXISTS (SELECT 1 FROM effect_bonuses es WHERE es.effect_id = e.id)) AS structured,
                      REPLACE(REPLACE(REPLACE(
                        REPLACE(REPLACE(e.verbose_name_template, '+{1}', '{1}'), '+{2}', '{2}'),
                        '{1}', CASE WHEN COALESCE(l.value, e.default_value) IS NULL THEN ''
                                    ELSE printf('%+d', COALESCE(l.value, e.default_value)) END),
                        '{2}', CASE WHEN COALESCE(l.value2, e.default_value2) IS NULL THEN ''
                                    ELSE printf('%+d', COALESCE(l.value2, e.default_value2)) END),
                        '%b1', COALESCE(bt.name, '')) AS rendered
               FROM set_bonus_tiers t JOIN set_bonuses sb ON sb.id = t.set_id
               JOIN set_bonus_tier_effects l ON l.tier_id = t.id
               JOIN effects e ON e.id = l.effect_id
               LEFT JOIN bonus_types bt ON bt.id = l.bonus_type_id)
             SELECT prose.tier_name, prose.tier_id, prose.rendered
             FROM lines prose JOIN lines structured ON structured.tier_id = prose.tier_id
             WHERE prose.structured = 0 AND structured.structured = 1
               AND prose.rendered = structured.rendered",
        ),
    ),
    IntegrityCheck::hard(
        "weapon_and_armor_stats_match_category",
        "weapons have weapon stats only, armor has armor stats only, shields have both (a shield bashes), and \
         jewelry and clothing have neither",
        OffenderQuery::Sql(
            "SELECT i.name, i.id, i.item_category \
             || CASE WHEN w.item_id IS NULL THEN ' without' ELSE ' with' END || ' weapon stats and' \
             || CASE WHEN a.item_id IS NULL THEN ' without' ELSE ' with' END || ' armor stats' \
             FROM items i LEFT JOIN item_weapon_stats w ON w.item_id = i.id \
             LEFT JOIN item_armor_stats a ON a.item_id = i.id \
             WHERE (w.item_id IS NOT NULL) <> (i.item_category IN ('Weapon', 'Shield')) \
             OR (a.item_id IS NOT NULL) <> (i.item_category IN ('Armor', 'Shield'))",
        ),
    ),
    IntegrityCheck::hard(
        "raid_loot_only_on_raids",
        "a quest drop with loot type raid comes from a quest that is a raid",
        OffenderQuery::Sql(
            "SELECT q.name, d.id, 'raid loot ' || COALESCE(i.name, a.name, '') || ' from a quest that is not a raid' \
             FROM sources d JOIN quests q ON q.id = d.quest_id LEFT JOIN items i ON i.id = d.item_id \
             LEFT JOIN augments a ON a.id = d.augment_id \
             WHERE d.loot_type = 'raid' AND q.is_raid = 0",
        ),
    ),
    IntegrityCheck::hard(
        "pack_wide_drops_repeat_quest_drops",
        "a pack-wide drop does not repeat the same owner's quest drop in that pack when chests match or either chest is unknown",
        OffenderQuery::Sql(
            "SELECT COALESCE(i.name, a.name, pack_drop.kind || ' source'), pack_drop.id,
                    'pack ' || p.name || ' chest ' || COALESCE(pack_drop.chest, '(unknown)') || ' repeats a quest drop'
             FROM sources pack_drop JOIN adventure_packs p ON p.id = pack_drop.pack_id
             LEFT JOIN items i ON i.id = pack_drop.item_id
             LEFT JOIN augments a ON a.id = pack_drop.augment_id
             WHERE pack_drop.kind = 'adventure_pack'
               AND EXISTS (
                 SELECT 1 FROM quests q JOIN sources quest_drop ON quest_drop.quest_id = q.id
                 WHERE q.pack_id = pack_drop.pack_id AND quest_drop.kind = 'quest'
                   AND quest_drop.item_id IS pack_drop.item_id
                   AND quest_drop.augment_id IS pack_drop.augment_id
                   AND (quest_drop.chest IS pack_drop.chest
                        OR quest_drop.chest IS NULL OR pack_drop.chest IS NULL)
               )
             ORDER BY COALESCE(i.name, a.name)",
        ),
    ),
    IntegrityCheck::hard(
        "item_sockets_use_known_labels",
        "every item socket is a slot type of a known family (standard, dino, lamordia, slavers, upgrade, crafting) \
         with a variant, and a standard socket is one of the nine colours",
        OffenderQuery::Sql(
            "SELECT i.name, i.id, 'socket ' || COALESCE(t.label, s.slot_id) || ' of family ' \
             || COALESCE(t.family, 'none') FROM item_augment_slots s JOIN items i ON i.id = s.item_id \
             LEFT JOIN augment_slot_types t ON t.id = s.slot_id \
             WHERE t.id IS NULL \
             OR t.family NOT IN ('standard', 'dino', 'lamordia', 'slavers', 'upgrade', 'crafting') \
             OR TRIM(t.variant) = '' \
             OR (t.family = 'standard' AND t.variant NOT IN \
                 ('red', 'green', 'blue', 'orange', 'purple', 'colorless', 'yellow', 'sun', 'moon'))",
        ),
    ),
    IntegrityCheck::hard(
        "trees_have_enhancements",
        "every enhancement tree has at least one enhancement",
        OffenderQuery::Sql(
            "SELECT t.name, t.id, 'no enhancements' FROM enhancement_trees t \
             WHERE NOT EXISTS (SELECT 1 FROM enhancements e WHERE e.tree_id = t.id)",
        ),
    ),
    IntegrityCheck::hard(
        "classes_have_full_progression",
        "every class has BAB and spell points for each of its 21 level entries (0 to 20); every heroic class has hit \
         points and skill points per level, a good or poor progression for each save, and class skills. Unknown, \
         Maetrim's placeholder for a level whose class is not chosen yet, is exempt",
        OffenderQuery::Sql(
            "SELECT name, id, CASE \
             WHEN json_array_length(COALESCE(bab, '[]')) <> 21 \
             THEN 'BAB has ' || json_array_length(COALESCE(bab, '[]')) || ' entries, not 21' \
             WHEN json_array_length(COALESCE(spell_points_per_level, '[]')) <> 21 \
             THEN 'spell points have ' || json_array_length(COALESCE(spell_points_per_level, '[]')) \
                  || ' entries, not 21' \
             WHEN COALESCE(hit_points, 0) <= 0 THEN 'no hit points per level' \
             WHEN COALESCE(skill_points, 0) <= 0 THEN 'no skill points per level' \
             WHEN NOT (COALESCE(fortitude, '') IN ('good', 'poor') AND COALESCE(reflex, '') IN ('good', 'poor') \
                       AND COALESCE(will, '') IN ('good', 'poor')) THEN 'a save with no progression' \
             ELSE 'no class skills' END \
             FROM classes c \
             WHERE json_array_length(COALESCE(bab, '[]')) <> 21 \
             OR json_array_length(COALESCE(spell_points_per_level, '[]')) <> 21 \
             OR (not_heroic = 0 AND name <> 'Unknown' AND ( \
                 COALESCE(hit_points, 0) <= 0 OR COALESCE(skill_points, 0) <= 0 \
                 OR NOT (COALESCE(fortitude, '') IN ('good', 'poor') AND COALESCE(reflex, '') IN ('good', 'poor') \
                         AND COALESCE(will, '') IN ('good', 'poor')) \
                 OR NOT EXISTS (SELECT 1 FROM class_skills s WHERE s.class_id = c.id)))",
        ),
    ),
    IntegrityCheck::warn(
        "quest_versions_pending_wiki_read",
        "named epic and legendary version pairs await a wiki pack and patron read",
        OffenderQuery::Built(reviewed_quest_version_differences),
    ),
    IntegrityCheck::warn(
        "items_without_a_source",
        "items (other than legacy ones) with no sources row. Sources still needing records include vendors, events, \
         DDO Store purchases, wilderness areas and items with no drop text. Becomes HARD once those are recorded; \
         the drop_location heads below are the work list",
        OffenderQuery::Built(items_without_a_source),
    )
    .ranking_top_details(15),
    IntegrityCheck::warn(
        "items_with_a_source_but_no_pack",
        "non-legacy items with a quest, quest chain, saga, adventure pack, challenge, crafting system or vendor \
         source but no adventure pack reached through any source; fill missing source pack links in the wiki data",
        OffenderQuery::Built(items_with_a_source_but_no_pack),
    ),
    IntegrityCheck::warn(
        "items_spanning_packs",
        "items with sources in several packs need review; wiki-confirmed drops and saga roll-ups are listed separately",
        OffenderQuery::Built(items_spanning_packs),
    ),
    IntegrityCheck::warn(
        "packs_with_several_patrons",
        "packs serving quests from several patrons need review; wiki-confirmed packs are allow-listed",
        OffenderQuery::Built(packs_with_several_patrons),
    ),
    IntegrityCheck::warn(
        "effects_named_after_stats",
        "text-only effects whose rendered name equals a stat's ignoring case and spaces; \
         families with stat rows are excluded",
        OffenderQuery::Sql(
            "SELECT e.name, e.id, 'named like the stat ' || s.name FROM effects e \
             JOIN effects s ON s.is_stat = 1 AND lower(replace(e.name, ' ', '')) = lower(replace(s.name, ' ', '')) \
             WHERE e.is_stat = 0 AND EXISTS (SELECT 1 FROM item_effects ie WHERE ie.effect_id = e.id) \
             AND NOT EXISTS (SELECT 1 FROM effect_bonuses es WHERE es.effect_id = e.id) \
             ORDER BY e.name",
        ),
    )
    .showing_every_offender(),
    IntegrityCheck::warn(
        "effect_types_not_classified",
        "stored modifier types that are neither stat-mapped nor engine_only; each entry counts the items using it",
        OffenderQuery::Built(effect_types_not_classified),
    )
    .showing_every_offender(),
    IntegrityCheck::warn(
        "effects_named_like_identifiers",
        "effect names with identifier casing, trailing digits, number markers, doubled words, Feat prefixes or buff-type suffixes need display names",
        OffenderQuery::Built(effects_named_like_identifiers),
    )
    .showing_every_offender(),
    IntegrityCheck::warn(
        "effects_with_tied_home_bonus_types",
        "in-use named effects with equally common carrier types; informational only, their file homes remain authoritative",
        OffenderQuery::Built(effects_with_tied_home_bonus_types),
    )
    .showing_every_offender(),
    IntegrityCheck::warn(
        "effect_templates_disagree_with_names",
        "in-use effect templates must include the effect name or a declared alias with matching numeric tokens",
        OffenderQuery::Built(effect_templates_disagree_with_names),
    )
    .showing_every_offender(),
    IntegrityCheck::warn(
        "effect_descriptions_with_fewer_critical_dice_than_hit_dice",
        "in-use effect descriptions whose critical-hit dice are fewer than their on-hit dice; a wiki template default \
         is not a game value, so check the source before trusting either",
        OffenderQuery::Built(effect_descriptions_with_fewer_critical_dice_than_hit_dice),
    )
    .showing_every_offender(),
    IntegrityCheck::warn(
        "owner_names_disagree_with_linked_effects",
        "an augment or slot option naming a declared effect links that effect",
        OffenderQuery::Built(owner_names_disagree_with_linked_effects),
    ),
    IntegrityCheck::hard(
        "effect_template_numbers_match_names",
        "a displayed number or Roman numeral in an effect template agrees with the effect name",
        OffenderQuery::Built(effect_template_numbers_mismatch_names),
    ),
    IntegrityCheck::hard(
        "universal_spell_power_has_no_elemental_types",
        "Equipment, Insight and Quality spell power is granted through the Potency group",
        OffenderQuery::Sql(
            "SELECT ob.owner_kind || ':' || ob.owner_id, ob.owner_id,
                    'Universal Spell Power with ' || bt.name
               FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
               JOIN bonus_types bt ON bt.id = ob.bonus_type_id
              WHERE s.name = 'Universal Spell Power' AND bt.name IN ('Equipment', 'Insight', 'Quality')",
        ),
    ),
    IntegrityCheck::hard(
        "skill_groups_share_home_bonus_type",
        "all six ability skill groups have the same declared home bonus type",
        OffenderQuery::Built(skill_groups_with_different_home_bonus_types),
    ),
    IntegrityCheck::warn(
        "effects_with_values_in_names",
        "family names containing + followed by a number may embed an owner value; wiki prose stays here until its family and amount fields are read",
        OffenderQuery::Sql(
            "SELECT name, id, 'value-looking text in family name' FROM effects
             WHERE name GLOB '*+[0-9]*' ORDER BY name",
        ),
    )
    .showing_every_offender(),
    IntegrityCheck::warn(
        "effects_with_amounts_named_with_digits",
        "a family name with a digit and amount slot may have a value embedded in its identity",
        OffenderQuery::Sql(
            "SELECT name, id, 'digit in family name with an amount slot' FROM effects
             WHERE (INSTR(COALESCE(verbose_name_template, '') || COALESCE(description_template, ''), '{1}') > 0
                    OR is_stat = 1) AND name GLOB '*[0-9]*' ORDER BY name",
        ),
    )
    .showing_every_offender(),
    IntegrityCheck::warn(
        "items_have_minimum_level",
        "every item has a minimum level of 1 or more. WARN until the follow-up: the Cannith Crafted blanks take their \
         level from the crafting step, and Quiver of Alacrity has MinLevel 0 in Maetrim's file",
        OffenderQuery::Sql(
            "SELECT name, id, 'minimum level ' || COALESCE(minimum_level, 'null') FROM items \
             WHERE minimum_level IS NULL OR minimum_level < 1 ORDER BY name",
        ),
    )
    .showing_every_offender(),
    IntegrityCheck::warn(
        "augments_have_slot_and_family",
        "every augment has a family and fits at least one socket. WARN until the follow-up: Insightful Spell Focus \
         Mastery has no <Type> in Maetrim's file (a correction candidate), and No Augment is his empty-socket \
         placeholder",
        OffenderQuery::Sql(
            "SELECT name, id, CASE WHEN TRIM(family) = '' THEN 'blank family' \
             ELSE family || ' augment fits no socket' END FROM augments a \
             WHERE TRIM(family) = '' OR NOT EXISTS (SELECT 1 FROM augment_slots s WHERE s.augment_id = a.id)",
        ),
    )
    .showing_every_offender(),
    IntegrityCheck::hard(
        "chains_and_sagas_have_quests",
        "every quest chain and saga links at least two quests",
        OffenderQuery::Sql(
            "SELECT c.name, c.id, 'quest chain with ' || COUNT(q.quest_id) || ' quest(s)' FROM quest_chains c \
             LEFT JOIN quest_chain_quests q ON q.chain_id = c.id GROUP BY c.id HAVING COUNT(q.quest_id) < 2 \
             UNION ALL \
             SELECT s.name, s.id, 'saga with ' || COUNT(q.quest_id) || ' quest(s)' FROM sagas s \
             LEFT JOIN saga_quests q ON q.saga_id = s.id GROUP BY s.id HAVING COUNT(q.quest_id) < 2",
        ),
    )
    .showing_every_offender(),
    IntegrityCheck::warn(
        "effect_links_missing_first_amount",
        "an owner of a family with a first amount lacks both an explicit value and a family default",
        OffenderQuery::Built(effect_links_missing_first_amount),
    ),
    IntegrityCheck::warn(
        "stat_links_missing_value",
        "direct stat links whose owner has no value and the stat has no default",
        OffenderQuery::Built(stat_links_missing_value),
    ),
    IntegrityCheck::warn(
        "effect_links_with_only_zero_bonuses",
        "owner links whose resolved bonuses are all zero; check whether the source meant to grant a bonus",
        OffenderQuery::Built(effect_links_with_only_zero_bonuses),
    )
    .showing_every_offender(),
    IntegrityCheck::warn(
        "effects_with_dice_but_no_damage_rows",
        "dice modifiers without structured damage rows, grouped by source kind and effect type; unresolved modifiers remain on the backfill list",
        OffenderQuery::Sql(
            "SELECT m.source_kind || ' / ' || m.effect_type, 0,
                    COUNT(*) || ' dice modifier(s): ' || SUM(m.effect_id IS NULL) || ' unresolved, '
                    || SUM(m.effect_id IS NOT NULL) || ' resolved without damage rows'
             FROM modifiers m
             WHERE (m.dice_number IS NOT NULL OR m.dice_sides IS NOT NULL)
               AND (m.effect_id IS NULL OR NOT EXISTS
                    (SELECT 1 FROM effect_damage d WHERE d.effect_id = m.effect_id))
             GROUP BY m.source_kind, m.effect_type ORDER BY m.source_kind, m.effect_type",
        ),
    ),
    IntegrityCheck::warn(
        "effects_with_unused_default",
        "families whose default amount is used by every link may have a constant instead",
        OffenderQuery::Built(effects_with_unused_default),
    ),
    IntegrityCheck::warn(
        "effects_text_only_with_item_values",
        "text-only families with amounts are candidates for a stat mapping",
        OffenderQuery::Sql(
            "SELECT e.name, e.id, 'text-only effect with amount slot(s)'
             FROM effects e WHERE e.is_stat = 0
               AND INSTR(COALESCE(e.verbose_name_template, '') || COALESCE(e.description_template, ''), '{1}') > 0
               AND NOT EXISTS
             (SELECT 1 FROM effect_bonuses s WHERE s.effect_id = e.id) ORDER BY e.name",
        ),
    ),
    IntegrityCheck::warn(
        "effect_families_sharing_stat_and_type",
        "distinct buff families affecting one stat with one bonus type; review identity before consolidating",
        OffenderQuery::Sql(
            "SELECT s.name || ' / ' || bt.name, s.id, GROUP_CONCAT(DISTINCT e.name)
             FROM owner_bonuses ob JOIN effects e ON e.id = ob.via_effect_id
             JOIN effects s ON s.id = ob.stat_id
             JOIN bonus_types bt ON bt.id = ob.bonus_type_id
             GROUP BY s.id, bt.id HAVING COUNT(DISTINCT e.id) > 1",
        ),
    ),
    IntegrityCheck::warn(
        "items_without_effects",
        "items that carry nothing beyond their base weapon or armor stats: no bonus, effect, enhancement bonus, \
         modifier, clicky, augment slot or set membership. Maetrim's files give these items nothing more, so each is \
         either genuinely bare (starter and event gear, ritual components) or a correction candidate whose wiki page \
         lists effects; the top categories are listed",
        OffenderQuery::Sql(
            "SELECT i.name, i.id, i.item_category FROM items i \
             WHERE i.enhancement_bonus IS NULL \
             AND NOT EXISTS (SELECT 1 FROM item_effects e WHERE e.item_id = i.id) \
             AND NOT EXISTS (SELECT 1 FROM modifiers m WHERE m.source_kind = 'item' AND m.source_id = i.id) \
             AND NOT EXISTS (SELECT 1 FROM item_clickies c WHERE c.item_id = i.id) \
             AND NOT EXISTS (SELECT 1 FROM item_augment_slots s WHERE s.item_id = i.id) \
             AND NOT EXISTS (SELECT 1 FROM set_bonus_items sbi WHERE sbi.item_id = i.id) ORDER BY i.name",
        ),
    )
    .ranking_top_details(5),
    IntegrityCheck::warn(
        "items_without_description",
        "items with a blank description; the rows below are the review work list",
        OffenderQuery::Sql(
            "SELECT name, id, item_category FROM items WHERE description IS NULL OR TRIM(description) = '' \
             ORDER BY name",
        ),
    )
    .ranking_top_details(5),
    IntegrityCheck::warn(
        "near_duplicate_item_names",
        "items whose names differ only by case, punctuation or spaces",
        OffenderQuery::Built(near_duplicate_item_names),
    )
    .showing_every_offender(),
    IntegrityCheck::warn(
        "raids_without_raid_loot",
        "raids with no sources row of loot type raid",
        OffenderQuery::Sql(
            "SELECT q.name, q.id, 'raid without raid loot' FROM quests q WHERE q.is_raid = 1 \
             AND NOT EXISTS (SELECT 1 FROM sources d WHERE d.quest_id = q.id AND d.loot_type = 'raid') ORDER BY q.name",
        ),
    )
    .showing_every_offender(),
    IntegrityCheck::warn(
        "quests_without_loot",
        "quests (not challenges) with no sources row; the database does not mark wilderness areas, so any among them \
         are listed too. The top packs are listed",
        OffenderQuery::Sql(
            "SELECT q.name, q.id, COALESCE(p.name, '(no pack)') FROM quests q \
             LEFT JOIN adventure_packs p ON p.id = q.pack_id WHERE q.is_challenge = 0 \
             AND NOT EXISTS (SELECT 1 FROM sources d WHERE d.quest_id = q.id) ORDER BY q.name",
        ),
    )
    .ranking_top_details(10),
    IntegrityCheck::warn(
        "unreferenced_stats",
        "stats that no effect an item, augment, feat or set tier carries is on. The guard against a duplicate or \
         non-stat name in the seed is the every_seed_stat_has_a_source_or_is_kept_without_one test in \
         crates/ddo-etl/tests/stats_seed.rs, which fails on any seed stat no effect map entry, fixture \
         bonus, wiki or correction bonus reaches unless its STATS_KEPT_WITHOUT_A_SOURCE list names it with a \
         reason. A full upstream build lists that list plus Caster Level, Critical Threat Range, Ki and Maximum \
         Caster Level, whose mapped effect types no current owner carries as a plain stat number",
        OffenderQuery::Built(unreferenced_stats),
    )
    .showing_every_offender(),
    IntegrityCheck::warn(
        "slot_types_no_augment_fits",
        "socket types no augment fits, with how many item sockets carry each. A socket an augment slot option \
         grants, or one holding the options an item upgrades through (item_augment_slot_options), is filled by \
         those options and is not listed. Three stay on a full upstream build: Cannith Weapon Extra on the crafting \
         tutorial's Fire Touch Heavy Mace, which no Cannith augment in Maetrim's files fits, and Random Effect 1 and \
         2 on +5 Engraved Cormyrian Leather Armor, whose random effects his files do not model",
        OffenderQuery::Sql(
            "SELECT t.label, t.id, (SELECT COUNT(*) FROM item_augment_slots s WHERE s.slot_id = t.id) \
             || ' item socket(s)' FROM augment_slot_types t \
             WHERE NOT EXISTS (SELECT 1 FROM augment_slots a WHERE a.slot_id = t.id) \
             AND NOT EXISTS (SELECT 1 FROM item_augment_slot_option_grants g WHERE g.slot_id = t.id) \
             AND NOT EXISTS (SELECT 1 FROM item_augment_slots s JOIN item_augment_slot_options o \
             ON o.item_id = s.item_id AND o.slot_order = s.sort_order WHERE s.slot_id = t.id) ORDER BY t.label",
        ),
    )
    .showing_every_offender(),
    IntegrityCheck::warn(
        "sets_without_members",
        "set bonuses no item, augment, filigree or item augment slot option belongs to. Two stay on a full upstream \
         build: Anthem Melody, which no item names in Maetrim's files or on ddowiki (his dead data), and Magewright's \
         Expertise, which the Nearly Finished upgrade of Magewright's Cloak grants on the wiki while his option names \
         no set and no correction kind adds one to an option",
        OffenderQuery::Sql(
            "SELECT s.name, s.id, CASE WHEN s.is_filigree_set = 1 THEN 'filigree set' ELSE 'set' END \
             FROM set_bonuses s \
             WHERE NOT EXISTS (SELECT 1 FROM set_bonus_items i WHERE i.set_id = s.id) \
             AND NOT EXISTS (SELECT 1 FROM set_bonus_augments a WHERE a.set_id = s.id) \
             AND NOT EXISTS (SELECT 1 FROM item_augment_slot_option_sets o WHERE o.set_id = s.id) \
             AND NOT EXISTS (SELECT 1 FROM filigrees f WHERE f.set_id = s.id) ORDER BY s.name",
        ),
    )
    .showing_every_offender(),
];

fn has_column(db: &Connection, table: &str, column: &str) -> Result<bool> {
    let column_count: i64 =
        db.query_row("SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2", [table, column], |row| row.get(0))?;
    Ok(column_count > 0)
}

fn offenders_from_sql(db: &Connection, sql: &str) -> Result<Vec<Offender>> {
    let mut statement = db.prepare(sql).with_context(|| format!("preparing {sql}"))?;
    let offenders = statement
        .query_map([], |row| {
            Ok(Offender {
                name: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                id: row.get(1)?,
                detail: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(offenders)
}

fn normalized_pack_name(name: &str) -> String {
    let lowercase = name.to_lowercase();
    lowercase
        .strip_prefix("the ")
        .unwrap_or(&lowercase)
        .chars()
        .filter(|character| character.is_alphanumeric())
        .collect()
}

fn adventure_pack_names_not_distinct(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let mut names = BTreeMap::new();
    let mut offenders = Vec::new();
    let mut statement = db.prepare("SELECT id, name FROM adventure_packs ORDER BY name")?;
    for row in statement.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))? {
        let (id, name) = row?;
        if let Some((other_id, other_name)) = names.insert(normalized_pack_name(&name), (id, name.clone())) {
            offenders.push(Offender { name, id: Some(id), detail: format!("matches {other_name:?} ({other_id})") });
        }
    }
    Ok(Findings { offenders: Some(offenders), notes: Vec::new() })
}

fn source_match_is_only_in_prerequisite(segment: &str, source_match: &str) -> bool {
    if source_match.is_empty() {
        return false;
    }
    let lowercase = segment.to_ascii_lowercase();
    let matching = source_match.to_ascii_lowercase();
    let spans = prerequisite_spans(segment);
    let matches =
        lowercase.match_indices(&matching).map(|(start, _)| start..start + matching.len()).collect::<Vec<_>>();
    !matches.is_empty()
        && matches
            .iter()
            .all(|candidate| spans.iter().any(|span| span.start <= candidate.start && candidate.end <= span.end))
}

fn sources_from_prerequisite_clauses(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let mut offenders = Vec::new();
    let mut statement = db.prepare(
        "SELECT s.id, COALESCE(i.name, a.name, s.kind || ' source'), s.drop_text_segment, s.source_match
         FROM sources s LEFT JOIN items i ON i.id = s.item_id LEFT JOIN augments a ON a.id = s.augment_id
         WHERE s.drop_text_segment IS NOT NULL ORDER BY s.id",
    )?;
    for row in statement.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?))
    })? {
        let (id, name, segment, source_match) = row?;
        if source_match_is_only_in_prerequisite(&segment, &source_match) {
            offenders.push(Offender { name, id: Some(id), detail: format!("{source_match:?} inside {segment:?}") });
        }
    }
    Ok(Findings { offenders: Some(offenders), notes: Vec::new() })
}

fn source_review_allowlist(db: &Connection, collection: &str) -> Result<BTreeMap<String, String>> {
    let rule = match collection {
        "items_spanning_packs" => "item",
        "packs_with_several_patrons" => "patron",
        _ => anyhow::bail!("unknown source review collection {collection}"),
    };
    let mut statement =
        db.prepare("SELECT DISTINCT entity_name, url FROM pack_exceptions WHERE rule = ?1 ORDER BY entity_name")?;
    let citations = statement.query_map([rule], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
    let mut reviewed = BTreeMap::new();
    for citation in citations {
        let (name, citation_url) = citation?;
        anyhow::ensure!(
            citation_url.starts_with("https://ddowiki.com/page/"),
            "source review {name:?} has no wiki page"
        );
        if let Some(previous) = reviewed.insert(name.clone(), citation_url.clone()) {
            anyhow::ensure!(previous == citation_url, "source review {name:?} has conflicting citations");
        }
    }
    Ok(reviewed)
}

fn quest_version_base(name: &str) -> Option<String> {
    if let Some(base) = name.strip_prefix("Epic ").or_else(|| name.strip_prefix("Legendary ")) {
        return Some(base.to_string());
    }
    if let Some(base) = name.strip_suffix(" - EPIC").or_else(|| name.strip_suffix(" Epic")) {
        return Some(base.to_string());
    }
    name.split_once(" - Epic ").map(|(area, challenge)| format!("{area} - {challenge}"))
}

fn pending_quest_versions(db: &Connection) -> Result<BTreeMap<String, String>> {
    let mut pending = BTreeMap::new();
    let mut statement =
        db.prepare("SELECT entity_name, reason, url FROM pack_exceptions WHERE rule = 'quest' ORDER BY entity_name")?;
    for row in statement
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)))?
    {
        let (name, reason, citation_url) = row?;
        anyhow::ensure!(
            !reason.trim().is_empty() && citation_url.starts_with("https://ddowiki.com/page/"),
            "quest version {name:?} needs a reason and wiki citation"
        );
        anyhow::ensure!(
            pending.insert(name.to_string(), reason.to_string()).is_none(),
            "quest version {name:?} is reviewed twice"
        );
    }
    Ok(pending)
}

fn quest_version_differences(db: &Connection) -> Result<Vec<Offender>> {
    let mut statement = db.prepare(
        "SELECT q.id, q.name, pack.name, patron.name FROM quests q
         LEFT JOIN adventure_packs pack ON pack.id = q.pack_id
         LEFT JOIN patrons patron ON patron.id = q.patron_id ORDER BY q.name",
    )?;
    let quests: BTreeMap<String, (i64, Option<String>, Option<String>)> = statement
        .query_map([], |row| Ok((row.get::<_, String>(1)?, (row.get(0)?, row.get(2)?, row.get(3)?))))?
        .collect::<rusqlite::Result<_>>()?;
    let mut offenders = Vec::new();
    for (version_name, (id, version_pack, version_patron)) in &quests {
        let Some(heroic_name) = quest_version_base(version_name) else { continue };
        let heroic_with_the = format!("The {heroic_name}");
        let heroic_with_a = format!("A {heroic_name}");
        let Some((heroic_name, (_, heroic_pack, heroic_patron))) =
            [heroic_name.as_str(), heroic_with_the.as_str(), heroic_with_a.as_str()]
                .into_iter()
                .find_map(|name| quests.get_key_value(name))
        else {
            continue;
        };
        if version_pack != heroic_pack || version_patron != heroic_patron {
            offenders.push(Offender {
                name: version_name.clone(),
                id: Some(*id),
                detail: format!(
                    "heroic {heroic_name:?}: pack {heroic_pack:?}, patron {heroic_patron:?}; version: pack {version_pack:?}, patron {version_patron:?}"
                ),
            });
        }
    }
    Ok(offenders)
}

fn unreviewed_quest_version_differences(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let pending = pending_quest_versions(db)?;
    let offenders =
        quest_version_differences(db)?.into_iter().filter(|quest| !pending.contains_key(&quest.name)).collect();
    Ok(Findings { offenders: Some(offenders), notes: Vec::new() })
}

fn reviewed_quest_version_differences(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let pending = pending_quest_versions(db)?;
    let differences =
        quest_version_differences(db)?.into_iter().map(|quest| (quest.name.clone(), quest)).collect::<BTreeMap<_, _>>();
    let mut offenders = Vec::new();
    for (name, reason) in pending {
        let difference = differences.get(&name);
        let id = match difference {
            Some(difference) => difference.id,
            None => db.query_row("SELECT id FROM quests WHERE name = ?1", [&name], |row| row.get(0)).optional()?,
        };
        let detail = match difference {
            Some(difference) => format!("{reason} {}", difference.detail),
            None => format!("{reason} Current version has no differing pack or patron, or its row is absent."),
        };
        offenders.push(Offender { name, id, detail });
    }
    Ok(Findings { offenders: Some(offenders), notes: Vec::new() })
}

fn items_spanning_packs(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let reviewed = source_review_allowlist(db, "items_spanning_packs")?;
    let mut by_item: BTreeMap<(i64, String), Vec<(String, String)>> = BTreeMap::new();
    let mut statement = db.prepare(
        "SELECT i.id, i.name, pack.name, source.source_kind
         FROM items i JOIN loot_adventure_packs source ON source.item_id = i.id
         JOIN adventure_packs pack ON pack.id = source.pack_id ORDER BY i.id, pack.name",
    )?;
    for row in statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))? {
        let (id, name, pack, kind): (i64, String, String, String) = row?;
        by_item.entry((id, name)).or_default().push((pack, kind));
    }
    let mut offenders = Vec::new();
    let mut saga_rollups = Vec::new();
    let mut reviewed_count = 0;
    for ((id, name), entries) in by_item {
        let packs = entries.iter().map(|(pack, _)| pack.as_str()).collect::<BTreeSet<_>>();
        if packs.len() < 2 {
            continue;
        }
        if reviewed.contains_key(&name) {
            reviewed_count += 1;
            continue;
        }
        if entries.iter().any(|(_, kind)| kind == "saga") {
            saga_rollups.push(name);
        } else {
            offenders.push(Offender { name, id: Some(id), detail: packs.into_iter().collect::<Vec<_>>().join(", ") });
        }
    }
    let notes = vec![format!(
        "{} wiki-reviewed true cross-pack items; {} saga roll-ups through another pack: {}",
        reviewed_count,
        saga_rollups.len(),
        saga_rollups.join(", ")
    )];
    Ok(Findings { offenders: Some(offenders), notes })
}

fn packs_with_several_patrons(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let reviewed = source_review_allowlist(db, "packs_with_several_patrons")?;
    let mut offenders = Vec::new();
    let mut reviewed_count = 0;
    let mut statement = db.prepare(
        "SELECT p.id, p.name, COUNT(DISTINCT q.patron_id), GROUP_CONCAT(DISTINCT patron.name)
         FROM adventure_packs p JOIN quests q ON q.pack_id = p.id
         JOIN patrons patron ON patron.id = q.patron_id
         WHERE p.name <> 'Free to Play' GROUP BY p.id HAVING COUNT(DISTINCT q.patron_id) > 1",
    )?;
    for row in statement.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, String>(3)?))
    })? {
        let (id, name, count, patrons) = row?;
        if reviewed.contains_key(&name) {
            reviewed_count += 1;
        } else {
            offenders.push(Offender { name, id: Some(id), detail: format!("{count} patrons: {patrons}") });
        }
    }
    Ok(Findings {
        offenders: Some(offenders),
        notes: vec![format!("{reviewed_count} wiki-reviewed multi-patron packs")],
    })
}

fn effects_named_like_identifiers(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let mut statement = db.prepare("SELECT name, id FROM effects ORDER BY name")?;
    let rows = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?;
    let mut offenders = Vec::new();
    for row in rows {
        let (name, id) = row?;
        let words = name.split_whitespace().collect::<Vec<_>>();
        let has_camel_case = name
            .chars()
            .zip(name.chars().skip(1))
            .any(|(left, right)| left.is_ascii_lowercase() && right.is_ascii_uppercase());
        let has_number_marker = words.iter().any(|word| matches!(*word, "Number" | "Numeral"));
        let has_repeated_word = words.windows(2).any(|pair| pair[0].eq_ignore_ascii_case(pair[1]));
        let has_buff_type_suffix = words.len() > 1
            && words.last().is_some_and(|word| matches!(*word, "Good" | "Lawful" | "Evil" | "Negative"));
        if has_camel_case
            || name.ends_with(|character: char| character.is_ascii_digit())
            || has_number_marker
            || has_repeated_word
            || words.first() == Some(&"Feat")
            || has_buff_type_suffix
        {
            offenders.push(Offender { name, id: Some(id), detail: "identifier-like name".to_string() });
        }
    }
    Ok(Findings { offenders: Some(offenders), notes: Vec::new() })
}

#[derive(Default)]
struct DescriptionCheckSkips {
    by_line: BTreeMap<(String, i64, i64), BTreeMap<String, BTreeSet<i64>>>,
    unused: BTreeMap<i64, Offender>,
}

fn description_check_skips(db: &Connection) -> Result<DescriptionCheckSkips> {
    let mut statement = db.prepare(
        "SELECT bug.id, bug.owner_id AS annotation_owner_id, identity.name, bug.kind, bug.field,
            bug.qualifier, bug.reason, bug.url, bug.read, bug.intended_value, note.owner_kind, note.owner_id, note.link_sort_order
         FROM game_bug_corrections bug
         JOIN entity_identity identity ON identity.kind = bug.owner_kind AND identity.entity_id = bug.owner_id
         LEFT JOIN note_lines note ON note.note_kind = 'game_bug' AND note.note_id = bug.id
         WHERE bug.skip_description_check = 1 ORDER BY bug.id",
    )?;
    let records = statement
        .query_map([], |row| {
            let kind: String = row.get("kind")?;
            let url: Option<String> = row.get("url")?;
            Ok((
                row.get::<_, i64>("id")?,
                row.get::<_, i64>("annotation_owner_id")?,
                row.get::<_, String>("name")?,
                ddo_etl::tree::DdobuilderCorrection {
                    kind: ddo_model::enums::CorrectionKind::parse(&kind).ok_or(rusqlite::Error::InvalidQuery)?,
                    field: row.get("field")?,
                    qualifier: row.get("qualifier")?,
                    original_name: None,
                    added_as_name: None,
                    upstream: None,
                    reason: row.get("reason")?,
                    source: url.map_or(ddo_etl::AnnotationSource::InGame, ddo_etl::AnnotationSource::Page),
                    read: row.get("read")?,
                },
                row.get::<_, Option<String>>("owner_kind")?,
                row.get::<_, Option<i64>>("owner_id")?,
                row.get::<_, Option<i64>>("link_sort_order")?,
                row.get::<_, String>("intended_value")?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut skips = DescriptionCheckSkips::default();
    if records.is_empty() {
        return Ok(skips);
    }
    let groupings = ddo_etl::groupings::Groupings::from_database(db)?;
    for (note_id, annotation_owner_id, name, note, owner_kind, owner_id, sort_order, intended) in records {
        let mut correction = ddo_etl::tree::correction_for_note(&note, &name)?;
        let target =
            correction.stat.as_deref().or(correction.effect.as_deref()).context("skip flag has no stat target")?;
        let stats = if note.field == "lines" {
            correction.from = ddo_etl::corrections::CorrectionValue::from_json(&intended)?;
            correction.upstream_line_stats()?.into_iter().flatten().collect()
        } else {
            groupings.expanded_stat_names(target)?
        };
        skips.unused.entry(note_id).or_insert_with(|| Offender {
            name,
            id: Some(annotation_owner_id),
            detail: format!("skip flag with nothing to skip: {} {}", note.field, note.qualifier),
        });
        if let (Some(kind), Some(owner), Some(order)) = (owner_kind, owner_id, sort_order) {
            let line = skips.by_line.entry((kind, owner, order)).or_default();
            for stat in stats {
                line.entry(stat).or_default().insert(note_id);
            }
        }
    }
    Ok(skips)
}

fn served_effect_lines_repeat_words(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let mut offenders = Vec::new();
    for (table, owner_column) in EFFECT_OWNER_LINKS {
        for (owner_id, lines) in ddo_api::served_effect_lines(db, table, owner_column)
            .map_err(|error| anyhow::anyhow!("rendering {table}: {error:?}"))?
        {
            for (sort_order, line) in lines {
                let title = line["verbose_name"].as_str().unwrap_or_default();
                if REPEATED_LINE_WORD_ALLOWANCES.contains(&title) {
                    continue;
                }
                let words = title.split_whitespace().map(str::to_lowercase).collect::<Vec<_>>();
                let repeats = words
                    .windows(2)
                    .filter_map(|pair| {
                        let previous = pair[0].trim_start_matches(|letter: char| !letter.is_alphanumeric());
                        let next = pair[1].trim_end_matches(|letter: char| !letter.is_alphanumeric());
                        (previous == next && previous.chars().any(char::is_alphabetic)).then_some(previous)
                    })
                    .collect::<BTreeSet<_>>();
                if !repeats.is_empty() {
                    offenders.push(Offender {
                        name: line["name"].as_str().unwrap_or_default().to_string(),
                        id: line["effect_id"].as_i64(),
                        detail: format!(
                            "{table} owner {owner_id}, line {sort_order}: repeated {}; {title}",
                            repeats.into_iter().collect::<Vec<_>>().join(", ")
                        ),
                    });
                }
            }
        }
    }
    Ok(Findings { offenders: Some(offenders), notes: Vec::new() })
}

fn effect_descriptions_without_their_amounts(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let mut skips = description_check_skips(db)?;
    let mut statement = db.prepare(
        "SELECT e.id, e.name, e.is_stat, e.verbose_name_template, bt.name
        FROM effects e LEFT JOIN bonus_types bt ON bt.id = e.home_bonus_type_id",
    )?;
    let mut percent_stats = BTreeSet::new();
    let mut home_types = BTreeMap::new();
    for row in statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, bool>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    })? {
        let (id, name, is_stat, template, home_type) = row?;
        if is_stat
            && template
                .as_deref()
                .is_some_and(|template| ddo_model::effect_amount::template_slot_states_percent(template, 1))
        {
            percent_stats.insert(name);
        }
        home_types.insert(id, home_type);
    }
    let mut offenders = Vec::new();
    let mut cause_counts = BTreeMap::<&str, usize>::new();
    for (table, owner_column) in EFFECT_OWNER_LINKS {
        for (owner_id, lines) in ddo_api::served_effect_lines(db, table, owner_column)
            .map_err(|error| anyhow::anyhow!("rendering {table}: {error:?}"))?
        {
            for (sort_order, line) in lines {
                let mut amount_findings = served_amount_findings(&line, &percent_stats);
                let home_type =
                    line["effect_id"].as_i64().and_then(|id| home_types.get(&id)).and_then(Option::as_deref);
                let description = line["description"].as_str();
                let text = description.or_else(|| line["verbose_name"].as_str()).unwrap_or_default();
                let text_kind = if description.is_some() {
                    EffectTextKind::Description
                } else if table == "set_bonus_tier_effects" {
                    EffectTextKind::SetLine
                } else {
                    EffectTextKind::EnchantmentLine
                };
                for bonus in line["bonuses"].as_array().into_iter().flatten() {
                    if bonus["bonus_type"]
                        .as_str()
                        .is_some_and(|bonus_type| !served_bonus_type_is_stated(text, bonus_type, home_type, text_kind))
                    {
                        amount_findings.insert((bonus["stat"].as_str().map(str::to_string), "type missing"));
                    }
                }
                let owner_kind = owner_column.strip_suffix("_id").context("effect owner column needs an _id suffix")?;
                let line_skips = skips.by_line.get(&(owner_kind.to_string(), owner_id, sort_order));
                let mut causes = BTreeSet::new();
                for (stat, cause) in amount_findings {
                    if let Some(notes) = stat.as_deref().and_then(|stat| line_skips.and_then(|skips| skips.get(stat))) {
                        for note_id in notes {
                            skips.unused.remove(note_id);
                        }
                    } else {
                        causes.insert(cause);
                    }
                }
                if causes.is_empty() {
                    continue;
                }
                for cause in &causes {
                    *cause_counts.entry(cause).or_default() += 1;
                }
                offenders.push(Offender {
                    name: line["name"].as_str().unwrap_or_default().to_string(),
                    id: line["effect_id"].as_i64(),
                    detail: format!(
                        "{table} owner {owner_id}, line {sort_order}: {}; description {}; line {}",
                        causes.into_iter().collect::<Vec<_>>().join("; "),
                        line["description"],
                        line["verbose_name"]
                    ),
                });
            }
        }
    }
    let mut notes =
        cause_counts.into_iter().map(|(cause, count)| format!("{cause}: {count} links")).collect::<Vec<_>>();
    if !skips.unused.is_empty() {
        notes.push(format!("skip flag with nothing to skip: {} records", skips.unused.len()));
        offenders.extend(skips.unused.into_values());
    }
    Ok(Findings { offenders: Some(offenders), notes })
}

fn effect_templates_with_digits_without_amounts(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let mut statement = db.prepare(
        "WITH bonuses AS (
            SELECT effect_id, COUNT(*) AS bonus_count, SUM(amount_from = 0) AS constant_count
            FROM effect_bonuses GROUP BY effect_id)
        SELECT e.id, e.name, e.verbose_name_template,
            COALESCE(b.bonus_count, 0), COALESCE(b.constant_count, 0)
        FROM effects e LEFT JOIN bonuses b ON b.effect_id = e.id
        WHERE e.is_stat = 0
          AND INSTR(COALESCE(e.verbose_name_template, '') || COALESCE(e.description_template, ''), '{1}') = 0
          AND INSTR(COALESCE(e.verbose_name_template, '') || COALESCE(e.description_template, ''), '{2}') = 0
        ORDER BY e.name",
    )?;
    let mut offenders = Vec::new();
    let mut cause_counts = BTreeMap::<&str, usize>::new();
    for row in statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
        ))
    })? {
        let (id, name, line, bonus_count, constant_count) = row?;
        let numbers = amount_tokens(&line);
        if !numbers.iter().any(|number| !number.is_amount_literal())
            || line_claims_unmodelled_amount(Some(&line), bonus_count > 0)
        {
            continue;
        }
        let title = effect_template_title(&line);
        let rank_named = name.rsplit_once(' ').is_some_and(|(_, rank)| {
            !rank.is_empty()
                && rank.bytes().all(|byte| byte.is_ascii_digit())
                && (title == name
                    || (constant_count > 0
                        && title.strip_prefix(&name).is_some_and(|suffix| suffix.starts_with(char::is_whitespace))))
        });
        let cause = match (constant_count > 0, rank_named && (bonus_count == 0 || constant_count > 0)) {
            (false, true) => "rank-named step, nothing modelled",
            (false, false) => "unmodelled mechanic number",
            (true, true) => "rank-named step beside a modelled constant",
            (true, false) => "number beside a modelled constant",
        };
        *cause_counts.entry(cause).or_default() += 1;
        offenders.push(Offender { name, id: Some(id), detail: cause.to_string() });
    }
    Ok(Findings {
        offenders: Some(offenders),
        notes: cause_counts.into_iter().map(|(cause, count)| format!("{cause}: {count} effects")).collect(),
    })
}

#[derive(Clone)]
struct EffectTemplates {
    name: String,
    omits_bonus_type: bool,
    is_stat: bool,
    is_group: bool,
    description: Option<String>,
    line: Option<String>,
    set_line: Option<String>,
    link_count: i64,
    home_bonus_type_id: Option<i64>,
}

struct TemplateBonus {
    target_id: i64,
    amount_from: i64,
    constant: Option<i64>,
    bonus_type_id: Option<i64>,
}

fn effect_templates_without_their_placeholders(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let links = EFFECT_OWNER_LINKS
        .iter()
        .map(|(table, _)| format!("SELECT effect_id FROM {table}"))
        .collect::<Vec<_>>()
        .join(" UNION ALL ");
    let mut statement = db.prepare(&format!(
        "WITH links AS ({links}), counts AS (
        SELECT effect_id, COUNT(*) AS link_count FROM links GROUP BY effect_id)
        SELECT e.id, e.name, e.is_stat, e.is_group, e.description_template, e.verbose_name_template,
            e.set_bonus_line_template, COALESCE(c.link_count, 0), e.home_bonus_type_id, e.omits_bonus_type
        FROM effects e LEFT JOIN counts c ON c.effect_id = e.id ORDER BY e.name"
    ))?;
    let effects = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                EffectTemplates {
                    name: row.get(1)?,
                    is_stat: row.get(2)?,
                    is_group: row.get(3)?,
                    description: row.get(4)?,
                    line: row.get(5)?,
                    set_line: row.get(6)?,
                    link_count: row.get(7)?,
                    home_bonus_type_id: row.get(8)?,
                    omits_bonus_type: row.get(9)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
    let constant_amount = ddo_model::effect_amount::rounded_amount_sql("constant", "scale", "rounding");
    let mut statement = db.prepare(&format!(
        "SELECT effect_id, target_effect_id, amount_from, {constant_amount}, bonus_type_id
        FROM effect_bonuses ORDER BY effect_id, sort_order"
    ))?;
    let mut bonuses_by_effect = BTreeMap::<i64, Vec<TemplateBonus>>::new();
    for row in statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            TemplateBonus {
                target_id: row.get(1)?,
                amount_from: row.get(2)?,
                constant: row.get(3)?,
                bonus_type_id: row.get(4)?,
            },
        ))
    })? {
        let (effect_id, bonus) = row?;
        bonuses_by_effect.entry(effect_id).or_default().push(bonus);
    }
    let mut statement = db.prepare("SELECT id, name FROM bonus_types")?;
    let bonus_types = statement
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
    let mut offenders = Vec::new();
    let mut counts = BTreeMap::<&str, (usize, i64)>::new();
    for (id, effect) in effects.iter().filter(|(_, effect)| !effect.is_stat) {
        let bonuses = bonuses_by_effect.get(id).map(Vec::as_slice).unwrap_or_default();
        let causes =
            template_causes(TemplateSurface::Effect, effect, bonuses, &effects, &bonuses_by_effect, &bonus_types);
        if causes.is_empty() {
            continue;
        }
        for cause in &causes {
            let count = counts.entry(cause).or_default();
            count.0 += 1;
            count.1 += effect.link_count;
        }
        offenders.push(Offender {
            name: effect.name.clone(),
            id: Some(*id),
            detail: format!("{} links: {}", effect.link_count, causes.into_iter().collect::<Vec<_>>().join("; ")),
        });
    }
    let alias_links = EFFECT_OWNER_LINKS
        .iter()
        .map(|(table, _)| format!("SELECT effect_id, alias FROM {table} WHERE alias IS NOT NULL"))
        .collect::<Vec<_>>()
        .join(" UNION ALL ");
    let mut statement = db.prepare(&format!(
        "WITH links AS ({alias_links}), counts AS (SELECT effect_id, alias, COUNT(*) AS link_count FROM links GROUP BY effect_id, alias)
         SELECT a.effect_id, a.name, a.verbose_name_template, COALESCE(c.link_count, 0), a.omits_bonus_type
           FROM effect_aliases a LEFT JOIN counts c ON c.effect_id = a.effect_id AND c.alias = a.name
          WHERE a.verbose_name_template IS NOT NULL ORDER BY a.name"
    ))?;
    for row in statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, bool>(4)?,
        ))
    })? {
        let (id, name, template, link_count, omits_bonus_type) = row?;
        let canonical = &effects[&id];
        let alias = EffectTemplates {
            name,
            omits_bonus_type,
            line: Some(template),
            description: None,
            set_line: None,
            link_count,
            ..canonical.clone()
        };
        let stat_bonus = [TemplateBonus { target_id: id, amount_from: 1, constant: None, bonus_type_id: None }];
        let bonuses = if canonical.is_stat {
            &stat_bonus[..]
        } else {
            bonuses_by_effect.get(&id).map(Vec::as_slice).unwrap_or_default()
        };
        let causes =
            template_causes(TemplateSurface::Alias, &alias, bonuses, &effects, &bonuses_by_effect, &bonus_types);
        if causes.is_empty() {
            continue;
        }
        for cause in &causes {
            let count = counts.entry(cause).or_default();
            count.0 += 1;
            count.1 += link_count;
        }
        offenders.push(Offender {
            name: alias.name,
            id: Some(id),
            detail: format!(
                "alias of {:?}, {} links: {}",
                canonical.name,
                link_count,
                causes.into_iter().collect::<Vec<_>>().join("; ")
            ),
        });
    }
    offenders.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(Findings {
        offenders: Some(offenders),
        notes: counts
            .into_iter()
            .map(|(cause, (effects, links))| format!("{cause}: {effects} effects, {links} links"))
            .collect(),
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TemplateSurface {
    Effect,
    Alias,
}

fn template_causes(
    surface: TemplateSurface,
    effect: &EffectTemplates,
    bonuses: &[TemplateBonus],
    effects: &BTreeMap<i64, EffectTemplates>,
    bonuses_by_effect: &BTreeMap<i64, Vec<TemplateBonus>>,
    bonus_types: &BTreeMap<i64, String>,
) -> BTreeSet<&'static str> {
    let mut causes = BTreeSet::new();
    let home_type = effect.home_bonus_type_id.and_then(|id| bonus_types.get(&id)).map(String::as_str);
    let slots =
        bonuses.iter().filter(|bonus| bonus.amount_from > 0).map(|bonus| bonus.amount_from).collect::<BTreeSet<_>>();
    let mut percent_slots = BTreeSet::new();
    let mut constants = Vec::new();
    for bonus in bonuses {
        let targets = if effects.get(&bonus.target_id).is_some_and(|target| target.is_group) {
            bonuses_by_effect
                .get(&bonus.target_id)
                .into_iter()
                .flatten()
                .map(|member| member.target_id)
                .collect::<Vec<_>>()
        } else {
            vec![bonus.target_id]
        };
        for target in targets {
            let percent = effects
                .get(&target)
                .and_then(|stat| stat.line.as_deref())
                .is_some_and(|template| ddo_model::effect_amount::template_slot_states_percent(template, 1));
            if percent && bonus.amount_from > 0 {
                percent_slots.insert(bonus.amount_from);
            }
            if let Some(constant) = bonus.constant {
                constants.push((constant, percent));
            }
        }
    }
    let constant_matches = |token: &AmountToken| {
        constants.iter().any(|(amount, percent)| token.matches_amount(*amount) && (!percent || token.percent))
    };
    if let Some(description) = &effect.description {
        let tokens = effect_text_tokens(description);
        for slot in &slots {
            if !tokens.iter().any(|token| matches!(token.kind, EffectTextTokenKind::AmountSlot(read) if read == *slot))
            {
                causes.insert("T1: description lacks a slot its rows read");
            }
        }
        for slot in [1, 2] {
            let placeholder = format!("{{{slot}}}");
            if description.contains(&placeholder)
                && !ddo_model::effect_amount::template_slot_states_percent(description, slot)
                && (percent_slots.contains(&slot)
                    || effect
                        .line
                        .as_deref()
                        .is_some_and(|line| ddo_model::effect_amount::template_slot_states_percent(line, slot)))
            {
                causes.insert("T5: unit only through the renderer's restore");
            }
        }
    }
    let typed_templates = match surface {
        TemplateSurface::Effect => vec![
            (effect.line.as_deref(), EffectTextKind::EnchantmentLine),
            (effect.description.as_deref(), EffectTextKind::Description),
            (effect.set_line.as_deref(), EffectTextKind::SetLine),
        ],
        TemplateSurface::Alias => vec![(effect.line.as_deref(), EffectTextKind::EnchantmentLine)],
    };
    for (text, kind) in typed_templates {
        let Some(text) = text else { continue };
        if kind == EffectTextKind::EnchantmentLine && effect.omits_bonus_type {
            continue;
        }
        if bonuses.iter().any(|bonus| bonus.bonus_type_id.is_none())
            && !effect_text_tokens(text).iter().any(|token| matches!(token.kind, EffectTextTokenKind::BonusType))
        {
            causes.insert("T3: link-typed template lacks %b1");
        }
        if kind != EffectTextKind::SetLine
            && bonuses
                .iter()
                .filter_map(|bonus| bonus.bonus_type_id)
                .filter_map(|id| bonus_types.get(&id))
                .any(|bonus_type| !served_bonus_type_is_stated(text, bonus_type, home_type, kind))
        {
            causes.insert("T4: fixed type word missing from template");
        }
    }
    for line in [effect.line.as_deref(), effect.set_line.as_deref()].into_iter().flatten() {
        let tokens = effect_text_tokens(line);
        let literal_for_slot = !slots.is_empty()
            && constants.is_empty()
            && tokens
                .iter()
                .any(|token| matches!(token.kind, EffectTextTokenKind::Number(amount) if amount.is_amount_literal()));
        let unread_slot = !slots.is_empty()
            && tokens.iter().any(|token| {
                matches!(token.kind, EffectTextTokenKind::AmountSlot(slot) if !slots.contains(&slot))
                    && (token.text.starts_with(['+', '-']) || token.text.ends_with('%'))
            });
        if literal_for_slot || unread_slot {
            causes.insert("T2: line claims a literal for a slot or a slot its rows do not read");
        }
    }
    if line_claims_unmodelled_amount(effect.line.as_deref(), !bonuses.is_empty()) {
        causes.insert("T7: line claims an amount without a bonus definition");
    }
    if !constants.is_empty()
        && [effect.description.as_deref(), effect.line.as_deref(), effect.set_line.as_deref()]
            .into_iter()
            .flatten()
            .any(|template| {
                amount_tokens(template).iter().any(|token| token.is_amount_literal() && !constant_matches(token))
            })
    {
        causes.insert("T6: amount literal does not match a granted constant");
    }
    causes
}

fn line_claims_unmodelled_amount(line: Option<&str>, has_bonuses: bool) -> bool {
    !has_bonuses && line.is_some_and(|line| amount_tokens(line).iter().any(AmountToken::is_amount_literal))
}

#[derive(Clone, Copy)]
struct AmountToken {
    amount: f64,
    signed: bool,
    percent: bool,
    literal: bool,
    identity: bool,
}

impl AmountToken {
    fn is_amount_literal(&self) -> bool {
        self.literal
    }

    fn matches_amount(&self, amount: i64) -> bool {
        if self.signed {
            self.amount == amount as f64
        } else {
            self.amount == amount.unsigned_abs() as f64
        }
    }
}

enum EffectTextTokenKind {
    Number(AmountToken),
    Word,
    AmountSlot(i64),
    BonusType,
    UnfilledPlaceholder,
}

struct EffectTextToken<'a> {
    text: &'a str,
    start: usize,
    end: usize,
    kind: EffectTextTokenKind,
}

fn effect_text_tokens(text: &str) -> Vec<EffectTextToken<'_>> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut position = 0;
    while position < bytes.len() {
        let start = position;
        let signed = matches!(bytes[position], b'+' | b'-');
        let value_start = position + usize::from(signed);
        if bytes.get(value_start) == Some(&b'{') && bytes.get(value_start + 1).is_some_and(u8::is_ascii_digit) {
            position = value_start + 1;
            while bytes.get(position).is_some_and(u8::is_ascii_alphanumeric) {
                position += 1;
            }
            if bytes.get(position) == Some(&b'}') {
                position += 1;
            }
            let kind = match &text[value_start..position] {
                "{1}" => EffectTextTokenKind::AmountSlot(1),
                "{2}" => EffectTextTokenKind::AmountSlot(2),
                _ => EffectTextTokenKind::UnfilledPlaceholder,
            };
            if bytes.get(position) == Some(&b'%') {
                position += 1;
            }
            tokens.push(EffectTextToken { text: &text[start..position], start, end: position, kind });
            continue;
        }
        if bytes[position] == b'%' && bytes.get(position + 1).is_some_and(u8::is_ascii_alphabetic) {
            position += 1;
            while bytes.get(position).is_some_and(u8::is_ascii_alphanumeric) {
                position += 1;
            }
            let kind = if &text[start..position] == "%b1" {
                EffectTextTokenKind::BonusType
            } else {
                EffectTextTokenKind::UnfilledPlaceholder
            };
            tokens.push(EffectTextToken { text: &text[start..position], start, end: position, kind });
            continue;
        }
        if bytes[position].is_ascii_alphabetic() {
            while bytes.get(position).is_some_and(u8::is_ascii_alphabetic) {
                position += 1;
            }
            tokens.push(EffectTextToken {
                text: &text[start..position],
                start,
                end: position,
                kind: EffectTextTokenKind::Word,
            });
            continue;
        }
        if signed && position > 0 && bytes[position - 1].is_ascii_digit() {
            position += 1;
            continue;
        }
        if !bytes.get(value_start).is_some_and(u8::is_ascii_digit) {
            position += 1;
            continue;
        }
        position = value_start;
        while bytes.get(position).is_some_and(|byte| {
            byte.is_ascii_digit() || (*byte == b'.' && bytes.get(position + 1).is_some_and(u8::is_ascii_digit))
        }) {
            position += 1;
        }
        if let Ok(amount) = text[start..position].parse() {
            let percent = bytes.get(position) == Some(&b'%');
            let before = text[..start]
                .split_whitespace()
                .next_back()
                .unwrap_or_default()
                .trim_matches(|character: char| !character.is_ascii_alphabetic())
                .to_ascii_lowercase();
            let after = text[position + usize::from(percent)..]
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .trim_matches(|character: char| !character.is_ascii_alphabetic())
                .to_ascii_lowercase();
            let dice = (bytes.get(position).is_some_and(|byte| matches!(byte, b'd' | b'D'))
                && bytes.get(position + 1).is_some_and(u8::is_ascii_digit))
                || (start >= 2 && matches!(bytes[start - 1], b'd' | b'D') && bytes[start - 2].is_ascii_digit());
            let excluded = dice
                || matches!(before.as_str(), "dc" | "level" | "cr" | "rank" | "tier")
                || matches!(
                    after.trim_end_matches('s'),
                    "second" | "minute" | "hour" | "round" | "turn" | "time" | "stack"
                );
            let identity = !dice
                && !bytes.get(position).is_some_and(u8::is_ascii_alphanumeric)
                && (start == 0 || !bytes[start - 1].is_ascii_alphanumeric());
            position += usize::from(percent);
            tokens.push(EffectTextToken {
                text: &text[start..position],
                start,
                end: position,
                kind: EffectTextTokenKind::Number(AmountToken {
                    amount,
                    signed,
                    percent,
                    literal: (signed || percent) && !excluded,
                    identity,
                }),
            });
        }
    }
    tokens
}

fn amount_tokens(text: &str) -> Vec<AmountToken> {
    effect_text_tokens(text)
        .into_iter()
        .filter_map(|token| match token.kind {
            EffectTextTokenKind::Number(amount) => Some(amount),
            _ => None,
        })
        .collect()
}

fn has_unfilled_effect_token(template: &str) -> bool {
    effect_text_tokens(template).iter().any(|token| matches!(token.kind, EffectTextTokenKind::UnfilledPlaceholder))
}

fn dice_terms(text: &str) -> Vec<(u32, u32)> {
    effect_text_tokens(text)
        .windows(3)
        .filter_map(|tokens| {
            let [count, separator, sides] = tokens else { return None };
            if separator.text != "d"
                || count.end != separator.start
                || separator.end != sides.start
                || (count.start > 0 && text[..count.start].ends_with(char::is_alphanumeric))
            {
                return None;
            }
            Some((
                count.text.trim_start_matches(['+', '-']).parse().ok()?,
                sides.text.trim_end_matches('%').parse().ok()?,
            ))
        })
        .collect()
}

fn effect_template_title(template: &str) -> String {
    let mut title = String::new();
    let mut cursor = 0;
    for token in effect_text_tokens(template) {
        if matches!(token.kind, EffectTextTokenKind::AmountSlot(_) | EffectTextTokenKind::BonusType) {
            title.push_str(&template[cursor..token.start]);
            cursor = token.end;
        }
    }
    title.push_str(&template[cursor..]);
    title.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn effect_identity_numbers(source: &str) -> Vec<String> {
    effect_text_tokens(source)
        .into_iter()
        .filter_map(|token| match token.kind {
            EffectTextTokenKind::Number(amount) if amount.identity => Some(amount.amount.abs().to_string()),
            EffectTextTokenKind::Word
                if matches!(
                    token.text,
                    "I" | "II"
                        | "III"
                        | "IV"
                        | "V"
                        | "VI"
                        | "VII"
                        | "VIII"
                        | "IX"
                        | "X"
                        | "XI"
                        | "XII"
                        | "XIII"
                        | "XIV"
                        | "XV"
                ) =>
            {
                Some(token.text.to_string())
            }
            _ => None,
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EffectTextKind {
    Description,
    EnchantmentLine,
    SetLine,
}

fn served_bonus_type_is_stated(
    text: &str,
    bonus_type: &str,
    home_type: Option<&str>,
    text_kind: EffectTextKind,
) -> bool {
    if ddo_api::bonus_type_line_word(bonus_type, home_type, ddo_api::BonusTypeWording::Enchantment).is_none() {
        return true;
    }
    let wording = if text_kind == EffectTextKind::SetLine {
        ddo_api::BonusTypeWording::SetBonus
    } else {
        ddo_api::BonusTypeWording::Enchantment
    };
    explicit_bonus_type_is_stated(
        text,
        bonus_type,
        ddo_api::bonus_type_line_word(bonus_type, home_type, wording),
        text_kind == EffectTextKind::Description,
    )
}

fn explicit_bonus_type_is_stated(
    text: &str,
    bonus_type: &str,
    line_word: Option<&str>,
    accepts_type_name: bool,
) -> bool {
    if matches!(bonus_type, "Stacking" | "Untyped") {
        return true;
    }
    let words = effect_text_tokens(text)
        .into_iter()
        .filter(|token| matches!(token.kind, EffectTextTokenKind::Word))
        .map(|token| token.text)
        .collect::<Vec<_>>();
    let contains_type = |name: &str| {
        let type_words = name.split_whitespace().collect::<Vec<_>>();
        !type_words.is_empty()
            && words.windows(type_words.len()).any(|phrase| {
                phrase.iter().zip(&type_words).all(|(word, expected)| word.eq_ignore_ascii_case(expected))
            })
    };
    (accepts_type_name && contains_type(bonus_type)) || line_word.is_some_and(contains_type)
}

fn served_amount_findings(
    line: &serde_json::Value,
    percent_stats: &BTreeSet<String>,
) -> BTreeSet<(Option<String>, &'static str)> {
    let mut findings = BTreeSet::new();
    let Some(bonuses) = line["bonuses"].as_array().filter(|bonuses| !bonuses.is_empty()) else { return findings };
    let description = line["description"].as_str();
    let verbose_name = line["verbose_name"].as_str().unwrap_or_default();
    let text_tokens = amount_tokens(description.unwrap_or(verbose_name));
    let granted_amounts = bonuses
        .iter()
        .filter_map(|bonus| {
            Some((
                bonus["stat"].as_str().map(str::to_string),
                bonus["value"].as_i64()?,
                bonus["stat"].as_str().is_some_and(|stat| percent_stats.contains(stat)),
                bonus["scale"].as_f64().is_some_and(|scale| scale != 1.0),
            ))
        })
        .collect::<Vec<_>>();
    for (stat, amount, percent, scaled) in &granted_amounts {
        if text_tokens.iter().any(|token| token.matches_amount(*amount) && (!percent || token.percent)) {
            continue;
        }
        let cause = if *percent && text_tokens.iter().any(|token| token.matches_amount(*amount)) {
            "unit missing"
        } else if *scaled {
            "scaled or rounded amount"
        } else if description.is_none() {
            "description missing, and the line short as well"
        } else {
            "description omits an amount or states a different one"
        };
        findings.insert((stat.clone(), cause));
    }
    for token in amount_tokens(verbose_name).iter().filter(|token| token.is_amount_literal()) {
        if granted_amounts
            .iter()
            .any(|(_, amount, percent, _)| token.matches_amount(*amount) && (!percent || token.percent))
        {
            continue;
        }
        let matching =
            granted_amounts.iter().filter(|(_, amount, _, _)| token.matches_amount(*amount)).collect::<Vec<_>>();
        if matching.is_empty() {
            findings.insert((None, "line claim that is not a granted amount"));
        } else {
            for (stat, _, _, _) in matching {
                findings.insert((stat.clone(), "unit missing"));
            }
        }
    }
    findings
}

fn effect_link_amount_counts(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let count = "CASE WHEN e.is_stat = 1 THEN 1
                 WHEN INSTR(COALESCE(e.verbose_name_template, '') || COALESCE(e.description_template, ''), '{2}') > 0 THEN 2
                 WHEN INSTR(COALESCE(e.verbose_name_template, '') || COALESCE(e.description_template, ''), '{1}') > 0 THEN 1 ELSE 0 END";
    let queries: Vec<String> = EFFECT_OWNER_LINKS
        .iter()
        .map(|(table, owner_column)| {
            format!(
                "SELECT e.name, e.id, '{table} owner ' || l.{owner_column} || ' carries wrong amount count'
                 FROM {table} l JOIN effects e ON e.id = l.effect_id
                 WHERE (l.value IS NOT NULL) + (l.value2 IS NOT NULL) > ({count})"
            )
        })
        .collect();
    Ok(Findings { offenders: Some(offenders_from_sql(db, &queries.join(" UNION ALL "))?), notes: Vec::new() })
}

fn effect_line_rendering_tokens(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let mut offenders = Vec::new();
    let mut templates_by_effect = BTreeMap::new();
    let mut statement = db.prepare(&format!(
        "SELECT e.id, e.name, e.verbose_name_template, e.set_bonus_line_template, e.description_template,
                home_type.name, fixed_type.name FROM effects e
         LEFT JOIN bonus_types home_type ON home_type.id = e.home_bonus_type_id
         LEFT JOIN ({fixed_types}) fixed_type ON fixed_type.effect_id=e.id",
        fixed_types = ddo_model::bonus_type_wording::FIXED_EFFECT_BONUS_TYPES_SQL,
    ))?;
    let effects = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            [row.get::<_, Option<String>>(2)?, row.get::<_, Option<String>>(3)?, row.get::<_, Option<String>>(4)?],
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
        ))
    })?;
    for effect in effects {
        let (effect_id, name, templates, home_type, fixed_type) = effect?;
        for (field, template) in ["verbose_name", "set_bonus_line", "description"].into_iter().zip(&templates) {
            if template.as_deref().is_some_and(has_unfilled_effect_token) {
                offenders.push(Offender {
                    name: name.clone(),
                    id: Some(effect_id),
                    detail: format!("{field} has an unfilled placeholder"),
                });
            }
        }
        templates_by_effect.insert(effect_id, (name, templates, home_type, fixed_type));
    }
    let links = EFFECT_OWNER_LINKS
        .iter()
        .map(|(table, _)| format!("SELECT effect_id, bonus_type_id FROM {table}"))
        .collect::<Vec<_>>()
        .join(" UNION ALL ");
    let mut statement = db.prepare(&format!(
        "WITH links AS ({links}) SELECT l.effect_id, bt.name, COUNT(*) FROM links l
         LEFT JOIN bonus_types bt ON bt.id = l.bonus_type_id
         GROUP BY l.effect_id, l.bonus_type_id"
    ))?;
    let types = statement
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, i64>(2)?)))?;
    for entry in types {
        let (effect_id, bonus_type, link_count) = entry?;
        let (name, templates, home_type, fixed_type) = &templates_by_effect[&effect_id];
        let resolved_type = bonus_type.as_deref().or(home_type.as_deref()).or(fixed_type.as_deref());
        for (field, template) in ["verbose_name", "set_bonus_line", "description"].into_iter().zip(templates) {
            let Some(template) = template else { continue };
            if effect_text_tokens(template).iter().any(|token| matches!(token.kind, EffectTextTokenKind::BonusType))
                && resolved_type.is_none()
            {
                offenders.push(Offender {
                    name: name.clone(),
                    id: Some(effect_id),
                    detail: format!("{link_count} {field} line(s) have an unfilled bonus-type token"),
                });
                continue;
            }
            let Some(bonus_type) = resolved_type else { continue };
            let shown_type = if field == "verbose_name" {
                ddo_api::bonus_type_line_word(bonus_type, home_type.as_deref(), ddo_api::BonusTypeWording::Enchantment)
                    .unwrap_or("")
            } else {
                bonus_type
            };
            let rendered = template.replace("%b1", shown_type).to_ascii_lowercase();
            let repeated_type =
                format!("{} {} bonus", shown_type.to_ascii_lowercase(), shown_type.to_ascii_lowercase());
            if !shown_type.is_empty() && rendered.contains(&repeated_type) {
                offenders.push(Offender {
                    name: name.clone(),
                    id: Some(effect_id),
                    detail: format!("{link_count} {field} line(s) repeat {bonus_type:?} in the amount position"),
                });
            }
        }
    }
    Ok(Findings { offenders: Some(offenders), notes: Vec::new() })
}

fn effect_links_missing_first_amount(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let queries: Vec<String> = EFFECT_OWNER_LINKS
        .iter()
        .map(|(table, owner_column)| {
            format!(
                "SELECT e.name, e.id, '{table} owner ' || l.{owner_column} || ' lacks first amount'
                 FROM {table} l JOIN effects e ON e.id = l.effect_id
                 WHERE e.is_stat = 0 AND INSTR(COALESCE(e.verbose_name_template, '') || COALESCE(e.description_template, ''), '{{1}}') > 0
                   AND l.value IS NULL AND e.default_value IS NULL"
            )
        })
        .collect();
    Ok(Findings { offenders: Some(offenders_from_sql(db, &queries.join(" UNION ALL "))?), notes: Vec::new() })
}

fn effect_links_with_only_zero_bonuses(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let links = EFFECT_OWNER_LINKS
        .iter()
        .map(|(table, owner_column)| {
            format!(
                "SELECT '{table}' AS owner_kind, {owner_column} AS owner_id, effect_id, value, value2, sort_order FROM {table}"
            )
        })
        .collect::<Vec<_>>()
        .join(" UNION ALL ");
    let amount = "CASE eb.amount_from WHEN 0 THEN eb.constant WHEN 1 THEN COALESCE(l.value, e.default_value) ELSE COALESCE(l.value2, e.default_value2) END";
    let rounded = ddo_model::effect_amount::rounded_amount_sql(amount, "eb.scale", "eb.rounding");
    let sql = format!(
        "WITH links AS ({links}), resolved AS (
           SELECT l.owner_kind, l.owner_id, l.sort_order, e.name, e.id,
                  COALESCE(l.value, e.default_value) AS amount
             FROM links l JOIN effects e ON e.id = l.effect_id WHERE e.is_stat = 1
           UNION ALL
           SELECT l.owner_kind, l.owner_id, l.sort_order, e.name, e.id, {rounded} AS amount
             FROM links l JOIN effects e ON e.id = l.effect_id
             JOIN effect_bonuses eb ON eb.effect_id = e.id WHERE e.is_stat = 0)
         SELECT name, id, owner_kind || ' owner ' || owner_id || ' has only zero bonuses'
           FROM resolved GROUP BY owner_kind, owner_id, sort_order, id
          HAVING COUNT(amount) > 0 AND SUM(amount <> 0) = 0"
    );
    Ok(Findings { offenders: Some(offenders_from_sql(db, &sql)?), notes: Vec::new() })
}

fn effects_with_unused_default(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let mut queries = Vec::new();
    for (slot, default_column, link_column) in [(1, "default_value", "value"), (2, "default_value2", "value2")] {
        let any_explicit = EFFECT_OWNER_LINKS
            .iter()
            .map(|(table, _)| format!("SELECT effect_id FROM {table} WHERE {link_column} IS NOT NULL"))
            .collect::<Vec<_>>()
            .join(" UNION ");
        queries.push(format!(
            "SELECT e.name, e.id, 'slot {slot} default is used by every link' FROM effects e
             WHERE e.{default_column} IS NOT NULL AND e.id NOT IN ({any_explicit})"
        ));
    }
    Ok(Findings { offenders: Some(offenders_from_sql(db, &queries.join(" UNION ALL "))?), notes: Vec::new() })
}

fn effects_with_tied_home_bonus_types(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let sql = "SELECT e.name, e.id, 'multiple bonus types each occur on ' || MAX(v.item_count) || ' items'
          FROM effects e JOIN effect_vocabulary_bonus_types v ON v.id = e.id
           AND v.kind = CASE WHEN e.is_stat THEN 'stat' WHEN e.is_group THEN 'group' ELSE 'effect' END
         WHERE NOT e.is_stat AND e.home_bonus_type_id IS NULL AND v.item_count > 0
         GROUP BY e.id
        HAVING SUM(v.item_count = (SELECT MAX(candidate.item_count) FROM effect_vocabulary_bonus_types candidate
                                    WHERE candidate.id = e.id AND candidate.kind = v.kind)) > 1";
    let offenders = offenders_from_sql(db, sql)?;
    Ok(Findings { offenders: Some(offenders), notes: Vec::new() })
}

#[derive(PartialEq, Eq)]
enum EffectTitleMismatch {
    Name,
    Number,
}

fn effect_title_mismatch(name: &str, template: &str) -> Option<EffectTitleMismatch> {
    let title = effect_template_title(template);
    let name_numbers = effect_identity_numbers(name);
    let title_numbers = effect_identity_numbers(&title);
    if !name_numbers.is_empty()
        && !title_numbers.is_empty()
        && !name_numbers.iter().all(|number| title_numbers.contains(number))
    {
        return Some(EffectTitleMismatch::Number);
    }
    let alias = EFFECT_MAP.template_name_aliases.get(name).map(String::as_str);
    let matched_name = std::iter::once(name)
        .chain(alias)
        .any(|candidate| title.to_ascii_lowercase().contains(&candidate.to_ascii_lowercase()));
    (!matched_name || (!name_numbers.is_empty() && name_numbers != title_numbers)).then_some(EffectTitleMismatch::Name)
}

fn effect_title_mismatches(db: &Connection, mismatch: EffectTitleMismatch) -> Result<Findings> {
    let offenders = in_use_effect_title_templates(db)?
        .into_iter()
        .filter_map(|EffectTitleTemplate { id, name, template, search_only_aliases }| {
            let title_mismatch = effect_title_mismatch(&name, &template);
            let matches_alias = title_mismatch == Some(EffectTitleMismatch::Name)
                && search_only_aliases.iter().any(|alias| {
                    effect_identity_numbers(alias) == effect_identity_numbers(&name)
                        && effect_title_mismatch(alias, &template).is_none()
                });
            (title_mismatch.as_ref() == Some(&mismatch) && !matches_alias).then_some(Offender {
                name,
                id: Some(id),
                detail: format!("template {template:?}"),
            })
        })
        .collect();
    Ok(Findings { offenders: Some(offenders), notes: Vec::new() })
}

fn effect_templates_disagree_with_names(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    effect_title_mismatches(db, EffectTitleMismatch::Name)
}

fn effect_template_numbers_mismatch_names(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    effect_title_mismatches(db, EffectTitleMismatch::Number)
}

fn owner_bonuses_repeated_across_links(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    type LinkBonuses = BTreeMap<(i64, Option<i64>), i64>;
    let mut statement = db.prepare(
        "SELECT ob.owner_kind, ob.owner_id, ob.effect_link_order, ob.stat_id, ob.bonus_type_id, ob.amount,
                COALESCE(via.name, stat.name)
           FROM owner_bonuses ob JOIN effects stat ON stat.id = ob.stat_id
           LEFT JOIN effects via ON via.id = ob.via_effect_id
          ORDER BY ob.owner_kind, ob.owner_id, ob.effect_link_order",
    )?;
    let mut links_by_owner: BTreeMap<(String, i64), BTreeMap<i64, (String, LinkBonuses)>> = BTreeMap::new();
    for row in statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, Option<i64>>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, String>(6)?,
        ))
    })? {
        let (owner_kind, owner_id, link_order, stat_id, bonus_type_id, amount, effect_name) = row?;
        links_by_owner
            .entry((owner_kind, owner_id))
            .or_default()
            .entry(link_order)
            .or_insert_with(|| (effect_name, LinkBonuses::new()))
            .1
            .insert((stat_id, bonus_type_id), amount);
    }
    let mut offenders = Vec::new();
    for ((owner_kind, owner_id), links) in &links_by_owner {
        for (repeating_order, (repeating_name, repeating_bonuses)) in links {
            let repeated_link = links.iter().find(|(granting_order, (_, granting_bonuses))| {
                granting_order != &repeating_order
                    && repeating_bonuses.keys().all(|key| granting_bonuses.contains_key(key))
                    && (repeating_bonuses.len() < granting_bonuses.len() || granting_order < &repeating_order)
            });
            if let Some((_, (granting_name, granting_bonuses))) = repeated_link {
                let amounts = if repeating_bonuses.iter().all(|(key, amount)| granting_bonuses[key] == *amount) {
                    "same amounts"
                } else {
                    "different amounts"
                };
                offenders.push(Offender {
                    name: format!("{owner_kind}:{owner_id}"),
                    id: Some(*owner_id),
                    detail: format!("{repeating_name} repeats {granting_name} ({amounts})"),
                });
            }
        }
    }
    Ok(Findings { offenders: Some(offenders), notes: Vec::new() })
}

fn has_fewer_critical_dice_than_hit_dice(description: &str) -> bool {
    let lowercase = description.to_ascii_lowercase();
    let Some((before_critical, _)) = lowercase.split_once("critical hit") else {
        return false;
    };
    let terms = dice_terms(before_critical);
    match (terms.first(), terms.last()) {
        (Some(hit), Some(critical)) if terms.len() > 1 => critical.0 * critical.1 < hit.0 * hit.1,
        _ => false,
    }
}

fn effect_descriptions_with_fewer_critical_dice_than_hit_dice(
    db: &Connection,
    _options: &IntegrityOptions,
) -> Result<Findings> {
    let links = EFFECT_OWNER_LINKS
        .iter()
        .map(|(table, _)| format!("SELECT effect_id FROM {table}"))
        .collect::<Vec<_>>()
        .join(" UNION ");
    let mut statement = db.prepare(&format!(
        "SELECT e.id, e.name, e.description_template FROM effects e
          WHERE e.id IN ({links}) AND e.description_template IS NOT NULL ORDER BY e.name"
    ))?;
    let offenders = statement
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter(|(_, _, description)| has_fewer_critical_dice_than_hit_dice(description))
        .map(|(id, name, description)| Offender { name, id: Some(id), detail: format!("description {description:?}") })
        .collect();
    Ok(Findings { offenders: Some(offenders), notes: Vec::new() })
}

fn owner_names_disagree_with_linked_effects(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let mut offenders = Vec::new();
    for effect_name in EFFECT_MAP.named_effect_ids.keys() {
        for (owner_table, link_table, owner_column) in [
            ("augments", "augment_effects", "augment_id"),
            ("item_augment_slot_options", "item_augment_slot_option_effects", "option_id"),
        ] {
            let sql = format!(
                "SELECT owner.name, owner.id, 'names ' || ?1 || ' but links other effects'
                   FROM {owner_table} owner
                  WHERE INSTR(LOWER(owner.name), LOWER(?1)) > 0
                    AND EXISTS (SELECT 1 FROM {link_table} link WHERE link.{owner_column} = owner.id)
                    AND NOT EXISTS (SELECT 1 FROM {link_table} link JOIN effects linked ON linked.id = link.effect_id
                                     WHERE link.{owner_column} = owner.id AND linked.name = ?1)"
            );
            let mut statement = db.prepare(&sql)?;
            let matches = statement
                .query_map([effect_name], |row| {
                    Ok(Offender { name: row.get(0)?, id: row.get(1)?, detail: row.get(2)? })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            offenders.extend(matches);
        }
    }
    Ok(Findings { offenders: Some(offenders), notes: Vec::new() })
}

struct EffectTitleTemplate {
    id: i64,
    name: String,
    template: String,
    search_only_aliases: Vec<String>,
}

fn in_use_effect_title_templates(db: &Connection) -> Result<Vec<EffectTitleTemplate>> {
    let links = EFFECT_OWNER_LINKS
        .iter()
        .map(|(table, _)| format!("SELECT effect_id FROM {table}"))
        .collect::<Vec<_>>()
        .join(" UNION ");
    let mut statement = db.prepare(&format!(
        "SELECT e.id, e.name, e.verbose_name_template, a.name FROM effects e
           LEFT JOIN effect_aliases a ON a.effect_id = e.id AND a.verbose_name_template IS NULL
          WHERE e.id IN ({links}) AND e.verbose_name_template IS NOT NULL"
    ))?;
    let mut templates = BTreeMap::new();
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let id = row.get(0)?;
        let template = templates.entry(id).or_insert(EffectTitleTemplate {
            id,
            name: row.get(1)?,
            template: row.get(2)?,
            search_only_aliases: Vec::new(),
        });
        if let Some(alias) = row.get::<_, Option<String>>(3)? {
            template.search_only_aliases.push(alias);
        }
    }
    Ok(templates.into_values().collect())
}

fn skill_groups_with_different_home_bonus_types(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let mut groups = Vec::new();
    for ability in EFFECT_MAP.skill_ability_groups.skills.keys() {
        let name = format!("{ability} Skills");
        let home: Option<Option<i64>> = db
            .query_row("SELECT home_bonus_type_id FROM effects WHERE name = ?1 AND is_group = 1", [&name], |row| {
                row.get(0)
            })
            .optional()?;
        if let Some(home) = home {
            groups.push((name, home));
        }
    }
    let distinct: BTreeSet<Option<i64>> = groups.iter().map(|(_, home)| *home).collect();
    let offenders = if distinct.len() <= 1 {
        Vec::new()
    } else {
        groups
            .into_iter()
            .map(|(name, home)| Offender { name, id: None, detail: format!("home bonus type id {home:?}") })
            .collect()
    };
    Ok(Findings { offenders: Some(offenders), notes: Vec::new() })
}

fn effect_bonus_type_sources(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let mut queries = vec!["SELECT e.name, e.id, 'fixed stat type conflicts with %b1 template' FROM effects e
         JOIN effect_bonuses s ON s.effect_id = e.id
         WHERE s.bonus_type_id IS NOT NULL
           AND NOT EXISTS (SELECT 1 FROM effect_bonuses linked WHERE linked.effect_id = e.id AND linked.bonus_type_id IS NULL)
           AND INSTR(COALESCE(e.verbose_name_template, '') || COALESCE(e.description_template, ''), '%b1') > 0"
        .to_string()];
    queries.extend(EFFECT_OWNER_LINKS.iter().map(|(table, owner_column)| {
        format!(
            "SELECT e.name, e.id, '{table} owner ' || l.{owner_column} || ' has wrong type source'
             FROM {table} l JOIN effects e ON e.id = l.effect_id
             WHERE e.is_stat = 0 AND
               ((l.bonus_type_id IS NULL AND EXISTS
                 (SELECT 1 FROM effect_bonuses eb WHERE eb.effect_id = e.id AND eb.bonus_type_id IS NULL))
                OR (l.bonus_type_id IS NOT NULL
                    AND INSTR(COALESCE(e.verbose_name_template, '') || COALESCE(e.description_template, ''), '%b1') = 0
                    AND NOT EXISTS
                      (SELECT 1 FROM effect_bonuses eb WHERE eb.effect_id = e.id AND eb.bonus_type_id IS NULL)))"
        )
    }));
    Ok(Findings { offenders: Some(offenders_from_sql(db, &queries.join(" UNION ALL "))?), notes: Vec::new() })
}

fn stat_link_findings(db: &Connection, condition: &str, detail: &str) -> Result<Findings> {
    let queries = EFFECT_OWNER_LINKS
        .iter()
        .map(|(table, owner_column)| {
            format!(
                "SELECT e.name, e.id, '{table} owner ' || l.{owner_column} || ' {detail}'
                 FROM {table} l JOIN effects e ON e.id = l.effect_id
                 WHERE e.is_stat = 1 AND ({condition})"
            )
        })
        .collect::<Vec<_>>();
    Ok(Findings { offenders: Some(offenders_from_sql(db, &queries.join(" UNION ALL "))?), notes: Vec::new() })
}

fn deferred_line_splits_are_live(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let offenders = ddo_etl::line_exceptions::deferred_split_violations(db)?
        .into_iter()
        .map(|violation| Offender { name: violation.owner_slug, id: None, detail: violation.detail })
        .collect();
    Ok(Findings { offenders: Some(offenders), notes: Vec::new() })
}

fn effects_sharing_a_stat_set(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let groupings = ddo_etl::groupings::Groupings::from_database(db)?;
    let offenders = groupings
        .pairs(ddo_etl::groupings::GroupingPairDomain::LinkValued)
        .into_iter()
        .map(|pair| {
            let mut stat_names =
                pair.first.stat_ids.iter().map(|id| groupings.by_id[id].name.as_str()).collect::<Vec<_>>();
            stat_names.sort_unstable();
            Offender {
                name: format!("{} / {}", pair.first.name, pair.second.name),
                id: Some(pair.first.id),
                detail: format!("{:?}: {}", pair.kind, stat_names.join(", ")),
            }
        })
        .collect();
    Ok(Findings { offenders: Some(offenders), notes: Vec::new() })
}

fn stat_links_without_bonus_types(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    stat_link_findings(db, "l.bonus_type_id IS NULL", "has no bonus type")
}

fn stat_links_at_routed_types(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let offenders = ddo_etl::build::stat_links_at_routed_types(db)?
        .into_iter()
        .map(|route| Offender {
            name: route.stat,
            id: Some(route.owner_id),
            detail: format!(
                "{} owner {}, line {}: {} must link {}",
                route.table, route.owner_id, route.sort_order, route.bonus_type, route.effect
            ),
        })
        .collect();
    Ok(Findings { offenders: Some(offenders), notes: Vec::new() })
}

fn stat_links_missing_value(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    stat_link_findings(db, "l.value IS NULL AND e.default_value IS NULL", "has no first amount")
}

fn effect_types_not_classified(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let mut unclassified_types = BTreeSet::new();
    let mut items_by_type: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut modifier_statement = db.prepare(
        "SELECT DISTINCT m.effect_type, i.name FROM modifiers m
         LEFT JOIN items i ON m.source_kind = 'item' AND i.id = m.source_id
         UNION SELECT DISTINCT types.value, i.name FROM modifiers m, json_each(m.extra_types) types
         LEFT JOIN items i ON m.source_kind = 'item' AND i.id = m.source_id",
    )?;
    for row in
        modifier_statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)))?
    {
        let (effect_type, item_name) = row?;
        if !EFFECT_MAP.is_classified_effect_type(&effect_type) {
            unclassified_types.insert(effect_type.clone());
            if let Some(item_name) = item_name {
                items_by_type.entry(effect_type).or_default().insert(item_name);
            }
        }
    }
    let offenders = unclassified_types
        .into_iter()
        .map(|name| {
            let count = items_by_type.get(&name).map_or(0, BTreeSet::len);
            Offender { name, id: None, detail: format!("{count} items with stored modifiers") }
        })
        .collect();
    Ok(Findings { offenders: Some(offenders), notes: Vec::new() })
}

fn legacy_items_without_a_reason(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    if !has_column(db, "items", "is_legacy")? {
        return Ok(Findings { offenders: None, notes: vec!["items.is_legacy is absent".to_string()] });
    }
    let offenders = offenders_from_sql(
        db,
        "SELECT i.name, i.id, 'is_legacy, yet it has a source, its name says neither (legacy) nor (historic), \
         and no correction sets is_legacy' FROM items i \
         WHERE i.is_legacy = 1 \
         AND lower(i.name) NOT LIKE '%(legacy)%' AND lower(i.name) NOT LIKE '%(historic)%' \
         AND NOT EXISTS (SELECT 1 FROM corrections c WHERE c.kind = 'item' AND c.name = i.name \
                         AND c.field = 'is_legacy') \
         AND EXISTS (SELECT 1 FROM sources d WHERE d.item_id = i.id)",
    )?;
    Ok(Findings { offenders: Some(offenders), notes: Vec::new() })
}

fn items_without_a_source(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let has_is_legacy = has_column(db, "items", "is_legacy")?;
    let legacy_filter = if has_is_legacy { "AND i.is_legacy = 0" } else { "" };
    let offenders = offenders_from_sql(
        db,
        &format!(
            "SELECT i.name, i.id, {DROP_LOCATION_HEAD_SQL} FROM items i \
             WHERE NOT EXISTS (SELECT 1 FROM sources d WHERE d.item_id = i.id) {legacy_filter} ORDER BY i.name"
        ),
    )?;
    let notes = if has_is_legacy { Vec::new() } else { vec!["items.is_legacy is absent; no item is excluded".into()] };
    Ok(Findings { offenders: Some(offenders), notes })
}

fn items_with_a_source_but_no_pack(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let offenders = offenders_from_sql(
        db,
        "SELECT i.name, i.id, GROUP_CONCAT(DISTINCT s.kind ORDER BY s.kind)
         FROM items i JOIN sources s ON s.item_id = i.id
         WHERE i.is_legacy = 0
           AND s.kind IN ('quest', 'quest_chain', 'saga', 'adventure_pack', 'challenge', 'crafting_system', 'vendor')
           AND NOT EXISTS (SELECT 1 FROM loot_adventure_packs packs WHERE packs.item_id = i.id)
         GROUP BY i.id ORDER BY i.name",
    )?;
    let mut item_counts_by_source_kind = BTreeMap::new();
    for offender in &offenders {
        for source_kind in offender.detail.split(',') {
            *item_counts_by_source_kind.entry(source_kind).or_insert(0) += 1;
        }
    }
    let source_kind_counts: Vec<String> =
        item_counts_by_source_kind.into_iter().map(|(source_kind, count)| format!("{source_kind}: {count}")).collect();
    let notes = if source_kind_counts.is_empty() {
        Vec::new()
    } else {
        vec![format!("items by source kind (an item can count under several kinds): {}", source_kind_counts.join(", "))]
    };
    Ok(Findings { offenders: Some(offenders), notes })
}

fn empty_tables(db: &Connection, options: &IntegrityOptions) -> Result<Findings> {
    let mut statement =
        db.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")?;
    let table_names = statement.query_map([], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut offenders = Vec::new();
    let mut notes = Vec::new();
    for table_name in table_names {
        if matches!(
            table_name.as_str(),
            "game_bug_corrections" | "verification_requests" | "verification_request_sources" | "note_lines"
        ) {
            notes.push(format!(
                "allowed empty: {table_name}: optional annotation table; zero recorded bugs or open questions is valid"
            ));
            continue;
        }
        if table_name == "effect_aliases" {
            notes.push("allowed empty: effect_aliases: aliases are optional; zero aliases is valid".to_string());
            continue;
        }
        if matches!(table_name.as_str(), "redirects" | "retired_slugs") {
            continue;
        }
        let row_count: i64 = db.query_row(&format!("SELECT COUNT(*) FROM \"{table_name}\""), [], |row| row.get(0))?;
        if row_count == 0 && !options.allowed_empty_tables.contains(&table_name) {
            offenders.push(Offender { name: table_name, id: None, detail: "no rows".to_string() });
        }
    }
    let allowed_text = match options.allowed_empty_tables.as_slice() {
        [] => "none".to_string(),
        allowed_tables => allowed_tables.join(", "),
    };
    notes.push(format!("allowed empty: {allowed_text}"));
    Ok(Findings { offenders: Some(offenders), notes })
}

fn redirect_findings(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let offenders = ddo_etl::tree::redirects::redirect_violations(db)?
        .into_iter()
        .map(|detail| Offender { name: detail.clone(), id: None, detail })
        .collect();
    Ok(Findings { offenders: Some(offenders), notes: Vec::new() })
}

fn folded_item_name(item_name: &str) -> String {
    item_name.chars().filter(|character| character.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

fn near_duplicate_item_names(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let mut statement = db.prepare("SELECT name, id FROM items ORDER BY name")?;
    let items = statement
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut items_by_folded_name: BTreeMap<String, Vec<(String, i64)>> = BTreeMap::new();
    for (item_name, item_id) in items {
        items_by_folded_name.entry(folded_item_name(&item_name)).or_default().push((item_name, item_id));
    }
    let mut offenders = Vec::new();
    for same_named_items in items_by_folded_name.values().filter(|same_named_items| same_named_items.len() > 1) {
        for (item_name, item_id) in same_named_items {
            let other_names: Vec<String> = same_named_items
                .iter()
                .filter(|(other_name, _)| other_name != item_name)
                .map(|(other_name, _)| format!("{other_name:?}"))
                .collect();
            offenders.push(Offender {
                name: item_name.clone(),
                id: Some(*item_id),
                detail: format!("same as {}", other_names.join(", ")),
            });
        }
    }
    Ok(Findings { offenders: Some(offenders), notes: Vec::new() })
}

fn unreferenced_stats(db: &Connection, _options: &IntegrityOptions) -> Result<Findings> {
    let offenders = offenders_from_sql(
        db,
        "SELECT s.name, s.id, s.category FROM effects s WHERE s.is_stat = 1 AND NOT EXISTS (
           SELECT 1 FROM owner_bonuses ob WHERE ob.stat_id = s.id) ORDER BY s.name",
    )?;
    Ok(Findings { offenders: Some(offenders), notes: Vec::new() })
}

fn findings_of(check: &IntegrityCheck, db: &Connection, options: &IntegrityOptions) -> Result<Findings> {
    match check.offender_query {
        OffenderQuery::Sql(sql) => Ok(Findings { offenders: Some(offenders_from_sql(db, sql)?), notes: Vec::new() }),
        OffenderQuery::Built(build_findings) => build_findings(db, options),
    }
}

fn top_details_of(offenders: &[Offender], limit: usize) -> Vec<(String, usize)> {
    let mut count_by_detail: Vec<(String, usize)> = Vec::new();
    for offender in offenders {
        match count_by_detail.iter_mut().find(|(detail, _)| *detail == offender.detail) {
            Some((_, count)) => *count += 1,
            None => count_by_detail.push((offender.detail.clone(), 1)),
        }
    }
    count_by_detail.sort_by(|(left_detail, left_count), (right_detail, right_count)| {
        right_count.cmp(left_count).then_with(|| left_detail.cmp(right_detail))
    });
    count_by_detail.truncate(limit);
    count_by_detail
}

pub struct CheckOutcome {
    pub name: &'static str,
    pub severity: Severity,
    pub description: &'static str,
    pub status: CheckStatus,
    pub offenders: Vec<Offender>,
    pub top_details: Vec<(String, usize)>,
    pub notes: Vec<String>,
    shown_offender_limit: usize,
}

fn outcome_of(check: &IntegrityCheck, db: &Connection, options: &IntegrityOptions) -> Result<CheckOutcome> {
    let findings = findings_of(check, db, options).with_context(|| format!("running check {}", check.name))?;
    let (status, offenders) = match findings.offenders {
        None => (CheckStatus::Skipped, Vec::new()),
        Some(offenders) if offenders.is_empty() => (CheckStatus::Passed, offenders),
        Some(offenders) => match check.severity {
            Severity::Hard => (CheckStatus::Failed, offenders),
            Severity::Warn => (CheckStatus::Warned, offenders),
        },
    };
    Ok(CheckOutcome {
        name: check.name,
        severity: check.severity,
        description: check.description,
        status,
        top_details: top_details_of(&offenders, check.top_detail_limit),
        offenders,
        notes: findings.notes,
        shown_offender_limit: check.shown_offender_limit,
    })
}

pub struct IntegrityReport {
    pub outcomes: Vec<CheckOutcome>,
}

impl IntegrityReport {
    pub fn outcome(&self, check_name: &str) -> Option<&CheckOutcome> {
        self.outcomes.iter().find(|outcome| outcome.name == check_name)
    }

    pub fn failed_hard_check_names(&self) -> Vec<&'static str> {
        self.outcomes
            .iter()
            .filter(|outcome| outcome.status == CheckStatus::Failed)
            .map(|outcome| outcome.name)
            .collect()
    }
}

fn offender_count_text(offender_count: usize) -> String {
    match offender_count {
        1 => "1 offender".to_string(),
        _ => format!("{offender_count} offenders"),
    }
}

impl fmt::Display for CheckOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status_text = match self.status {
            CheckStatus::Passed => "ok".to_string(),
            CheckStatus::Skipped => "skipped".to_string(),
            CheckStatus::Failed => format!("FAIL ({})", offender_count_text(self.offenders.len())),
            CheckStatus::Warned => format!("WARN ({})", offender_count_text(self.offenders.len())),
        };
        writeln!(formatter, "check {}: {status_text}", self.name)?;
        if !matches!(self.status, CheckStatus::Passed) {
            writeln!(formatter, "  {}", self.description)?;
        }
        for offender in self.offenders.iter().take(self.shown_offender_limit) {
            let id_text = offender.id.map(|id| format!(" #{id}")).unwrap_or_default();
            writeln!(formatter, "  {:?}{id_text}: {}", offender.name, offender.detail)?;
        }
        if self.offenders.len() > self.shown_offender_limit {
            writeln!(formatter, "  ... and {} more", self.offenders.len() - self.shown_offender_limit)?;
        }
        if !self.top_details.is_empty() {
            writeln!(formatter, "  top {}:", self.top_details.len())?;
            for (detail, count) in &self.top_details {
                writeln!(formatter, "  {count:>6}  {detail}")?;
            }
        }
        for note in &self.notes {
            writeln!(formatter, "  note: {note}")?;
        }
        Ok(())
    }
}

impl fmt::Display for IntegrityReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for outcome in &self.outcomes {
            write!(formatter, "{outcome}")?;
        }
        let warned_count = self.outcomes.iter().filter(|outcome| outcome.status == CheckStatus::Warned).count();
        match self.failed_hard_check_names().as_slice() {
            [] => write!(formatter, "integrity: every HARD check passed, {warned_count} WARN check(s) reported"),
            failed_names => write!(formatter, "integrity: HARD check(s) failed: {}", failed_names.join(", ")),
        }
    }
}

pub fn integrity_report(db: &Connection, options: &IntegrityOptions) -> Result<IntegrityReport> {
    let outcomes =
        INTEGRITY_CHECKS.iter().map(|check| outcome_of(check, db, options)).collect::<Result<Vec<CheckOutcome>>>()?;
    Ok(IntegrityReport { outcomes })
}

#[cfg(test)]
mod served_amount_tests {
    use super::*;
    use rusqlite::trace::{TraceEvent, TraceEventCodes};
    use serde_json::json;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn amount_literals_exclude_ranks_dice_thresholds_durations_and_ranges() {
        let excluded = [
            "15d6",
            "+15d6",
            "7d8",
            "DC 25",
            "DC +25",
            "level +5",
            "CR +4",
            "rank +3",
            "tier +2",
            "for 10 seconds",
            "for +10 minutes",
            "+2 hours",
            "+1 round",
            "+2 turns",
            "+3 times",
            "+4 stacks",
            "Dragon's Edge 7",
            "Spell Lore VI",
            "Critical Multiplier (19-20)",
            "+{1}%",
            "{2}",
        ];
        let unexpected = excluded
            .into_iter()
            .filter(|text| amount_tokens(text).iter().any(AmountToken::is_amount_literal))
            .collect::<Vec<_>>();
        assert!(unexpected.is_empty(), "misclassified literals: {unexpected:?}");
        for text in ["+20%", "-6", "20%", "+2.5%", "damage: -6.", "DC 25, +3%", "+3%, +2 stacks"] {
            assert!(amount_tokens(text).iter().any(AmountToken::is_amount_literal), "{text}");
        }
    }

    #[test]
    fn template_consumers_share_placeholder_and_numeric_boundaries() {
        let template = "Spell Lore VI +{1}% (%b1)";
        assert_eq!(effect_identity_numbers(template), ["VI"]);
        assert!(!has_unfilled_effect_token(template));
        assert!(!amount_tokens(template).iter().any(AmountToken::is_amount_literal));
        for template in ["Gain {3}", "Gain %v1", "Gain %i1"] {
            assert!(has_unfilled_effect_token(template), "{template}");
        }
        for template in ["Gain +20%bonus", "15d6", "DC +25", "for +10 seconds", "Dragon's Edge 7", "Spell Lore VI"] {
            assert!(!has_unfilled_effect_token(template), "{template}");
        }
    }

    #[test]
    fn punctuation_signs_preserve_the_following_token() {
        for (text, expected) in [
            ("two-handed", vec!["two", "handed"]),
            ("-%b1", vec!["%b1"]),
            ("+%v1", vec!["%v1"]),
            ("+-6", vec!["-6"]),
            ("--{2}%", vec!["-{2}%"]),
            ("-éclair", vec!["clair"]),
            ("+ -", vec![]),
        ] {
            let tokens = effect_text_tokens(text);
            assert_eq!(tokens.iter().map(|token| token.text).collect::<Vec<_>>(), expected, "{text}");
        }
        assert!(matches!(effect_text_tokens("-%b1")[0].kind, EffectTextTokenKind::BonusType));
        assert!(has_unfilled_effect_token("+%v1"));
    }

    #[test]
    fn dice_terms_keep_counts_beside_signs_and_percent_units() {
        for (text, expected) in [
            ("-1d4", vec![(1, 4)]),
            ("1d6%", vec![(1, 6)]),
            ("+2d8 and -3d10%", vec![(2, 8), (3, 10)]),
            ("1d4 and 2d6", vec![(1, 4), (2, 6)]),
            ("1.5d6", vec![]),
            ("1d6.5", vec![]),
            ("word1d4", vec![]),
            ("", vec![]),
        ] {
            assert_eq!(dice_terms(text), expected, "{text}");
        }
    }

    #[test]
    fn type_words_follow_description_and_line_conventions() {
        for (text, bonus_type, home, description, stated) in [
            ("+4 Insightful bonus", "Insight", None, true, true),
            ("+4 Insight bonus", "Insight", None, true, true),
            ("+4 Profane bonus", "Insight", None, true, false),
            ("a -6 PENALTY", "Penalty", None, true, true),
            ("+2 Damage", "Stacking", None, true, true),
            ("+2 Damage", "Untyped", None, true, true),
            ("+2 Damage", "Equipment", None, false, true),
            ("+2 Damage", "Enhancement", None, false, true),
            ("+2 Damage", "Insight", Some("Insight"), false, true),
            ("+2 Damage", "Insight", Some("Insight"), true, true),
            ("+2 Damage", "Equipment", None, true, true),
            ("+2 Damage", "Enhancement", None, true, true),
            ("insightfully +4", "Insight", None, true, false),
            ("+4 Natural Armor bonus", "Natural Armor", None, true, true),
            ("+4 armor enhancement bonus", "Armor Enhancement", None, true, true),
        ] {
            assert_eq!(
                served_bonus_type_is_stated(
                    text,
                    bonus_type,
                    home,
                    if description { EffectTextKind::Description } else { EffectTextKind::EnchantmentLine }
                ),
                stated,
                "{text}, {bonus_type}"
            );
        }
    }

    fn line_with_bonuses(description: Option<&str>, line: &str, amounts: &[i64], scale: f64) -> serde_json::Value {
        json!({"description": description, "verbose_name": line,
            "bonuses": amounts.iter().map(|amount| json!({"stat": "Doublestrike", "value": amount, "scale": scale})).collect::<Vec<_>>()})
    }

    #[test]
    fn served_amounts_require_every_slot_and_respect_sign_and_unit() {
        let examples = [
            (Some("+4 Enhancement bonus to hit and 6 Damage for Sneak Attacks."), "Deception +4", vec![4, 6], false, true),
            (Some("Passive: +4 Insightful bonus on Charisma based skill checks, imposing a -6 penalty on Hide checks."), "Insightful Command +4", vec![4, -6], false, true),
            (Some("+4 bonus to hit."), "Deception +4", vec![4, 6], false, false),
            (Some("22 points of damage"), "Dragon's Edge 7", vec![22], false, true),
            (Some("-6 penalty"), "Command", vec![-6], false, true),
            (Some("+6 penalty"), "Command", vec![-6], false, false),
            (Some("reduced by 4 SP"), "Efficiency", vec![-4], false, true),
            (Some("3% bonus"), "Doublestrike +3%", vec![3], true, true),
            (Some("3 bonus"), "Doublestrike +3%", vec![3], true, false),
            (Some("3% bonus"), "Doublestrike +3", vec![3], true, false),
            (Some("3 bonus"), "Doublestrike +4", vec![3], false, false),
        ];
        for (description, line, amounts, percent, passes) in examples {
            let percent_stats = if percent { BTreeSet::from(["Doublestrike".to_string()]) } else { BTreeSet::new() };
            let causes = served_amount_findings(&line_with_bonuses(description, line, &amounts, 1.0), &percent_stats);
            assert_eq!(causes.is_empty(), passes, "{description:?}, {line}: {causes:?}");
        }
        let riposte = line_with_bonuses(Some("+10 bonus to AC and to Saves"), "Riposte +10", &[5], 0.5);
        assert!(served_amount_findings(&riposte, &BTreeSet::new())
            .iter()
            .any(|(_, cause)| *cause == "scaled or rounded amount"));
        let missing = line_with_bonuses(None, "Doublestrike", &[3], 1.0);
        assert!(served_amount_findings(&missing, &BTreeSet::new())
            .iter()
            .any(|(_, cause)| *cause == "description missing, and the line short as well"));
        let empty = line_with_bonuses(None, "Flavor +99", &[], 1.0);
        assert!(served_amount_findings(&empty, &BTreeSet::new()).is_empty());
    }

    #[test]
    fn text_checks_use_fixed_query_counts_for_one_and_fifty_effects_and_owners() {
        static STATEMENTS: AtomicUsize = AtomicUsize::new(0);
        fn trace(event: TraceEvent<'_>) {
            if matches!(event, TraceEvent::Stmt(_, _)) {
                STATEMENTS.fetch_add(1, Ordering::SeqCst);
            }
        }
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/ddo-etl/tests/fixtures/data");
        for owner_count in [1, 50] {
            let scratch = tempfile::tempdir().unwrap();
            for relative in [
                "upstream/commit.toml",
                "upstream/source-aliases.toml",
                "effects/bonus-types.toml",
                "effects/stats/doublestrike.toml",
                "effects/stats/doubleshot.toml",
            ] {
                let destination = scratch.path().join(relative);
                std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
                std::fs::copy(source.join(relative), destination).unwrap();
            }
            let sets = scratch.path().join("packs/shared/sets");
            std::fs::create_dir_all(&sets).unwrap();
            let effects = scratch.path().join("effects");
            let group =
                std::fs::read_to_string(source.join("effects/stat-groups/doublestrike-and-doubleshot.toml")).unwrap();
            for index in 0..owner_count {
                std::fs::write(
                    effects.join(format!("amount-rank-{index}.toml")),
                    format!("stat_set_verdict = 'distinct'\nstat_set_reason = 'Each rank has a different default amount'\ndefault_value = {}\n{}\n[[alias]]\nname = 'Amount Alias {index}'\nverbose_name_template = '%b1 Amount Alias {index} +{{1}}%'\n[[citation]]\nurl = 'https://ddowiki.com/page/Doublestrike'\nread = '2026-10-23'\n", index + 1, group.replace("Doublestrike and Doubleshot", &format!("Amount Rank {index}"))),
                )
                .unwrap();
                std::fs::write(sets.join(format!("amount-probe-{index}.toml")), format!(
                    "name = \"Amount Probe {index}\"\n[[tier]]\nequipped_count = 2\n[[tier.effect]]\neffect = \"Amount Rank {index}\"\nvalue = {}\nbonus_type = \"Artifact\"\n[[game_bug_correction]]\nkind = \"set_tier_bonus\"\nfield = \"value\"\nqualifier = \"equipped_count 2, Doublestrike / Artifact\"\nintended = {}\nreason = \"The tooltip states the intended amount.\"\nin_game = true\nread = \"2026-10-23\"\n", index + 1, index + 1
                )).unwrap();
            }
            let mut db = Connection::open_in_memory().unwrap();
            ddo_etl::tree::build_from_data(scratch.path(), &mut db).unwrap();
            db.trace_v2(TraceEventCodes::SQLITE_TRACE_STMT, Some(trace));
            STATEMENTS.store(0, Ordering::SeqCst);
            let findings =
                effect_descriptions_without_their_amounts(&db, &IntegrityOptions { allowed_empty_tables: Vec::new() })
                    .unwrap();
            db.trace_v2(TraceEventCodes::empty(), None);
            assert_eq!(STATEMENTS.load(Ordering::SeqCst), 17, "{owner_count} owners");
            assert!(findings.offenders.unwrap().is_empty());
            db.execute("UPDATE game_bug_corrections SET skip_description_check = 1", []).unwrap();
            db.trace_v2(TraceEventCodes::SQLITE_TRACE_STMT, Some(trace));
            STATEMENTS.store(0, Ordering::SeqCst);
            let findings =
                effect_descriptions_without_their_amounts(&db, &IntegrityOptions { allowed_empty_tables: Vec::new() })
                    .unwrap();
            db.trace_v2(TraceEventCodes::empty(), None);
            assert_eq!(STATEMENTS.load(Ordering::SeqCst), 19, "{owner_count} flagged owners");
            let stale = findings.offenders.unwrap();
            assert_eq!(stale.len(), owner_count);
            assert!(stale.iter().all(|finding| finding.detail.contains("skip flag with nothing to skip")));
            db.trace_v2(TraceEventCodes::SQLITE_TRACE_STMT, Some(trace));
            STATEMENTS.store(0, Ordering::SeqCst);
            let findings = effect_templates_without_their_placeholders(
                &db,
                &IntegrityOptions { allowed_empty_tables: Vec::new() },
            )
            .unwrap();
            db.trace_v2(TraceEventCodes::empty(), None);
            assert_eq!(STATEMENTS.load(Ordering::SeqCst), 4, "{owner_count} effects");
            assert!(findings.offenders.unwrap().is_empty());
            for (name, statement_count) in [
                ("served_effect_lines_repeat_words", 15),
                ("effect_templates_with_digits_without_amounts", 1),
                ("effect_line_rendering_tokens", 2),
                ("effect_templates_disagree_with_names", 1),
                ("effect_template_numbers_match_names", 1),
                ("effect_descriptions_with_fewer_critical_dice_than_hit_dice", 1),
            ] {
                let check = INTEGRITY_CHECKS.iter().find(|check| check.name == name).unwrap();
                let OffenderQuery::Built(query) = &check.offender_query else { panic!("built check") };
                db.trace_v2(TraceEventCodes::SQLITE_TRACE_STMT, Some(trace));
                STATEMENTS.store(0, Ordering::SeqCst);
                let findings = query(&db, &IntegrityOptions { allowed_empty_tables: Vec::new() }).unwrap();
                db.trace_v2(TraceEventCodes::empty(), None);
                assert_eq!(STATEMENTS.load(Ordering::SeqCst), statement_count, "{name}: {owner_count} owners");
                assert!(findings.offenders.unwrap().is_empty(), "{name}");
            }
        }
    }
}
