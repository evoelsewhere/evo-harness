//! Context a plugin's run attaches, checked here (docs/features/plugin-ai/
//! spec.md, FR-17): the open chat's last messages (`chat:read`, never an
//! incognito chat), files from places the plugin may read
//! (`files:read:<folder>` or `files:read:workspace`), and one of its own
//! skills. The engine reads the files and the skill; nothing outside these
//! reaches it.
//!
//! ```text
//! context: { openChat?: true | { lastMessages: n }, files?: [path], skill?: name }
//! ```

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::Refusal;

/// The open chat's messages a run takes by default, and at most.
pub const CHAT_MESSAGES: u64 = 20;
pub const MAX_CHAT_MESSAGES: u64 = 100;
/// Files a run attaches, at most.
pub const MAX_FILES: usize = 10;

fn bad(message: impl Into<String>) -> Refusal {
    ("bad_request", message.into())
}

/// The places `approved` lets a plugin read, as folders: absolute ones, and
/// the open workspace for `files:read:workspace`.
pub fn read_places(approved: &[String], workspace: Option<&Path>) -> Vec<PathBuf> {
    approved
        .iter()
        .filter_map(|p| p.strip_prefix("files:read:"))
        .filter_map(|place| if place == "workspace" { workspace.map(Path::to_path_buf) } else { Some(PathBuf::from(place)) })
        .collect()
}

/// `path` as a real file inside one of `places`, or why not. A relative
/// path is taken from the workspace.
pub fn readable(path: &str, places: &[PathBuf], workspace: Option<&Path>) -> Result<PathBuf, Refusal> {
    let given = Path::new(path);
    let full = if given.is_absolute() {
        given.to_path_buf()
    } else {
        workspace.ok_or_else(|| bad(format!("`{path}` is relative and no workspace is open")))?.join(given)
    };
    let real = dunce_canonical(&full).map_err(|_| ("not_found", format!("there is no file {path}")))?;
    if !real.is_file() {
        return Err(bad(format!("{path} is not a file")));
    }
    let inside = places.iter().filter_map(|place| dunce_canonical(place).ok()).any(|place| real.starts_with(&place));
    if !inside {
        return Err(("no_permission", format!("the plugin may not read {path}: it is outside the places it may read")));
    }
    Ok(real)
}

/// A path without Windows' `\\?\` prefix, links resolved.
fn dunce_canonical(path: &Path) -> std::io::Result<PathBuf> {
    let real = std::fs::canonicalize(path)?;
    let text = real.to_string_lossy();
    Ok(match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => real,
    })
}

/// The SKILL.md of the plugin's skill `name`: a folder `name` in one of its
/// skill folders, or the plugin itself when it is one skill.
pub fn skill_file(skill_dirs: &[PathBuf], plugin_name: &str, name: &str) -> Option<PathBuf> {
    if name.is_empty() || name.contains(['/', '\\']) || name.contains("..") {
        return None;
    }
    skill_dirs.iter().find_map(|dir| {
        let nested = dir.join(name).join("SKILL.md");
        if nested.is_file() {
            return Some(nested);
        }
        let root = dir.join("SKILL.md");
        (name == plugin_name && root.is_file()).then_some(root)
    })
}

/// What a request's `context` asks for, before it is gathered.
#[derive(Debug, Default, PartialEq)]
pub struct Asked {
    pub chat: Option<u64>,
    pub files: Vec<String>,
    pub skill: Option<String>,
}

/// Read `context` and check the plugin's permissions for each part.
pub fn asked(context: Option<&Value>, approved: &[String]) -> Result<Asked, Refusal> {
    let Some(context) = context.filter(|c| !c.is_null()) else { return Ok(Asked::default()) };
    let context = context.as_object().ok_or_else(|| bad("`context` must be an object"))?;
    let mut out = Asked::default();
    match context.get("openChat") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => {}
        Some(Value::Bool(true)) => out.chat = Some(CHAT_MESSAGES),
        Some(Value::Object(o)) => out.chat = Some(o.get("lastMessages").and_then(Value::as_u64).unwrap_or(CHAT_MESSAGES).clamp(1, MAX_CHAT_MESSAGES)),
        Some(_) => return Err(bad("`context.openChat` must be true or { lastMessages }")),
    }
    if out.chat.is_some() && !approved.iter().any(|p| p == "chat:read") {
        return Err(("no_permission", "attaching the open chat needs the `chat:read` permission".into()));
    }
    if let Some(files) = context.get("files").filter(|f| !f.is_null()) {
        let files = files.as_array().ok_or_else(|| bad("`context.files` must be a list of paths"))?;
        if files.len() > MAX_FILES {
            return Err(bad(format!("a run attaches at most {MAX_FILES} files")));
        }
        for file in files {
            out.files.push(file.as_str().filter(|f| !f.is_empty()).ok_or_else(|| bad("`context.files` must be a list of paths"))?.to_string());
        }
        if !out.files.is_empty() && !approved.iter().any(|p| p.starts_with("files:read:")) {
            return Err(("no_permission", "attaching files needs a `files:read:<place>` permission".into()));
        }
    }
    if let Some(skill) = context.get("skill").filter(|s| !s.is_null()) {
        out.skill = Some(skill.as_str().filter(|s| !s.is_empty()).ok_or_else(|| bad("`context.skill` must be one of the plugin's skill names"))?.to_string());
    }
    Ok(out)
}

/// Gather `asked` for a run from `window` (the view's window, or the main
/// one): the engine's `context`, or `None` when nothing was asked.
pub async fn gather(
    app: &tauri::AppHandle,
    asked: Asked,
    approved: &[String],
    skill_dirs: &[PathBuf],
    plugin_name: &str,
    window: &str,
) -> Result<Option<Value>, Refusal> {
    if asked == Asked::default() {
        return Ok(None);
    }
    let open = crate::plugin_ui::bridge::active_chat(app, window);
    let workspace = open.as_deref().map(|chat| crate::agent_tools::chat_folder(app, chat));
    let mut context = json!({});
    if let Some(limit) = asked.chat {
        let chat = open.clone().ok_or(("no_chat", "no chat is open to attach (incognito chats never are)".to_string()))?;
        let title = crate::plugin_ui::bridge::visible_chat(app, &chat).map(|row| row.title).unwrap_or_default();
        let reply = crate::plugin_ui::bridge::chat_messages(app, &chat, limit).await?;
        let lines: Vec<Value> = reply["messages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|m| matches!(m["role"].as_str(), Some("user" | "assistant")))
            .filter(|m| m["text"].as_str().is_some_and(|t| !t.trim().is_empty()))
            .map(|m| json!({ "role": m["role"], "text": m["text"] }))
            .collect();
        context["chat"] = json!({ "title": title, "messages": lines });
    }
    if !asked.files.is_empty() {
        let places = read_places(approved, workspace.as_deref());
        let files = asked.files.iter().map(|f| readable(f, &places, workspace.as_deref()).map(|p| p.to_string_lossy().to_string())).collect::<Result<Vec<_>, _>>()?;
        context["files"] = json!(files);
    }
    if let Some(name) = &asked.skill {
        let path = skill_file(skill_dirs, plugin_name, name).ok_or_else(|| bad(format!("{plugin_name} has no skill `{name}`")))?;
        context["skill"] = json!({ "name": name, "path": path.to_string_lossy() });
    }
    Ok(Some(context))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh folder, removed when dropped.
    struct Scratch(PathBuf);
    impl Scratch {
        fn new(name: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!("evo-ai-context-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn perms(list: &[&str]) -> Vec<String> {
        list.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn each_part_of_the_context_needs_its_permission() {
        assert_eq!(asked(None, &[]).unwrap(), Asked::default());
        let chat = json!({ "openChat": true });
        assert_eq!(asked(Some(&chat), &[]).unwrap_err().0, "no_permission");
        assert_eq!(asked(Some(&chat), &perms(&["chat:read"])).unwrap().chat, Some(CHAT_MESSAGES));
        let many = json!({ "openChat": { "lastMessages": 1000 } });
        assert_eq!(asked(Some(&many), &perms(&["chat:read"])).unwrap().chat, Some(MAX_CHAT_MESSAGES));
        let files = json!({ "files": ["a.md"] });
        assert_eq!(asked(Some(&files), &perms(&["chat:read"])).unwrap_err().0, "no_permission");
        assert_eq!(asked(Some(&files), &perms(&["files:read:workspace"])).unwrap().files, ["a.md"]);
        let eleven = json!({ "files": vec!["a"; 11] });
        assert_eq!(asked(Some(&eleven), &perms(&["files:read:workspace"])).unwrap_err().0, "bad_request");
        assert_eq!(asked(Some(&json!({ "skill": "analyst" })), &[]).unwrap().skill.as_deref(), Some("analyst"), "its own skills need no permission");
        assert!(asked(Some(&json!("chat")), &[]).is_err());
    }

    #[test]
    fn files_are_read_only_inside_approved_places() {
        let dir = Scratch::new("files");
        let place = dir.path().join("notes");
        let other = dir.path().join("secrets");
        std::fs::create_dir_all(&place).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(place.join("a.md"), "a").unwrap();
        std::fs::write(other.join("key.txt"), "k").unwrap();
        let approved = vec![format!("files:read:{}", place.display()), "files:read:workspace".to_string()];
        let workspace = dir.path().join("ws");
        std::fs::create_dir_all(workspace.join("src")).unwrap();
        std::fs::write(workspace.join("src/main.rs"), "fn main() {}").unwrap();
        let places = read_places(&approved, Some(&workspace));
        assert_eq!(places.len(), 2);
        assert!(readable(&place.join("a.md").to_string_lossy(), &places, None).is_ok());
        assert!(readable("src/main.rs", &places, Some(&workspace)).is_ok(), "relative to the workspace");
        assert_eq!(readable(&other.join("key.txt").to_string_lossy(), &places, None).unwrap_err().0, "no_permission");
        let escape = place.join("..").join("secrets").join("key.txt");
        assert_eq!(readable(&escape.to_string_lossy(), &places, None).unwrap_err().0, "no_permission", "`..` does not leave the place");
        assert_eq!(readable(&place.join("gone.md").to_string_lossy(), &places, None).unwrap_err().0, "not_found");
        assert_eq!(read_places(&approved, None).len(), 1, "no workspace open: only the named folder");
    }

    #[test]
    fn a_skill_is_one_of_the_plugins_own() {
        let dir = Scratch::new("skill");
        let skills = dir.path().join("skills");
        std::fs::create_dir_all(skills.join("analyst")).unwrap();
        std::fs::write(skills.join("analyst/SKILL.md"), "x").unwrap();
        std::fs::write(dir.path().join("SKILL.md"), "root").unwrap();
        let dirs = vec![skills.clone(), dir.path().to_path_buf()];
        assert_eq!(skill_file(&dirs, "stocks", "analyst"), Some(skills.join("analyst/SKILL.md")));
        assert_eq!(skill_file(&dirs, "stocks", "stocks"), Some(dir.path().join("SKILL.md")), "a plugin that is one skill");
        assert_eq!(skill_file(&dirs, "stocks", "../analyst"), None);
        assert_eq!(skill_file(&dirs, "stocks", "missing"), None);
    }
}
