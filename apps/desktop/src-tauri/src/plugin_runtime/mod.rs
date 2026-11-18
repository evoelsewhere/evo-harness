//! Plugin backends' runtimes and their preparation
//! (docs/features/plugin-settings/spec.md, FR-10, FR-18; plan 3.5; ADR 0022).
//!
//! - [`install`]: Deno (TypeScript, JavaScript) and uv (Python), pinned
//!   versions checked against their published SHA-256, downloaded into
//!   `<agent dir>/runtimes` the first time a plugin needs one, shared by all.
//! - [`prepare`]: a plugin's dependencies, once per version of its files:
//!   `deno cache` (network to the package registries only) or `uv sync
//!   --frozen` into its data folder. A marker in the data folder says which
//!   files were prepared.
//!
//! The shell starts the backends through the MCP pool (`plugins.rs`), which
//! runs them on demand.

pub mod install;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use evo_plugins::runtime::Language;
use evo_plugins::Plugin;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

pub use install::Tool;

/// Where a plugin's preparation stands.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preparation {
    /// `downloading`, `installing`, `ready` or `failed`.
    pub phase: String,
    pub message: Option<String>,
}

#[derive(Default)]
pub struct RuntimeState {
    prepared: Mutex<HashMap<String, Preparation>>,
    /// One download per tool at a time.
    pub(crate) downloads: tokio::sync::Mutex<()>,
}

impl RuntimeState {
    pub fn of(&self, plugin: &str) -> Option<Preparation> {
        self.prepared.lock().unwrap().get(plugin).cloned()
    }

    fn set(&self, app: &AppHandle, plugin: &str, phase: &str, message: Option<String>) {
        let preparation = Preparation { phase: phase.into(), message };
        self.prepared.lock().unwrap().insert(plugin.to_string(), preparation.clone());
        let _ = app.emit("plugins://runtime", json!({ "id": plugin, "preparation": preparation }));
    }
}

/// `<agent dir>/runtimes`.
pub fn runtimes_dir() -> PathBuf {
    crate::mcp::agent_dir().join("runtimes")
}

/// The tool a backend runs on.
pub fn tool_for(language: Language) -> Tool {
    match language {
        Language::Typescript | Language::Javascript => Tool::Deno,
        Language::Python => Tool::Uv,
    }
}

/// Files whose change means the dependencies must be prepared again.
const DEPENDENCY_FILES: &[&str] = &["deno.json", "deno.jsonc", "deno.lock", "package.json", "pyproject.toml", "uv.lock"];

/// What the marker records: the runtime version, the entry and the
/// dependency files' contents (FNV-1a).
fn fingerprint(plugin: &Plugin, tool: Tool) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut mix = |bytes: &[u8]| {
        for byte in bytes {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
        }
    };
    mix(tool.version().as_bytes());
    if let Some(runtime) = &plugin.runtime {
        mix(runtime.entry.as_bytes());
    }
    for name in DEPENDENCY_FILES {
        if let Ok(bytes) = std::fs::read(plugin.root.join(name)) {
            mix(name.as_bytes());
            mix(&bytes);
        }
    }
    format!("{hash:016x}")
}

fn marker(plugin: &Plugin) -> PathBuf {
    plugin.data.join(".evo-runtime")
}

/// Whether `plugin`'s backend can start now: its runtime is here and its
/// dependencies are prepared for these files.
pub fn ready(plugin: &Plugin) -> bool {
    let Some(runtime) = &plugin.runtime else { return false };
    let tool = tool_for(runtime.language);
    install::installed(tool).is_some() && std::fs::read_to_string(marker(plugin)).is_ok_and(|m| m.trim() == fingerprint(plugin, tool))
}

/// The environment Deno runs with for plugins (both preparing and running).
pub fn deno_env() -> Vec<(String, String)> {
    vec![
        ("DENO_DIR".into(), runtimes_dir().join("deno-cache").display().to_string()),
        // Behind TLS inspection the system's certificates are needed (spike S2).
        ("DENO_TLS_CA_STORE".into(), "system,mozilla".into()),
        ("DENO_NO_UPDATE_CHECK".into(), "1".into()),
    ]
}

/// The environment uv runs with for plugins.
pub fn uv_env(plugin: &Plugin) -> Vec<(String, String)> {
    let runtimes = runtimes_dir();
    vec![
        ("UV_PROJECT_ENVIRONMENT".into(), plugin.data.join("venv").display().to_string()),
        ("UV_CACHE_DIR".into(), runtimes.join("uv-cache").display().to_string()),
        ("UV_PYTHON_INSTALL_DIR".into(), runtimes.join("python").display().to_string()),
        // Only Python that uv manages: never the person's own installs.
        ("UV_PYTHON_PREFERENCE".into(), "only-managed".into()),
        ("UV_NO_PROGRESS".into(), "1".into()),
    ]
}

/// The registries `deno cache` may reach.
const REGISTRIES: &str = "jsr.io:443,registry.npmjs.org:443,deno.land:443";

/// The last lines of a command's output, for the Runtime tab.
fn tail(output: &std::process::Output) -> String {
    let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(12)..].join("\n")
}

async fn run(program: &Path, args: &[String], env: Vec<(String, String)>, cwd: &Path) -> Result<(), String> {
    let mut command = tokio::process::Command::new(program);
    command.args(args).envs(env).current_dir(cwd).kill_on_drop(true);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = tokio::time::timeout(std::time::Duration::from_secs(600), command.output())
        .await
        .map_err(|_| "it took more than 10 minutes".to_string())?
        .map_err(|e| format!("could not start {}: {e}", program.display()))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(tail(&output))
    }
}

/// Get the runtime and the dependencies of `plugin`'s backend ready, then
/// have the plugins read again so the backend joins the pool. Does nothing
/// when another preparation of it runs.
pub fn prepare(app: &AppHandle, plugin: Plugin) {
    let Some(runtime) = plugin.runtime.clone() else { return };
    let state = app.state::<RuntimeState>();
    if state.of(&plugin.id).is_some_and(|p| p.phase == "downloading" || p.phase == "installing") {
        return;
    }
    // A failure stays until the plugin's files change or it is retried.
    if state.of(&plugin.id).is_some_and(|p| p.phase == "failed") {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<RuntimeState>();
        let tool = tool_for(runtime.language);
        let binary = match install::installed(tool) {
            Some(path) => path,
            None => {
                state.set(&app, &plugin.id, "downloading", Some(format!("Getting {} {}", tool.name(), tool.version())));
                match install::ensure(&app, tool).await {
                    Ok(path) => path,
                    Err(error) => return state.set(&app, &plugin.id, "failed", Some(format!("Could not get {}: {error}", tool.name()))),
                }
            }
        };
        state.set(&app, &plugin.id, "installing", Some("Preparing its dependencies".into()));
        let _ = std::fs::create_dir_all(&plugin.data);
        let result = match tool {
            Tool::Deno => {
                let args = vec![
                    "cache".into(),
                    "--node-modules-dir=none".into(),
                    format!("--allow-import={REGISTRIES}"),
                    plugin.root.join(&runtime.entry).display().to_string(),
                ];
                run(&binary, &args, deno_env(), &plugin.root).await
            }
            Tool::Uv => {
                let args = vec!["sync".into(), "--frozen".into(), "--compile-bytecode".into(), "--project".into(), plugin.root.display().to_string()];
                run(&binary, &args, uv_env(&plugin), &plugin.root).await
            }
        };
        match result {
            Ok(()) => {
                let _ = std::fs::write(marker(&plugin), fingerprint(&plugin, tool));
                state.set(&app, &plugin.id, "ready", None);
                crate::plugins::changed_public(&app).await;
            }
            Err(output) => state.set(&app, &plugin.id, "failed", Some(output)),
        }
    });
}

/// Forget a failed preparation so the next change tries again.
pub fn retry(app: &AppHandle, plugin: &str) {
    app.state::<RuntimeState>().prepared.lock().unwrap().remove(plugin);
}

/// The Runtime tab's view of a plugin: its preparation and its servers in
/// the pool (status, error, last log lines; secrets hidden by the pool).
#[tauri::command]
pub fn plugin_runtime_state(app: AppHandle, id: String, name: String) -> Value {
    let preparation = app.state::<RuntimeState>().of(&id);
    let snapshot = app.try_state::<crate::mcp::McpState>().map(|s| s.0.snapshot()).unwrap_or(Value::Null);
    let servers: Vec<Value> = snapshot["servers"]
        .as_array()
        .map(|all| {
            all.iter()
                .filter(|s| s["plugin"].as_str() == Some(name.as_str()))
                .map(|s| {
                    let log: Vec<Value> = s["log"].as_array().map(|l| l.iter().rev().take(40).rev().cloned().collect()).unwrap_or_default();
                    json!({ "key": s["key"], "name": s["name"], "status": s["status"], "error": s["error"], "tools": s["tools"].as_array().map_or(0, Vec::len), "log": log })
                })
                .collect()
        })
        .unwrap_or_default();
    json!({ "preparation": preparation, "servers": servers })
}

/// Start, stop or restart a plugin's servers by hand (FR-13). A start
/// clears a failed preparation so it runs again.
#[tauri::command]
pub async fn plugin_runtime_control(app: AppHandle, id: String, name: String, action: String) -> Result<(), String> {
    let pool = app.state::<crate::mcp::McpState>().0.clone();
    let keys = pool.plugin_keys(&name);
    match action.as_str() {
        "start" | "restart" => {
            retry(&app, &id);
            if action == "restart" {
                for key in &keys {
                    pool.sleep(key).await;
                }
            }
            crate::plugins::changed_public(&app).await;
            let mut problems = Vec::new();
            for key in pool.plugin_keys(&name) {
                if let Err(error) = pool.wake(&key).await {
                    problems.push(error);
                }
            }
            if problems.is_empty() {
                Ok(())
            } else {
                Err(problems.join("; "))
            }
        }
        "stop" => {
            for key in keys {
                pool.sleep(&key).await;
            }
            Ok(())
        }
        other => Err(format!("unknown action `{other}`")),
    }
}

/// The person's start mode and idle time for a plugin's code (FR-11).
#[tauri::command]
pub async fn plugin_runtime_mode(app: AppHandle, id: String, mode: String, idle_minutes: Option<u32>) -> Result<(), String> {
    let mode = match mode.as_str() {
        "onDemand" => evo_plugins::runtime::StartMode::OnDemand,
        "withApp" => evo_plugins::runtime::StartMode::WithApp,
        "off" => evo_plugins::runtime::StartMode::Off,
        other => return Err(format!("unknown start mode `{other}`")),
    };
    let idle = idle_minutes.filter(|m| (1..=1440).contains(m));
    tauri::async_runtime::spawn_blocking(move || {
        let store = evo_plugins::Store::in_agent_dir(&crate::mcp::agent_dir());
        let installed = store.installed().into_iter().find(|i| i.id == id).ok_or_else(|| format!("`{id}` is not installed"))?;
        if mode == evo_plugins::runtime::StartMode::WithApp && !installed.approved_permissions.iter().any(|p| p == "startup") {
            return Err("allow it to start when Evo opens first (Permissions)".to_string());
        }
        store.edit(&id, |entry| {
            entry.start_mode = Some(mode);
            if idle.is_some() {
                entry.idle_stop_minutes = idle;
            }
        })
    })
    .await
    .map_err(|e| e.to_string())??;
    crate::plugins::changed_public(&app).await;
    Ok(())
}
