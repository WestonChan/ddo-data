use serde_json::Value;
use std::collections::BTreeSet;
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
    validate_response_with_properties(value, schema, schemas, path, &BTreeSet::new())
}

fn declared_properties(schema: &Value, schemas: &serde_json::Map<String, Value>, properties: &mut BTreeSet<String>) {
    if let Some(reference) =
        schema["$ref"].as_str().and_then(|reference| reference.strip_prefix("#/components/schemas/"))
    {
        if let Some(target) = schemas.get(reference) {
            declared_properties(target, schemas, properties);
        }
    }
    if let Some(fields) = schema["properties"].as_object() {
        properties.extend(fields.keys().cloned());
    }
    for part in schema["allOf"].as_array().into_iter().flatten() {
        declared_properties(part, schemas, properties);
    }
}

fn validate_response_with_properties(
    value: &Value,
    schema: &Value,
    schemas: &serde_json::Map<String, Value>,
    path: &mut String,
    inherited_properties: &BTreeSet<String>,
) -> Result<(), String> {
    if let Some(allowed) = schema.as_bool() {
        return if allowed { Ok(()) } else { Err(format!("{path}: schema forbids this value")) };
    }
    if !schema.is_object() {
        return Err(format!("{path}: missing or invalid schema"));
    }
    if let Some(reference) = schema["$ref"].as_str() {
        let name =
            reference.strip_prefix("#/components/schemas/").ok_or_else(|| format!("{path}: invalid {reference}"))?;
        let target = schemas.get(name).ok_or_else(|| format!("{path}: missing {name}"))?;
        return validate_response_with_properties(value, target, schemas, path, inherited_properties);
    }
    if let Some(choices) = schema["oneOf"].as_array().or_else(|| schema["anyOf"].as_array()) {
        if choices.iter().any(|choice| validate_response(value, choice, schemas, path).is_ok()) {
            return Ok(());
        }
        return Err(format!("{path}: {value} matches no schema choice"));
    }
    if let Some(parts) = schema["allOf"].as_array() {
        let mut properties = inherited_properties.clone();
        declared_properties(schema, schemas, &mut properties);
        for part in parts {
            validate_response_with_properties(value, part, schemas, path, &properties)?;
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
    if !allowed_types.is_empty()
        && !allowed_types.contains(&actual_type)
        && !(actual_type == "integer" && allowed_types.contains(&"number"))
    {
        return Err(format!("{path}: expected {allowed_types:?}, got {actual_type}"));
    }
    if let Some(object) = value.as_object() {
        let properties = schema["properties"].as_object();
        let additional = schema.get("additionalProperties");
        if properties.is_none() && additional.is_none() && !allowed_types.is_empty() {
            return Err(format!("{path}: untyped object"));
        }
        for required in schema["required"].as_array().into_iter().flatten() {
            let name = required.as_str().ok_or_else(|| format!("{path}: invalid required field"))?;
            if !object.contains_key(name) {
                return Err(format!("{path}: missing {name}"));
            }
        }
        if properties.is_none() && additional.is_none() {
            return Ok(());
        }
        for (name, field) in object {
            if additional.is_none()
                && properties.is_none_or(|fields| !fields.contains_key(name))
                && inherited_properties.contains(name)
            {
                continue;
            }
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
        let Some(item_schema) = schema.get("items") else {
            return if allowed_types.is_empty() { Ok(()) } else { Err(format!("{path}: untyped array")) };
        };
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn composed_objects_validate_all_declared_fields_and_reject_undocumented_fields() {
        let schemas = json!({"Socket":{"type":"object","properties":{"label":{"type":"string"}},"required":["label"]}})
            .as_object()
            .unwrap()
            .clone();
        let schema = json!({"allOf":[{"$ref":"#/components/schemas/Socket"},{"type":"object","properties":{"choices":{"type":"array","items":{"type":"string"}}},"required":["choices"]}]});
        for (value, valid) in [
            (json!({"label":"blue","choices":["gem"]}), true),
            (json!({"label":"blue","choices":["gem"],"unknown":true}), false),
            (json!({"label":"blue"}), false),
            (json!({"label":4,"choices":[]}), false),
            (json!({"label":"blue","choices":[4]}), false),
        ] {
            assert_eq!(response_matches_schema(&value, &schema, &schemas, "socket").is_ok(), valid, "{value}");
        }
        let closed = json!({"allOf":[schema,{"type":"object","properties":{},"additionalProperties":false}]});
        assert!(response_matches_schema(&json!({"label":"blue","choices":[]}), &closed, &schemas, "socket").is_err());
    }
}
