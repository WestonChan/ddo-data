pub mod augment_slot;
pub mod bonus_type;
pub mod buff;
pub mod drop_location;
pub mod effect;
pub mod effect_map;
pub mod item_version;
pub mod legacy_drop_source;
pub mod material;
pub mod placement;
pub mod source_alias;

pub(crate) fn is_iso_date(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    let [year, month, day] = parts.as_slice() else {
        return false;
    };
    let is_digit_run = |s: &str, length: usize| s.len() == length && s.bytes().all(|b| b.is_ascii_digit());
    if !(is_digit_run(year, 4) && is_digit_run(month, 2) && is_digit_run(day, 2)) {
        return false;
    }
    let (year, month, day): (u32, u32, u32) = (year.parse().unwrap(), month.parse().unwrap(), day.parse().unwrap());
    let is_leap_year = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days_in_month).contains(&day)
}
