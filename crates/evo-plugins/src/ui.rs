//! A plugin's views and the permissions they ask for (`ui` and
//! `permissions` in `plugin.json`; docs/features/plugin-ui/spec.md, FR-1,
//! FR-2). This crate only reads and checks them; the desktop shell hosts
//! the views (ADR 0021).

use std::path::Path;

use serde_json::Value;

/// The permissions a plugin's views may ask for, besides
/// `network:<https origin>` (plugin-ui spec section 4.3).
pub const PERMISSIONS: &[&str] = &[
    "chat:read",
    "chat:send",
    "tools:call",
    "composer:write",
    "storage",
];

/// The permissions of a plugin's running code (docs/features/plugin-settings/spec.md,
/// FR-8): run its backend and connector servers, run hooks, receive secrets,
/// start with evo. Besides these, `files:read:<path>` and `files:write:<path>`
/// (`<path>` is `workspace` or an absolute folder).
pub const CODE_PERMISSIONS: &[&str] = &["runtime", "hooks", "secrets", "startup"];

/// The permissions of plugin AI (docs/features/plugin-ai/spec.md, FR-13,
/// FR-16): use the person's models, and which of evo's tools its runs may use.
pub const AI_PERMISSIONS: &[&str] = &[
    "ai",
    "ai.tools:read",
    "ai.tools:edit",
    "ai.tools:command",
    "ai.tools:web",
    "ai.tools:apps",
];

/// A permission as the manifest declares it.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionInfo {
    pub id: String,
    /// Why the plugin needs it, in its own words.
    pub reason: Option<String>,
    /// The plugin works without it and asks when it needs it (FR-9).
    pub optional: bool,
    /// Not declared: the plugin's parts need it (hooks, servers, secrets).
    pub implied: bool,
}

/// One view: an HTML page inside the plugin, shown in a slot.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    /// Unique within the plugin: `[a-z0-9_-]`.
    pub id: String,
    pub title: String,
    /// Path of the HTML entry, relative to the plugin folder, with `/`.
    pub entry: String,
    /// Path of an SVG or PNG icon, relative to the plugin folder.
    pub icon: Option<String>,
    /// Sidebar pages: shown in the sidebar rather than under More.
    pub default_shown: bool,
    /// Panels: one key, used with Ctrl/⌘.
    pub shortcut: Option<String>,
}

/// The views of a plugin, by slot.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginUi {
    pub sidebar: Vec<View>,
    pub panels: Vec<View>,
    pub cards: Vec<View>,
    pub composer: Vec<View>,
}

impl PluginUi {
    pub fn is_empty(&self) -> bool {
        self.sidebar.is_empty()
            && self.panels.is_empty()
            && self.cards.is_empty()
            && self.composer.is_empty()
    }
}

const SLOTS: &[&str] = &["sidebar", "panels", "cards", "composer"];

/// `path` relative to `root`, kept inside it; the cleaned relative path.
fn inside(root: &Path, path: &str) -> Option<String> {
    let clean = path.trim().trim_start_matches("./").replace('\\', "/");
    if clean.is_empty()
        || clean.starts_with('/')
        || Path::new(&clean).is_absolute()
        || clean.split('/').any(|part| part == ".." || part.is_empty())
    {
        return None;
    }
    root.join(&clean).starts_with(root).then_some(clean)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Read `ui`: each slot's views, checked. A view that is wrong is left out,
/// and the reason is in the problems.
pub fn parse_ui(root: &Path, value: Option<&Value>) -> (PluginUi, Vec<String>) {
    let mut ui = PluginUi::default();
    let mut problems = Vec::new();
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return (ui, problems);
    };
    let Some(slots) = value.as_object() else {
        problems.push("ui: expected an object of slots".into());
        return (ui, problems);
    };
    for (slot, list) in slots {
        if !SLOTS.contains(&slot.as_str()) {
            problems.push(format!(
                "ui: unknown slot `{slot}` (use sidebar, panels, cards or composer)"
            ));
            continue;
        }
        let Some(items) = list.as_array() else {
            problems.push(format!("ui.{slot}: expected a list"));
            continue;
        };
        let mut views = Vec::new();
        for item in items {
            match parse_view(root, slot, item, &views) {
                Ok((view, mut minor)) => {
                    problems.append(&mut minor);
                    views.push(view);
                }
                Err(problem) => problems.push(problem),
            }
        }
        match slot.as_str() {
            "sidebar" => ui.sidebar = views,
            "panels" => ui.panels = views,
            "cards" => ui.cards = views,
            _ => ui.composer = views,
        }
    }
    (ui, problems)
}

/// One view, or why it is left out. Minor problems (a bad icon, a bad
/// shortcut) drop only that field.
fn parse_view(
    root: &Path,
    slot: &str,
    item: &Value,
    seen: &[View],
) -> Result<(View, Vec<String>), String> {
    let text = |key: &str| {
        item.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let id = text("id").ok_or_else(|| format!("ui.{slot}: a view has no id"))?;
    if !valid_id(&id) {
        return Err(format!(
            "ui.{slot}: the id `{id}` may use only a-z, 0-9, - and _"
        ));
    }
    if seen.iter().any(|view| view.id == id) {
        return Err(format!("ui.{slot}: the id `{id}` is used twice"));
    }
    let title = text("title").ok_or_else(|| format!("ui.{slot}.{id}: no title"))?;
    let raw = text("entry").ok_or_else(|| format!("ui.{slot}.{id}: no entry"))?;
    let entry = inside(root, &raw)
        .ok_or_else(|| format!("ui.{slot}.{id}: the entry `{raw}` is outside the plugin"))?;
    if !entry.to_ascii_lowercase().ends_with(".html") {
        return Err(format!(
            "ui.{slot}.{id}: the entry `{raw}` must be an .html file"
        ));
    }
    if !root.join(&entry).is_file() {
        return Err(format!("ui.{slot}.{id}: the entry `{raw}` does not exist"));
    }
    let mut minor = Vec::new();
    let icon = text("icon").and_then(|raw| {
        let found = inside(root, &raw).filter(|path| {
            let lower = path.to_ascii_lowercase();
            (lower.ends_with(".svg") || lower.ends_with(".png")) && root.join(path).is_file()
        });
        if found.is_none() {
            minor.push(format!(
                "ui.{slot}.{id}: the icon `{raw}` must be an .svg or .png file inside the plugin"
            ));
        }
        found
    });
    let shortcut = text("shortcut").and_then(|key| {
        let mut chars = key.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) if c.is_ascii_alphanumeric() && slot == "panels" => {
                Some(c.to_ascii_uppercase().to_string())
            }
            _ => {
                minor.push(format!(
                    "ui.{slot}.{id}: the shortcut `{key}` must be one letter or digit, on a panel"
                ));
                None
            }
        }
    });
    let default_shown = slot == "sidebar"
        && item
            .get("defaultShown")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    Ok((
        View {
            id,
            title,
            entry,
            icon,
            default_shown,
            shortcut,
        },
        minor,
    ))
}

/// `https://host[:port]`, lower-case and without a trailing slash; `None`
/// when it is not an https origin.
fn https_origin(raw: &str) -> Option<String> {
    let rest = raw.strip_prefix("https://")?;
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    let (host, port) = match rest.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (rest, None),
    };
    let host_ok = !host.is_empty()
        && host.len() <= 253
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && !host.starts_with('.')
        && !host.ends_with('.');
    let port_ok =
        port.is_none_or(|p| !p.is_empty() && p.len() <= 5 && p.chars().all(|c| c.is_ascii_digit()));
    (host_ok && port_ok).then(|| {
        format!(
            "https://{}{}",
            host.to_ascii_lowercase(),
            port.map(|p| format!(":{p}")).unwrap_or_default()
        )
    })
}

/// A `files:<mode>:<path>` folder: `workspace` or an absolute path.
fn files_path(path: &str) -> bool {
    path == "workspace"
        || path.starts_with('/')
        || (path.len() > 2
            && path.as_bytes()[1] == b':'
            && path.as_bytes()[0].is_ascii_alphabetic())
}

/// Read `permissions`: known ones and https `network:` origins, each once,
/// as ids (optional ones too).
pub fn parse_permissions(value: Option<&Value>) -> (Vec<String>, Vec<String>) {
    let (infos, problems) = parse_permission_info(value);
    (infos.into_iter().map(|info| info.id).collect(), problems)
}

/// Read `permissions`: each a string, or `{id, reason, optional}`.
pub fn parse_permission_info(value: Option<&Value>) -> (Vec<PermissionInfo>, Vec<String>) {
    let mut out: Vec<PermissionInfo> = Vec::new();
    let mut problems = Vec::new();
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return (out, problems);
    };
    let Some(items) = value.as_array() else {
        problems.push("permissions: expected a list".into());
        return (out, problems);
    };
    for item in items {
        let (raw, reason, optional) = match item {
            Value::String(raw) => (raw.trim().to_string(), None, false),
            Value::Object(object) => match object.get("id").and_then(Value::as_str) {
                Some(id) => (
                    id.trim().to_string(),
                    object
                        .get("reason")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string),
                    object
                        .get("optional")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                ),
                None => {
                    problems.push(format!("permissions: an object needs an `id`, not {item}"));
                    continue;
                }
            },
            _ => {
                problems.push(format!("permissions: expected a string, not {item}"));
                continue;
            }
        };
        let raw = raw.as_str();
        let permission = if PERMISSIONS.contains(&raw)
            || CODE_PERMISSIONS.contains(&raw)
            || AI_PERMISSIONS.contains(&raw)
        {
            Some(raw.to_string())
        } else if let Some(path) = raw
            .strip_prefix("files:read:")
            .or_else(|| raw.strip_prefix("files:write:"))
        {
            if files_path(path) {
                Some(raw.to_string())
            } else {
                problems.push(format!(
                    "permissions: `{raw}` must name `workspace` or an absolute folder"
                ));
                None
            }
        } else if let Some(origin) = raw.strip_prefix("network:") {
            match https_origin(origin) {
                Some(origin) => Some(format!("network:{origin}")),
                None => {
                    problems.push(format!("permissions: `{raw}` must name an https origin, like network:https://api.example.com"));
                    None
                }
            }
        } else {
            problems.push(format!(
                "permissions: `{raw}` is not a permission evo knows"
            ));
            None
        };
        if let Some(id) = permission {
            if !out.iter().any(|info| info.id == id) {
                out.push(PermissionInfo {
                    id,
                    reason,
                    optional,
                    implied: false,
                });
            }
        }
    }
    (out, problems)
}

/// A permission in the words the person reads when approving it.
pub fn describe_permission(permission: &str) -> String {
    match permission {
        "chat:read" => "Read all your chats".into(),
        "chat:send" => "Send messages in your chats".into(),
        "tools:call" => "Use evo's app and connector tools, with your approval".into(),
        "composer:write" => "Put text in the message box".into(),
        "storage" => "Keep its own data".into(),
        "runtime" => "Run its own code on this computer".into(),
        "hooks" => "Run commands while the agent works".into(),
        "secrets" => "Use the keys and accounts you give it".into(),
        "startup" => "Start when evo opens".into(),
        "ai" => "Use your AI models — this may cost money on your providers".into(),
        "ai.tools:read" => "Let its AI read and search your files".into(),
        "ai.tools:edit" => "Let its AI change your files".into(),
        "ai.tools:command" => "Let its AI run commands on this computer".into(),
        "ai.tools:web" => "Let its AI search and read the web".into(),
        "ai.tools:apps" => "Let its AI use evo's browser and terminals".into(),
        other => {
            if let Some(origin) = other.strip_prefix("network:") {
                format!("Connect to {origin}")
            } else if let Some(path) = other.strip_prefix("files:read:") {
                if path == "workspace" {
                    "Read files in the workspace".into()
                } else {
                    format!("Read files in {path}")
                }
            } else if let Some(path) = other.strip_prefix("files:write:") {
                if path == "workspace" {
                    "Change files in the workspace".into()
                } else {
                    format!("Change files in {path}")
                }
            } else {
                other.to_string()
            }
        }
    }
}

/// The host label a plugin's views are served under (`evo-plugin://<key>/`):
/// one origin per plugin (ADR 0021). A readable part of the id plus a hash
/// of the whole id, as a valid hostname label.
pub fn host_key(id: &str) -> String {
    // FNV-1a, 32 bits: stable across runs and builds.
    let hash = id.bytes().fold(0x811c_9dc5u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    });
    let mut readable = String::new();
    for c in id.chars().flat_map(char::to_lowercase) {
        let c = if c.is_ascii_lowercase() || c.is_ascii_digit() {
            c
        } else {
            '-'
        };
        if !(c == '-' && (readable.is_empty() || readable.ends_with('-'))) {
            readable.push(c);
        }
    }
    let readable: String = readable
        .trim_end_matches('-')
        .chars()
        .take(63 - 9)
        .collect();
    let readable = readable.trim_end_matches('-');
    if readable.is_empty() {
        format!("p{hash:08x}")
    } else {
        format!("{readable}-{hash:08x}")
    }
}

#[cfg(test)]
mod tests;
