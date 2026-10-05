use crate::golden_refresh::{golden_age_report, today_day};
use anyhow::{bail, Context, Result};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use ddo_api::{app, AppState};
use http_body_util::BodyExt;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use tower::ServiceExt;

const GOLDEN_LINES: &str = include_str!("../data/tooltip_golden.json");

#[derive(Default)]
struct TooltipComparison {
    enhancement_bonuses_checked: usize,
    matched_fields: usize,
    matched_lines: usize,
    known_lines: usize,
    known_by_reason: BTreeMap<String, usize>,
    unrecorded: Vec<String>,
}

pub fn check_wiki_tooltips(db_path: &Path) -> Result<String> {
    let corpus: Value = serde_json::from_str(GOLDEN_LINES)?;
    let db = Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
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
                compare_lines(owner_name, &owner["lines"], &detail["effects"], &mut comparison)?;
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
        "wiki tooltips: {} lines matched as-is, {} lines with recorded differences, {} matched fields, {} enhancement bonuses checked; known differences: {:?}; {}",
        comparison.matched_lines, comparison.known_lines, comparison.matched_fields,
        comparison.enhancement_bonuses_checked, comparison.known_by_reason, age_report
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
            if golden["known_differences"]["enhancement_bonus"]["reason"].as_str().is_some()
                && golden["known_differences"]["enhancement_bonus"]["served"].as_i64() == actual =>
        {
            let reason = golden["known_differences"]["enhancement_bonus"]["reason"].as_str().unwrap();
            *comparison.known_by_reason.entry(reason.to_string()).or_default() += 1;
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
        compare_field(&line_location, golden, served, "name", &wiki_name, different_line_counts, comparison);
        compare_field(&line_location, golden, served, "verbose_name", wiki_text, different_line_counts, comparison);
        if golden["tooltip_is_standard"] == true {
            let tooltip = golden["wiki_tooltip"].as_str().context("standard tooltip text")?;
            let tooltip = tooltip.split_once(':').map_or(tooltip, |(_, body)| body.trim());
            compare_field(&line_location, golden, served, "description", tooltip, different_line_counts, comparison);
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

fn compare_field(
    line_location: &str,
    golden: &Value,
    served: &Value,
    field: &str,
    expected: &str,
    different_line_counts: bool,
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
            "value_in_wiki_name" => field == "name" && actual.chars().any(|character| character.is_ascii_digit()),
            "set_line_form" => field == "name" && golden["kind"] == "set_tier_line",
            "source_wording"
            | "source_line_style"
            | "wiki_may_be_stale"
            | "nonstandard_tooltip"
            | "value_disagreement" => field == "description" && !actual.is_empty(),
            "folds" => field == "description" && !actual.is_empty(),
            "grouping" => different_line_counts,
            _ => false,
        };
        if !valid_shape {
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
    use super::{collapse_whitespace, compare_enhancement_bonus, compare_field, wiki_label, TooltipComparison};
    use serde_json::json;

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
        compare_field("Probe [line 0]", &golden, &served, "name", "Evocation Focus", false, &mut comparison);
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
}
