//! A plugin's backend (`runtime.server` in its manifest) and how evo runs it
//! (docs/features/plugin-settings/spec.md, FR-10, FR-11, FR-15, FR-16;
//! plan 3.5; ADR 0022). TypeScript and JavaScript run in Deno with
//! permissions built from the person's approvals; Python runs from its uv
//! environment, with no enforcement. This module only builds the command.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Language {
    Typescript,
    Javascript,
    Python,
}

impl Language {
    /// Deno enforces network and file permissions; Python does not (FR-15).
    pub fn enforced(self) -> bool {
        !matches!(self, Language::Python)
    }
}

/// When the backend runs (FR-11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StartMode {
    #[default]
    OnDemand,
    WithApp,
    Off,
}

pub const DEFAULT_IDLE_MINUTES: u32 = 10;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Runtime {
    pub language: Language,
    /// The entry file, relative to the plugin, with `/`.
    pub entry: String,
    pub startup: StartMode,
    pub idle_stop_minutes: u32,
    /// `NAME: template`: `${user_config.KEY}`, `${setting.KEY}`,
    /// `${secret.KEY}`, `${PLUGIN_ROOT}`, `${PLUGIN_DATA}`.
    pub env: BTreeMap<String, String>,
}

fn env_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && !name.starts_with("EVO_")
}

/// Read `runtime.server`; `None` when the plugin has no backend.
pub fn parse_runtime(root: &Path, value: Option<&Value>) -> (Option<Runtime>, Vec<String>) {
    let mut problems = Vec::new();
    let Some(server) = value.filter(|v| !v.is_null()).and_then(|v| v.get("server")) else {
        return (None, problems);
    };
    let Some(object) = server.as_object() else {
        problems.push("runtime.server: expected an object".into());
        return (None, problems);
    };
    let text = |name: &str| {
        object
            .get(name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let language = match text("language").as_deref() {
        Some("typescript") => Language::Typescript,
        Some("javascript") => Language::Javascript,
        Some("python") => Language::Python,
        other => {
            problems.push(format!("runtime.server: `language` must be typescript, javascript or python, not {other:?}"));
            return (None, problems);
        }
    };
    let Some(raw) = text("entry") else {
        problems.push("runtime.server: no `entry`".into());
        return (None, problems);
    };
    let entry = raw.trim_start_matches("./").replace('\\', "/");
    if entry.split('/').any(|p| p == ".." || p.is_empty())
        || Path::new(&entry).is_absolute()
        || !root.join(&entry).is_file()
    {
        problems.push(format!(
            "runtime.server: the entry `{raw}` is not a file inside the plugin"
        ));
        return (None, problems);
    }
    let startup = match text("startup").as_deref() {
        None | Some("onDemand") => StartMode::OnDemand,
        Some("withApp") => StartMode::WithApp,
        Some("off") => StartMode::Off,
        Some(other) => {
            problems.push(format!(
                "runtime.server: unknown `startup` `{other}` (onDemand, withApp or off)"
            ));
            StartMode::OnDemand
        }
    };
    let idle_stop_minutes = match object.get("idleStopMinutes").and_then(Value::as_u64) {
        Some(minutes) if (1..=1440).contains(&minutes) => minutes as u32,
        Some(_) => {
            problems.push("runtime.server: `idleStopMinutes` must be 1 to 1440".into());
            DEFAULT_IDLE_MINUTES
        }
        None => DEFAULT_IDLE_MINUTES,
    };
    let mut env = BTreeMap::new();
    if let Some(map) = object.get("env").and_then(Value::as_object) {
        for (name, value) in map {
            match value.as_str() {
                Some(template) if env_name(name) => {
                    env.insert(name.clone(), template.to_string());
                }
                _ => problems.push(format!("runtime.server.env: `{name}` must be an upper-case name (not EVO_…) with a text value")),
            }
        }
    }
    if language == Language::Python && !root.join("uv.lock").is_file() {
        problems.push(
            "runtime.server: a Python backend needs a `uv.lock` next to its `pyproject.toml`"
                .into(),
        );
    }
    (
        Some(Runtime {
            language,
            entry,
            startup,
            idle_stop_minutes,
            env,
        }),
        problems,
    )
}

/// An approved `network:https://host[:port]` as Deno's `host:port`.
pub fn net_host(permission: &str) -> Option<String> {
    let origin = permission
        .strip_prefix("network:")?
        .strip_prefix("https://")?;
    Some(if origin.contains(':') {
        origin.to_string()
    } else {
        format!("{origin}:443")
    })
}

/// What [`launch`] needs to know.
pub struct LaunchInput<'a> {
    pub runtime: &'a Runtime,
    pub root: &'a Path,
    pub data: &'a Path,
    /// Approved permissions of the plugin.
    pub approved: &'a [String],
    pub deno: &'a Path,
    /// The host API's port (FR-26), reachable from the backend.
    pub host_port: Option<u16>,
    /// The workspace folder the process works for, when it has one.
    pub workspace: Option<&'a Path>,
    /// Non-secret values (`settings::effective`).
    pub values: &'a Map<String, Value>,
}

/// A command evo runs. `env` values may still hold `${secret.KEY}`: the
/// shell fills them from the vault ([`fill_secrets`]) just before starting.
#[derive(Debug, Clone, PartialEq)]
pub struct Launch {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: PathBuf,
}

/// Paths of approved `files:<mode>:<path>` permissions.
fn approved_paths(input: &LaunchInput, mode: &str) -> Vec<String> {
    let prefix = format!("files:{mode}:");
    input
        .approved
        .iter()
        .filter_map(|p| p.strip_prefix(&prefix))
        .filter_map(|p| match p {
            "workspace" => input.workspace.map(|w| w.display().to_string()),
            other => Some(other.to_string()),
        })
        .collect()
}

/// Fill `${…}` in an env template, keeping `${secret.KEY}` for the shell.
fn render(template: &str, input: &LaunchInput) -> String {
    let mut out = template
        .replace("${PLUGIN_ROOT}", &input.root.display().to_string())
        .replace("${PLUGIN_DATA}", &input.data.display().to_string());
    for (key, value) in input.values {
        let shown = match value {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        };
        out = out
            .replace(&format!("${{user_config.{key}}}"), &shown)
            .replace(&format!("${{setting.{key}}}"), &shown);
    }
    out
}

/// The command for a plugin's backend.
pub fn launch(input: &LaunchInput) -> Launch {
    let runtime = input.runtime;
    let entry = input.root.join(&runtime.entry);
    let mut env: BTreeMap<String, String> = runtime
        .env
        .iter()
        .map(|(name, template)| (name.clone(), render(template, input)))
        .collect();
    if let Some(port) = input.host_port {
        env.insert("EVO_PLUGIN_HOST".into(), format!("http://127.0.0.1:{port}"));
    }
    match runtime.language {
        Language::Typescript | Language::Javascript => {
            let mut hosts: Vec<String> =
                input.approved.iter().filter_map(|p| net_host(p)).collect();
            if let Some(port) = input.host_port {
                hosts.push(format!("127.0.0.1:{port}"));
            }
            let mut read = vec![
                input.root.display().to_string(),
                input.data.display().to_string(),
            ];
            read.extend(approved_paths(input, "read"));
            read.extend(approved_paths(input, "write"));
            let mut write = vec![input.data.display().to_string()];
            write.extend(approved_paths(input, "write"));
            let mut names: Vec<String> = vec!["EVO_PLUGIN_HOST".into(), "EVO_PLUGIN_KEY".into()];
            names.extend(runtime.env.keys().cloned());
            let mut args = vec![
                "run".into(),
                "--cached-only".into(),
                "--no-prompt".into(),
                "--node-modules-dir=none".into(),
            ];
            if !hosts.is_empty() {
                args.push(format!("--allow-net={}", hosts.join(",")));
            }
            args.push(format!("--allow-read={}", read.join(",")));
            args.push(format!("--allow-write={}", write.join(",")));
            args.push(format!("--allow-env={}", names.join(",")));
            args.extend(["--deny-run".into(), "--deny-ffi".into()]);
            args.push(entry.display().to_string());
            Launch {
                program: input.deno.to_path_buf(),
                args,
                env,
                cwd: input.data.to_path_buf(),
            }
        }
        Language::Python => {
            let venv = input.data.join("venv");
            let python = if cfg!(windows) {
                venv.join("Scripts").join("python.exe")
            } else {
                venv.join("bin").join("python")
            };
            env.insert("PYTHONDONTWRITEBYTECODE".into(), "1".into());
            Launch {
                program: python,
                args: vec![entry.display().to_string()],
                env,
                cwd: input.data.to_path_buf(),
            }
        }
    }
}

/// The secret keys an env still names (`${secret.KEY}`).
pub fn secret_keys(env: &BTreeMap<String, String>) -> Vec<String> {
    let mut keys = Vec::new();
    for value in env.values() {
        let mut rest = value.as_str();
        while let Some(start) = rest.find("${secret.") {
            let after = &rest[start + 9..];
            let Some(end) = after.find('}') else { break };
            let key = &after[..end];
            if !keys.iter().any(|k| k == key) {
                keys.push(key.to_string());
            }
            rest = &after[end..];
        }
    }
    keys
}

/// Fill `${secret.KEY}` in `env` with `lookup(KEY)`; a variable whose secret
/// is missing is left out (never sent half-filled).
pub fn fill_secrets(env: &mut BTreeMap<String, String>, lookup: impl Fn(&str) -> Option<String>) {
    let names: Vec<String> = env.keys().cloned().collect();
    for name in names {
        let value = env[&name].clone();
        let keys = secret_keys(&BTreeMap::from([(name.clone(), value.clone())]));
        let mut filled = value;
        let mut complete = true;
        for key in keys {
            match lookup(&key) {
                Some(secret) => filled = filled.replace(&format!("${{secret.{key}}}"), &secret),
                None => complete = false,
            }
        }
        if complete {
            env.insert(name, filled);
        } else {
            env.remove(&name);
        }
    }
}

#[cfg(test)]
mod tests;
