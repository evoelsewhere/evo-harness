//! `preview_start`: start a project's dev server from `.evo/launch.json` in a
//! terminal tab, wait for its ready line, and open it in the browser.
//!
//! The file is read, never written. It holds a command the project (not the
//! person) chose, so nothing starts without the approval a terminal command
//! needs: the work is done by `terminal_run`, called in process, and its gate
//! shows the shell, folder and command in "Ask every time" and "Approve edits"
//! and stays silent in "Approve for me". The page is opened with
//! `browser_navigate`, which never asks for a local address. Both tools belong
//! to other groups and are looked up by name when the call runs.
//!
//! Spec: docs/features/agent-browser (FR-21), plan 3.2, 3.4.

use std::net::{SocketAddr, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::super::{call_tool, BoxFuture, Class, Content, Tool, ToolCtx, ToolOutput};
use super::optional_str;

/// Where a project keeps its launch configurations.
const LAUNCH_FILE: &str = ".evo/launch.json";
/// The largest launch file read.
const MAX_FILE_BYTES: u64 = 256 * 1024;
/// What a dev server usually prints when it can be used.
pub const DEFAULT_READY: &str = r"ready|listening|running|started|localhost:\d+";
/// How long to wait for the ready line.
pub const READY_TIMEOUT_MS: u64 = 60_000;
/// How long to wait for the port to accept connections once the ready line came.
const PORT_WAIT: Duration = Duration::from_secs(15);

const EXAMPLE: &str = r#"{
  "configurations": [
    { "name": "dev", "command": "npm run dev", "port": 5173 }
  ]
}"#;

/// One launch configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub name: String,
    pub command: String,
    /// A folder relative to the project (or absolute); default the project folder.
    pub cwd: Option<String>,
    pub port: Option<u16>,
    /// A regular expression the server prints when ready.
    pub ready: Option<String>,
    /// A shell id or name as `terminal_list` shows them.
    pub shell: Option<String>,
}

fn invalid(message: impl std::fmt::Display) -> String {
    format!("{LAUNCH_FILE} is not valid: {message}\nIt looks like this:\n{EXAMPLE}")
}

/// Read the configurations of a launch file's text.
pub fn parse_launch(text: &str) -> Result<Vec<Config>, String> {
    let root: Value = serde_json::from_str(text).map_err(|e| invalid(format!("not JSON ({e})")))?;
    let list = root
        .get("configurations")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("it needs a \"configurations\" array"))?;
    if list.is_empty() {
        return Err(invalid("\"configurations\" is empty"));
    }
    let mut configs: Vec<Config> = Vec::new();
    for (index, item) in list.iter().enumerate() {
        let at = format!("configuration {}", index + 1);
        let field = |key: &str| -> Result<Option<String>, String> {
            match item.get(key) {
                None | Some(Value::Null) => Ok(None),
                Some(Value::String(text)) if !text.trim().is_empty() => Ok(Some(text.trim().to_string())),
                Some(_) => Err(invalid(format!("{at}: \"{key}\" must be a non-empty string"))),
            }
        };
        let name = field("name")?.ok_or_else(|| invalid(format!("{at} needs a \"name\"")))?;
        let command = field("command")?.ok_or_else(|| invalid(format!("{at} (\"{name}\") needs a \"command\"")))?;
        let port = match item.get("port") {
            None | Some(Value::Null) => None,
            Some(value) => match value.as_u64().filter(|port| (1..=65535).contains(port)) {
                Some(port) => Some(port as u16),
                None => return Err(invalid(format!("\"{name}\": \"port\" must be a number from 1 to 65535"))),
            },
        };
        if configs.iter().any(|known| known.name == name) {
            return Err(invalid(format!("the name \"{name}\" is used twice")));
        }
        configs.push(Config { cwd: field("cwd")?, ready: field("ready")?, shell: field("shell")?, name, command, port });
    }
    Ok(configs)
}

/// The configuration called `name`, or the first.
pub fn pick<'a>(configs: &'a [Config], name: Option<&str>) -> Result<&'a Config, String> {
    match name.map(str::trim).filter(|name| !name.is_empty()) {
        None => configs.first().ok_or_else(|| invalid("\"configurations\" is empty")),
        Some(wanted) => configs.iter().find(|config| config.name == wanted).ok_or_else(|| {
            let names: Vec<&str> = configs.iter().map(|config| config.name.as_str()).collect();
            format!("{LAUNCH_FILE} has no configuration named \"{wanted}\". It has: {}.", names.join(", "))
        }),
    }
}

/// The folder a configuration runs in: the project folder, or `cwd` inside it
/// (a relative path may not climb out), or an absolute `cwd` as written.
pub fn resolve_cwd(folder: &Path, cwd: Option<&str>) -> Result<PathBuf, String> {
    let Some(cwd) = cwd else {
        return Ok(folder.to_path_buf());
    };
    let path = Path::new(cwd);
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    if path.components().any(|part| matches!(part, Component::ParentDir | Component::Prefix(_) | Component::RootDir)) {
        return Err(format!("\"cwd\" of this configuration (\"{cwd}\") must stay inside the project folder."));
    }
    Ok(folder.join(path))
}

/// The arguments of the `terminal_run` call that starts `config`.
pub fn terminal_args(config: &Config, folder: &Path) -> Result<Value, String> {
    let cwd = resolve_cwd(folder, config.cwd.as_deref())?;
    let mut args = json!({
        "command": config.command,
        "cwd": cwd.to_string_lossy(),
        "waitFor": config.ready.as_deref().unwrap_or(DEFAULT_READY),
        "timeoutMs": READY_TIMEOUT_MS,
    });
    if let Some(shell) = &config.shell {
        args["shell"] = json!(shell);
    }
    Ok(args)
}

/// The page address of a configuration's port.
pub fn page_url(port: u16) -> String {
    format!("http://localhost:{port}")
}

/// The text parts of a result.
fn text_of(output: &ToolOutput) -> String {
    output
        .content
        .iter()
        .filter_map(|part| match part {
            Content::Text(text) => Some(text.as_str()),
            Content::Image { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether the terminal's answer says the ready line never came.
pub fn timed_out(text: &str) -> bool {
    text.to_ascii_lowercase().contains("timed out")
}

/// The tab id in a terminal result ("Tab t3 · Git Bash …", "tabId: t3"), when there is one (best effort).
pub fn find_tab_id(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower[from..].find("tab") {
        let start = from + found;
        from = start + 3;
        // A word of its own (not "table"), optionally "Tab id"/"tabId", then ":" or spaces, then the id.
        let before_ok = start == 0 || !lower[..start].chars().next_back().is_some_and(|c| c.is_ascii_alphanumeric());
        if !before_ok {
            continue;
        }
        let mut rest = &text[from..];
        if rest.to_ascii_lowercase().starts_with("id") {
            rest = &rest[2..];
        } else if rest.starts_with(|c: char| c.is_ascii_alphanumeric()) {
            continue;
        }
        let id: String = rest.trim_start_matches([' ', ':', '=', '"', '\'']).chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
        // Tab ids carry a number ("t3", "7", "term-2"); this skips a word like "tab results".
        if id.chars().any(|c| c.is_ascii_digit()) {
            return Some(id);
        }
    }
    None
}

/// Whether something accepts connections on `port` of this computer (IPv4 or IPv6).
fn port_open(port: u16) -> bool {
    [SocketAddr::from(([127, 0, 0, 1], port)), SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], port))]
        .iter()
        .any(|addr| TcpStream::connect_timeout(addr, Duration::from_millis(300)).is_ok())
}

async fn wait_for_port(ctx: &ToolCtx, port: u16) -> bool {
    let deadline = Instant::now() + PORT_WAIT;
    loop {
        if tauri::async_runtime::spawn_blocking(move || port_open(port)).await.unwrap_or(false) {
            return true;
        }
        if ctx.cancelled() || Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
}

// ── The tool ────────────────────────────────────────────────────────────────

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string", "description": "A configuration of the project's .evo/launch.json; default the first." }
        },
        "additionalProperties": false
    })
}

/// A terminal or browser tool that another group registers; a clear message while it is not.
fn missing_tool(output: &ToolOutput, tool: &str) -> Option<ToolOutput> {
    (output.is_error && text_of(output).starts_with("Unknown tool")).then(|| ToolOutput::error(format!("{tool} is not available in this build yet, so preview_start cannot use it.")))
}

async fn run(ctx: ToolCtx, args: Value) -> ToolOutput {
    let name = match optional_str(&args, "name") {
        Ok(name) => name.map(str::to_string),
        Err(message) => return ToolOutput::error(message),
    };
    let folder = ctx.folder();
    let file = folder.join(LAUNCH_FILE);
    let text = match std::fs::metadata(&file) {
        Ok(meta) if meta.len() > MAX_FILE_BYTES => return ToolOutput::error(invalid(format!("it is larger than {} KB", MAX_FILE_BYTES / 1024))),
        Ok(_) => match std::fs::read_to_string(&file) {
            Ok(text) => text,
            Err(error) => return ToolOutput::error(format!("Could not read {}: {error}", file.display())),
        },
        Err(_) => {
            return ToolOutput::error(format!(
                "There is no {LAUNCH_FILE} in {}. Create it to describe the dev server (the person or you can write it); it looks like this:\n{EXAMPLE}",
                folder.display()
            ))
        }
    };
    let config = match parse_launch(&text).and_then(|configs| pick(&configs, name.as_deref()).cloned()) {
        Ok(config) => config,
        Err(message) => return ToolOutput::error(message),
    };
    let terminal_args = match terminal_args(&config, &folder) {
        Ok(args) => args,
        Err(message) => return ToolOutput::error(message),
    };

    // The terminal tool's gate asks the person (shell, folder, command) and may refuse.
    let started = call_tool(&ctx, "terminal_run", terminal_args).await;
    if let Some(missing) = missing_tool(&started, "terminal_run") {
        return missing;
    }
    if started.is_error || ctx.cancelled() {
        return started;
    }
    let started_text = text_of(&started);
    let tab = find_tab_id(&started_text).map(|id| format!(" (terminal tab {id})")).unwrap_or_default();
    let mut report = format!("Started \"{}\": {}{tab}\n{started_text}", config.name, config.command);

    let Some(port) = config.port else {
        report.push_str("\n\nNo port is set for this configuration, so no page was opened.");
        return ToolOutput::text(report);
    };
    if timed_out(&started_text) {
        report.push_str(&format!(
            "\n\nThe ready line did not appear within {} s, so the page was not opened. The tab keeps running: read it with terminal_read, then open {} with browser_navigate.",
            READY_TIMEOUT_MS / 1000,
            page_url(port)
        ));
        return ToolOutput::text(report);
    }
    if !wait_for_port(&ctx, port).await {
        if ctx.cancelled() {
            return ToolOutput::text("");
        }
        report.push_str(&format!(
            "\n\nNothing accepts connections on port {port} yet, so the page was not opened. Check the tab with terminal_read, then open {} with browser_navigate.",
            page_url(port)
        ));
        return ToolOutput::text(report);
    }
    let page = call_tool(&ctx, "browser_navigate", json!({ "url": page_url(port) })).await;
    if let Some(missing) = missing_tool(&page, "browser_navigate") {
        report.push_str(&format!("\n\nThe server is up, but {}", text_of(&missing)));
        return ToolOutput::text(report);
    }
    let heading = if page.is_error { "Opening the page failed" } else { "Page" };
    report.push_str(&format!("\n\n{heading}:\n{}", text_of(&page)));
    ToolOutput::text(report)
}

fn run_tool(ctx: ToolCtx, args: Value) -> BoxFuture<ToolOutput> {
    Box::pin(run(ctx, args))
}

pub fn tools() -> Vec<Tool> {
    vec![Tool {
        name: "preview_start",
        description: "Start the project's dev server from a saved launch configuration (`.evo/launch.json`: {\"configurations\": [{\"name\", \"command\", \"cwd\"?, \"port\"?, \"ready\"? (regular expression), \"shell\"?}]}) in a terminal tab, wait for its ready line (up to 60 s), then open http://localhost:<port> in the browser tool and return the tab id and the page summary. `name` picks a configuration (default the first). The command runs in the terminal tool, so it asks like terminal_run does; the file is never changed.",
        schema,
        // Runs a command: gated by the terminal tool it calls, once, with the command shown.
        class: Class::Change,
        run: run_tool,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"{ "configurations": [
        { "name": "web", "command": "npm run dev", "port": 5173 },
        { "name": "api", "command": "cargo run", "cwd": "server", "port": 8080, "ready": "listening on", "shell": "git-bash" }
    ] }"#;

    #[test]
    fn a_launch_file_parses_with_its_optional_fields() {
        let configs = parse_launch(FILE).unwrap();
        assert_eq!(configs.len(), 2);
        assert_eq!(configs[0], Config { name: "web".into(), command: "npm run dev".into(), cwd: None, port: Some(5173), ready: None, shell: None });
        assert_eq!(configs[1].cwd.as_deref(), Some("server"));
        assert_eq!(configs[1].ready.as_deref(), Some("listening on"));
        assert_eq!(configs[1].shell.as_deref(), Some("git-bash"));
        // A server with no port is allowed: nothing is opened.
        assert_eq!(parse_launch(r#"{"configurations":[{"name":"w","command":"watch"}]}"#).unwrap()[0].port, None);
    }

    #[test]
    fn a_bad_launch_file_says_what_is_wrong_and_shows_an_example() {
        for (text, expect) in [
            ("not json", "not JSON"),
            ("{}", "\"configurations\" array"),
            (r#"{"configurations":[]}"#, "empty"),
            (r#"{"configurations":[{"command":"x"}]}"#, "needs a \"name\""),
            (r#"{"configurations":[{"name":"a"}]}"#, "needs a \"command\""),
            (r#"{"configurations":[{"name":"a","command":"x","port":0}]}"#, "port"),
            (r#"{"configurations":[{"name":"a","command":"x","port":70000}]}"#, "port"),
            (r#"{"configurations":[{"name":"a","command":"x","port":"80"}]}"#, "port"),
            (r#"{"configurations":[{"name":"a","command":7}]}"#, "\"command\" must be"),
            (r#"{"configurations":[{"name":"a","command":"x"},{"name":"a","command":"y"}]}"#, "used twice"),
        ] {
            let error = parse_launch(text).unwrap_err();
            assert!(error.contains(expect), "{text}: {error}");
            assert!(error.contains("\"configurations\": ["), "the error shows an example: {error}");
            assert!(error.contains(LAUNCH_FILE));
        }
    }

    #[test]
    fn the_default_is_the_first_configuration_and_a_name_picks_one() {
        let configs = parse_launch(FILE).unwrap();
        assert_eq!(pick(&configs, None).unwrap().name, "web");
        assert_eq!(pick(&configs, Some("  ")).unwrap().name, "web");
        assert_eq!(pick(&configs, Some("api")).unwrap().name, "api");
        let error = pick(&configs, Some("db")).unwrap_err();
        assert!(error.contains("\"db\"") && error.contains("web, api"), "{error}");
    }

    #[test]
    fn terminal_arguments_apply_the_defaults() {
        let configs = parse_launch(FILE).unwrap();
        let folder = Path::new("proj");
        let web = terminal_args(&configs[0], folder).unwrap();
        assert_eq!(web["command"], "npm run dev");
        assert_eq!(web["waitFor"], DEFAULT_READY);
        assert_eq!(web["timeoutMs"], 60_000);
        assert_eq!(web["cwd"], "proj");
        assert!(web.get("shell").is_none());
        let api = terminal_args(&configs[1], folder).unwrap();
        assert_eq!(api["waitFor"], "listening on");
        assert_eq!(api["shell"], "git-bash");
        assert_eq!(Path::new(api["cwd"].as_str().unwrap()), folder.join("server"));
    }

    #[test]
    fn a_relative_cwd_cannot_leave_the_project() {
        let folder = Path::new("proj");
        assert_eq!(resolve_cwd(folder, None).unwrap(), folder);
        assert_eq!(resolve_cwd(folder, Some("web/app")).unwrap(), folder.join("web/app"));
        assert!(resolve_cwd(folder, Some("../other")).is_err());
        assert!(resolve_cwd(folder, Some("a/../../b")).is_err());
        let absolute = std::env::temp_dir();
        assert_eq!(resolve_cwd(folder, absolute.to_str()).unwrap(), absolute);
    }

    #[test]
    fn the_default_ready_pattern_matches_what_dev_servers_print() {
        // The pattern is handed to the terminal tool; check its alternatives are the documented ones.
        assert_eq!(DEFAULT_READY.split('|').collect::<Vec<_>>(), ["ready", "listening", "running", "started", r"localhost:\d+"]);
        assert_eq!(page_url(5173), "http://localhost:5173");
    }

    #[test]
    fn the_tab_id_is_found_in_a_terminal_result() {
        assert_eq!(find_tab_id("Tab t3 · Git Bash · C:\\work · running"), Some("t3".into()));
        assert_eq!(find_tab_id("Started in tabId: 7\nready on :3000"), Some("7".into()));
        assert_eq!(find_tab_id("opened tab \"term-2\" ..."), Some("term-2".into()));
        assert_eq!(find_tab_id("a table of contents"), None);
        assert_eq!(find_tab_id("nothing here"), None);
        assert_eq!(find_tab_id("see the tab results below"), None);
    }

    #[test]
    fn a_missed_ready_line_is_recognised() {
        assert!(timed_out("Timed out after 60000 ms waiting for /ready/."));
        assert!(!timed_out("Matched \"ready\" after 3 s"));
    }

    #[test]
    fn a_tool_that_is_not_registered_gets_a_clear_message() {
        let missing = ToolOutput::error("Unknown tool terminal_run.");
        let message = missing_tool(&missing, "terminal_run").unwrap();
        assert!(message.is_error && text_of(&message).contains("not available in this build yet"));
        assert!(missing_tool(&ToolOutput::error("The person declined: run"), "terminal_run").is_none());
        assert!(missing_tool(&ToolOutput::text("ok"), "terminal_run").is_none());
    }

    #[test]
    fn something_listening_is_found_and_a_closed_port_is_not() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(port_open(port));
        drop(listener);
        assert!(!port_open(port));
    }
}
