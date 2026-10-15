pub(crate) fn recipe_focus_path(note: &str) -> Option<String> {
    let word_after = |label: &str| {
        note.split_once(label)
            .and_then(|(_, remainder)| remainder.split(|character: char| !character.is_ascii_alphabetic()).next())
            .filter(|word| !word.is_empty())
    };
    if let (Some(invasion), Some(subjugation)) = (word_after("Invasion focus of "), word_after("Subjugation focus of "))
    {
        return Some(format!("Invasion {invasion} / Subjugation {subjugation}"));
    }
    if let (Some(focus), Some(gem), Some(essence)) =
        (word_after("focus type "), word_after("gem type "), word_after("essence type "))
    {
        return Some(format!("{focus}:{gem}:{essence}"));
    }
    note.split_once("(page code ")
        .and_then(|(_, remainder)| remainder.split_once(')'))
        .map(|(page_code, _)| page_code.to_string())
}
