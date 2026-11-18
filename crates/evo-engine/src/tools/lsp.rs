//! Code intelligence (docs/features/code-intelligence/spec.md): the errors
//! that come with `edit` / `write` results ([`check_edits`], FR-15–FR-21),
//! the opt-in `lsp` tool (FR-1) and the language-server RPCs (`evo-lsp`).
//!
//! Servers live in `<agent dir>/lsp/<id>` when evo installs them; a server on
//! PATH, in the Rust toolchain or shipped with a VS Code-style editor is used
//! as it is. One server per language and project root starts on first use
//! and runs while the engine does. `disabledLanguageServers` in settings
//! switches servers off.
//!
//! RPC (Customize → Code intelligence): `list_language_servers` (the catalog
//! with what is installed, what the project uses, what is running and any
//! install in progress), `install_language_server {id}` (runs in the
//! background; progress in the list), `uninstall_language_server {id}`,
//! `set_language_server_enabled {id, enabled}`,
//! `set_code_intelligence_options {editDiagnostics?, lspTool?}`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use async_trait::async_trait;
use evo_agent::agent_tool::AgentTool;
use evo_agent::error::AgentError;
use evo_agent::types::{AgentToolResult, TextContentOrImage, ToolExecutionMode, ToolResultPartial};
use evo_ai::types::{Schema, Tool};
use evo_lsp::{catalog, detect, edit_check, install, ops, Pool};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

fn lsp_dir() -> PathBuf {
    crate::config::agent_dir()
        .map(|d| d.join("lsp"))
        .unwrap_or_else(|_| PathBuf::from(".evo-lsp"))
}

fn disabled() -> Vec<String> {
    crate::settings::load_settings()
        .ok()
        .and_then(|s| s.disabled_language_servers)
        .unwrap_or_default()
}

/// Whether `edit` / `write` results carry the server's errors (FR-20); read
/// at each edit, so the switch applies at once.
pub fn edit_diagnostics_enabled() -> bool {
    crate::settings::load_settings()
        .ok()
        .and_then(|s| s.edit_diagnostics)
        .unwrap_or(true)
}

/// Whether chats get the `lsp` tool (FR-1); read when a chat's tools are
/// built.
pub fn tool_enabled() -> bool {
    crate::settings::load_settings()
        .ok()
        .and_then(|s| s.lsp_tool)
        .unwrap_or(false)
}

pub fn pool() -> &'static Pool {
    static POOL: OnceLock<Pool> = OnceLock::new();
    POOL.get_or_init(|| {
        let pool = Pool::new(lsp_dir());
        pool.set_disabled(disabled());
        pool
    })
}

// ── The tool ─────────────────────────────────────────────────────────────────

pub struct LspTool {
    schema: Tool,
    cwd: PathBuf,
}

pub fn create_lsp_tool(cwd: PathBuf) -> Arc<dyn AgentTool> {
    Arc::new(LspTool {
        schema: Tool {
            name: "lsp".into(),
            description: "Code intelligence from the project's language servers. Operations: \
                 `diagnostics` (errors and warnings in `path` — run it after editing a file to \
                 catch type errors; without `path`, what the servers already reported), \
                 `definition`, `type_definition`, `implementation`, `references`, `hover` (type \
                 and docs) for a `symbol` on `line` of `path`, `symbols` (outline of `path`), \
                 `workspace_symbols` (find `query` across the project of `path`'s language). More \
                 precise than grep for code: references find real uses, not text matches. The first \
                 call for a project can take a while as the server indexes it."
                .into(),
            parameters: Schema::new(json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["diagnostics", "definition", "type_definition", "implementation", "references", "hover", "symbols", "workspace_symbols"]
                    },
                    "path": { "type": "string", "description": "The file, relative to the project or absolute." },
                    "line": { "type": "number", "description": "1-based line of the symbol." },
                    "symbol": { "type": "string", "description": "The identifier on that line (its first whole-word match is used)." },
                    "character": { "type": "number", "description": "1-based column, instead of `symbol`." },
                    "query": { "type": "string", "description": "For workspace_symbols: the name to look for." }
                },
                "required": ["operation"],
                "additionalProperties": false
            })),
            constrained_sampling: None,
        },
        cwd,
    })
}

fn text<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
}

impl LspTool {
    async fn run(&self, params: &Value) -> Result<String, String> {
        let pool = pool();
        let op = text(params, "operation").ok_or("`operation` is required")?;
        let path = text(params, "path");
        match op {
            "diagnostics" => ops::diagnostics(pool, &self.cwd, path).await,
            "symbols" => ops::symbols(pool, &self.cwd, path.ok_or("`symbols` needs `path`")?).await,
            "workspace_symbols" => {
                let query = text(params, "query")
                    .or_else(|| text(params, "symbol"))
                    .ok_or("`workspace_symbols` needs `query`")?;
                let path =
                    path.ok_or("`workspace_symbols` needs `path` (any file of the language)")?;
                ops::workspace_symbols(pool, &self.cwd, query, path).await
            }
            op => {
                let path = path.ok_or_else(|| format!("`{op}` needs `path`"))?;
                let line = params
                    .get("line")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| format!("`{op}` needs `line`"))?
                    as u32;
                let character = params
                    .get("character")
                    .and_then(Value::as_u64)
                    .map(|c| c as u32);
                ops::at(
                    pool,
                    &self.cwd,
                    op,
                    path,
                    line,
                    text(params, "symbol"),
                    character,
                )
                .await
            }
        }
    }
}

#[async_trait]
impl AgentTool for LspTool {
    fn schema(&self) -> &Tool {
        &self.schema
    }

    fn label(&self) -> &str {
        "lsp"
    }

    fn execution_mode(&self) -> ToolExecutionMode {
        ToolExecutionMode::Parallel
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        params: Value,
        signal: CancellationToken,
        _on_update: Arc<dyn Fn(ToolResultPartial) + Send + Sync>,
    ) -> Result<AgentToolResult, AgentError> {
        let out = tokio::select! {
            _ = signal.cancelled() => return Err(AgentError::Tool("cancelled".into())),
            out = self.run(&params) => out,
        };
        let text = out.map_err(AgentError::Tool)?;
        Ok(AgentToolResult {
            content: vec![TextContentOrImage::text(text)],
            details: json!({ "operation": params.get("operation"), "path": params.get("path") }),
            ..Default::default()
        })
    }
}

// ── Errors with the edit ─────────────────────────────────────────────────────

/// `inner` (`edit` or `write`) with the language server's errors for the
/// changed file added to its result (FR-15–FR-21). Everything else — schema,
/// approval, the result's details — is `inner`'s own.
pub fn check_edits(inner: Arc<dyn AgentTool>, cwd: PathBuf) -> Arc<dyn AgentTool> {
    Arc::new(EditCheck { inner, cwd })
}

struct EditCheck {
    inner: Arc<dyn AgentTool>,
    cwd: PathBuf,
}

impl EditCheck {
    /// The file a call changes, as `edit` / `write` resolve it.
    fn file(&self, params: &Value) -> Option<PathBuf> {
        tool_file(params, &self.cwd)
    }
}

/// The file a `read` / `edit` / `write` call names, as those tools resolve it.
fn tool_file(params: &Value, cwd: &Path) -> Option<PathBuf> {
    let path = evo_tools::path_utils::normalize_tool_path(text(params, "path")?);
    let path = match path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        Some(rest) => dirs_home()?.join(rest),
        None => PathBuf::from(&path),
    };
    Some(if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    })
}

/// `inner` (`read`) that, after reading a file, starts its language server
/// in the background, so the agent's first edit of it is checked too
/// (FR-22). Not in a read-only session, whose servers could build the
/// project, nor with the edit check switched off.
pub fn warm_on_read(inner: Arc<dyn AgentTool>, cwd: PathBuf) -> Arc<dyn AgentTool> {
    Arc::new(WarmOnRead { inner, cwd })
}

struct WarmOnRead {
    inner: Arc<dyn AgentTool>,
    cwd: PathBuf,
}

#[async_trait]
impl AgentTool for WarmOnRead {
    fn schema(&self) -> &Tool {
        self.inner.schema()
    }

    fn label(&self) -> &str {
        self.inner.label()
    }

    fn execution_mode(&self) -> ToolExecutionMode {
        self.inner.execution_mode()
    }

    fn prepare_arguments(&self, args: Value) -> Result<Value, AgentError> {
        self.inner.prepare_arguments(args)
    }

    async fn execute(
        &self,
        tool_call_id: &str,
        params: Value,
        signal: CancellationToken,
        on_update: Arc<dyn Fn(ToolResultPartial) + Send + Sync>,
    ) -> Result<AgentToolResult, AgentError> {
        let file = tool_file(&params, &self.cwd);
        let result = self
            .inner
            .execute(tool_call_id, params, signal, on_update)
            .await?;
        if let Some(file) = file {
            if file.is_file()
                && edit_diagnostics_enabled()
                && !crate::session_roles::current_is_read_only()
            {
                pool().warm(&file, &self.cwd);
            }
        }
        Ok(result)
    }
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

#[async_trait]
impl AgentTool for EditCheck {
    fn schema(&self) -> &Tool {
        self.inner.schema()
    }

    fn label(&self) -> &str {
        self.inner.label()
    }

    fn execution_mode(&self) -> ToolExecutionMode {
        self.inner.execution_mode()
    }

    fn prepare_arguments(&self, args: Value) -> Result<Value, AgentError> {
        self.inner.prepare_arguments(args)
    }

    async fn execute(
        &self,
        tool_call_id: &str,
        params: Value,
        signal: CancellationToken,
        on_update: Arc<dyn Fn(ToolResultPartial) + Send + Sync>,
    ) -> Result<AgentToolResult, AgentError> {
        let file = self.file(&params).filter(|_| edit_diagnostics_enabled());
        let Some(file) = file else {
            return self
                .inner
                .execute(tool_call_id, params, signal, on_update)
                .await;
        };
        let pool = pool();
        let before = edit_check::before_edit(pool, &file, &self.cwd).await;
        let mut result = self
            .inner
            .execute(tool_call_id, params, signal.clone(), on_update)
            .await?;
        let note = tokio::select! {
            _ = signal.cancelled() => None,
            note = edit_check::after_edit(pool, &self.cwd, &file, before, edit_check::EDIT_WAIT) => note,
        };
        if let Some(note) = note {
            result.content.push(TextContentOrImage::text(note));
        }
        Ok(result)
    }
}

// ── Installs and the list ────────────────────────────────────────────────────

#[derive(Default, Clone)]
struct InstallState {
    running: bool,
    log: Vec<String>,
    error: Option<String>,
}

fn installs() -> &'static Mutex<HashMap<String, InstallState>> {
    static INSTALLS: OnceLock<Mutex<HashMap<String, InstallState>>> = OnceLock::new();
    INSTALLS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Start installing `id` in the background.
pub fn start_install(id: &str) -> Result<(), String> {
    let server = catalog::find(id).ok_or_else(|| format!("unknown language server `{id}`"))?;
    {
        let mut all = installs().lock().unwrap();
        let state = all.entry(id.to_string()).or_default();
        if state.running {
            return Err(format!("{} is already being installed", server.name));
        }
        *state = InstallState {
            running: true,
            log: Vec::new(),
            error: None,
        };
    }
    let id = id.to_string();
    tokio::spawn(async move {
        let log_id = id.clone();
        let log = move |line: String| {
            if let Some(state) = installs().lock().unwrap().get_mut(&log_id) {
                state.log.extend(line.lines().map(str::to_string));
                let excess = state.log.len().saturating_sub(60);
                state.log.drain(..excess);
            }
        };
        let result = install::install(server, &lsp_dir(), &log).await;
        if result.is_ok() {
            // A fresh install replaces whatever was running before.
            pool().stop(Some(server.id)).await;
        }
        if let Some(state) = installs().lock().unwrap().get_mut(&id) {
            state.running = false;
            state.error = result.err();
        }
    });
    Ok(())
}

pub async fn uninstall(id: &str) -> Result<(), String> {
    let server = catalog::find(id).ok_or_else(|| format!("unknown language server `{id}`"))?;
    pool().stop(Some(server.id)).await;
    install::uninstall(server, &lsp_dir())?;
    installs().lock().unwrap().remove(id);
    Ok(())
}

pub fn set_enabled(id: &str, enabled: bool) -> Result<(), String> {
    catalog::find(id).ok_or_else(|| format!("unknown language server `{id}`"))?;
    let mut list = disabled();
    list.retain(|x| x != id);
    if !enabled {
        list.push(id.to_string());
    }
    let saved = list.clone();
    crate::settings::update_settings_value(move |settings| {
        if saved.is_empty() {
            settings.remove("disabledLanguageServers");
        } else {
            settings.insert("disabledLanguageServers".into(), json!(saved));
        }
    })?;
    pool().set_disabled(list);
    Ok(())
}

/// Save the switches of Customize → Code intelligence (FR-20); a key left
/// out keeps its value.
pub fn set_options(edit_diagnostics: Option<bool>, lsp_tool: Option<bool>) -> Result<(), String> {
    crate::settings::update_settings_value(move |settings| {
        if let Some(on) = edit_diagnostics {
            settings.insert("editDiagnostics".into(), json!(on));
        }
        if let Some(on) = lsp_tool {
            settings.insert("lspTool".into(), json!(on));
        }
    })
}

/// Everything Customize → Code intelligence shows, for a chat in `cwd`.
pub async fn describe(cwd: &Path) -> Value {
    let dir = lsp_dir();
    let detected = {
        let cwd = cwd.to_path_buf();
        tokio::task::spawn_blocking(move || detect::detect(&cwd))
            .await
            .unwrap_or_default()
    };
    let running = pool().running().await;
    let off = disabled();
    let installs = installs().lock().unwrap().clone();
    let catalog_dir = dir.clone();
    let servers: Vec<Value> = tokio::task::spawn_blocking(move || {
        catalog::SERVERS
            .iter()
            .map(|server| {
                let located = install::locate(server, &catalog_dir);
                let problem = located
                    .as_ref()
                    .and_then(|l| install::problem(server, &l.path, &catalog_dir));
                let state = installs.get(server.id).cloned().unwrap_or_default();
                json!({
                    "id": server.id,
                    "name": server.name,
                    "language": server.language,
                    "extensions": server.extensions.iter().map(|(e, _)| *e).collect::<Vec<_>>(),
                    "install": server.install,
                    "requires": server.requires,
                    "missing": install::missing_requirements(server),
                    "installed": located,
                    "enabled": !off.iter().any(|x| x == server.id),
                    "installing": state.running,
                    "log": state.log,
                    "error": state.error,
                    "problem": problem,
                })
            })
            .collect()
    })
    .await
    .unwrap_or_default();
    json!({
        "options": {
            "editDiagnostics": edit_diagnostics_enabled(),
            "lspTool": tool_enabled(),
        },
        "servers": servers,
        "detected": detected,
        "running": running,
        "lspDir": dir,
        "folder": cwd,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_list_names_every_server() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();
        let list = describe(dir.path()).await;
        assert_eq!(
            list["servers"].as_array().unwrap().len(),
            catalog::SERVERS.len()
        );
        assert_eq!(list["detected"][0]["server"], "rust-analyzer");
        assert!(start_install("nope").is_err());
        assert!(list["options"]["editDiagnostics"].is_boolean());
        assert!(list["options"]["lspTool"].is_boolean());
    }

    fn write_tool(cwd: &Path) -> Arc<dyn AgentTool> {
        let env = Arc::new(evo_tools::OsExecutionEnv::with_cwd(cwd.to_path_buf()));
        let env_dyn: Arc<dyn evo_tools::ExecutionEnv> = env.clone();
        let mut_env: Arc<dyn evo_tools::MutatingEnv> = env;
        let ctx = evo_tools::ExecutionToolContext::new(env_dyn, Some(mut_env));
        check_edits(evo_tools::create_write_tool(&ctx), cwd.to_path_buf())
    }

    // docs/features/code-intelligence/spec.md (FR-15, FR-17).
    #[tokio::test]
    async fn edit_check_keeps_the_tool_and_adds_nothing_without_a_server() {
        let dir = tempfile::tempdir().unwrap();
        let tool = write_tool(dir.path());
        assert_eq!(tool.schema().name, "write");
        let started = std::time::Instant::now();
        let result = tool
            .execute(
                "t1",
                json!({ "path": "notes.unknownext", "content": "hello" }),
                CancellationToken::new(),
                Arc::new(|_| {}),
            )
            .await
            .unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        assert_eq!(result.content.len(), 1, "{:?}", result.content);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("notes.unknownext")).unwrap(),
            "hello"
        );
    }

    #[test]
    fn edit_check_resolves_paths_as_the_tools_do() {
        let cwd = PathBuf::from(if cfg!(windows) { r"C:\p" } else { "/p" });
        let check = EditCheck {
            inner: create_lsp_tool(cwd.clone()),
            cwd: cwd.clone(),
        };
        assert_eq!(
            check.file(&json!({ "path": "@src/a.rs" })),
            Some(cwd.join("src/a.rs"))
        );
        assert_eq!(check.file(&json!({})), None);
    }
}
