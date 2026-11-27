use std::sync::OnceLock;

use serde_json::Value;

static SCHEMA: OnceLock<Value> = OnceLock::new();

pub(super) fn validate_record(record: &Value) -> Result<(), String> {
    let root = SCHEMA.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../../../docs/formats/rejection-trace-v1/record-v1.schema.json"
        ))
        .unwrap()
    });
    let kind = &record["type"];
    let variant = root["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .find(|variant| &variant["properties"]["type"]["const"] == kind)
        .ok_or_else(|| format!("unknown record type {kind}"))?;
    validate(record, variant, root)
}

// Require explicit handling of every structural keyword encountered in the wire schema.
fn validate(value: &Value, schema: &Value, root: &Value) -> Result<(), String> {
    for (keyword, constraint) in schema.as_object().unwrap() {
        let valid = match keyword.as_str() {
            "$ref" => {
                let pointer = constraint.as_str().unwrap().strip_prefix('#').unwrap();
                validate(value, root.pointer(pointer).unwrap(), root)?;
                true
            }
            "type" => matches_type(value, constraint.as_str().unwrap()),
            "const" => value == constraint,
            "enum" => constraint.as_array().unwrap().contains(value),
            "required" => constraint
                .as_array()
                .unwrap()
                .iter()
                .all(|name| value.get(name.as_str().unwrap()).is_some()),
            "additionalProperties" => {
                assert_eq!(constraint, false);
                value.as_object().is_none_or(|fields| {
                    fields
                        .keys()
                        .all(|name| schema["properties"].get(name).is_some())
                })
            }
            "properties" => {
                for (name, property) in constraint.as_object().unwrap() {
                    if let Some(field) = value.get(name) {
                        validate(field, property, root)
                            .map_err(|error| format!(".{name}{error}"))?;
                    }
                }
                true
            }
            "oneOf" => {
                let branches = constraint.as_array().unwrap();
                let matched = branches
                    .iter()
                    .filter(|branch| validate(value, branch, root).is_ok())
                    .count();
                if matched != 1 {
                    return Err(format!(
                        " matches {matched} of {} oneOf branches",
                        branches.len()
                    ));
                }
                true
            }
            "allOf" => {
                for branch in constraint.as_array().unwrap() {
                    validate(value, branch, root)?;
                }
                true
            }
            "if" => {
                if validate(value, constraint, root).is_ok() {
                    validate(value, &schema["then"], root)?;
                }
                true
            }
            "then" | "minContains" | "maxContains" => true,
            "dependentRequired" => constraint
                .as_object()
                .unwrap()
                .iter()
                .all(|(name, fields)| {
                    value.get(name).is_none()
                        || fields
                            .as_array()
                            .unwrap()
                            .iter()
                            .all(|field| value.get(field.as_str().unwrap()).is_some())
                }),
            "contains" => value.as_array().is_none_or(|items| {
                let count = items
                    .iter()
                    .filter(|item| validate(item, constraint, root).is_ok())
                    .count() as u64;
                let min = schema["minContains"].as_u64().unwrap_or(1);
                let max = schema["maxContains"].as_u64().unwrap_or(u64::MAX);
                (min..=max).contains(&count)
            }),
            "items" => {
                if let Some(items) = value.as_array() {
                    for (index, item) in items.iter().enumerate() {
                        validate(item, constraint, root)
                            .map_err(|error| format!("[{index}]{error}"))?;
                    }
                }
                true
            }
            "uniqueItems" => {
                assert_eq!(constraint, true);
                value.as_array().is_none_or(|items| {
                    items
                        .iter()
                        .enumerate()
                        .all(|(index, item)| !items[..index].contains(item))
                })
            }
            "minItems" => value
                .as_array()
                .is_none_or(|items| items.len() as u64 >= constraint.as_u64().unwrap()),
            "maxItems" => value
                .as_array()
                .is_none_or(|items| items.len() as u64 <= constraint.as_u64().unwrap()),
            "maxLength" => value
                .as_str()
                .is_none_or(|text| text.chars().count() as u64 <= constraint.as_u64().unwrap()),
            "x-maxUtf8Bytes" => value
                .as_str()
                .is_none_or(|text| text.len() as u64 <= constraint.as_u64().unwrap()),
            "x-maxEncodedJsonBytes" => value
                .as_str()
                .is_none_or(|_| value.to_string().len() as u64 <= constraint.as_u64().unwrap()),
            "pattern" => value
                .as_str()
                .is_none_or(|text| matches_pattern(text, constraint.as_str().unwrap())),
            "minimum" => integer(value).is_none_or(|number| number >= integer(constraint).unwrap()),
            "maximum" => integer(value).is_none_or(|number| number <= integer(constraint).unwrap()),
            other => panic!("unimplemented record schema keyword {other}"),
        };
        if !valid {
            return Err(format!(" violates {keyword} {constraint}"));
        }
    }
    Ok(())
}

fn integer(value: &Value) -> Option<i128> {
    value
        .as_u64()
        .map(i128::from)
        .or_else(|| value.as_i64().map(i128::from))
}

fn matches_type(value: &Value, expected: &str) -> bool {
    match expected {
        "array" => value.is_array(),
        "boolean" => value.is_boolean(),
        "integer" => integer(value).is_some(),
        "null" => value.is_null(),
        "object" => value.is_object(),
        "string" => value.is_string(),
        other => panic!("unimplemented record schema type {other}"),
    }
}

fn matches_pattern(text: &str, pattern: &str) -> bool {
    let lowercase_hex = |byte: u8| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte);
    match pattern {
        "^[ -~]+$" => !text.is_empty() && text.bytes().all(|byte| (b' '..=b'~').contains(&byte)),
        "^[0-9a-f]{64}$" => text.len() == 64 && text.bytes().all(lowercase_hex),
        "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$" => {
            text.len() == 36
                && text.bytes().enumerate().all(|(index, byte)| match index {
                    8 | 13 | 18 | 23 => byte == b'-',
                    _ => lowercase_hex(byte),
                })
        }
        other => panic!("unimplemented record schema pattern {other}"),
    }
}
