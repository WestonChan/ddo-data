use std::ops::Range;

const SEGMENT_SEPARATORS: [char; 2] = [';', '\n'];
const RARE_LOOT_MARKERS: [&str; 3] = ["(rare)", "(rare )", "rare drop"];
const RARE_MONSTER_MARKER: &str = "rare encounter";

pub fn marks_rare_loot(segment: &str) -> bool {
    let segment = segment.to_lowercase();
    RARE_LOOT_MARKERS.iter().any(|m| segment.contains(m)) && !segment.contains(RARE_MONSTER_MARKER)
}

pub fn segment_containing(drop_location: &str, byte_offset: usize) -> &str {
    let start = drop_location[..byte_offset].rfind(SEGMENT_SEPARATORS).map_or(0, |i| i + 1);
    let end = drop_location[byte_offset..].find(SEGMENT_SEPARATORS).map_or(drop_location.len(), |i| byte_offset + i);
    &drop_location[start..end]
}

pub fn chest_following(
    drop_location: &str,
    quest_name_end: usize,
    quest_name_spans: &[Range<usize>],
) -> Option<String> {
    let segment_end =
        drop_location[quest_name_end..].find(SEGMENT_SEPARATORS).map_or(drop_location.len(), |i| quest_name_end + i);
    let next_quest_name_span = quest_name_spans
        .iter()
        .filter(|span| span.start >= quest_name_end && span.start < segment_end)
        .min_by_key(|span| span.start);
    match next_quest_name_span {
        Some(next_span) => chest_label(&drop_location[quest_name_end..next_span.start])
            .or_else(|| chest_following(drop_location, next_span.end, quest_name_spans)),
        None => chest_label(&drop_location[quest_name_end..segment_end]),
    }
}

pub fn chest_label(text_after_quest_name: &str) -> Option<String> {
    let text_without_asides =
        text_without_parentheticals(&text_after_quest_name.to_lowercase()).replace(ESCAPED_AMPERSAND, "&");
    let words: Vec<&str> = text_without_asides.split_whitespace().collect();
    let joined_words = words.join(" ");
    let label = joined_words.trim_start_matches(CHEST_LABEL_LEADING_PUNCTUATION).trim_start();
    let label = RARE_DROP_PREFIXES.iter().find_map(|prefix| label.strip_prefix(prefix)).unwrap_or(label);
    let label = label.trim().trim_end_matches(CHEST_LABEL_TRAILING_PUNCTUATION).trim();
    let label = label.trim_end_matches(['&', ' ']);
    (!label.is_empty() && !LIST_CONNECTORS.contains(&label)).then(|| label.to_string())
}

const CHEST_LABEL_LEADING_PUNCTUATION: [char; 4] = [',', ':', '-', ' '];
const CHEST_LABEL_TRAILING_PUNCTUATION: [char; 3] = ['.', ',', ' '];
const ESCAPED_AMPERSAND: &str = "&amp";
const LIST_CONNECTORS: [&str; 2] = ["and", "or"];
const RARE_DROP_PREFIXES: [&str; 2] = ["rare drop in ", "rare drop "];

fn text_without_parentheticals(text: &str) -> String {
    let mut depth = 0usize;
    let mut kept_text = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '(' => depth += 1,
            ')' if depth > 0 => depth -= 1,
            _ if depth == 0 => kept_text.push(character),
            _ => {}
        }
    }
    kept_text
}
