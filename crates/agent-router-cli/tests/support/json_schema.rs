//! A small JSON Schema checker for the embedded output schemas. It implements
//! only the keywords those schemas use, and `check_schema` fails on any other
//! keyword, so a schema can never silently rely on a rule this file ignores.
//! It is not a full 2020-12 validator and does not run the metaschema.
#![allow(dead_code)]
use serde_json::Value;

const ANNOTATIONS: [&str; 5] = ["$schema", "$id", "title", "description", "$comment"];
const KEYWORDS: [&str; 15] = [
    "$defs",
    "$ref",
    "type",
    "const",
    "enum",
    "required",
    "properties",
    "additionalProperties",
    "items",
    "minItems",
    "minimum",
    "minLength",
    "maxLength",
    "oneOf",
    "anyOf",
];
const TYPES: [&str; 7] = [
    "null", "boolean", "object", "array", "string", "integer", "number",
];

/// Structural checks of the schema itself: known keywords only, valid `type`
/// names, `required` names declared under `properties`, `$ref` targets that exist.
pub fn check_schema(root: &Value) {
    assert_eq!(
        root["$schema"], "https://json-schema.org/draft/2020-12/schema",
        "dialect"
    );
    assert!(root["$id"].as_str().is_some_and(|id| !id.is_empty()), "$id");
    walk(root, root, "#");
}

fn walk(root: &Value, schema: &Value, at: &str) {
    let Some(map) = schema.as_object() else {
        // A boolean sub-schema (additionalProperties: false).
        assert!(
            schema.is_boolean(),
            "{at}: schema must be an object or boolean"
        );
        return;
    };
    for key in map.keys() {
        assert!(
            ANNOTATIONS.contains(&key.as_str()) || KEYWORDS.contains(&key.as_str()),
            "{at}: unsupported keyword {key}"
        );
    }
    if let Some(types) = map.get("type") {
        let names: Vec<&Value> = match types {
            Value::Array(items) => items.iter().collect(),
            single => vec![single],
        };
        for name in names {
            assert!(
                name.as_str().is_some_and(|n| TYPES.contains(&n)),
                "{at}: invalid type {name}"
            );
        }
    }
    if let Some(reference) = map.get("$ref") {
        let reference = reference.as_str().unwrap();
        assert!(
            resolve(root, reference).is_some(),
            "{at}: dangling {reference}"
        );
    }
    if let Some(required) = map.get("required") {
        let properties = map.get("properties").and_then(Value::as_object);
        for name in required.as_array().unwrap() {
            let name = name.as_str().unwrap();
            assert!(
                properties.is_some_and(|p| p.contains_key(name)),
                "{at}: required {name} is not a declared property"
            );
        }
    }
    for (key, child) in map {
        match key.as_str() {
            "properties" | "$defs" => {
                for (name, sub) in child.as_object().unwrap() {
                    walk(root, sub, &format!("{at}/{key}/{name}"));
                }
            }
            "items" | "additionalProperties" => walk(root, child, &format!("{at}/{key}")),
            "oneOf" | "anyOf" => {
                for (index, sub) in child.as_array().unwrap().iter().enumerate() {
                    walk(root, sub, &format!("{at}/{key}/{index}"));
                }
            }
            _ => {}
        }
    }
}

fn resolve<'a>(root: &'a Value, reference: &str) -> Option<&'a Value> {
    let pointer = reference.strip_prefix('#')?;
    root.pointer(pointer)
}

/// Every violation of `schema` by `value`; empty when the value conforms.
pub fn violations(schema: &Value, value: &Value) -> Vec<String> {
    let mut found = Vec::new();
    validate(schema, schema, value, "$", &mut found);
    found
}

fn type_matches(name: &str, value: &Value) -> bool {
    match name {
        "null" => value.is_null(),
        "boolean" => value.is_boolean(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        _ => false,
    }
}

fn validate(root: &Value, schema: &Value, value: &Value, at: &str, found: &mut Vec<String>) {
    let Some(map) = schema.as_object() else {
        return;
    };
    if let Some(reference) = map.get("$ref") {
        let target = resolve(root, reference.as_str().unwrap()).unwrap();
        validate(root, target, value, at, found);
    }
    if map.get("const").is_some_and(|expected| expected != value) {
        found.push(format!("{at}: expected {}, found {value}", map["const"]));
    }
    if let Some(Value::Array(options)) = map.get("enum") {
        if !options.contains(value) {
            found.push(format!("{at}: {value} is not one of {options:?}"));
        }
    }
    if let Some(types) = map.get("type") {
        let names: Vec<&str> = match types {
            Value::Array(items) => items.iter().filter_map(Value::as_str).collect(),
            single => single.as_str().into_iter().collect(),
        };
        if !names.iter().any(|name| type_matches(name, value)) {
            found.push(format!("{at}: {value} is not of type {names:?}"));
            return;
        }
    }
    if let (Some(minimum), Some(number)) = (map.get("minimum"), value.as_f64()) {
        if number < minimum.as_f64().unwrap() {
            found.push(format!("{at}: {number} is below {minimum}"));
        }
    }
    if let Some(text) = value.as_str() {
        let length = text.chars().count() as u64;
        if map
            .get("minLength")
            .and_then(Value::as_u64)
            .is_some_and(|m| length < m)
        {
            found.push(format!("{at}: string shorter than minLength"));
        }
        if map
            .get("maxLength")
            .and_then(Value::as_u64)
            .is_some_and(|m| length > m)
        {
            found.push(format!("{at}: string longer than maxLength"));
        }
    }
    if let Some(object) = value.as_object() {
        for name in map
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let name = name.as_str().unwrap();
            if !object.contains_key(name) {
                found.push(format!("{at}: missing required {name}"));
            }
        }
        let properties = map.get("properties").and_then(Value::as_object);
        for (key, child) in object {
            let path = format!("{at}.{key}");
            match (
                properties.and_then(|p| p.get(key)),
                map.get("additionalProperties"),
            ) {
                (Some(sub), _) => validate(root, sub, child, &path, found),
                (None, Some(Value::Bool(false))) => found.push(format!("{path}: unknown key")),
                (None, Some(extra)) => validate(root, extra, child, &path, found),
                (None, None) => {}
            }
        }
    }
    if let Some(items) = value.as_array() {
        if map
            .get("minItems")
            .and_then(Value::as_u64)
            .is_some_and(|m| (items.len() as u64) < m)
        {
            found.push(format!("{at}: fewer than minItems"));
        }
        if let Some(sub) = map.get("items") {
            for (index, item) in items.iter().enumerate() {
                validate(root, sub, item, &format!("{at}[{index}]"), found);
            }
        }
    }
    if let Some(Value::Array(options)) = map.get("oneOf") {
        let matching = options
            .iter()
            .filter(|option| violations_in(root, option, value).is_empty())
            .count();
        if matching != 1 {
            found.push(format!(
                "{at}: matches {matching} oneOf branches, expected 1"
            ));
        }
    }
    if let Some(Value::Array(options)) = map.get("anyOf") {
        if !options
            .iter()
            .any(|option| violations_in(root, option, value).is_empty())
        {
            found.push(format!("{at}: matches no anyOf branch"));
        }
    }
}

fn violations_in(root: &Value, schema: &Value, value: &Value) -> Vec<String> {
    let mut found = Vec::new();
    validate(root, schema, value, "$", &mut found);
    found
}
