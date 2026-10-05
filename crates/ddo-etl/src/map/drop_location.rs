use ddo_model::enums::SagaTier;
use std::ops::Range;

const SEGMENT_SEPARATORS: [char; 2] = [';', '\n'];
const RARE_LOOT_MARKERS: [&str; 3] = ["(rare)", "(rare )", "rare drop"];
const RARE_MONSTER_MARKER: &str = "rare encounter";

pub fn marks_rare_loot(segment: &str) -> bool {
    let segment = segment.to_lowercase();
    RARE_LOOT_MARKERS.iter().any(|m| segment.contains(m)) && !segment.contains(RARE_MONSTER_MARKER)
}

pub fn names_quest_end_reward(segment: &str) -> bool {
    let segment = segment.to_lowercase();
    segment.contains(REWARD_MARKER) && !OTHER_GIVER_REWARD_MARKERS.iter().any(|m| segment.contains(m))
}

pub fn names_chest_drop(segment: &str) -> bool {
    !names_quest_end_reward(segment) || segment.to_lowercase().contains(CHEST_MARKER)
}

pub fn names_store_purchase(segment: &str) -> bool {
    let lowercase_segment = segment.to_lowercase();
    STORE_PURCHASE_MARKERS.iter().any(|marker| lowercase_segment.contains(marker))
}

pub fn starts_with_saga_tier_aside(text_after_name: &str) -> bool {
    text_after_name
        .trim_start()
        .strip_prefix('(')
        .and_then(|aside| aside.split_once(')'))
        .is_some_and(|(aside, _)| SagaTier::ALL.iter().any(|tier| aside.trim().eq_ignore_ascii_case(tier.as_str())))
}

pub fn names_saga(segment: &str) -> bool {
    let lowercase_segment = segment.to_lowercase();
    lowercase_segment.contains(SAGA_REWARD_LIST_MARKER)
        || lowercase_segment.split(|character: char| !character.is_alphanumeric()).any(|word| word == SAGA_WORD)
}

pub fn reward_giver_name(segment: &str) -> Option<String> {
    let lowercase_segment = segment.to_ascii_lowercase();
    if !lowercase_segment.contains(REWARD_MARKER) {
        return None;
    }
    let name_end = REWARD_GIVER_NAME_ENDS
        .iter()
        .filter_map(|name_end_marker| lowercase_segment.find(name_end_marker))
        .min()
        .unwrap_or(segment.len());
    let name = segment[..name_end].trim();
    let name = REWARD_GIVER_LEADING_CONNECTORS
        .iter()
        .find_map(|connector| {
            name.get(..connector.len())
                .filter(|start| start.eq_ignore_ascii_case(connector))
                .map(|_| &name[connector.len()..])
        })
        .unwrap_or(name)
        .trim();
    (!name.is_empty()).then(|| name.to_string())
}

pub fn saga_tier_credited_to(segment: &str, saga_name: &str) -> Option<SagaTier> {
    let last_mention_start = quest_name_spans(segment, saga_name).last().map_or(0, |span| span.start);
    segment[last_mention_start..]
        .to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .find_map(|word| SagaTier::ALL.iter().copied().find(|tier| tier.as_str() == word))
}

pub fn segment_head(segment: &str) -> &str {
    segment.split([',', '(', ':']).next().unwrap_or(segment).trim()
}

pub fn segment_ranges(drop_text: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    for (separator_index, _) in drop_text.match_indices(SEGMENT_SEPARATORS) {
        ranges.push(start..separator_index);
        start = separator_index + 1;
    }
    ranges.push(start..drop_text.len());
    ranges
}

pub fn prerequisite_spans(drop_text: &str) -> Vec<Range<usize>> {
    let lowercase = drop_text.to_ascii_lowercase();
    let mut spans = Vec::new();
    let mut from = 0;
    while let Some(relative_start) = lowercase[from..].find("(requires") {
        let start = from + relative_start;
        let end = lowercase[start..].find(')').map_or(drop_text.len(), |relative_end| start + relative_end + 1);
        spans.push(start..end);
        from = end;
    }
    let mut from = 0;
    while let Some(relative_start) = lowercase[from..].find("requires completion of") {
        let start = from + relative_start;
        if !spans.iter().any(|span| span.start <= start && start < span.end) {
            let end =
                lowercase[start..].find([')', ';', '\n']).map_or(drop_text.len(), |relative_end| start + relative_end);
            spans.push(start..end);
        }
        from = start + "requires completion of".len();
    }
    spans
}

const SAGA_WORD: &str = "saga";
const SAGA_REWARD_LIST_MARKER: &str = "true elite";
const STORE_PURCHASE_MARKERS: [&str; 4] = ["ddo store", "collector's edition", "fan bundle", "bonus items pack"];
const REWARD_GIVER_NAME_ENDS: [&str; 6] = [",", "(", " saga", " quest chain", " chain end", " end reward"];
const REWARD_GIVER_LEADING_CONNECTORS: [&str; 3] = ["or ", "and ", "also "];
const REWARD_MARKER: &str = "reward";
const CHEST_MARKER: &str = "chest";
const OTHER_GIVER_REWARD_MARKERS: [&str; 2] = ["chain", "saga"];

pub fn segment_spanning<'text>(drop_location: &'text str, quest_name_span: &Range<usize>) -> &'text str {
    let start = drop_location[..quest_name_span.start].rfind(SEGMENT_SEPARATORS).map_or(0, |i| i + 1);
    let end = drop_location[quest_name_span.end..]
        .find(SEGMENT_SEPARATORS)
        .map_or(drop_location.len(), |i| quest_name_span.end + i);
    &drop_location[start..end]
}

pub fn quest_name_spans(drop_text: &str, quest_name: &str) -> Vec<Range<usize>> {
    let Some(first_name_character) = quest_name.chars().next() else {
        return Vec::new();
    };
    let mut spans: Vec<Range<usize>> = Vec::new();
    for (start, text_character) in drop_text.char_indices() {
        let is_inside_previous_span = spans.last().is_some_and(|span| start < span.end);
        if is_inside_previous_span || !characters_match_ignoring_case(text_character, first_name_character) {
            continue;
        }
        let matched_end = quest_name_end(drop_text, start, quest_name).filter(|end| {
            drop_text[start..*end] == *quest_name
                || (text_character == first_name_character && is_whole_words(drop_text, start..*end))
        });
        if let Some(end) = matched_end {
            spans.push(start..end);
        }
    }
    spans
}

fn quest_name_end(drop_text: &str, start: usize, quest_name: &str) -> Option<usize> {
    let mut text_characters = drop_text[start..].char_indices().peekable();
    let mut name_characters = quest_name.chars().peekable();
    while let Some(name_character) = name_characters.next() {
        if name_character.is_whitespace() {
            while name_characters.next_if(|character| character.is_whitespace()).is_some() {}
            text_characters.next_if(|(_, character)| character.is_whitespace())?;
            while text_characters.next_if(|(_, character)| character.is_whitespace()).is_some() {}
            continue;
        }
        let (_, text_character) = text_characters.next()?;
        if !characters_match_ignoring_case(text_character, name_character) {
            return None;
        }
    }
    Some(text_characters.peek().map_or(drop_text.len(), |(offset, _)| start + offset))
}

fn characters_match_ignoring_case(text_character: char, name_character: char) -> bool {
    text_character == name_character || text_character.to_lowercase().eq(name_character.to_lowercase())
}

fn is_whole_words(text: &str, span: Range<usize>) -> bool {
    let is_word_character = |character: char| character.is_alphanumeric();
    !text[..span.start].chars().next_back().is_some_and(is_word_character)
        && !text[span.end..].chars().next().is_some_and(is_word_character)
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
    let label = label_without_reward_clauses(label);
    let label = label_without_list_connectors_at_its_ends(label.trim_end_matches(CHEST_LABEL_TRAILING_PUNCTUATION));
    (!label.is_empty()).then(|| label.to_string())
}

fn label_without_reward_clauses(label: &str) -> String {
    let mut kept_label = String::with_capacity(label.len());
    let mut separator_before_clause = "";
    let mut text_left = label;
    loop {
        let next_separator = CLAUSE_SEPARATORS
            .iter()
            .filter_map(|separator| text_left.find(separator).map(|start| (start, *separator)))
            .min_by_key(|(start, _)| *start);
        let (clause, separator_after_clause) = match next_separator {
            Some((start, separator)) => (&text_left[..start], separator),
            None => (text_left, ""),
        };
        if !names_reward(clause) {
            if !kept_label.is_empty() {
                kept_label.push_str(separator_before_clause);
            }
            kept_label.push_str(clause);
        }
        if separator_after_clause.is_empty() {
            return kept_label;
        }
        text_left = &text_left[clause.len() + separator_after_clause.len()..];
        separator_before_clause = separator_after_clause;
    }
}

fn names_reward(clause: &str) -> bool {
    clause.split(|character: char| !character.is_alphanumeric()).any(|word| REWARD_WORDS.contains(&word))
}

const CLAUSE_SEPARATORS: [&str; 3] = [",", " and ", " or "];
const REWARD_WORDS: [&str; 2] = ["reward", "rewards"];

fn label_without_list_connectors_at_its_ends(label: &str) -> &str {
    let mut label = label.trim_matches(LIST_SEPARATOR_CHARACTERS);
    loop {
        let label_without_a_connector = LIST_CONNECTORS.iter().find_map(|connector| {
            label
                .strip_suffix(connector)
                .filter(|text_before| text_before.is_empty() || text_before.ends_with(LIST_SEPARATOR_CHARACTERS))
                .or_else(|| label.strip_prefix(connector).filter(|text_after| text_after.starts_with(' ')))
        });
        match label_without_a_connector {
            Some(shorter_label) => label = shorter_label.trim_matches(LIST_SEPARATOR_CHARACTERS),
            None => return label,
        }
    }
}

const CHEST_LABEL_LEADING_PUNCTUATION: [char; 4] = [',', ':', '-', ' '];
const CHEST_LABEL_TRAILING_PUNCTUATION: [char; 3] = ['.', ',', ' '];
const ESCAPED_AMPERSAND: &str = "&amp";
const LIST_CONNECTORS: [&str; 2] = ["and", "or"];
const LIST_SEPARATOR_CHARACTERS: [char; 3] = [',', ' ', '&'];
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

const DROP_TEXT_MARKER: &str = "Drops in";
const DROP_LOCATION_MARKER: &str = "Drop Location:";

pub fn drop_text_in_description(description: &str) -> Option<&str> {
    let Some((_, text_after_marker)) = description.split_once(DROP_TEXT_MARKER) else {
        let (_, text_after_drop_location_marker) = description.split_once(DROP_LOCATION_MARKER)?;
        return Some(text_after_drop_location_marker.trim());
    };
    let text_after_update = text_after_marker
        .trim_start()
        .strip_prefix('U')
        .filter(|text| text.starts_with(|character: char| character.is_ascii_digit()))
        .map_or(text_after_marker, |text| text.trim_start_matches(|character: char| character.is_ascii_digit()));
    let text_after_quest_word = text_after_update.trim_start().strip_prefix("Quest").unwrap_or(text_after_update);
    let drop_text = text_after_quest_word.trim_start().trim_start_matches([':', ';']).trim_start();
    Some(drop_text.strip_prefix(DROP_LOCATION_MARKER).unwrap_or(drop_text).trim())
}

#[cfg(test)]
mod prerequisite_tests {
    use super::prerequisite_spans;

    #[test]
    fn prerequisite_spans_cover_only_the_prerequisite_in_real_drop_text() {
        let text = "Stealing from Sorcere, Romira and Juvian 's chest (requires completion of Sleeping with the Fishes and visits to The Cerulean Hills and Ruins of Myth Drannor )";
        let spans = prerequisite_spans(text);
        assert_eq!(spans.len(), 1);
        assert_eq!(&text[spans[0].clone()], "(requires completion of Sleeping with the Fishes and visits to The Cerulean Hills and Ruins of Myth Drannor )");
        let unparenthesized =
            "Stealing from Sorcere requires completion of Sleeping with the Fishes; Flocked Together chest";
        let spans = prerequisite_spans(unparenthesized);
        assert_eq!(&unparenthesized[spans[0].clone()], "requires completion of Sleeping with the Fishes");
        let multiline = "Romira and Juvian's right chest (rare, requires completion of Sleeping with the Fishes)\nThe Wizard's Labyrinth, end chest (rare)\nToil and Trouble, end chest (rare)";
        let spans = prerequisite_spans(multiline);
        assert_eq!(&multiline[spans[0].clone()], "requires completion of Sleeping with the Fishes");
        assert!(!spans.iter().any(|span| span.end > multiline.find('\n').unwrap()));
    }
}
