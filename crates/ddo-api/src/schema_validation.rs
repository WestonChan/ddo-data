use serde_json::Value;
use std::fmt::Write;

pub fn response_matches_schema(
    value: &Value,
    schema: &Value,
    schemas: &serde_json::Map<String, Value>,
    path: &str,
) -> Result<(), String> {
    validate_response(value, schema, schemas, &mut path.to_string())
}

fn validate_response(
    value: &Value,
    schema: &Value,
    schemas: &serde_json::Map<String, Value>,
    path: &mut String,
) -> Result<(), String> {
    if let Some(reference) = schema["$ref"].as_str() {
        let name =
            reference.strip_prefix("#/components/schemas/").ok_or_else(|| format!("{path}: invalid {reference}"))?;
        let target = schemas.get(name).ok_or_else(|| format!("{path}: missing {name}"))?;
        return validate_response(value, target, schemas, path);
    }
    if let Some(choices) = schema["oneOf"].as_array().or_else(|| schema["anyOf"].as_array()) {
        if choices.iter().any(|choice| validate_response(value, choice, schemas, path).is_ok()) {
            return Ok(());
        }
        return Err(format!("{path}: {value} matches no schema choice"));
    }
    if let Some(parts) = schema["allOf"].as_array() {
        for part in parts {
            validate_response(value, part, schemas, path)?;
        }
        return Ok(());
    }
    let allowed_types: Vec<&str> = match &schema["type"] {
        Value::String(kind) => vec![kind],
        Value::Array(kinds) => kinds.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    let actual_type = match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    };
    if !allowed_types.contains(&actual_type) && !(actual_type == "integer" && allowed_types.contains(&"number")) {
        return Err(format!("{path}: expected {allowed_types:?}, got {actual_type}"));
    }
    if let Some(object) = value.as_object() {
        let properties = schema["properties"].as_object();
        let additional = schema.get("additionalProperties");
        if properties.is_none() && additional.is_none() {
            return Err(format!("{path}: untyped object"));
        }
        for required in schema["required"].as_array().into_iter().flatten() {
            let name = required.as_str().ok_or_else(|| format!("{path}: invalid required field"))?;
            if !object.contains_key(name) {
                return Err(format!("{path}: missing {name}"));
            }
        }
        for (name, field) in object {
            let field_schema = properties
                .and_then(|fields| fields.get(name))
                .or(additional)
                .ok_or_else(|| format!("{path}: undocumented {name}"))?;
            let parent_length = path.len();
            path.push('.');
            path.push_str(name);
            let validated = validate_response(field, field_schema, schemas, path);
            path.truncate(parent_length);
            validated?;
        }
    }
    if let Some(items) = value.as_array() {
        let item_schema = schema.get("items").ok_or_else(|| format!("{path}: untyped array"))?;
        for (index, item) in items.iter().enumerate() {
            let parent_length = path.len();
            write!(path, "[{index}]").expect("writing to a String cannot fail");
            let validated = validate_response(item, item_schema, schemas, path);
            path.truncate(parent_length);
            validated?;
        }
    }
    Ok(())
}
