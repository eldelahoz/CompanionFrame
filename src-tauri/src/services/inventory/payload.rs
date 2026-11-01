//! Inventory payload helpers: shape check, envelope unwrapping and indented export text.
//! Port of `config/shared/inventoryPayload.ts` and `services/jsonIndent.ts`.

use serde_json::Value;

const DEFAULT_MAX_UNWRAP_DEPTH: usize = 4;
const MAX_SAFE_UNWRAP_DEPTH: usize = 12;

const INVENTORY_ARRAY_KEYS: [&str; 6] = [
    "Suits",
    "RawUpgrades",
    "Upgrades",
    "Arcanes",
    "LevelKeys",
    "MiscItems",
];
const ENVELOPE_KEYS: [&str; 5] = [
    "InventoryJson",
    "inventoryJson",
    "inventory_json",
    "payload",
    "data",
];

pub fn has_inventory_shape(v: &Value) -> bool {
    INVENTORY_ARRAY_KEYS
        .iter()
        .any(|k| v.get(k).is_some_and(Value::is_array))
}

fn first_envelope_value(v: &Value) -> Option<&Value> {
    ENVELOPE_KEYS.iter().find_map(|k| v.get(k))
}

/// Peels `InventoryJson`/`payload`/`data` envelopes (objects or JSON strings) off `value`.
/// Returns whatever it reached; callers check `has_inventory_shape`.
pub fn unwrap_inventory_payload(value: Value, max_depth: Option<usize>) -> Value {
    let max_depth = match max_depth {
        Some(d) if d >= 1 => d.min(MAX_SAFE_UNWRAP_DEPTH),
        _ => DEFAULT_MAX_UNWRAP_DEPTH,
    };
    let mut current = value;
    for _ in 0..max_depth {
        if has_inventory_shape(&current) || !current.is_object() {
            return current;
        }
        let next = match first_envelope_value(&current) {
            Some(Value::String(s)) => match serde_json::from_str::<Value>(s) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("failed to parse nested inventory payload string: {e}");
                    return current;
                }
            },
            Some(v @ Value::Object(_)) => v.clone(),
            _ => return current,
        };
        current = next;
    }
    current
}

/// The inventory's own JSON text inside a payload, or `None` when it sits under an object
/// envelope. Keeps integers beyond 2^53 exact, unlike the parsed value.
pub fn unwrap_inventory_text(text: &str) -> Result<Option<String>, serde_json::Error> {
    let mut current = text.to_owned();
    for _ in 0..MAX_SAFE_UNWRAP_DEPTH {
        let value: Value = serde_json::from_str(&current)?;
        if has_inventory_shape(&value) {
            return Ok(Some(current));
        }
        match first_envelope_value(&value) {
            Some(Value::String(s)) if value.is_object() => current = s.clone(),
            _ => return Ok(None),
        }
    }
    Ok(None)
}

/// Lays out valid JSON text like `JSON.stringify(v, null, 2)` without parsing it, so 64-bit
/// integers (inventory seeds, nemesis fp) keep every digit.
pub fn indent_json_text(text: &str) -> String {
    let b = text.as_bytes();
    let is_space = |c: u8| matches!(c, b' ' | b'\n' | b'\r' | b'\t');
    let pad = |out: &mut String, depth: usize| (0..depth).for_each(|_| out.push_str("  "));
    let mut out = String::with_capacity(text.len() * 2);
    let (mut depth, mut i) = (0usize, 0usize);
    while i < b.len() {
        let c = b[i];
        match c {
            b'"' => {
                let mut end = i + 1;
                while end < b.len() && b[end] != b'"' {
                    end += if b[end] == b'\\' { 2 } else { 1 };
                }
                let end = (end + 1).min(b.len());
                out.push_str(&text[i..end]);
                i = end;
            }
            b'{' | b'[' => {
                let mut next = i + 1;
                while next < b.len() && is_space(b[next]) {
                    next += 1;
                }
                let close = if c == b'{' { b'}' } else { b']' };
                if next < b.len() && b[next] == close {
                    out.push(c as char);
                    out.push(close as char);
                    i = next + 1;
                } else {
                    depth += 1;
                    out.push(c as char);
                    out.push('\n');
                    pad(&mut out, depth);
                    i += 1;
                }
            }
            b'}' | b']' => {
                depth = depth.saturating_sub(1);
                out.push('\n');
                pad(&mut out, depth);
                out.push(c as char);
                i += 1;
            }
            b',' => {
                out.push_str(",\n");
                pad(&mut out, depth);
                i += 1;
            }
            b':' => {
                out.push_str(": ");
                i += 1;
            }
            _ if is_space(c) => i += 1,
            _ => {
                let mut end = i + 1;
                while end < b.len() && !is_space(b[end]) && !b",:{}[]\"".contains(&b[end]) {
                    end += 1;
                }
                out.push_str(&text[i..end]);
                i = end;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detects_inventory_like_payloads() {
        assert!(has_inventory_shape(&json!({"Suits": []})));
        assert!(has_inventory_shape(&json!({"Upgrades": []})));
        assert!(!has_inventory_shape(&json!({})));
    }

    #[test]
    fn unwraps_nested_object_envelopes() {
        let raw = json!({"payload": {"data": {"Suits": [{"ItemType": "A"}], "Upgrades": []}}});
        let out = unwrap_inventory_payload(raw, None);
        assert_eq!(out["Suits"][0]["ItemType"], "A");
    }

    #[test]
    fn unwraps_nested_json_strings() {
        let raw =
            json!({"inventory_json": json!({"LevelKeys": [{"ItemType": "RelicX"}]}).to_string()});
        let out = unwrap_inventory_payload(raw, None);
        assert_eq!(out["LevelKeys"][0]["ItemType"], "RelicX");
    }

    #[test]
    fn unparsable_envelope_string_is_returned_as_is() {
        let raw = json!({"data": "{nope"});
        assert!(!has_inventory_shape(&unwrap_inventory_payload(raw, None)));
    }

    #[test]
    fn text_unwrap_keeps_big_integers() {
        let inner = r#"{"Suits":[],"Seed":18446744073709551615}"#;
        let outer = json!({"InventoryJson": inner}).to_string();
        assert_eq!(
            unwrap_inventory_text(&outer).unwrap().as_deref(),
            Some(inner)
        );
        assert_eq!(
            unwrap_inventory_text(r#"{"data":{"Suits":[]}}"#).unwrap(),
            None
        );
    }

    #[test]
    fn indents_like_stringify_with_every_digit() {
        let text = r#"{"a":[1,{"b":"x\"y"}],"e":{},"f":[],"seed":18446744073709551615}"#;
        let expect = "{\n  \"a\": [\n    1,\n    {\n      \"b\": \"x\\\"y\"\n    }\n  ],\n  \"e\": {},\n  \"f\": [],\n  \"seed\": 18446744073709551615\n}";
        assert_eq!(indent_json_text(text), expect);
    }
}
