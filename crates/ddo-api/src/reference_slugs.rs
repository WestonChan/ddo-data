use serde_json::Value;

pub fn response_has_reference_slugs(response: &Value, path: &str) -> Result<(), String> {
    match response {
        Value::Object(fields) => {
            for (field, value) in fields {
                let slug_field = if field == "id" {
                    Some("slug".to_string())
                } else {
                    field.strip_suffix("_id").map(|prefix| format!("{prefix}_slug"))
                };
                if let Some(slug_field) = slug_field {
                    let slug =
                        fields.get(&slug_field).ok_or_else(|| format!("{path}.{field}: missing {slug_field}"))?;
                    if value.is_null() != slug.is_null() || (!slug.is_null() && slug.as_str().is_none_or(str::is_empty))
                    {
                        return Err(format!(
                            "{path}.{field}: {slug_field} must be a nonempty slug, null exactly when {field} is null"
                        ));
                    }
                }
                response_has_reference_slugs(value, &format!("{path}.{field}"))?;
            }
        }
        Value::Array(rows) => {
            for (index, row) in rows.iter().enumerate() {
                response_has_reference_slugs(row, &format!("{path}[{index}]"))?;
            }
        }
        _ => {}
    }
    Ok(())
}
