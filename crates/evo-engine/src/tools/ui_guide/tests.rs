use super::*;
use evo_harness::types::ToolReplay;
use serde_json::Map;

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| (*name).to_owned()).collect()
}

/// The heading of each guide in `text`, in order.
fn headings(text: &str) -> Vec<&str> {
    text.lines()
        .filter_map(|line| line.strip_prefix("## "))
        .collect()
}

#[test]
fn guides_come_in_request_order_once_each() {
    let (text, shown) = guide(&names(&["table", "stats", "TABLE", " stats ", "bars"]));
    assert_eq!(headings(&text), ["table", "stats", "bars"]);
    assert_eq!(shown, ["table", "stats", "bars"]);
    // The shared rules come first, once.
    assert!(text.starts_with("# Evo UI blocks"), "{text}");
    assert_eq!(text.matches("# Evo UI blocks").count(), 1);
    assert!(!text.contains("Unknown:"), "{text}");
}

#[test]
fn unknown_names_are_listed_after_the_guides() {
    let (text, shown) = guide(&names(&["pie", "chart", "gauge", "pie"]));
    assert_eq!(headings(&text), ["chart"]);
    assert_eq!(shown, ["chart"]);
    assert!(text.trim_end().ends_with("Unknown: pie, gauge"), "{text}");
}

#[test]
fn with_no_known_name_it_lists_the_known_ones() {
    for requested in [names(&["pie"]), Vec::new()] {
        let (text, shown) = guide(&requested);
        assert!(shown.is_empty());
        assert!(headings(&text).is_empty(), "{text}");
        assert!(
            text.contains(
                "Known components: stats, bars, chart, table, steps, callout, actions, formats."
            ),
            "{text}"
        );
        assert_eq!(
            text.contains("Unknown: pie"),
            !requested.is_empty(),
            "{text}"
        );
    }
}

#[test]
fn every_component_has_a_guide_with_fields_one_example_and_rules() {
    for name in COMPONENTS {
        let text = lf(guide_of(name).unwrap());
        assert!(text.starts_with(&format!("## {name}\n")), "{name}");
        assert_eq!(text.matches("```evo-ui").count(), 1, "{name}: one example");
        assert!(text.contains("\nRules:\n"), "{name}");
        if name != "formats" {
            assert!(text.contains("\nFields:\n"), "{name}");
        }
    }
}

#[test]
fn the_guides_state_the_rules_the_spec_asks_for() {
    let (all, _) = guide(&names(&COMPONENTS));
    for needle in [
        // FR-11: when to use a block, numbers, format, source, prose.
        "three or more related values",
        "Never estimate",
        "raw numbers",
        "Name the `source`",
        "Do not repeat the block's data in prose",
        // FR-6 / plan 5: a percentage, not a ratio.
        "a percentage, not a ratio. `25` → 25%",
        "a time in seconds",
        // FR-22: the two actions and nothing else.
        "`{\"send\": text}`",
        "`{\"open\": path}`",
        "`https://` link",
        "Anything else is refused",
        // FR-14 limits.
        "1 to 8 tiles",
        "1 to 50 bars",
        "1 to 6 series",
        "1,000 points",
        "1 to 500 rows",
        "1 to 12 columns",
        "1 to 4 buttons",
    ] {
        assert!(all.contains(needle), "the guides must say {needle:?}");
    }
    // Where an item takes an action, the guide points at the rules.
    for name in ["bars", "table"] {
        let text = guide_of(name).unwrap();
        assert!(
            text.contains("`action` (optional)") && text.contains("Read the `actions` guide"),
            "{name}"
        );
    }
    assert!(!all.to_lowercase().contains("claude"));
}

// ── The examples are blocks the desktop draws ─────────────────────────────

/// The bodies of the `evo-ui` fences in `text`.
fn examples(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut body: Option<Vec<&str>> = None;
    for line in text.lines() {
        match &mut body {
            None if line.trim() == "```evo-ui" => body = Some(Vec::new()),
            Some(lines) if line.trim() == "```" => {
                found.push(lines.join("\n"));
                body = None;
            }
            Some(lines) => lines.push(line),
            None => {}
        }
    }
    assert!(body.is_none(), "an example fence is not closed");
    found
}

type Json = Map<String, Value>;

fn obj<'a>(value: &'a Value, what: &str) -> Result<&'a Json, String> {
    value.as_object().ok_or(format!("{what} must be an object"))
}

fn list<'a>(json: &'a Json, key: &str) -> Result<&'a Vec<Value>, String> {
    json.get(key)
        .and_then(Value::as_array)
        .ok_or(format!("\"{key}\" must be a list"))
}

fn string(json: &Json, key: &str) -> Result<(), String> {
    json.get(key)
        .and_then(Value::as_str)
        .map(|_| ())
        .ok_or(format!("\"{key}\" must be a string"))
}

fn number(json: &Json, key: &str) -> Result<(), String> {
    json.get(key)
        .and_then(Value::as_f64)
        .map(|_| ())
        .ok_or(format!("\"{key}\" must be a number"))
}

fn one_of(json: &Json, key: &str, options: &[&str], required: bool) -> Result<(), String> {
    match json.get(key) {
        None if !required => Ok(()),
        Some(Value::String(value)) if options.contains(&value.as_str()) => Ok(()),
        _ => Err(format!("\"{key}\" must be one of {options:?}")),
    }
}

fn format_ok(json: &Json, key: &str) -> Result<(), String> {
    const NAMED: [&str; 6] = ["number", "compact", "percent", "bytes", "duration", "date"];
    match json.get(key) {
        None => Ok(()),
        Some(Value::String(name)) if NAMED.contains(&name.as_str()) => Ok(()),
        Some(Value::Object(currency))
            if currency.len() == 1
                && currency
                    .get("currency")
                    .and_then(Value::as_str)
                    .is_some_and(|code| {
                        code.len() == 3 && code.chars().all(|c| c.is_ascii_alphabetic())
                    }) =>
        {
            Ok(())
        }
        Some(other) => Err(format!("\"{key}\" is not a format: {other}")),
    }
}

/// `parse.ts` `action`: exactly one key, `send` or `open`, a non-empty string.
fn action_ok(json: &Json, key: &str, required: bool) -> Result<(), String> {
    let Some(value) = json.get(key) else {
        return if required {
            Err(format!("\"{key}\" is required"))
        } else {
            Ok(())
        };
    };
    let action = obj(value, key)?;
    let valid = action.len() == 1
        && ["send", "open"].iter().any(|k| {
            action
                .get(*k)
                .and_then(Value::as_str)
                .is_some_and(|text| !text.trim().is_empty())
        });
    if !valid {
        return Err(format!("\"{key}\" is not an action: {value}"));
    }
    if let Some(target) = action.get("open").and_then(Value::as_str) {
        if target.contains("://") && !target.starts_with("https://") {
            return Err(format!("only https links open: {target}"));
        }
    }
    Ok(())
}

fn items<'a>(json: &'a Json, key: &str, limit: usize) -> Result<Vec<&'a Json>, String> {
    let all = list(json, key)?;
    if all.is_empty() || all.len() > limit {
        return Err(format!("\"{key}\" must have 1 to {limit} entries"));
    }
    all.iter().map(|item| obj(item, key)).collect()
}

/// The checks of `apps/desktop/src/lib/ui-blocks/parse.ts` for a block that
/// draws with nothing dropped and no limit hit.
fn check_block(source: &str) -> Result<&'static str, String> {
    let json: Value = serde_json::from_str(source).map_err(|e| format!("not JSON: {e}"))?;
    let json = obj(&json, "the block")?;
    if json.get("v") != Some(&json!(1)) {
        return Err("\"v\" must be 1".into());
    }
    let ty = json.get("type").and_then(Value::as_str).unwrap_or("");
    let ty = COMPONENTS
        .into_iter()
        .filter(|name| *name != "formats")
        .find(|name| *name == ty)
        .ok_or(format!("unknown type {ty:?}"))?;
    for key in ["title", "source"] {
        if json.contains_key(key) {
            string(json, key)?;
        }
    }
    match ty {
        "stats" => {
            for item in items(json, "items", 8)? {
                string(item, "label")?;
                if !(item
                    .get("value")
                    .is_some_and(|v| v.is_number() || v.is_string()))
                {
                    return Err("a tile's value must be a number or a string".into());
                }
                format_ok(item, "format")?;
                format_ok(item, "deltaFormat")?;
                if item.contains_key("delta") {
                    number(item, "delta")?;
                }
            }
        }
        "bars" => {
            format_ok(json, "format")?;
            for item in items(json, "items", 50)? {
                string(item, "label")?;
                number(item, "value")?;
                action_ok(item, "action", false)?;
            }
        }
        "chart" => {
            one_of(json, "kind", &["line", "bar", "area"], true)?;
            format_ok(json, "format")?;
            let x = list(json, "x")?;
            if x.is_empty() || !x.iter().all(|v| v.is_string() || v.is_number()) {
                return Err("\"x\" must be labels".into());
            }
            let series = items(json, "series", 6)?;
            if x.len() * series.len() > 1000 {
                return Err("over 1,000 points".into());
            }
            for one in series {
                string(one, "name")?;
                let values = list(one, "values")?;
                if values.len() != x.len() {
                    return Err("a series must have as many values as x".into());
                }
                if !values.iter().all(|v| v.is_number() || v.is_null()) {
                    return Err("values must be numbers or null".into());
                }
            }
        }
        "table" => {
            let columns = items(json, "columns", 12)?;
            for column in &columns {
                string(column, "key")?;
                format_ok(column, "format")?;
                one_of(column, "align", &["left", "right"], false)?;
            }
            for row in items(json, "rows", 500)? {
                action_ok(row, "action", false)?;
                for column in &columns {
                    let key = column["key"].as_str().unwrap();
                    if !row
                        .get(key)
                        .is_none_or(|v| v.is_string() || v.is_number() || v.is_null())
                    {
                        return Err(format!("cell {key} must be a string, number or null"));
                    }
                }
            }
        }
        "steps" => {
            for item in items(json, "items", usize::MAX)? {
                string(item, "title")?;
                one_of(
                    item,
                    "status",
                    &["done", "current", "todo", "failed", "skipped"],
                    false,
                )?;
            }
        }
        "callout" => {
            one_of(json, "tone", &["info", "success", "warning", "error"], true)?;
            string(json, "text")?;
        }
        "actions" => {
            for item in items(json, "items", 4)? {
                string(item, "label")?;
                action_ok(item, "action", true)?;
            }
        }
        _ => unreachable!(),
    }
    Ok(ty)
}

#[test]
fn every_ui_guide_example_is_a_valid_block() {
    let mut types = Vec::new();
    for name in COMPONENTS {
        let found = examples(&lf(guide_of(name).unwrap()));
        assert_eq!(found.len(), 1, "{name}");
        let ty = check_block(&found[0]).unwrap_or_else(|error| panic!("{name}: {error}"));
        if name != "formats" {
            assert_eq!(ty, name, "the {name} guide shows another component");
        }
        types.push(ty);
    }
    // The checker itself refuses what the parser refuses.
    for bad in [
        r#"{"type":"stats","items":[{"label":"a","value":1}]}"#,
        r#"{"v":1,"type":"sparkle"}"#,
        r#"{"v":1,"type":"chart","kind":"line","x":["a","b"],"series":[{"name":"s","values":[1]}]}"#,
        r#"{"v":1,"type":"bars","items":[{"label":"a","value":1,"action":{"run":"x"}}]}"#,
        r#"{"v":1,"type":"stats","items":[{"label":"a","value":1}"#,
    ] {
        assert!(check_block(bad).is_err(), "{bad}");
    }
}

#[test]
fn the_index_is_one_rule_per_dash_line() {
    let rules = index_rules();
    assert_eq!(rules.len(), 5, "{rules:#?}");
    assert!(rules[0].starts_with("When a reply gives three or more related values"));
    assert!(
        rules[0].contains("\n  {\"v\":1"),
        "continuations keep their indent"
    );
    assert!(rules
        .iter()
        .all(|rule| !rule.starts_with("- ") && !rule.contains('\r')));
    assert!(rules.iter().any(|rule| rule.contains("ui_guide")));
}

#[tokio::test]
async fn ui_guide_tool_returns_text_and_details() {
    let tool = create_ui_guide_tool();
    assert_eq!(tool.schema().name, TOOL);
    assert!(matches!(tool.execution_mode(), ToolExecutionMode::Parallel));
    let result = tool
        .execute(
            "id",
            json!({"components": ["bars", "nope"]}),
            CancellationToken::new(),
            Arc::new(|_| {}),
        )
        .await
        .unwrap();
    let TextContentOrImage::Text(text) = &result.content[0] else {
        panic!()
    };
    assert!(text.text.contains("## bars") && text.text.ends_with("Unknown: nope"));
    assert_eq!(result.details, json!({"components": ["bars"]}));
    for bad in [
        json!({}),
        json!({"components": 3}),
        json!({"components": [1]}),
    ] {
        assert!(tool
            .execute("id", bad, CancellationToken::new(), Arc::new(|_| {}))
            .await
            .is_err());
    }
}

#[test]
fn ui_guide_is_registered_for_every_session_as_a_safe_read() {
    use evo_tools::{ExecutionEnv, MutatingEnv, OsExecutionEnv};
    let env = Arc::new(OsExecutionEnv::with_cwd(std::path::PathBuf::from(".")));
    let env_dyn: Arc<dyn ExecutionEnv> = env.clone();
    let mut_env: Arc<dyn MutatingEnv> = env;
    let context = evo_tools::ExecutionToolContext::new(env_dyn, Some(mut_env));
    let all = crate::tools::register_all(&context, &Default::default());
    let (_, tool) = all
        .iter()
        .find(|(name, _)| *name == TOOL)
        .expect("ui_guide is registered");
    assert_eq!(tool.replay, ToolReplay::Safe);
    let row = crate::tools::info::info(TOOL).expect("ui_guide has a row");
    assert_eq!(row.class, crate::tools::info::Class::Read);
    assert!(row.read_only_safe);
    assert!(!row.reserved);
    assert_eq!(
        crate::approval::classify(TOOL, &json!({"components": ["stats"]})),
        crate::approval::Kind::Read
    );
}
