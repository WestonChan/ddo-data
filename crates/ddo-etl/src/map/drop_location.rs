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
