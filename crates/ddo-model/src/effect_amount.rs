pub fn line_template_states_percent(template: &str, amount_slot: i64) -> bool {
    matches!(amount_slot, 1 | 2) && template.contains(&format!("{{{amount_slot}}}%"))
}

pub fn rounded_amount_sql(amount: &str, scale: &str, rounding: &str) -> String {
    let scaled_amount = format!("(({amount}) * ({scale}))");
    let truncated_amount = format!("CAST({scaled_amount} AS INTEGER)");
    format!(
        "CASE {rounding} WHEN 'down' THEN {truncated_amount} - ({scaled_amount} < {truncated_amount})
         WHEN 'up' THEN {truncated_amount} + ({scaled_amount} > {truncated_amount})
         ELSE CAST(round({scaled_amount}) AS INTEGER) END"
    )
}

#[cfg(test)]
mod tests {
    use super::line_template_states_percent;

    #[test]
    fn percent_units_belong_to_their_amount_slots() {
        for (template, slot, expected) in [
            ("+{1}%", 1, true),
            ("+{1}%", 2, false),
            ("+{1} and +{2}%", 1, false),
            ("+{1} and +{2}%", 2, true),
            ("+{1}% and +{2}%", 2, true),
            ("+20%", 1, false),
            ("+{3}%", 3, false),
            ("+{0}%", 0, false),
            ("", 1, false),
        ] {
            assert_eq!(line_template_states_percent(template, slot), expected, "{template}, slot {slot}");
        }
    }
}
