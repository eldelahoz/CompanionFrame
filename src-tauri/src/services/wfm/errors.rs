//! Error type and the status classification from WFHelper PR #59.
//!
//! 401/403/408/429/5xx and network failures are transient: never cache them as "no data".
//! Only 400/404 mean the item has no data.

use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WfmError {
    /// 401 or no token: the session is missing, expired or the credentials are wrong.
    Unauthorized {
        detail: String,
    },
    RateLimited {
        retry_after_secs: Option<u32>,
    },
    Network {
        detail: String,
    },
    Api {
        status: u16,
        detail: String,
    },
    /// Local failure (bad input, keyring, unreadable body).
    Invalid {
        detail: String,
    },
    /// The item is sold per subtype and none was chosen; carries the API's list so the caller can ask.
    SubtypeRequired {
        subtypes: Vec<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    NoData,
    Transient,
    Failed,
}

pub fn classify_status(status: u16) -> Class {
    match status {
        400 | 404 => Class::NoData,
        401 | 403 | 408 | 429 | 500..=599 => Class::Transient,
        _ => Class::Failed,
    }
}

impl WfmError {
    pub fn class(&self) -> Class {
        match self {
            Self::Unauthorized { .. } | Self::RateLimited { .. } | Self::Network { .. } => {
                Class::Transient
            }
            Self::Api { status, .. } => classify_status(*status),
            Self::Invalid { .. } | Self::SubtypeRequired { .. } => Class::Failed,
        }
    }

    pub fn invalid(detail: impl ToString) -> Self {
        Self::Invalid {
            detail: detail.to_string(),
        }
    }
}

impl std::fmt::Display for WfmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthorized { detail } => write!(f, "unauthorized: {detail}"),
            Self::RateLimited { retry_after_secs } => {
                write!(f, "rate limited ({retry_after_secs:?}s)")
            }
            Self::Network { detail } => write!(f, "network error: {detail}"),
            Self::Api { status, detail } => write!(f, "HTTP {status}: {detail}"),
            Self::Invalid { detail } => f.write_str(detail),
            Self::SubtypeRequired { subtypes } => write!(
                f,
                "warframe.market needs a subtype for this item: {}",
                subtypes.join(", ")
            ),
        }
    }
}

impl std::error::Error for WfmError {}

fn flatten(v: &Value, depth: usize) -> Vec<String> {
    match v {
        Value::String(s) if !s.trim().is_empty() => vec![s.trim().to_owned()],
        Value::Array(a) if depth <= 4 => a
            .iter()
            .flat_map(|e| flatten(e, depth + 1))
            .take(4)
            .collect(),
        Value::Object(o) if depth <= 4 => o
            .iter()
            .flat_map(|(k, e)| {
                flatten(e, depth + 1).into_iter().map(move |m| {
                    // A bare leaf gets its field name: {perTrade:"app.field.required"}.
                    if m.contains(|c: char| c == ':' || c.is_whitespace()) {
                        m
                    } else {
                        format!("{k}: {m}")
                    }
                })
            })
            .take(4)
            .collect(),
        _ => vec![],
    }
}

/// Human-readable detail from a WFM error body (port of WFHelper's `extractWfmErrorDetail`).
pub fn error_detail(body: &Value, object_fallback: Option<&str>) -> Option<String> {
    let non_empty = |v: Option<&Value>| {
        v.and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned)
    };
    let error = body.get("error");
    if let Some(s) = non_empty(error) {
        return Some(s);
    }
    if let Some(s) = non_empty(error.and_then(|e| e.get("message"))) {
        return Some(s);
    }
    if let Some(s) = non_empty(body.get("message")) {
        return Some(s);
    }
    if error.is_some_and(|e| e.is_object() || e.is_array()) {
        let msgs = flatten(error.unwrap(), 0);
        return if msgs.is_empty() {
            object_fallback.map(str::to_owned)
        } else {
            Some(msgs.join("; "))
        };
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_400_and_404_mean_no_data() {
        for s in [400, 404] {
            assert_eq!(classify_status(s), Class::NoData, "{s}");
        }
        for s in [401, 403, 408, 429, 500, 502, 503, 599] {
            assert_eq!(classify_status(s), Class::Transient, "{s}");
        }
        assert_eq!(classify_status(405), Class::Failed);
    }

    #[test]
    fn error_variants_classify() {
        let unauth = WfmError::Unauthorized {
            detail: String::new(),
        };
        assert_eq!(unauth.class(), Class::Transient);
        assert_eq!(
            WfmError::RateLimited {
                retry_after_secs: None
            }
            .class(),
            Class::Transient
        );
        assert_eq!(
            WfmError::Network {
                detail: String::new()
            }
            .class(),
            Class::Transient
        );
        assert_eq!(
            WfmError::Api {
                status: 404,
                detail: String::new()
            }
            .class(),
            Class::NoData
        );
        assert_eq!(WfmError::invalid("x").class(), Class::Failed);
    }

    #[test]
    fn detail_surfaces_nested_field_errors() {
        let body = json!({"apiVersion":"0.25.0","data":null,"error":{"inputs":{"perTrade":"app.field.required"}}});
        assert_eq!(
            error_detail(&body, None).unwrap(),
            "perTrade: app.field.required"
        );
    }

    #[test]
    fn detail_handles_string_message_and_empty() {
        assert_eq!(
            error_detail(&json!({"error":"Item not found"}), None).unwrap(),
            "Item not found"
        );
        assert_eq!(
            error_detail(&json!({"error":{"message":"boom"}}), None).unwrap(),
            "boom"
        );
        assert_eq!(
            error_detail(&json!({"message":"nope"}), None).unwrap(),
            "nope"
        );
        assert_eq!(error_detail(&json!("HTTP 400"), None), None);
        assert_eq!(error_detail(&json!({}), None), None);
        assert_eq!(
            error_detail(&json!({"error":{}}), Some("Invalid credentials.")).unwrap(),
            "Invalid credentials."
        );
    }
}
