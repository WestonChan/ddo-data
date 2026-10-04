pub fn rounded_amount_sql(amount: &str, scale: &str, rounding: &str) -> String {
    let scaled_amount = format!("(({amount}) * ({scale}))");
    let truncated_amount = format!("CAST({scaled_amount} AS INTEGER)");
    format!(
        "CASE {rounding} WHEN 'down' THEN {truncated_amount} - ({scaled_amount} < {truncated_amount})
         WHEN 'up' THEN {truncated_amount} + ({scaled_amount} > {truncated_amount})
         ELSE CAST(round({scaled_amount}) AS INTEGER) END"
    )
}
