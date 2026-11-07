//! `browser_mock`: block, delay or answer the requests a page makes
//! (docs/features/agent-browser FR-14, FR-22, T24), through the DevTools
//! `Fetch` domain (Windows).
//!
//! The rules of a tab live here (module state). `Fetch.enable` pauses only the
//! requests that may match a rule; a small task per tab reads the
//! `Fetch.requestPaused` events and answers each: `Fetch.failRequest`,
//! `Fetch.continueRequest` (after the delay) or `Fetch.fulfillRequest`.
//! Matching (glob, first match wins) is a pure function.
//!
//! When mocks end. The call's cancel token is NOT the chat's turn, so mocks do
//! not end with the turn. They end when the tab navigates to another origin
//! than the one they were set on, on `clear`, when the tab closes, and after
//! 20 minutes without a `browser_mock` call (so a forgotten mock cannot hold a
//! page's requests forever). Setting one asks the person; clearing and listing
//! never do.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::AppHandle;

use super::driver::{cut_chars, finish, gate_page, resolve_page, Page, Stop, ToolResult};
use crate::agent_tools::gate::origin_of;
use crate::agent_tools::{BoxFuture, Class, Tool, ToolCtx, ToolOutput};
use crate::browser as shell;

/// Mocks end this long after the last `browser_mock` call.
const TTL: Duration = Duration::from_secs(20 * 60);
const MAX_RULES: usize = 50;
const MAX_DELAY_MS: u64 = 30_000;
const MAX_BODY: usize = 1_000_000;
const POLL: Duration = Duration::from_millis(40);

// ── Rules (pure) ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Block,
    Delay(u64),
    Respond { status: u16, body: String, content_type: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub pattern: String,
    pub action: Action,
    /// Requests this rule answered.
    pub hits: u64,
}

/// What to do with a paused request.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Fail,
    /// Continue after this many milliseconds (0: at once).
    Continue(u64),
    Fulfill { status: u16, body: String, content_type: String },
}

/// `*` matches any text (also `/`), `?` one character. The whole of `text` must match.
pub fn glob_matches(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut pi, mut ti) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || (p[pi] != '*' && p[pi] == t[ti])) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ti));
            pi += 1;
        } else if let Some((star_pi, star_ti)) = star {
            // Let the last `*` take one more character.
            pi = star_pi + 1;
            ti = star_ti + 1;
            star = Some((star_pi, star_ti + 1));
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

fn has_wildcard(pattern: &str) -> bool {
    pattern.contains(['*', '?'])
}

/// Whether `pattern` matches the request URL: a glob over the whole URL (or the URL without its
/// query and fragment); a pattern without wildcards matches any URL containing it.
pub fn pattern_matches(pattern: &str, url: &str) -> bool {
    if !has_wildcard(pattern) {
        return url.contains(pattern);
    }
    let bare = url.split(['?', '#']).next().unwrap_or(url);
    glob_matches(pattern, url) || glob_matches(pattern, bare)
}

/// The first rule that matches (rules keep the order they were added in).
pub fn first_match<'a>(rules: &'a [Rule], url: &str) -> Option<(usize, &'a Rule)> {
    rules.iter().enumerate().find(|(_, rule)| pattern_matches(&rule.pattern, url))
}

pub fn decide(rules: &[Rule], url: &str) -> (Option<usize>, Verdict) {
    match first_match(rules, url) {
        None => (None, Verdict::Continue(0)),
        Some((index, rule)) => (
            Some(index),
            match &rule.action {
                Action::Block => Verdict::Fail,
                Action::Delay(ms) => Verdict::Continue(*ms),
                Action::Respond { status, body, content_type } => Verdict::Fulfill { status: *status, body: body.clone(), content_type: content_type.clone() },
            },
        ),
    }
}

/// The pattern `Fetch.enable` pauses requests for: wide enough to catch what the rule matches
/// (a query string after the pattern, any text around a plain one); `decide` makes the final call.
pub fn fetch_pattern(pattern: &str) -> String {
    if !has_wildcard(pattern) {
        format!("*{pattern}*")
    } else if pattern.ends_with('*') {
        pattern.to_string()
    } else {
        format!("{pattern}*")
    }
}

pub fn fetch_patterns(rules: &[Rule]) -> Vec<Value> {
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for rule in rules {
        let pattern = fetch_pattern(&rule.pattern);
        if !seen.contains(&pattern) {
            out.push(json!({ "urlPattern": pattern, "requestStage": "Request" }));
            seen.push(pattern);
        }
    }
    out
}

/// A JSON-looking body is `application/json`, anything else plain text.
pub fn default_content_type(body: &str) -> &'static str {
    let start = body.trim_start();
    if (start.starts_with('{') || start.starts_with('[')) && serde_json::from_str::<Value>(body).is_ok() {
        "application/json"
    } else {
        "text/plain; charset=utf-8"
    }
}

/// The headers of a mocked response: the type, no caching, and open to any page (so a mocked API
/// answers a cross-origin fetch of the page).
pub fn response_headers(content_type: &str) -> Value {
    json!([
        { "name": "Content-Type", "value": content_type },
        { "name": "Access-Control-Allow-Origin", "value": "*" },
        { "name": "Cache-Control", "value": "no-store" },
    ])
}

/// Read a block, delay or respond call into a rule.
pub fn parse_rule(action: &str, args: &Value) -> Result<Rule, String> {
    let pattern = args.get("pattern").and_then(Value::as_str).map(str::trim).unwrap_or("");
    if pattern.is_empty() {
        return Err(format!("{action} needs a pattern: a URL glob such as */api/* or https://cdn.test/*.png."));
    }
    let action = match action {
        "block" => Action::Block,
        "delay" => {
            let ms = match args.get("delayMs") {
                None | Some(Value::Null) => 1000.0,
                Some(value) => value.as_f64().filter(|n| n.is_finite() && *n >= 0.0).ok_or("delayMs must be a number of milliseconds (0 or more).")?,
            };
            Action::Delay((ms as u64).min(MAX_DELAY_MS))
        }
        "respond" => {
            let status = match args.get("status") {
                None | Some(Value::Null) => 200,
                Some(value) => value.as_u64().filter(|s| (100..600).contains(s)).ok_or("status must be an HTTP status code (100 to 599).")? as u16,
            };
            let body = args.get("body").and_then(Value::as_str).unwrap_or("").to_string();
            if body.len() > MAX_BODY {
                return Err(format!("body is too long ({} bytes; at most {MAX_BODY}).", body.len()));
            }
            let content_type = match args.get("contentType").and_then(Value::as_str).map(str::trim).filter(|c| !c.is_empty()) {
                Some(content_type) => content_type.to_string(),
                None => default_content_type(&body).to_string(),
            };
            Action::Respond { status, body, content_type }
        }
        other => return Err(format!("Unknown action \"{other}\": use block, delay, respond, list or clear.")),
    };
    Ok(Rule { pattern: pattern.to_string(), action, hits: 0 })
}

/// Add a rule; one with the same pattern is replaced where it stands (so the order is kept).
pub fn add_rule(rules: &mut Vec<Rule>, rule: Rule) -> Result<(), String> {
    match rules.iter().position(|r| r.pattern == rule.pattern) {
        Some(index) => rules[index] = rule,
        None if rules.len() >= MAX_RULES => return Err(format!("There are already {MAX_RULES} mock rules on this tab; clear some first.")),
        None => rules.push(rule),
    }
    Ok(())
}

/// Remove the rules of `pattern`; how many there were.
pub fn remove_pattern(rules: &mut Vec<Rule>, pattern: &str) -> usize {
    let before = rules.len();
    rules.retain(|rule| rule.pattern != pattern);
    before - rules.len()
}

pub fn describe_rule(rule: &Rule) -> String {
    let what = match &rule.action {
        Action::Block => "block".to_string(),
        Action::Delay(ms) => format!("delay {ms}ms"),
        Action::Respond { status, body, content_type } => format!("respond {status} {content_type} ({} characters)", body.chars().count()),
    };
    format!("{what}: {}", cut_chars(&rule.pattern, 200))
}

pub fn render_list(rules: &[Rule], tab: &str, origin: &str) -> String {
    if rules.is_empty() {
        return format!("No mocks on tab {tab}.");
    }
    let mut out = format!("Mocks on tab {tab} (site {origin}); the first matching rule wins:");
    for (index, rule) in rules.iter().enumerate() {
        out.push_str(&format!("\n{}. {} — {} hit{}", index + 1, describe_rule(rule), rule.hits, if rule.hits == 1 { "" } else { "s" }));
    }
    out.push_str("\nThey end when the tab goes to another site, on clear, or 20 minutes after the last browser_mock call.");
    out
}

// ── Per-tab state and the answering task ────────────────────────────────────

struct TabMocks {
    rules: Vec<Rule>,
    /// The origin the mocks were set on; leaving it clears them.
    origin: String,
    /// The task that answers this tab's paused requests.
    task: u64,
    deadline: Instant,
}

static MOCKS: Mutex<BTreeMap<String, TabMocks>> = Mutex::new(BTreeMap::new());
static TASKS: AtomicU64 = AtomicU64::new(0);

/// Stop mocking a tab: forget its rules and let its requests through.
async fn disable(app: &AppHandle, tab: &str) {
    MOCKS.lock().expect("mock rules lock").remove(tab);
    let _ = shell::cdp(app, tab, "Fetch.disable", json!({})).await;
}

fn spawn_task(app: AppHandle, tab: String, task: u64, mut cursor: u64) {
    tauri::async_runtime::spawn(async move {
        let names = vec!["Fetch.requestPaused".to_string()];
        loop {
            tokio::time::sleep(POLL).await;
            // The state this tick works with; a task that is no longer the tab's ends.
            let (rules, origin, expired) = {
                let mocks = MOCKS.lock().expect("mock rules lock");
                match mocks.get(&tab) {
                    Some(m) if m.task == task => (m.rules.clone(), m.origin.clone(), Instant::now() >= m.deadline),
                    _ => return,
                }
            };
            if shell::tab_record(&app, &tab).is_none() {
                MOCKS.lock().expect("mock rules lock").remove(&tab);
                return;
            }
            let here = origin_of(&shell::native_url(&app, &tab).unwrap_or_default());
            if expired || (!here.is_empty() && here != origin) {
                disable(&app, &tab).await;
                return;
            }
            let Ok(page) = shell::cdp_events(&app, &tab, cursor, Some(&names), Some(200)) else {
                MOCKS.lock().expect("mock rules lock").remove(&tab);
                return;
            };
            cursor = page.next;
            for event in page.events {
                answer(&app, &tab, task, &rules, &event.params).await;
            }
        }
    });
}

/// Answer one paused request.
async fn answer(app: &AppHandle, tab: &str, task: u64, rules: &[Rule], params: &Value) {
    use base64::Engine as _;
    let Some(id) = params.get("requestId").and_then(Value::as_str) else { return };
    let url = params.pointer("/request/url").and_then(Value::as_str).unwrap_or("");
    // A request paused at its response (not asked for) just goes on.
    let at_response = params.get("responseStatusCode").is_some() || params.get("responseErrorReason").is_some();
    let (index, verdict) = if at_response { (None, Verdict::Continue(0)) } else { decide(rules, url) };
    if let Some(index) = index {
        if let Some(m) = MOCKS.lock().expect("mock rules lock").get_mut(tab).filter(|m| m.task == task) {
            if let Some(rule) = m.rules.get_mut(index) {
                rule.hits += 1;
            }
        }
    }
    let (app, tab, id) = (app.clone(), tab.to_string(), id.to_string());
    match verdict {
        Verdict::Fail => {
            let _ = shell::cdp(&app, &tab, "Fetch.failRequest", json!({ "requestId": id, "errorReason": "BlockedByClient" })).await;
        }
        Verdict::Continue(0) => {
            let _ = shell::cdp(&app, &tab, "Fetch.continueRequest", json!({ "requestId": id })).await;
        }
        Verdict::Continue(ms) => {
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(ms)).await;
                let _ = shell::cdp(&app, &tab, "Fetch.continueRequest", json!({ "requestId": id })).await;
            });
        }
        Verdict::Fulfill { status, body, content_type } => {
            let body = base64::engine::general_purpose::STANDARD.encode(body.as_bytes());
            let _ = shell::cdp(
                &app,
                &tab,
                "Fetch.fulfillRequest",
                json!({ "requestId": id, "responseCode": status, "responseHeaders": response_headers(&content_type), "body": body }),
            )
            .await;
        }
    }
}

// ── The tool ────────────────────────────────────────────────────────────────

const MOCK: &str = "Block, delay or answer the requests a page makes until cleared, to test failures, slow networks and API responses. action: block (the request fails), delay (delayMs, default 1000, then it goes on), respond (answers with status, body and contentType without the network), list, clear (all rules, or only those of pattern). pattern is a URL glob: * is any text, ? one character; without wildcards it matches URLs containing it; the first matching rule wins. Mocks stay on the tab while it stays on the same site: they end when it navigates to another site, on clear, and 20 minutes after the last browser_mock call. They do not end when the chat's turn ends, so clear them when done. Asks the person before block, delay and respond.";

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": { "type": "string", "enum": ["block", "delay", "respond", "clear", "list"] },
            "pattern": { "type": "string", "description": "URL glob, e.g. */api/users* or https://cdn.test/*.png (block, delay, respond; optional for clear)." },
            "delayMs": { "type": "number", "description": "Delay in milliseconds (delay; default 1000, at most 30000)." },
            "status": { "type": "number", "description": "HTTP status of the answer (respond; default 200)." },
            "body": { "type": "string", "description": "Body of the answer (respond)." },
            "contentType": { "type": "string", "description": "Content type of the answer (respond; default application/json for JSON bodies, else text/plain)." },
            "tabId": { "type": "string", "description": "A tab id from browser_tabs; default: the active tab." },
        },
        "required": ["action"],
        "additionalProperties": false,
    })
}

fn run(ctx: ToolCtx, args: Value) -> BoxFuture<ToolOutput> {
    Box::pin(async move { finish(mock(&ctx, &args).await) })
}

pub fn tools() -> Vec<Tool> {
    vec![Tool { name: "browser_mock", description: MOCK, schema, class: Class::Change, run }]
}

async fn mock(ctx: &ToolCtx, args: &Value) -> ToolResult {
    let page = resolve_page(ctx, args)?;
    let action = args.get("action").and_then(Value::as_str).unwrap_or("");
    if !["block", "delay", "respond", "clear", "list"].contains(&action) {
        return Err(format!("Unknown action \"{action}\": use block, delay, respond, list or clear.").into());
    }
    if !cfg!(windows) {
        return Err("browser_mock is not available on this platform: request interception needs the DevTools protocol (Windows).".into());
    }
    match action {
        "list" => {
            let mut mocks = MOCKS.lock().expect("mock rules lock");
            let text = match mocks.get_mut(&page.tab) {
                Some(m) => {
                    m.deadline = Instant::now() + TTL;
                    render_list(&m.rules, &page.tab, &m.origin)
                }
                None => format!("No mocks on tab {}.", page.tab),
            };
            Ok(ToolOutput::text(text))
        }
        "clear" => clear(&page, args.get("pattern").and_then(Value::as_str).map(str::trim).filter(|p| !p.is_empty())).await,
        _ => {
            let rule = parse_rule(action, args).map_err(Stop::from)?;
            gate_page(ctx, &page, &format!("Mock requests: {}", describe_rule(&rule))).await?;
            set(&page, rule).await
        }
    }
}

async fn clear(page: &Page, pattern: Option<&str>) -> ToolResult {
    let (removed, remaining) = {
        let mut mocks = MOCKS.lock().expect("mock rules lock");
        let Some(m) = mocks.get_mut(&page.tab) else {
            return Ok(ToolOutput::text(format!("No mocks on tab {}.", page.tab)));
        };
        let removed = match pattern {
            Some(pattern) => remove_pattern(&mut m.rules, pattern),
            None => std::mem::take(&mut m.rules).len(),
        };
        m.deadline = Instant::now() + TTL;
        (removed, m.rules.clone())
    };
    if remaining.is_empty() {
        disable(&page.app, &page.tab).await;
        return Ok(ToolOutput::text(format!("Cleared {removed} mock rule{}; requests go through as normal.", if removed == 1 { "" } else { "s" })));
    }
    page.cdp("Fetch.enable", json!({ "patterns": fetch_patterns(&remaining) })).await?;
    Ok(ToolOutput::text(format!("Cleared {removed} rule{} of {}; {} still active.", if removed == 1 { "" } else { "s" }, pattern.unwrap_or("*"), remaining.len())))
}

async fn set(page: &Page, rule: Rule) -> ToolResult {
    shell::cdp_subscribe(&page.app, &page.tab, vec!["Fetch.requestPaused".to_string()]).await?;
    // Start reading after what the ring already holds: older events were answered or are gone.
    let cursor = shell::cdp_events(&page.app, &page.tab, 0, Some(&["-".to_string()]), Some(1)).map(|p| p.next).unwrap_or(1);
    let origin = origin_of(&page.native_url());
    let (patterns, new_task, summary) = {
        let mut mocks = MOCKS.lock().expect("mock rules lock");
        let created = !mocks.contains_key(&page.tab);
        let m = mocks.entry(page.tab.clone()).or_insert_with(|| TabMocks {
            rules: Vec::new(),
            origin: origin.clone(),
            task: TASKS.fetch_add(1, Ordering::Relaxed) + 1,
            deadline: Instant::now() + TTL,
        });
        let summary = describe_rule(&rule);
        if let Err(error) = add_rule(&mut m.rules, rule) {
            if created {
                mocks.remove(&page.tab);
            }
            return Err(error.into());
        }
        m.deadline = Instant::now() + TTL;
        (fetch_patterns(&m.rules), created.then_some(m.task), summary)
    };
    if let Err(error) = page.cdp("Fetch.enable", json!({ "patterns": patterns })).await {
        disable(&page.app, &page.tab).await;
        return Err(format!("The browser refused request interception: {error}").into());
    }
    if let Some(task) = new_task {
        spawn_task(page.app.clone(), page.tab.clone(), task, cursor);
    }
    Ok(ToolOutput::text(format!(
        "Mock set on tab {} — {summary}. It applies to requests from now on (a request already sent is not affected) until browser_mock clear, a navigation to another site, or 20 minutes without a browser_mock call.",
        page.tab
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(pattern: &str, action: Action) -> Rule {
        Rule { pattern: pattern.into(), action, hits: 0 }
    }

    #[test]
    fn globs_match_the_whole_text() {
        assert!(glob_matches("*", ""));
        assert!(glob_matches("*", "anything/at/all"));
        assert!(glob_matches("https://a.test/*", "https://a.test/x/y?z=1"));
        assert!(glob_matches("*.png", "https://a.test/img/logo.png"));
        assert!(!glob_matches("*.png", "https://a.test/img/logo.png?v=2"));
        assert!(glob_matches("*/api/*/items", "https://a.test/api/v2/items"));
        assert!(!glob_matches("*/api/*/items", "https://a.test/api/v2/items/9"));
        assert!(glob_matches("a?c", "abc") && !glob_matches("a?c", "ac") && !glob_matches("a?c", "abbc"));
        assert!(glob_matches("a*b*c", "aXXbYYc") && glob_matches("a*b*c", "abc") && !glob_matches("a*b*c", "aXXbYY"));
        assert!(glob_matches("**", "x") && glob_matches("a**b", "ab"));
        assert!(!glob_matches("a", "ab") && !glob_matches("ab", "a") && glob_matches("", "") && !glob_matches("", "a"));
        // Backtracking: the first `*` must not take too much.
        assert!(glob_matches("*aab", "aaaab"));
        assert!(glob_matches("é*ü", "éxxü"));
    }

    #[test]
    fn a_plain_pattern_matches_urls_containing_it_and_a_glob_ignores_the_query() {
        assert!(pattern_matches("/api/users", "https://a.test/api/users?id=3"));
        assert!(!pattern_matches("/api/users", "https://a.test/api/items"));
        assert!(pattern_matches("*.png", "https://a.test/logo.png?v=2"));
        assert!(pattern_matches("*.png", "https://a.test/logo.png#x"));
        assert!(!pattern_matches("*.png", "https://a.test/logo.jpg"));
        assert!(pattern_matches("https://a.test/api/*", "https://a.test/api/users"));
    }

    #[test]
    fn the_first_matching_rule_wins() {
        let rules = vec![
            rule("*/api/slow*", Action::Delay(500)),
            rule("*/api/*", Action::Respond { status: 503, body: "{}".into(), content_type: "application/json".into() }),
            rule("*.png", Action::Block),
        ];
        let (index, verdict) = decide(&rules, "https://a.test/api/slow/1");
        assert_eq!((index, verdict), (Some(0), Verdict::Continue(500)));
        let (index, verdict) = decide(&rules, "https://a.test/api/users");
        assert_eq!(index, Some(1));
        assert_eq!(verdict, Verdict::Fulfill { status: 503, body: "{}".into(), content_type: "application/json".into() });
        assert_eq!(decide(&rules, "https://a.test/logo.png"), (Some(2), Verdict::Fail));
        assert_eq!(decide(&rules, "https://a.test/index.html"), (None, Verdict::Continue(0)));
        assert_eq!(decide(&[], "https://a.test/"), (None, Verdict::Continue(0)));
    }

    #[test]
    fn fetch_patterns_are_wide_enough_for_the_rules() {
        assert_eq!(fetch_pattern("/api/users"), "*/api/users*");
        assert_eq!(fetch_pattern("*.png"), "*.png*");
        assert_eq!(fetch_pattern("https://a.test/*"), "https://a.test/*");
        let rules = vec![rule("*.png", Action::Block), rule("*.png*", Action::Block), rule("/x", Action::Block)];
        let patterns = fetch_patterns(&rules);
        assert_eq!(patterns.len(), 2, "equal patterns are listed once");
        assert_eq!(patterns[0], json!({ "urlPattern": "*.png*", "requestStage": "Request" }));
    }

    #[test]
    fn a_response_gets_a_type_and_open_headers() {
        assert_eq!(default_content_type("{\"a\":1}"), "application/json");
        assert_eq!(default_content_type(" [1,2]"), "application/json");
        assert_eq!(default_content_type("{not json"), "text/plain; charset=utf-8");
        assert_eq!(default_content_type("hello"), "text/plain; charset=utf-8");
        assert_eq!(default_content_type(""), "text/plain; charset=utf-8");
        let headers = response_headers("text/html");
        assert_eq!(headers[0], json!({ "name": "Content-Type", "value": "text/html" }));
        assert_eq!(headers[1]["name"], "Access-Control-Allow-Origin");
    }

    #[test]
    fn rules_are_read_from_the_arguments() {
        assert_eq!(parse_rule("block", &json!({ "pattern": " *.png " })).unwrap(), rule("*.png", Action::Block));
        assert_eq!(parse_rule("delay", &json!({ "pattern": "/api" })).unwrap().action, Action::Delay(1000));
        assert_eq!(parse_rule("delay", &json!({ "pattern": "/api", "delayMs": 250 })).unwrap().action, Action::Delay(250));
        assert_eq!(parse_rule("delay", &json!({ "pattern": "/api", "delayMs": 999_999 })).unwrap().action, Action::Delay(30_000));
        assert_eq!(
            parse_rule("respond", &json!({ "pattern": "/api", "status": 404, "body": "{\"error\":\"gone\"}" })).unwrap().action,
            Action::Respond { status: 404, body: "{\"error\":\"gone\"}".into(), content_type: "application/json".into() }
        );
        assert_eq!(
            parse_rule("respond", &json!({ "pattern": "/x", "contentType": "text/html", "body": "<b>x</b>" })).unwrap().action,
            Action::Respond { status: 200, body: "<b>x</b>".into(), content_type: "text/html".into() }
        );
        assert!(parse_rule("block", &json!({})).unwrap_err().starts_with("block needs a pattern"));
        assert!(parse_rule("block", &json!({ "pattern": "  " })).is_err());
        assert!(parse_rule("delay", &json!({ "pattern": "/a", "delayMs": -1 })).is_err());
        assert!(parse_rule("respond", &json!({ "pattern": "/a", "status": 99 })).is_err());
        assert!(parse_rule("respond", &json!({ "pattern": "/a", "status": "ok" })).is_err());
        assert!(parse_rule("respond", &json!({ "pattern": "/a", "body": "x".repeat(MAX_BODY + 1) })).is_err());
        assert!(parse_rule("explode", &json!({ "pattern": "/a" })).unwrap_err().starts_with("Unknown action"));
    }

    #[test]
    fn adding_replaces_the_same_pattern_in_place_and_is_bounded() {
        let mut rules = Vec::new();
        add_rule(&mut rules, rule("a", Action::Block)).unwrap();
        add_rule(&mut rules, rule("b", Action::Block)).unwrap();
        add_rule(&mut rules, rule("a", Action::Delay(5))).unwrap();
        assert_eq!(rules.iter().map(|r| r.pattern.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(rules[0].action, Action::Delay(5));
        for i in 0..(MAX_RULES - 2) {
            add_rule(&mut rules, rule(&format!("p{i}"), Action::Block)).unwrap();
        }
        assert_eq!(rules.len(), MAX_RULES);
        assert!(add_rule(&mut rules, rule("one more", Action::Block)).is_err());
        add_rule(&mut rules, rule("a", Action::Block)).unwrap();
        assert_eq!(remove_pattern(&mut rules, "a"), 1);
        assert_eq!(remove_pattern(&mut rules, "a"), 0);
        assert_eq!(rules.len(), MAX_RULES - 1);
    }

    #[test]
    fn the_list_reads_in_order_with_hits() {
        assert_eq!(render_list(&[], "browser-1", "https://a.test"), "No mocks on tab browser-1.");
        let mut rules = vec![rule("*.png", Action::Block), rule("/api", Action::Respond { status: 500, body: "boom".into(), content_type: "text/plain".into() })];
        rules[0].hits = 3;
        rules[1].hits = 1;
        let text = render_list(&rules, "browser-1", "https://a.test");
        assert!(text.starts_with("Mocks on tab browser-1 (site https://a.test); the first matching rule wins:\n1. block: *.png — 3 hits\n2. respond 500 text/plain (4 characters): /api — 1 hit\n"), "{text}");
        assert!(text.contains("20 minutes"));
    }

    #[test]
    fn the_tool_is_a_change_with_a_closed_schema() {
        let tool = &tools()[0];
        assert_eq!((tool.name, tool.class), ("browser_mock", Class::Change));
        let schema = (tool.schema)();
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["required"], json!(["action"]));
        assert_eq!(schema["properties"]["action"]["enum"], json!(["block", "delay", "respond", "clear", "list"]));
        assert_eq!(schema["properties"]["tabId"]["type"], "string");
        assert!(tool.description.contains("do not end when the chat's turn ends"));
    }
}
