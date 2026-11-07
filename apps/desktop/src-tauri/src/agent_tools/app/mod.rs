//! App-control tools of the agent (ADR 0013): open or close a side panel, read
//! the Usage page's numbers, search and read past chats, start a project's
//! dev server. They run in the shell like the browser and terminal tools and
//! are registered by `agent_tools::registry`.
//!
//! Spec: docs/features/agent-browser (FR-18 – FR-21), plan 3.2 – 3.5.

pub mod panel;
pub mod preview;
pub mod sessions;
pub mod usage;

use serde_json::Value;

use super::Tool;

/// Every tool of this group.
pub fn tools() -> Vec<Tool> {
    let mut tools = Vec::new();
    tools.extend(panel::tools());
    tools.extend(usage::tools());
    tools.extend(sessions::tools());
    tools.extend(preview::tools());
    tools
}

// ── Argument helpers (pure, shared by the tools) ────────────────────────────

/// An optional string argument; a value of another type is an error.
pub(crate) fn optional_str<'a>(args: &'a Value, key: &str) -> Result<Option<&'a str>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.as_str())),
        Some(_) => Err(format!("`{key}` must be a string.")),
    }
}

/// A required, non-blank string argument.
pub(crate) fn required_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    match optional_str(args, key)? {
        Some(text) if !text.trim().is_empty() => Ok(text.trim()),
        _ => Err(format!("`{key}` is required.")),
    }
}

/// An optional whole number between `min` and `max`; `default` when absent.
pub(crate) fn optional_count(args: &Value, key: &str, default: u64, min: u64, max: u64) -> Result<u64, String> {
    let Some(value) = args.get(key).filter(|value| !value.is_null()) else {
        return Ok(default);
    };
    let number = match value {
        Value::Number(number) => number.as_u64().or_else(|| number.as_f64().filter(|f| f.fract() == 0.0 && *f >= 0.0).map(|f| f as u64)),
        _ => None,
    };
    match number {
        Some(n) if (min..=max).contains(&n) => Ok(n),
        _ => Err(format!("`{key}` must be a whole number from {min} to {max}.")),
    }
}

/// An optional boolean argument; `default` when absent.
pub(crate) fn optional_bool(args: &Value, key: &str, default: bool) -> Result<bool, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Bool(flag)) => Ok(*flag),
        Some(_) => Err(format!("`{key}` must be true or false.")),
    }
}

/// One of `allowed` (`default` when absent).
pub(crate) fn choice(args: &Value, key: &str, allowed: &[&'static str], default: &'static str) -> Result<&'static str, String> {
    match optional_str(args, key)? {
        None => Ok(default),
        Some(text) => allowed
            .iter()
            .find(|candidate| candidate.eq_ignore_ascii_case(text.trim()))
            .copied()
            .ok_or_else(|| format!("`{key}` must be one of: {}.", allowed.join(", "))),
    }
}

/// The first `max` characters of `text`, and whether anything was cut.
pub(crate) fn cut(text: &str, max: usize) -> (String, bool) {
    match text.char_indices().nth(max) {
        Some((end, _)) => (text[..end].to_string(), true),
        None => (text.to_string(), false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strings_are_validated() {
        let args = json!({ "a": "x", "b": 3, "c": "  ", "d": null });
        assert_eq!(optional_str(&args, "a").unwrap(), Some("x"));
        assert_eq!(optional_str(&args, "d").unwrap(), None);
        assert_eq!(optional_str(&args, "zz").unwrap(), None);
        assert!(optional_str(&args, "b").is_err());
        assert_eq!(required_str(&json!({ "q": "  hi " }), "q").unwrap(), "hi");
        assert!(required_str(&args, "c").is_err());
        assert!(required_str(&args, "zz").is_err());
    }

    #[test]
    fn counts_have_defaults_and_bounds() {
        assert_eq!(optional_count(&json!({}), "n", 10, 1, 30).unwrap(), 10);
        assert_eq!(optional_count(&json!({ "n": 5 }), "n", 10, 1, 30).unwrap(), 5);
        assert_eq!(optional_count(&json!({ "n": 5.0 }), "n", 10, 1, 30).unwrap(), 5);
        for bad in [json!({ "n": 0 }), json!({ "n": 31 }), json!({ "n": -1 }), json!({ "n": 1.5 }), json!({ "n": "7" })] {
            assert!(optional_count(&bad, "n", 10, 1, 30).is_err(), "{bad}");
        }
    }

    #[test]
    fn choices_ignore_case_and_name_the_options() {
        let allowed = ["open", "close"];
        assert_eq!(choice(&json!({}), "action", &allowed, "open").unwrap(), "open");
        assert_eq!(choice(&json!({ "action": "CLOSE" }), "action", &allowed, "open").unwrap(), "close");
        let error = choice(&json!({ "action": "toggle" }), "action", &allowed, "open").unwrap_err();
        assert!(error.contains("open, close"), "{error}");
        assert!(optional_bool(&json!({ "b": "yes" }), "b", true).is_err());
        assert!(!optional_bool(&json!({ "b": false }), "b", true).unwrap());
    }

    #[test]
    fn cutting_counts_characters_not_bytes() {
        assert_eq!(cut("héllo", 3), ("hél".to_string(), true));
        assert_eq!(cut("hi", 3), ("hi".to_string(), false));
        assert_eq!(cut("abc", 3), ("abc".to_string(), false));
    }

    #[test]
    fn the_group_registers_five_unique_tools_with_closed_schemas() {
        let tools = tools();
        let mut names: Vec<_> = tools.iter().map(|tool| tool.name).collect();
        names.sort_unstable();
        assert_eq!(names, ["app_panel", "preview_start", "session_read", "session_search", "usage_get"]);
        for tool in &tools {
            let schema = (tool.schema)();
            assert_eq!(schema["type"], "object", "{}", tool.name);
            assert_eq!(schema["additionalProperties"], false, "{}", tool.name);
            assert!(!tool.description.is_empty());
        }
    }
}
