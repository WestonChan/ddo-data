use crate::golden_refresh::{golden_age_report, today_day};
use anyhow::{bail, Context, Result};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use ddo_api::{app, AppState};
use http_body_util::BodyExt;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use tower::ServiceExt;

const GOLDEN_LINES: &str = include_str!("../data/tooltip_golden.json");

#[derive(Default)]
struct TooltipComparison {
    enhancement_bonuses_checked: usize,
    stat_template_names_checked: usize,
    matched_fields: usize,
    matched_lines: usize,
    known_lines: usize,
    known_by_reason: BTreeMap<String, usize>,
    unrecorded: Vec<String>,
}

#[derive(Clone, Copy)]
struct GoldenLineContext {
    different_line_counts: bool,
    wiki_owner: bool,
}

#[derive(Default)]
struct StatTemplateName {
    canonical_name: String,
    search_only_aliases: BTreeSet<String>,
}

type OwnerStatTemplateNames = BTreeMap<(String, i64, Option<i64>), BTreeMap<usize, StatTemplateName>>;

fn stat_template_names(db: &Connection) -> Result<OwnerStatTemplateNames> {
    let mut statement = db.prepare(
        "WITH ordered_links AS (
             SELECT 'items' AS collection, 'item' AS owner_kind, item_id AS owner_id, item_id AS bonus_owner_id,
                    NULL AS equipped_count, effect_id, alias, sort_order,
                    ROW_NUMBER() OVER (PARTITION BY item_id ORDER BY sort_order) - 1 AS line_index
               FROM item_effects
             UNION ALL
             SELECT 'augments', 'augment', augment_id, augment_id, NULL, effect_id, alias, sort_order,
                    ROW_NUMBER() OVER (PARTITION BY augment_id ORDER BY sort_order) - 1
               FROM augment_effects
             UNION ALL
             SELECT 'sets', 'set_bonus_tier', tier.set_id, link.tier_id, tier.equipped_count,
                    link.effect_id, link.alias, link.sort_order,
                    ROW_NUMBER() OVER (PARTITION BY link.tier_id ORDER BY link.sort_order) - 1
               FROM set_bonus_tier_effects link JOIN set_bonus_tiers tier ON tier.id = link.tier_id
         )
         SELECT l.collection, l.owner_id, l.equipped_count, l.line_index, e.name, a.name
           FROM ordered_links l JOIN effects e ON e.id = l.effect_id
           LEFT JOIN effect_aliases a ON a.effect_id = e.id AND a.verbose_name_template IS NULL
          WHERE e.is_stat = 1 AND l.alias IS NULL
            AND e.verbose_name_template IS NOT NULL
            AND (l.owner_kind <> 'set_bonus_tier' OR NOT EXISTS (
                 SELECT 1 FROM owner_bonuses ob WHERE ob.owner_kind = l.owner_kind
                  AND ob.owner_id = l.bonus_owner_id AND ob.effect_link_order = l.sort_order AND ob.amount IS NOT NULL))
            AND EXISTS (SELECT 1 FROM corrections c WHERE c.kind = 'effect'
                         AND c.name = e.name AND c.field = 'verbose_name_template')",
    )?;
    let mut names = OwnerStatTemplateNames::new();
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let owner = (row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, Option<i64>>(2)?);
        let line_index = usize::try_from(row.get::<_, i64>(3)?)?;
        let name = names.entry(owner).or_default().entry(line_index).or_default();
        name.canonical_name = row.get(4)?;
        if let Some(alias) = row.get::<_, Option<String>>(5)? {
            name.search_only_aliases.insert(alias);
        }
    }
    Ok(names)
}

pub fn check_wiki_tooltips(db_path: &Path) -> Result<String> {
    let corpus: Value = serde_json::from_str(GOLDEN_LINES)?;
    let db = Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let template_names = stat_template_names(&db)?;
    let state = AppState::open(db_path)?;
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let mut comparison = TooltipComparison::default();
    let mut reread_commands = std::collections::BTreeSet::new();
    for (collection, table, name_key, route) in [
        ("items", "items", "item", "items"),
        ("augments", "augments", "augment", "augments"),
        ("sets", "set_bonuses", "set", "sets"),
    ] {
        for owner in corpus[collection].as_array().context("golden owner list")? {
            let owner_name = owner[name_key].as_str().context("golden owner name")?;
            let mismatches_before = comparison.unrecorded.len();
            let owner_id: i64 = db
                .query_row(&format!("SELECT id FROM {table} WHERE name = ?1"), [owner_name], |row| row.get(0))
                .with_context(|| format!("golden {collection} owner {owner_name:?}"))?;
            let wiki_owner = collection != "sets"
                && db.query_row(
                    &format!("SELECT provenance = 'wiki' FROM {table} WHERE id = ?1"),
                    [owner_id],
                    |row| row.get::<_, bool>(0),
                )?;
            let detail = runtime.block_on(served_detail(&state, &format!("/v1/{route}/{owner_id}")))?;
            if collection == "sets" {
                for tier in owner["tiers"].as_array().context("golden set tiers")? {
                    let pieces = tier["pieces"].as_i64().context("golden equipped count")?;
                    let served_tier = detail["tiers"]
                        .as_array()
                        .context("served set tiers")?
                        .iter()
                        .find(|row| row["equipped_count"] == pieces)
                        .with_context(|| format!("{owner_name} {pieces}-piece tier"))?;
                    compare_lines(
                        &format!("{owner_name} ({pieces} pieces)"),
                        &tier["lines"],
                        &served_tier["effects"],
                        &mut comparison,
                        wiki_owner,
                        template_names.get(&(collection.to_string(), owner_id, Some(pieces))),
                    )?;
                }
            } else {
                if collection == "items" {
                    for golden in owner["lines"].as_array().context("golden item lines")? {
                        if golden["kind"] == "enhancement_bonus" {
                            compare_enhancement_bonus(owner_name, golden, &detail, &mut comparison);
                        }
                    }
                }
                compare_lines(
                    owner_name,
                    &owner["lines"],
                    &detail["effects"],
                    &mut comparison,
                    wiki_owner,
                    template_names.get(&(collection.to_string(), owner_id, None)),
                )?;
            }
            if comparison.unrecorded.len() > mismatches_before {
                let selector = match collection {
                    "items" => "item",
                    "augments" => "augment",
                    _ => "set",
                };
                reread_commands.insert(format!(
                    "cargo xtask golden-sample --db {} --{selector} {:?}",
                    db_path.display(),
                    owner_name
                ));
            }
        }
    }
    let age_report = golden_age_report(&corpus, today_day()?)?;
    if !comparison.unrecorded.is_empty() {
        bail!(
            "{} unrecorded wiki tooltip mismatch(es):\n{}\nRe-read the affected owner in a browser; start with:\n{}\n{}",
            comparison.unrecorded.len(),
            comparison.unrecorded.join("\n"),
            reread_commands.into_iter().collect::<Vec<_>>().join("\n"),
            age_report
        );
    }
    Ok(format!(
        "wiki tooltips: {} lines matched as-is, {} lines with recorded differences, {} matched fields, {} enhancement bonuses checked, {} stat-template names checked; known differences: {:?}; {}",
        comparison.matched_lines, comparison.known_lines, comparison.matched_fields,
        comparison.enhancement_bonuses_checked, comparison.stat_template_names_checked, comparison.known_by_reason, age_report
    ))
}

fn compare_enhancement_bonus(owner_name: &str, golden: &Value, detail: &Value, comparison: &mut TooltipComparison) {
    comparison.enhancement_bonuses_checked += 1;
    let expected = golden["wiki_text"]
        .as_str()
        .and_then(|line| line.strip_suffix(" Enhancement Bonus"))
        .and_then(|amount| amount.parse::<i64>().ok());
    let actual = detail["enhancement_bonus"].as_i64();
    match (expected, actual) {
        (Some(expected), Some(actual)) if expected == actual => {
            if golden["known_differences"].get("enhancement_bonus").is_some() {
                comparison
                    .unrecorded
                    .push(format!("{owner_name}: recorded enhancement bonus difference now matches the wiki"));
            }
        }
        (Some(_), _)
            if golden["known_differences"]["enhancement_bonus"]["code"].as_str() == Some("value_disagreement")
                && golden["known_differences"]["enhancement_bonus"]["served"].as_i64() == actual =>
        {
            *comparison.known_by_reason.entry("value_disagreement".to_string()).or_default() += 1;
        }
        _ => comparison.unrecorded.push(format!(
            "{owner_name}: wiki enhancement bonus {expected:?}, served item enhancement_bonus {actual:?}"
        )),
    }
}

async fn served_detail(state: &AppState, path: &str) -> Result<Value> {
    let response = app(state.clone()).oneshot(Request::get(path).body(Body::empty())?).await?;
    if response.status() != StatusCode::OK {
        bail!("GET {path} returned {}", response.status());
    }
    Ok(serde_json::from_slice(&response.into_body().collect().await?.to_bytes())?)
}

fn compare_lines(
    owner_name: &str,
    golden_lines: &Value,
    served_lines: &Value,
    comparison: &mut TooltipComparison,
    wiki_owner: bool,
    template_names: Option<&BTreeMap<usize, StatTemplateName>>,
) -> Result<()> {
    let served_lines = served_lines.as_array().context("served effect lines")?;
    let golden_lines = golden_lines.as_array().context("golden effect lines")?;
    let matched_golden_count = golden_lines.iter().filter(|line| line["ours_line"].is_object()).count();
    let different_line_counts = matched_golden_count != served_lines.len();
    for golden in golden_lines {
        if golden["tooltip_is_standard"].is_string() {
            bail!("{owner_name}: tooltip_is_standard must be a boolean, not {:?}", golden["tooltip_is_standard"]);
        }
        if !matches!(golden["kind"].as_str(), Some("enchantment" | "set_tier_line"))
            || golden["wiki_text"].is_null()
            || golden["ours_line"].is_null()
        {
            continue;
        }
        let line_index = golden["ours_line"]["index"].as_u64().context("golden line index")? as usize;
        let Some(served) = served_lines.get(line_index) else {
            comparison.unrecorded.push(format!("{owner_name}: line {line_index} is absent"));
            continue;
        };
        let wiki_text = golden["wiki_text"].as_str().context("golden wiki text")?;
        let matched_before = comparison.matched_fields;
        let unrecorded_before = comparison.unrecorded.len();
        let wiki_name =
            golden["wiki_name"].as_str().map(str::to_string).unwrap_or_else(|| wiki_label(wiki_text, golden));
        let line_location = format!("{owner_name} [line {line_index}]");
        let context = GoldenLineContext { different_line_counts, wiki_owner };
        if let Some(template_name) = template_names.and_then(|names| names.get(&line_index)) {
            compare_stat_template_name(&line_location, golden, served, &wiki_name, template_name, comparison);
        } else {
            compare_field(&line_location, golden, served, "name", &wiki_name, context, comparison);
        }
        compare_field(&line_location, golden, served, "verbose_name", wiki_text, context, comparison);
        if golden["tooltip_is_standard"] == true {
            let tooltip = golden["wiki_tooltip"].as_str().context("standard tooltip text")?;
            let tooltip = tooltip.split_once(':').map_or(tooltip, |(_, body)| body.trim());
            compare_field(&line_location, golden, served, "description", tooltip, context, comparison);
        }
        if comparison.unrecorded.len() == unrecorded_before {
            if comparison.matched_fields - matched_before == if golden["tooltip_is_standard"] == true { 3 } else { 2 } {
                comparison.matched_lines += 1;
            } else {
                comparison.known_lines += 1;
            }
        }
    }
    Ok(())
}

fn compare_stat_template_name(
    line_location: &str,
    golden: &Value,
    served: &Value,
    wiki_name: &str,
    template_name: &StatTemplateName,
    comparison: &mut TooltipComparison,
) {
    comparison.stat_template_names_checked += 1;
    let canonical_name = &template_name.canonical_name;
    let actual = served["name"].as_str().unwrap_or("");
    if actual != canonical_name {
        comparison.unrecorded.push(format!(
            "{line_location}: {canonical_name:?} stat template requires canonical name, served {actual:?}"
        ));
    } else if !template_name.search_only_aliases.contains(wiki_name) {
        comparison.unrecorded.push(format!(
            "{line_location}: {canonical_name:?} has no search-only alias for tooltip label {wiki_name:?}"
        ));
    } else if golden["known_differences"].get("name").is_some() {
        comparison
            .unrecorded
            .push(format!("{line_location}: {canonical_name:?} stat template name cannot have a recorded difference"));
    } else {
        comparison.matched_fields += 1;
    }
}

fn compare_field(
    line_location: &str,
    golden: &Value,
    served: &Value,
    field: &str,
    expected: &str,
    context: GoldenLineContext,
    comparison: &mut TooltipComparison,
) {
    let actual = served[field].as_str().unwrap_or("");
    if collapse_whitespace(actual) == collapse_whitespace(expected) {
        comparison.matched_fields += 1;
        if golden["known_differences"].get(field).is_some() {
            comparison.unrecorded.push(format!(
                "{line_location}: {:?} {field}: recorded difference now matches the wiki",
                golden["ours"]
            ));
        }
    } else if let (Some(code), Some(recorded)) =
        (golden["known_differences"][field]["code"].as_str(), golden["known_differences"][field]["served"].as_str())
    {
        let valid_shape = match code {
            "value_in_wiki_name" => {
                context.wiki_owner && field == "name" && actual.chars().any(|character| character.is_ascii_digit())
            }
            "value_in_source_name" => {
                !context.wiki_owner && field == "name" && actual.chars().any(|character| character.is_ascii_digit())
            }
            "set_line_form" => field == "name" && golden["kind"] == "set_tier_line",
            "source_wording" | "source_line_style" | "nonstandard_tooltip" | "value_disagreement" => {
                field == "description" && !actual.is_empty()
            }
            "folds" => field == "description" && !actual.is_empty(),
            "grouping" => context.different_line_counts,
            _ => false,
        };
        if !valid_shape
            || (field == "description"
                && code != "value_disagreement"
                && numeric_tokens(actual) != numeric_tokens(expected))
        {
            comparison.unrecorded.push(format!("{line_location}: invalid {code:?} reason for {field}"));
        } else if collapse_whitespace(actual) == collapse_whitespace(recorded) {
            *comparison.known_by_reason.entry(code.to_string()).or_default() += 1;
        } else {
            comparison.unrecorded.push(format!(
                "{line_location}: {:?} {field}: known difference changed from {:?} to {:?}",
                golden["ours"], recorded, actual
            ));
        }
    } else {
        comparison.unrecorded.push(format!(
            "{line_location}: {:?} {field}: expected {:?}, served {:?}",
            golden["ours"],
            collapse_whitespace(expected),
            collapse_whitespace(actual)
        ));
    }
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn numeric_tokens(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    tokens.extend(
        text.split(|character: char| !character.is_ascii_alphabetic())
            .filter(|word| word.eq_ignore_ascii_case("one"))
            .map(|_| "1".to_string()),
    );
    let mut index = 0;
    while index < bytes.len() {
        if !bytes[index].is_ascii_digit() {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index + 1 < bytes.len() && bytes[index] == b'd' && bytes[index + 1].is_ascii_digit() {
            let dice_number: u64 = text[start..index].parse().unwrap_or(0);
            index += 1;
            let sides_start = index;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
            let dice_sides: u64 = text[sides_start..index].parse().unwrap_or(0);
            tokens.push(format!("{}-{}", dice_number, dice_number.saturating_mul(dice_sides)));
            continue;
        }
        let range_start = index;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index + 1 < bytes.len() && &bytes[index..index + 2] == b"to" {
            index += 2;
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            if index < bytes.len() && bytes[index].is_ascii_digit() {
                let range_end_start = index;
                while index < bytes.len() && bytes[index].is_ascii_digit() {
                    index += 1;
                }
                tokens.push(format!("{}-{}", &text[start..range_start], &text[range_end_start..index]));
                continue;
            }
        }
        index = range_start;
        if index < bytes.len() && bytes[index] == b'%' {
            index += 1;
        }
        tokens.push(text[start..index].to_string());
    }
    tokens.sort();
    tokens
}

fn wiki_label(wiki_text: &str, golden: &Value) -> String {
    let mut words = wiki_text.split_whitespace().collect::<Vec<_>>();
    if words
        .first()
        .is_some_and(|word| word.starts_with('+') && word.chars().any(|character| character.is_ascii_digit()))
    {
        words.remove(0);
    }
    if words.last().is_some_and(|word| word.trim_start_matches(['+', '-']).trim_end_matches('%').parse::<i64>().is_ok())
    {
        words.pop();
    }
    let mut label = words.join(" ");
    let type_name = golden["ours_line"]["type"].as_str().unwrap_or("");
    let adjective = if type_name == "Insight" { "Insightful" } else { type_name };
    if !adjective.is_empty() && !golden["ours"].as_str().unwrap_or("").starts_with(adjective) {
        if let Some(stripped) = label.strip_prefix(&format!("{adjective} ")) {
            label = stripped.to_string();
        }
    }
    label
}

#[cfg(test)]
mod tests {
    use super::{
        collapse_whitespace, compare_enhancement_bonus, compare_field, compare_lines, numeric_tokens,
        stat_template_names, wiki_label, GoldenLineContext, TooltipComparison,
    };
    use serde_json::json;

    fn golden_line_comparison(
        owner_name: &str,
        verbose_name: &str,
        wiki_name: &str,
        removed_alias: Option<&str>,
    ) -> TooltipComparison {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../data");
        ddo_etl::tree::build_from_data(&root, &mut db).unwrap();
        if let Some(alias) = removed_alias {
            db.execute("DELETE FROM effect_aliases WHERE name = ?1", [alias]).unwrap();
        }
        let owner_id: i64 =
            db.query_row("SELECT id FROM items WHERE name = ?1", [owner_name], |row| row.get(0)).unwrap();
        let mut owners = ddo_api::served_effect_lines(&db, "item_effects", "item_id").unwrap();
        let lines = owners.remove(&owner_id).unwrap().into_iter().map(|(_, line)| line).collect::<Vec<_>>();
        let line_index = lines.iter().position(|line| line["verbose_name"] == verbose_name).unwrap();
        let golden = json!([{"kind": "enchantment", "wiki_name": wiki_name, "wiki_text": verbose_name, "ours": lines[line_index]["name"], "ours_line": {"index": line_index}}]);
        let mut comparison = TooltipComparison::default();
        let names = stat_template_names(&db).unwrap();
        compare_lines(
            owner_name,
            &golden,
            &json!(lines),
            &mut comparison,
            false,
            names.get(&("items".to_string(), owner_id, None)),
        )
        .unwrap();
        comparison
    }

    #[test]
    fn stat_template_line_checks_the_canonical_name_and_search_only_alias() {
        let comparison = golden_line_comparison("Epic Wrath of Siberys", "Reflex +10", "Reflex", None);
        assert!(comparison.unrecorded.is_empty(), "{:?}", comparison.unrecorded);
        assert_eq!(comparison.matched_lines, 1);
        assert_eq!(comparison.stat_template_names_checked, 1);
    }

    #[test]
    fn stat_template_line_rejects_a_tooltip_label_without_a_search_only_alias() {
        let comparison = golden_line_comparison("Epic Wrath of Siberys", "Reflex +10", "Reflex", Some("Reflex"));
        assert_eq!(comparison.unrecorded.len(), 1);
        assert!(comparison.unrecorded[0].contains("Reflex Save"));
        assert!(comparison.unrecorded[0].contains("search-only alias"), "{:?}", comparison.unrecorded);
    }

    #[test]
    fn non_template_line_keeps_exact_tooltip_name_matching() {
        let comparison =
            golden_line_comparison("Legendary Flightfoot Greaves", "Natural Armor Bonus +13", "Natural Armor", None);
        assert_eq!(comparison.unrecorded.len(), 1);
        assert_eq!(comparison.stat_template_names_checked, 0);
        assert!(comparison.unrecorded[0].contains("expected \"Natural Armor\", served \"Natural Armor Bonus\""));
    }

    #[test]
    fn wiki_labels_remove_the_rendered_amount_and_secondary_type() {
        assert_eq!(
            wiki_label("Insightful Combustion +71", &json!({"ours": "Combustion", "ours_line": {"type": "Insight"}})),
            "Combustion"
        );
        assert_eq!(
            wiki_label("+15 Orb Bonus", &json!({"ours": "Orb Bonus", "ours_line": {"type": "Orb"}})),
            "Orb Bonus"
        );
        assert_eq!(collapse_whitespace("  a\n b  "), "a b");
        assert_eq!(
            wiki_label("DR 30/Adamantine", &json!({"ours": "DR", "ours_line": {"type": null}})),
            "DR 30/Adamantine"
        );
    }

    #[test]
    fn known_differences_are_removed_when_the_served_field_matches_wiki() {
        let golden = json!({
            "known_differences": {"name": {"reason": "Old model", "served": "Evocation Spell Focus"}}
        });
        let served = json!({"name": "Evocation Focus"});
        let mut comparison = TooltipComparison::default();
        compare_field(
            "Probe [line 0]",
            &golden,
            &served,
            "name",
            "Evocation Focus",
            GoldenLineContext { different_line_counts: false, wiki_owner: false },
            &mut comparison,
        );
        assert_eq!(comparison.unrecorded.len(), 1);
    }

    #[test]
    fn enhancement_bonus_golden_lines_check_the_item_field() {
        let golden = json!({"kind": "enhancement_bonus", "wiki_text": "+15 Enhancement Bonus"});
        let mut comparison = TooltipComparison::default();
        compare_enhancement_bonus("Golden sword", &golden, &json!({"enhancement_bonus": 15}), &mut comparison);
        assert_eq!(comparison.enhancement_bonuses_checked, 1);
        assert!(comparison.unrecorded.is_empty());
        compare_enhancement_bonus("Golden sword", &golden, &json!({"enhancement_bonus": 14}), &mut comparison);
        assert_eq!(comparison.unrecorded.len(), 1);
    }

    #[test]
    fn recorded_wording_cannot_hide_a_dice_or_amount_difference() {
        let golden = json!({"known_differences": {"description": {
            "code": "source_wording", "served": "Deals 14d6 Chaos damage."
        }}});
        let served = json!({"description": "Deals 14d6 Chaos damage."});
        let mut comparison = TooltipComparison::default();
        compare_field(
            "Probe",
            &golden,
            &served,
            "description",
            "Deals 16d6 Chaos damage.",
            GoldenLineContext { different_line_counts: false, wiki_owner: false },
            &mut comparison,
        );
        assert_eq!(comparison.unrecorded.len(), 1);
        assert_eq!(numeric_tokens("7 to 28"), vec!["7-28"]);
        assert_eq!(numeric_tokens("7d4"), numeric_tokens("7 to 28"));
        assert_ne!(numeric_tokens("7d6"), numeric_tokens("7 to 28"));
    }

    #[test]
    fn value_in_wiki_name_requires_wiki_provenance() {
        let golden = json!({"known_differences": {"name": {
            "code": "value_in_wiki_name", "served": "Fiery 7"
        }}});
        let served = json!({"name": "Fiery 7"});
        let mut comparison = TooltipComparison::default();
        compare_field(
            "Probe",
            &golden,
            &served,
            "name",
            "Fiery",
            GoldenLineContext { different_line_counts: false, wiki_owner: false },
            &mut comparison,
        );
        assert_eq!(comparison.unrecorded.len(), 1);
    }
}
