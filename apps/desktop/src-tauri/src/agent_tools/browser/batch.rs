//! `browser_batch` (docs/features/agent-browser FR-7, T19): run up to 20 steps
//! (any browser tool but itself) in order and return each result.
//!
//! Every step goes through [`call_tool`], so it is gated by its own tool's class
//! and runs with the call's chat and cancellation. A step that fails ends the
//! batch; so does a stopped turn. The sequencing ([`run_steps`]) takes the way
//! of running one step as a closure, which is how it is tested.

use std::collections::HashSet;
use std::future::Future;

use serde_json::{json, Value};

use super::driver::{cut_chars, finish, tab_id_arg, ToolResult};
use crate::agent_tools::{call_tool, BoxFuture, Class, Content, Tool, ToolCtx, ToolOutput};

const BATCH: &str = "Run several browser tools in one call, in order, and return every result: steps is a list of 1 to 20 {tool, args} (any browser_* tool except browser_batch, with that tool's own arguments). Each step is approved by its own tool as usual. It stops at the first step that fails, or when the turn is stopped, and says so; steps without a tabId use the batch's tabId. Only the last picture (screenshot) is kept. The combined text is cut at 20,000 characters. Use it for a known sequence (navigate, type, click, read); look at the page between steps when the next one depends on what is there.";

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "steps": {
                "type": "array",
                "minItems": 1,
                "maxItems": 20,
                "description": "The steps, in order.",
                "items": {
                    "type": "object",
                    "properties": {
                        "tool": { "type": "string", "description": "A browser tool name, e.g. browser_click." },
                        "args": { "type": "object", "description": "That tool's arguments." },
                    },
                    "required": ["tool"],
                    "additionalProperties": false,
                },
            },
            "tabId": { "type": "string", "description": "A tab id from browser_tabs for the steps that name none; default: the active tab." },
        },
        "required": ["steps"],
        "additionalProperties": false,
    })
}

fn run(ctx: ToolCtx, args: Value) -> BoxFuture<ToolOutput> {
    Box::pin(async move { finish(batch(&ctx, &args).await) })
}

/// The tool of this module.
pub fn tools() -> Vec<Tool> {
    vec![Tool { name: "browser_batch", description: BATCH, schema, class: Class::Act, run }]
}

// ── Arguments ───────────────────────────────────────────────────────────────

pub const MAX_STEPS: usize = 20;
/// The combined text of a batch is at most this long (NFR-2).
pub const MAX_TEXT: usize = 20_000;

#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub tool: String,
    pub args: Value,
}

/// Validate `steps`: 1 to 20 browser tools that exist, none of them a batch. A step with no `tabId`
/// gets the batch's.
pub fn parse_steps(args: &Value, known: &dyn Fn(&str) -> bool) -> Result<Vec<Step>, String> {
    let Some(Value::Array(items)) = args.get("steps") else {
        return Err("browser_batch needs steps: a list of {tool, args}.".into());
    };
    if items.is_empty() || items.len() > MAX_STEPS {
        return Err(format!("steps must hold 1 to {MAX_STEPS} steps, not {}.", items.len()));
    }
    let batch_tab = tab_id_arg(args);
    let mut steps = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let n = index + 1;
        let Some(tool) = item.get("tool").and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()) else {
            return Err(format!("Step {n} needs a tool name."));
        };
        if tool == "browser_batch" {
            return Err(format!("Step {n}: a batch cannot hold a batch."));
        }
        if !tool.starts_with("browser_") {
            return Err(format!("Step {n}: {tool} is not a browser tool; only browser_* tools can be batched."));
        }
        if !known(tool) {
            return Err(format!("Step {n}: unknown tool {tool}."));
        }
        let mut step_args = match item.get("args") {
            None | Some(Value::Null) => json!({}),
            Some(value @ Value::Object(_)) => value.clone(),
            Some(_) => return Err(format!("Step {n}: args must be an object.")),
        };
        if let (Some(tab), None) = (&batch_tab, tab_id_arg(&step_args)) {
            step_args["tabId"] = json!(tab);
        }
        steps.push(Step { tool: tool.to_string(), args: step_args });
    }
    Ok(steps)
}

// ── Running the steps ───────────────────────────────────────────────────────

/// What one step did, as the batch keeps it.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub tool: String,
    pub text: String,
    pub error: bool,
    pub has_image: bool,
}

/// How a batch went.
#[derive(Debug)]
pub struct Report {
    pub entries: Vec<Entry>,
    /// The last picture any step returned.
    pub image: Option<Content>,
    /// The number of steps asked for.
    pub total: usize,
    /// The batch ended because the turn was stopped.
    pub stopped: bool,
}

impl Report {
    /// The step (1-based) that failed, if one did.
    pub fn failed_at(&self) -> Option<usize> {
        self.entries.iter().position(|entry| entry.error).map(|i| i + 1)
    }
}

/// Run `steps` in order with `run_one`, stopping at the first failing step or when `cancelled()`.
pub async fn run_steps<F, Fut>(steps: &[Step], cancelled: impl Fn() -> bool, mut run_one: F) -> Report
where
    F: FnMut(Step) -> Fut,
    Fut: Future<Output = ToolOutput>,
{
    let mut report = Report { entries: Vec::with_capacity(steps.len()), image: None, total: steps.len(), stopped: false };
    for step in steps {
        if cancelled() {
            report.stopped = true;
            break;
        }
        let output = run_one(step.clone()).await;
        let mut texts = Vec::new();
        let mut has_image = false;
        for part in &output.content {
            match part {
                Content::Text(text) if !text.is_empty() => texts.push(text.as_str()),
                Content::Text(_) => {}
                Content::Image { .. } => {
                    has_image = true;
                    report.image = Some(part.clone());
                }
            }
        }
        report.entries.push(Entry { tool: step.tool.clone(), text: texts.join("\n"), error: output.is_error, has_image });
        if output.is_error {
            break;
        }
        if cancelled() {
            report.stopped = true;
            break;
        }
    }
    report
}

/// `1. browser_navigate — Loaded …`, the lines of a longer result indented under it.
pub fn entry_text(number: usize, entry: &Entry, image_note: &str) -> String {
    let body = if entry.text.is_empty() { "(no output)" } else { entry.text.as_str() };
    let mut lines = body.lines();
    let first = lines.next().unwrap_or("");
    let mut out = format!("{number}. {} — {}{first}", entry.tool, if entry.error { "ERROR: " } else { "" });
    for line in lines {
        out.push_str("\n   ");
        out.push_str(line);
    }
    out.push_str(image_note);
    out
}

/// The closing line: how far the batch got.
pub fn summary_line(report: &Report) -> String {
    let ran = report.entries.len();
    if let Some(at) = report.failed_at() {
        let left = report.total - at;
        return if left == 0 {
            format!("Step {at} of {} failed.", report.total)
        } else {
            format!("Stopped at step {at} of {}: it failed, so the remaining {left} step{} did not run.", report.total, if left == 1 { "" } else { "s" })
        };
    }
    if report.stopped {
        return format!("Stopped: the turn was stopped after {ran} of {} steps.", report.total);
    }
    format!("All {} step{} ran.", report.total, if report.total == 1 { "" } else { "s" })
}

/// `body` and `summary` together within `max` characters: the body is cut, with a note, never the summary.
pub fn fit(body: &str, summary: &str, max: usize) -> String {
    let whole = format!("{body}\n{summary}");
    if whole.chars().count() <= max {
        return whole;
    }
    let note = format!("\n… (the output of the steps is cut at {max} characters)");
    let room = max.saturating_sub(note.chars().count() + summary.chars().count() + 1);
    format!("{}{note}\n{summary}", cut_chars(body, room))
}

/// The result of the whole batch: numbered entries, the summary, and the last picture.
pub fn render(report: &Report) -> ToolOutput {
    let last_image = report.entries.iter().rposition(|entry| entry.has_image);
    let body: Vec<String> = report
        .entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let note = match (entry.has_image, last_image == Some(i)) {
                (false, _) => "",
                (true, true) => "\n   (picture attached)",
                (true, false) => "\n   (picture not kept: only the last one is attached)",
            };
            entry_text(i + 1, entry, note)
        })
        .collect();
    let text = fit(&body.join("\n"), &summary_line(report), MAX_TEXT);
    let mut content = Vec::new();
    if let Some(image) = &report.image {
        content.push(image.clone());
    }
    content.push(Content::Text(text));
    let mut output = ToolOutput::text(String::new());
    output.content = content;
    output.is_error = report.failed_at().is_some();
    output
}

async fn batch(ctx: &ToolCtx, args: &Value) -> ToolResult {
    let names: HashSet<String> = crate::agent_tools::static_definitions().iter().filter_map(|d| d.get("name").and_then(Value::as_str).map(str::to_string)).collect();
    let steps = parse_steps(args, &|tool| names.contains(tool))?;
    let report = run_steps(&steps, || ctx.cancelled(), |step| async move { call_tool(ctx, &step.tool, step.args).await }).await;
    Ok(render(&report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    fn known(tool: &str) -> bool {
        tool.starts_with("browser_") && tool != "browser_nothing"
    }

    fn parse(args: Value) -> Result<Vec<Step>, String> {
        parse_steps(&args, &known)
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap().block_on(future)
    }

    #[test]
    fn steps_are_one_to_twenty_browser_tools() {
        assert!(parse(json!({})).unwrap_err().contains("needs steps"));
        assert!(parse(json!({ "steps": [] })).unwrap_err().contains("1 to 20"));
        let many: Vec<Value> = (0..21).map(|_| json!({ "tool": "browser_key" })).collect();
        assert!(parse(json!({ "steps": many })).unwrap_err().contains("not 21"));
        let twenty: Vec<Value> = (0..20).map(|_| json!({ "tool": "browser_key" })).collect();
        assert_eq!(parse(json!({ "steps": twenty })).unwrap().len(), 20);
        assert!(parse(json!({ "steps": [{}] })).unwrap_err().contains("Step 1 needs a tool name"));
        assert!(parse(json!({ "steps": [{ "tool": "browser_key" }, { "tool": "browser_batch" }] })).unwrap_err().contains("Step 2: a batch cannot hold a batch"));
        assert!(parse(json!({ "steps": [{ "tool": "terminal_run" }] })).unwrap_err().contains("not a browser tool"));
        assert!(parse(json!({ "steps": [{ "tool": "browser_nothing" }] })).unwrap_err().contains("unknown tool browser_nothing"));
        assert!(parse(json!({ "steps": [{ "tool": "browser_key", "args": "Enter" }] })).unwrap_err().contains("args must be an object"));
    }

    #[test]
    fn a_step_with_no_args_has_empty_args_and_inherits_the_batch_tab() {
        let steps = parse(json!({ "tabId": "browser-3", "steps": [
            { "tool": "browser_read_page" },
            { "tool": "browser_click", "args": { "ref": "ref_1" } },
            { "tool": "browser_key", "args": { "key": "Enter", "tabId": "browser-9" } },
        ] }))
        .unwrap();
        assert_eq!(steps[0].args, json!({ "tabId": "browser-3" }));
        assert_eq!(steps[1].args, json!({ "ref": "ref_1", "tabId": "browser-3" }));
        assert_eq!(steps[2].args["tabId"], "browser-9", "a step's own tab wins");
        let plain = parse(json!({ "steps": [{ "tool": "browser_read_page", "args": null }] })).unwrap();
        assert_eq!(plain[0].args, json!({}));
    }

    fn steps(names: &[&str]) -> Vec<Step> {
        names.iter().map(|tool| Step { tool: tool.to_string(), args: json!({}) }).collect()
    }

    fn ok(text: &str) -> ToolOutput {
        ToolOutput::text(text)
    }

    #[test]
    fn steps_run_in_order_and_each_result_is_kept() {
        let seen = RefCell::new(Vec::new());
        let report = block_on(run_steps(&steps(&["browser_navigate", "browser_type", "browser_click", "browser_read_page"]), || false, |step| {
            seen.borrow_mut().push(step.tool.clone());
            let text = format!("did {}", step.tool);
            async move { ok(&text) }
        }));
        assert_eq!(*seen.borrow(), ["browser_navigate", "browser_type", "browser_click", "browser_read_page"]);
        assert_eq!(report.entries.len(), 4);
        assert_eq!(report.failed_at(), None);
        let out = render(&report);
        assert!(!out.is_error);
        let Content::Text(text) = &out.content[0] else { panic!("text first when there is no picture") };
        assert!(text.starts_with("1. browser_navigate — did browser_navigate\n2. browser_type — did browser_type"));
        assert!(text.contains("4. browser_read_page — did browser_read_page"));
        assert!(text.ends_with("All 4 steps ran."));
    }

    #[test]
    fn the_first_failure_ends_the_batch() {
        let runs = Cell::new(0);
        let report = block_on(run_steps(&steps(&["browser_navigate", "browser_click", "browser_key", "browser_read_page"]), || false, |step| {
            runs.set(runs.get() + 1);
            let fail = step.tool == "browser_click";
            async move {
                if fail {
                    ToolOutput::error("ref_9 is no longer on the page")
                } else {
                    ok("fine")
                }
            }
        }));
        assert_eq!(runs.get(), 2, "the steps after the failure did not run");
        assert_eq!(report.failed_at(), Some(2));
        let out = render(&report);
        assert!(out.is_error);
        let Content::Text(text) = &out.content[0] else { panic!() };
        assert!(text.contains("2. browser_click — ERROR: ref_9 is no longer on the page"));
        assert!(text.ends_with("Stopped at step 2 of 4: it failed, so the remaining 2 steps did not run."));
        assert!(!text.contains("3. browser_key"));
    }

    #[test]
    fn a_failure_in_the_last_step_has_nothing_left_to_skip() {
        let report = block_on(run_steps(&steps(&["browser_navigate", "browser_click"]), || false, |step| {
            let fail = step.tool == "browser_click";
            async move { if fail { ToolOutput::error("no") } else { ok("fine") } }
        }));
        assert_eq!(summary_line(&report), "Step 2 of 2 failed.");
    }

    #[test]
    fn a_stopped_turn_ends_the_batch_between_steps() {
        let runs = Cell::new(0);
        let report = block_on(run_steps(&steps(&["browser_wait", "browser_click", "browser_key"]), || runs.get() >= 1, |_| {
            runs.set(runs.get() + 1);
            async { ok("waited") }
        }));
        assert_eq!(runs.get(), 1);
        assert!(report.stopped);
        assert_eq!(summary_line(&report), "Stopped: the turn was stopped after 1 of 3 steps.");
        assert!(!render(&report).is_error, "a stop is not a failure");
        let before = block_on(run_steps(&steps(&["browser_key"]), || true, |_| async { ok("never") }));
        assert!(before.entries.is_empty() && before.stopped);
    }

    fn picture(tag: &str) -> ToolOutput {
        ToolOutput::image(tag.to_string(), "image/jpeg", format!("shot {tag}"))
    }

    #[test]
    fn only_the_last_picture_is_kept() {
        let report = block_on(run_steps(&steps(&["browser_screenshot", "browser_click", "browser_screenshot"]), || false, |step| {
            let out = if step.tool == "browser_screenshot" { picture(&format!("p{}", step.tool.len())) } else { ok("clicked") };
            async move { out }
        }));
        let out = render(&report);
        assert_eq!(out.content.len(), 2);
        let Content::Image { data, mime_type } = &out.content[0] else { panic!("the picture comes first") };
        assert_eq!((data.as_str(), mime_type.as_str()), ("p18", "image/jpeg"));
        let Content::Text(text) = &out.content[1] else { panic!() };
        assert!(text.contains("1. browser_screenshot — shot p18\n   (picture not kept: only the last one is attached)"));
        assert!(text.contains("3. browser_screenshot — shot p18\n   (picture attached)"));
    }

    #[test]
    fn a_multi_line_result_is_indented_under_its_number() {
        let entry = Entry { tool: "browser_click".into(), text: "Clicked ref_1.\nNow: button \"Go\"\nPage: A — https://a.test/".into(), error: false, has_image: false };
        assert_eq!(entry_text(2, &entry, ""), "2. browser_click — Clicked ref_1.\n   Now: button \"Go\"\n   Page: A — https://a.test/");
        let empty = Entry { text: String::new(), ..entry };
        assert_eq!(entry_text(1, &empty, ""), "1. browser_click — (no output)");
    }

    #[test]
    fn the_combined_text_is_cut_but_the_summary_stays() {
        let big = "x".repeat(15_000);
        let report = block_on(run_steps(&steps(&["browser_read_page", "browser_read_page"]), || false, |_| {
            let text = big.clone();
            async move { ok(&text) }
        }));
        let out = render(&report);
        let Content::Text(text) = &out.content[0] else { panic!() };
        assert!(text.chars().count() <= 20_000, "{}", text.chars().count());
        assert!(text.contains("cut at 20000 characters"));
        assert!(text.ends_with("All 2 steps ran."));
        assert_eq!(fit("short", "done", 100), "short\ndone");
    }

    #[test]
    fn the_tool_is_an_act_that_takes_up_to_twenty_steps() {
        let tool = &tools()[0];
        assert_eq!((tool.name, tool.class), ("browser_batch", Class::Act));
        let schema = (tool.schema)();
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["properties"]["steps"]["maxItems"], 20);
        assert_eq!(schema["properties"]["steps"]["minItems"], 1);
        assert_eq!(schema["required"], json!(["steps"]));
    }
}
