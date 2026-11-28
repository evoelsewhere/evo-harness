//! Memory: what the agent keeps across chats.
//!
//! * **Where**: personal memories (every project) in `<agent dir>/memory/`;
//!   a project's in `<agent dir>/projects/<folder slug>/memory/`, outside the
//!   repository so they stay private.
//! * **Shape**: one markdown file per memory, `<name>.md`, with frontmatter
//!   `name`, `description` (one line, used to judge relevance), `type`
//!   (`user`, `feedback`, `project` or `reference`) and `updated`, then the
//!   memory itself. `MEMORY.md` in each folder is the index, one line per
//!   memory, rewritten on every change.
//! * **In a chat**: the system prompt carries how to use memory and both
//!   indexes ([`prompt_block`], re-read on reload); the `memory` tool saves,
//!   reads, lists and deletes ([`crate::tools::memory::create_memory_tool`]).
//! * **In the app**: the Memory panel lists, edits, adds and deletes, and
//!   switches memory off (`memoryEnabled: false` in settings) — RPC
//!   `list_memories`, `save_memory`, `delete_memory`, `set_memory_enabled`.
//! * **Topics** (`docs/features/memory-topics-and-chat-recall/spec.md`): a
//!   memory is a short document about one subject, changed in place
//!   ([`update_in`]) rather than replaced. Its frontmatter also says who
//!   changed it last (`by`: `chat`, `index` or `person`) and whether it holds
//!   a sensitive subject. A project can do without personal topics
//!   (`memoryPersonalOff`), sensitive subjects are saved only when allowed
//!   (`memorySensitive`), and [`reset`] forgets everything.

use std::path::{Path, PathBuf};

use evo_harness::frontmatter::parse_frontmatter;
use serde_json::{json, Value};

pub const TYPES: &[&str] = &["user", "feedback", "project", "reference"];
/// Entries the tidy-up moved out of topics; not a topic itself
/// (learning-layers plan 3.4).
pub const ARCHIVE: &str = "archive.md";
/// Index lines put in the system prompt per scope.
const MAX_INDEX_LINES: usize = 200;
const MAX_CONTENT: usize = 20_000;
const MAX_DESCRIPTION: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Personal,
    Project,
}

impl Scope {
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "personal" | "user" | "global" => Ok(Scope::Personal),
            "project" => Ok(Scope::Project),
            other => Err(format!("unknown scope `{other}` (personal or project)")),
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Scope::Personal => "personal",
            Scope::Project => "project",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Memory {
    pub name: String,
    pub description: String,
    pub kind: String,
    pub content: String,
    pub updated: u64,
    pub path: PathBuf,
    /// Who changed it last: `chat` (the agent in a chat), `index` (automatic
    /// indexing) or `person`. `None` for files written before topics.
    pub by: Option<String>,
    /// It holds a sensitive subject (saved because the person allowed it).
    pub sensitive: bool,
    /// A hash of the whole file, so a save from the panel can tell the topic
    /// changed since it was opened (learning-layers FR-3).
    pub hash: String,
}

/// Who writes a topic, and whether what they write is a sensitive subject.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Meta<'a> {
    /// `chat`, `index` or `person`; `None` keeps what the file says.
    pub by: Option<&'a str>,
    pub sensitive: bool,
}

/// The lines a change took out of a topic and the lines it put in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diff {
    pub removed: Vec<String>,
    pub added: Vec<String>,
}

pub fn enabled() -> bool {
    crate::settings::load_settings()
        .ok()
        .and_then(|s| s.memory_enabled)
        .unwrap_or(true)
}

/// Whether sensitive subjects (health, religion, political views, …) may be
/// saved: **Include sensitive subjects**, off unless `memorySensitive: true`.
pub fn sensitive_enabled() -> bool {
    crate::settings::load_settings()
        .ok()
        .and_then(|s| s.memory_sensitive)
        .unwrap_or(false)
}

pub fn set_sensitive(on: bool) -> Result<(), String> {
    crate::settings::update_settings_value(|settings| {
        if on {
            settings.insert("memorySensitive".into(), json!(true));
        } else {
            settings.remove("memorySensitive");
        }
    })
}

/// Whether chats in `cwd` use personal topics: **Use personal memory**, on
/// unless the folder's slug is listed in `memoryPersonalOff`.
pub fn personal_enabled(cwd: &Path) -> bool {
    let slug = slug(cwd);
    !crate::settings::load_settings()
        .ok()
        .and_then(|s| s.memory_personal_off)
        .is_some_and(|off| off.contains(&slug))
}

/// The folders that do without personal topics after `slug` is switched.
fn personal_off_after(current: &[String], slug: &str, on: bool) -> Vec<String> {
    let mut off: Vec<String> = current
        .iter()
        .filter(|known| known.as_str() != slug)
        .cloned()
        .collect();
    if !on {
        off.push(slug.to_string());
    }
    off
}

pub fn set_personal(cwd: &Path, on: bool) -> Result<(), String> {
    let slug = slug(cwd);
    crate::settings::update_settings_value(|settings| {
        let current: Vec<String> = settings
            .get("memoryPersonalOff")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let off = personal_off_after(&current, &slug, on);
        if off.is_empty() {
            settings.remove("memoryPersonalOff");
        } else {
            settings.insert("memoryPersonalOff".into(), json!(off));
        }
    })
}

pub fn set_enabled(on: bool) -> Result<(), String> {
    crate::settings::update_settings_value(|settings| {
        if on {
            settings.remove("memoryEnabled");
        } else {
            settings.insert("memoryEnabled".into(), json!(false));
        }
    })
}

/// A folder path as one folder name: `C:\work\app` → `C--work-app`.
pub fn slug(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

pub fn dir_in(agent_dir: &Path, scope: Scope, cwd: &Path) -> PathBuf {
    match scope {
        Scope::Personal => agent_dir.join("memory"),
        Scope::Project => agent_dir.join("projects").join(slug(cwd)).join("memory"),
    }
}

pub(crate) fn dir(scope: Scope, cwd: &Path) -> Result<PathBuf, String> {
    if let (Scope::Project, Some(own)) = (scope, own_dir()) {
        return Ok(own);
    }
    let agent = crate::config::agent_dir().map_err(|e| e.to_string())?;
    Ok(dir_in(&agent, scope, cwd))
}

/// A mate's own memory folder: with it, the `project` scope is the mate's
/// memory wherever the session works, and never the folder's project
/// memory (docs/features/evo-mates/spec.md, FR-11, FR-26).
fn own_dir() -> Option<PathBuf> {
    crate::session_roles::current()
        .and_then(|role| role.memory_dir.as_deref())
        .filter(|dir| !dir.trim().is_empty())
        .map(PathBuf::from)
}

/// The mate this engine speaks as, for the memory prompt.
fn own_name() -> Option<String> {
    crate::session_roles::current()
        .filter(|role| role.memory_dir.is_some())
        .and_then(|role| role.mate.as_ref())
        .map(|mate| mate.name.clone())
}

/// `Prefers Tabs!` → `prefers-tabs`.
pub fn clean_name(name: &str) -> Result<String, String> {
    let mut out = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out: String = out.trim_end_matches('-').chars().take(64).collect();
    if out.is_empty() || out == "memory" {
        return Err("a memory needs a short name (letters, digits, dashes)".into());
    }
    Ok(out)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn one_line(text: &str, max: usize) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= max {
        line
    } else {
        let mut cut: String = line.chars().take(max - 1).collect();
        cut.push('…');
        cut
    }
}

fn read(path: &Path) -> Option<Memory> {
    let source = std::fs::read_to_string(path).ok()?;
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    let (front, body) = parse_frontmatter(source.trim_start_matches('\u{feff}'))
        .unwrap_or((json!({}), source.clone()));
    let text = |key: &str| front.get(key).and_then(Value::as_str).map(str::to_string);
    Some(Memory {
        name: text("name").unwrap_or_else(|| stem.clone()),
        description: text("description").unwrap_or_else(|| one_line(&body, 120)),
        kind: text("type")
            .or_else(|| {
                front
                    .pointer("/metadata/type")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "project".into()),
        content: body.trim().to_string(),
        updated: front
            .get("updated")
            .and_then(Value::as_u64)
            .unwrap_or_else(|| {
                std::fs::metadata(path)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_millis() as u64)
            }),
        path: path.to_path_buf(),
        hash: hash_text(&source),
        by: text("by"),
        sensitive: front
            .get("sensitive")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// A short hash of a topic file's whole text (learning-layers FR-3).
pub fn hash_text(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(
        text.replace(
            "

", "
",
        )
        .as_bytes(),
    );
    digest.iter().take(12).map(|b| format!("{b:02x}")).collect()
}

/// The memories in `dir`, newest first.
pub fn list_dir(dir: &Path) -> Vec<Memory> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<Memory> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "md")
                && p.file_name()
                    .is_some_and(|n| n != "MEMORY.md" && n != ARCHIVE)
        })
        .filter_map(|p| read(&p))
        .collect();
    out.sort_by(|a, b| b.updated.cmp(&a.updated).then(a.name.cmp(&b.name)));
    out
}

pub(crate) fn write_index(dir: &Path) -> Result<(), String> {
    let memories = list_dir(dir);
    let index = dir.join("MEMORY.md");
    if memories.is_empty() {
        let _ = std::fs::remove_file(index);
        return Ok(());
    }
    let mut text = String::new();
    for m in &memories {
        text.push_str(&format!(
            "- [{}]({}.md) — {}\n",
            m.name, m.name, m.description
        ));
    }
    std::fs::write(index, text).map_err(|e| e.to_string())
}

/// Why a sensitive subject was not saved; names the setting that allows it.
pub const SENSITIVE_OFF: &str =
    "sensitive subjects are not saved: **Include sensitive subjects** is off in Settings → Memory";

/// The text of a topic file.
fn topic_file(
    name: &str,
    description: &str,
    kind: &str,
    content: &str,
    by: Option<&str>,
    sensitive: bool,
) -> String {
    // Secrets never reach disk (learning-layers FR-9).
    let description = crate::learning::redact::redact(description);
    let content = crate::learning::redact::redact(content);
    let mut front = format!(
        "---\nname: {name}\ndescription: {}\ntype: {kind}\nupdated: {}\n",
        serde_json::to_string(&description).unwrap_or_default(),
        now_ms()
    );
    if let Some(by) = by.filter(|by| ["chat", "index", "person"].contains(by)) {
        front.push_str(&format!("by: {by}\n"));
    }
    if sensitive {
        front.push_str("sensitive: true\n");
    }
    format!("{front}---\n\n{}\n", content.replace("\r\n", "\n"))
}

/// Write `text` to `path` through a temporary file and a rename, so a reader
/// (another chat's engine, the index run) never sees half a topic.
pub(crate) fn write_through_rename(path: &Path, text: &str) -> Result<(), String> {
    let partial = path.with_extension(format!("md.{}.partial", std::process::id()));
    std::fs::write(&partial, text).map_err(|e| e.to_string())?;
    std::fs::rename(&partial, path).map_err(|e| {
        let _ = std::fs::remove_file(&partial);
        e.to_string()
    })
}

fn check_content(content: &str) -> Result<(), String> {
    if content.is_empty() {
        return Err("a memory needs content".into());
    }
    if content.chars().count() > MAX_CONTENT {
        return Err(format!(
            "keep a memory under {MAX_CONTENT} characters; split it"
        ));
    }
    Ok(())
}

/// Save (or replace) a memory; `previous` renames one.
pub fn save_in(
    dir: &Path,
    name: &str,
    description: &str,
    kind: &str,
    content: &str,
    previous: Option<&str>,
    meta: Meta<'_>,
) -> Result<Memory, String> {
    let name = clean_name(name)?;
    if !TYPES.contains(&kind) {
        return Err(format!("type must be one of {}", TYPES.join(", ")));
    }
    let content = content.trim();
    check_content(content)?;
    let description = one_line(
        if description.trim().is_empty() {
            content
        } else {
            description
        },
        MAX_DESCRIPTION,
    );
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{name}.md"));
    let by = meta
        .by
        .map(str::to_string)
        .or_else(|| read(&path).and_then(|m| m.by));
    write_through_rename(
        &path,
        &topic_file(
            &name,
            &description,
            kind,
            content,
            by.as_deref(),
            meta.sensitive,
        ),
    )?;
    if let Some(old) = previous.and_then(|p| clean_name(p).ok()) {
        if old != name {
            let _ = std::fs::remove_file(dir.join(format!("{old}.md")));
        }
    }
    write_index(dir)?;
    read(&path).ok_or_else(|| "could not read the memory back".into())
}

/// The lines `old` has that `new` does not, and the other way round, around
/// the lines both start and end with.
pub fn line_diff(old: &str, new: &str) -> Diff {
    let before: Vec<&str> = old.lines().collect();
    let after: Vec<&str> = new.lines().collect();
    let head = before
        .iter()
        .zip(&after)
        .take_while(|(a, b)| a == b)
        .count();
    let tail = before[head..]
        .iter()
        .rev()
        .zip(after[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let lines = |all: &[&str]| -> Vec<String> {
        all[head..all.len() - tail]
            .iter()
            .filter(|line| !line.trim().is_empty())
            .map(|line| (*line).to_string())
            .collect()
    };
    Diff {
        removed: lines(&before),
        added: lines(&after),
    }
}

/// Change part of a topic without sending all of it: replace `old_text`
/// (which must occur exactly once) with `new_text`, or with no `old_text` add
/// `new_text` at the end. The topic's name, description and type stay. Fails,
/// leaving the topic as it was, when the text to replace is absent or occurs
/// more than once (spec FR-3).
pub fn update_in(
    dir: &Path,
    name: &str,
    old_text: Option<&str>,
    new_text: &str,
    meta: Meta<'_>,
) -> Result<(Memory, Diff), String> {
    let name = clean_name(name)?;
    let path = dir.join(format!("{name}.md"));
    let current = read(&path).ok_or_else(|| format!("no memory named `{name}`"))?;
    let new_text = new_text.replace("\r\n", "\n");
    let content = match old_text
        .map(|t| t.replace("\r\n", "\n"))
        .filter(|t| !t.is_empty())
    {
        Some(old) => match current.content.matches(old.as_str()).count() {
            0 => {
                return Err(format!(
                    "`old_text` was not found in `{name}`; read the topic and try again"
                ))
            }
            1 => current.content.replacen(old.as_str(), &new_text, 1),
            n => {
                return Err(format!(
                    "`old_text` occurs {n} times in `{name}`; give more of the text around it"
                ))
            }
        },
        None if new_text.trim().is_empty() => {
            return Err("nothing to add: give `new_text`, or `old_text` to replace".into())
        }
        None => format!("{}\n{}", current.content, new_text.trim()),
    };
    let content = content.trim();
    if content.is_empty() {
        return Err(format!(
            "that would leave `{name}` empty; delete the topic instead"
        ));
    }
    check_content(content)?;
    let diff = line_diff(&current.content, content);
    let by = meta.by.map(str::to_string).or(current.by.clone());
    write_through_rename(
        &path,
        &topic_file(
            &name,
            &current.description,
            &current.kind,
            content,
            by.as_deref(),
            current.sensitive || meta.sensitive,
        ),
    )?;
    write_index(dir)?;
    let updated = read(&path).ok_or("could not read the memory back")?;
    Ok((updated, diff))
}

/// Replace a topic's body, keeping its name, description, type and marks;
/// `expected` is the file as it was read, and a topic that changed since is
/// left alone (learning-layers plan 3.6). The file before and after.
pub fn rewrite_body_in(
    dir: &Path,
    name: &str,
    body: &str,
    expected: &str,
) -> Result<(String, String), String> {
    let name = clean_name(name)?;
    let path = dir.join(format!("{name}.md"));
    let before = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if before != expected {
        return Err(format!("`{name}` changed since it was read"));
    }
    let current = read(&path).ok_or_else(|| format!("no memory named `{name}`"))?;
    let body = body.trim();
    check_content(body)?;
    write_through_rename(
        &path,
        &topic_file(
            &name,
            &current.description,
            &current.kind,
            body,
            current.by.as_deref(),
            current.sensitive,
        ),
    )?;
    write_index(dir)?;
    let after = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    Ok((before, after))
}

pub fn delete_in(dir: &Path, name: &str) -> Result<(), String> {
    let name = clean_name(name)?;
    match std::fs::remove_file(dir.join(format!("{name}.md"))) {
        Ok(()) => write_index(dir),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err(format!("no memory named `{name}`"))
        }
        Err(e) => Err(e.to_string()),
    }
}

pub(crate) fn find_in(dir: &Path, name: &str) -> Option<Memory> {
    let name = clean_name(name).ok()?;
    read(&dir.join(format!("{name}.md")))
}

pub(crate) fn index_lines(memories: &[Memory]) -> String {
    let mut lines: Vec<String> = memories
        .iter()
        .take(MAX_INDEX_LINES)
        .map(|m| format!("- {} ({}): {}", m.name, m.kind, m.description))
        .collect();
    if memories.len() > MAX_INDEX_LINES {
        lines.push(format!(
            "- … {} more; `memory` with action `list` shows them all",
            memories.len() - MAX_INDEX_LINES
        ));
    }
    if lines.is_empty() {
        "(none yet)".into()
    } else {
        lines.join("\n")
    }
}

/// What the system prompt says about memory, for the given switches and
/// lists. `personal` is `None` when this project does without personal topics.
#[cfg(test)]
pub(crate) fn prompt_text(
    cwd: &Path,
    personal: Option<&[Memory]>,
    project: &[Memory],
    sensitive: bool,
) -> String {
    prompt_text_as(cwd, personal, project, sensitive, None)
}

/// [`prompt_text`], for a mate when `mate` names it: its `project` scope is
/// its own memory (docs/features/evo-mates/spec.md, FR-11, FR-51).
pub(crate) fn prompt_text_as(
    cwd: &Path,
    personal: Option<&[Memory]>,
    project: &[Memory],
    sensitive: bool,
    mate: Option<&str>,
) -> String {
    let only_own = "Save only what the person states or what your own work showed, never another mate's claims.";
    let scopes = match (mate, personal.is_some()) {
        (Some(name), true) => format!(
            "Use scope `project` for your own memory as {name}: how the person wants you to work (style, \
preferences, focus), your tasks, and what you learn in your work. Use scope `personal` only for facts about \
the person that every mate and chat needs (their name, role, company); when unsure, use `project`. Other \
mates do not see your own memory. {only_own}"
        ),
        (Some(name), false) => format!(
            "Use scope `project` for your own memory as {name}: what the person tells you and what you \
learn in your work (personal topics are switched off here). {only_own}"
        ),
        (None, true) => "Use scope `personal` for what holds in every project and `project` for this one.".to_string(),
        (None, false) => "This project keeps its own memory: use scope `project` only (personal topics are \
switched off here)."
            .to_string(),
    };
    let project_heading = match mate {
        Some(name) => format!("## Your own topics as {name}"),
        None => format!("## Topics for this project ({})", cwd.display()),
    };
    let sensitive_rule = if sensitive {
        "The person allows sensitive subjects (health, race or ethnicity, religion, political \
views, sexual orientation or gender identity) in memory: when you save one, set `sensitive: true`."
    } else {
        "Do not save sensitive subjects (health, race or ethnicity, religion, political views, \
sexual orientation or gender identity), even when they come up. If the person asks you to \
remember one, say that **Include sensitive subjects** in Settings → Memory allows it."
    };
    let personal_list = personal.map_or(String::new(), |list| {
        format!("## Personal topics\n{}\n\n", index_lines(list))
    });
    format!(
        "# Memory\n\n\
You have a memory that lasts across chats, kept with the `memory` tool. Use it to build up \
what makes you more useful to this person over time.\n\n\
Memory is a short list of topics: one short document per subject. Save what will matter in \
later chats and is not in the code or the project's files:\n\
- `user`: who they are, their role, expertise and preferences;\n\
- `feedback`: how they want you to work — corrections and approaches they confirmed, with the \
reason, so you can apply it to new cases;\n\
- `project`: goals, decisions, deadlines and constraints of ongoing work (write dates in full);\n\
- `reference`: where things live outside the code (dashboards, docs, tickets).\n\
{scopes}\n\n\
Before you save, look in the lists below for a topic that already covers the subject and \
change it with `action: update` (`old_text` and `new_text` to replace a part, `new_text` \
alone to add at its end). Add a topic (`action: save`) only for a new subject. Keep topics \
short; take out what turns out wrong, and delete a topic that has nothing left. When the \
person asks you to remember, change or forget something, do it right away and say so.\n\n\
Do not save one-off details of this chat or what the repository or its history already \
records. Never save identity numbers, financial account numbers, passwords, keys or tokens, \
even when asked: say that you cannot keep them in memory. {sensitive_rule}\n\n\
The lists below are names and one-line descriptions. Read a topic (`action: read`) before \
relying on it: it may be out of date — check what it names still exists.\n\n\
{personal_list}{project_heading}\n{}",
        index_lines(project),
    )
}

/// How to use memory, and what is in it, for the system prompt.
pub fn prompt_block(cwd: &Path) -> Option<String> {
    if !enabled() {
        return None;
    }
    let personal = if personal_enabled(cwd) {
        Some(list_dir(&dir(Scope::Personal, cwd).ok()?))
    } else {
        None
    };
    let project = list_dir(&dir(Scope::Project, cwd).ok()?);
    Some(prompt_text_as(
        cwd,
        personal.as_deref(),
        &project,
        sensitive_enabled(),
        own_name().as_deref(),
    ))
}

pub(crate) fn entry(m: &Memory, scope: Scope) -> Value {
    json!({
        "name": m.name,
        "description": m.description,
        "type": m.kind,
        "content": m.content,
        "updated": m.updated,
        "path": m.path,
        "scope": scope.label(),
        "by": m.by,
        "sensitive": m.sensitive,
        "hash": m.hash,
    })
}

/// How many topics there are under `agent_dir`: personal and every project's.
pub fn total_in(agent_dir: &Path) -> usize {
    memory_dirs(agent_dir)
        .iter()
        .map(|dir| list_dir(dir).len())
        .sum()
}

/// The personal memory folder and every project's, whether or not they exist.
fn memory_dirs(agent_dir: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![agent_dir.join("memory")];
    if let Ok(projects) = std::fs::read_dir(agent_dir.join("projects")) {
        dirs.extend(projects.flatten().map(|entry| entry.path().join("memory")));
    }
    dirs
}

/// Forget everything under `agent_dir`: every topic and index, personal and
/// in every project. Returns how many topics went. Other files in those
/// folders are left alone.
pub fn reset_in(agent_dir: &Path) -> Result<usize, String> {
    let mut deleted = 0;
    for dir in memory_dirs(agent_dir) {
        for memory in list_dir(&dir) {
            std::fs::remove_file(&memory.path)
                .map_err(|e| format!("{}: {e}", memory.path.display()))?;
            deleted += 1;
        }
        let _ = std::fs::remove_file(dir.join("MEMORY.md"));
    }
    Ok(deleted)
}

/// **Reset memory**: forget every topic in every scope and the memory rows
/// of the journal (spec FR-9). Returns how many topics went.
pub fn reset() -> Result<usize, String> {
    let agent = crate::config::agent_dir().map_err(|e| e.to_string())?;
    let deleted = reset_in(&agent)?;
    let conn = crate::app_db::open().map_err(|e| e.to_string())?;
    crate::app_db::journal::clear(&conn, "memory").map_err(|e| e.to_string())?;
    Ok(deleted)
}

/// The body of a topic file: its text without the frontmatter.
pub(crate) fn body_of(file: &str) -> String {
    parse_frontmatter(file.trim_start_matches('\u{feff}'))
        .map(|(_, body)| body)
        .unwrap_or_else(|_| file.to_string())
        .trim()
        .to_string()
}

/// Whose memory a scope folder is, as the journal names it: `personal`,
/// `project:<slug>`, or `mate:<id>` for a mate's own memory
/// (docs/features/learning-layers/plan.md, 3.1).
pub fn owner(scope: Scope, cwd: &Path) -> String {
    match scope {
        Scope::Personal => "personal".into(),
        Scope::Project => match own_mate_id() {
            Some(id) => format!("mate:{id}"),
            None => format!("project:{}", slug(cwd)),
        },
    }
}

/// The mate whose own memory this engine's `project` scope is.
fn own_mate_id() -> Option<String> {
    crate::session_roles::current()
        .filter(|role| {
            role.memory_dir
                .as_deref()
                .is_some_and(|dir| !dir.trim().is_empty())
        })
        .and_then(|role| role.mate.as_ref())
        .map(|mate| mate.id.clone())
}

/// One recorded change as the Memory panel shows it: the lines it took out
/// and put in, not the whole files.
fn change_entry(change: &crate::app_db::journal::Entry) -> Value {
    let diff = line_diff(
        &change.before.as_deref().map(body_of).unwrap_or_default(),
        &change.after.as_deref().map(body_of).unwrap_or_default(),
    );
    json!({
        "id": change.id,
        "at": change.at_ms,
        "owner": change.owner,
        "scope": if change.owner == "personal" { "personal" } else { "project" },
        "name": change.name,
        "action": change.action,
        "by": change.by,
        "chatId": change.chat_id,
        "chatTitle": change.chat_title,
        "removed": diff.removed,
        "added": diff.added,
        "undoable": change.undoable,
        "undone": change.undone_at.is_some(),
    })
}

/// The changes to personal topics and to the topics of the project in `cwd`
/// (a mate's own topics, for a mate), newest first (learning-layers FR-1).
pub fn changes(cwd: &Path, limit: u32) -> Result<Value, String> {
    let conn = crate::app_db::open().map_err(|e| e.to_string())?;
    let project = owner(Scope::Project, cwd);
    let list = crate::app_db::journal::list(&conn, &["personal", &project], "memory", limit)
        .map_err(|e| e.to_string())?;
    Ok(json!({ "changes": list.iter().map(change_entry).collect::<Vec<_>>() }))
}

/// Record a change indexing made to topic `name` in `folder` (a memory
/// folder of `scope`), with the topic file before and after.
pub fn record_change(
    scope: Scope,
    cwd: &Path,
    name: &str,
    chat_id: Option<&str>,
    before: Option<String>,
    after: Option<String>,
) -> Result<(), String> {
    journal_row(&owner(scope, cwd), name, "index", chat_id, before, after).map(|_| ())
}

/// Record a change to topic `name` of `owner` by `by`, with the topic file
/// before and after. `None` when nothing changed.
pub fn journal_row(
    owner: &str,
    name: &str,
    by: &str,
    chat_id: Option<&str>,
    before: Option<String>,
    after: Option<String>,
) -> Result<Option<i64>, String> {
    let action = match (&before, &after) {
        (a, b) if a == b => return Ok(None),
        (None, Some(_)) => "added",
        (Some(_), None) => "deleted",
        _ => "updated",
    };
    record_row(new_row(
        "memory", owner, name, action, by, chat_id, before, after,
    ))
}

/// Texts longer than this are not kept in the journal: such a row cannot be
/// undone (learning-layers FR-2).
pub const JOURNAL_TEXT_MAX: usize = 200_000;

/// A journal row with the texts dropped when one held a secret or was too
/// long to keep; such a row says it cannot be undone (FR-2).
#[allow(clippy::too_many_arguments)]
pub(crate) fn new_row(
    subject: &str,
    owner: &str,
    name: &str,
    action: &str,
    by: &str,
    chat_id: Option<&str>,
    before: Option<String>,
    after: Option<String>,
) -> crate::app_db::journal::NewRow {
    let keepable = |text: &Option<String>| {
        text.as_deref().is_none_or(|text| {
            text.len() <= JOURNAL_TEXT_MAX && !crate::learning::redact::has_secret(text)
        })
    };
    let undoable = keepable(&before) && keepable(&after);
    let changed = line_diff(
        &before.as_deref().map(body_of).unwrap_or_default(),
        &after.as_deref().map(body_of).unwrap_or_default(),
    );
    let lines: Vec<String> = changed
        .removed
        .iter()
        .map(|line| format!("- {line}"))
        .chain(changed.added.iter().map(|line| format!("+ {line}")))
        .collect();
    let diff: String = crate::learning::redact::redact(&lines.join("\n"))
        .chars()
        .take(4_000)
        .collect();
    crate::app_db::journal::NewRow {
        at_ms: now_ms() as i64,
        owner: owner.to_string(),
        subject: subject.into(),
        name: name.to_string(),
        action: action.into(),
        by: by.into(),
        chat_id: chat_id.map(str::to_string),
        reply_id: None,
        diff: Some(diff),
        before: if undoable { before } else { None },
        after: if undoable { after } else { None },
        undoable,
    }
}

/// Store a journal row; its id.
pub(crate) fn record_row(row: crate::app_db::journal::NewRow) -> Result<Option<i64>, String> {
    let mut conn = crate::app_db::open().map_err(|e| e.to_string())?;
    crate::app_db::journal::record(&mut conn, &row)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// What undoing a change does to its topic file in `dir`: put `before` back
/// (or remove the topic it added, or bring back the one it deleted). Refused
/// when the topic is no longer what the change left: it was changed since.
pub fn undo_in(
    dir: &Path,
    name: &str,
    before: Option<&str>,
    after: Option<&str>,
) -> Result<(), String> {
    let name = clean_name(name)?;
    let path = dir.join(format!("{name}.md"));
    let current = std::fs::read_to_string(&path).ok();
    if current.as_deref() != after {
        return Err(format!(
            "`{name}` was changed since; open the topic to edit it"
        ));
    }
    match before {
        Some(text) => {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            write_through_rename(&path, text)?;
        }
        None => std::fs::remove_file(&path).map_err(|e| e.to_string())?,
    }
    write_index(dir)
}

/// Undo change `id` from the Memory panel of a chat in `cwd`.
pub fn undo_change(cwd: &Path, id: i64) -> Result<(), String> {
    let conn = crate::app_db::open().map_err(|e| e.to_string())?;
    let change = crate::app_db::journal::get(&conn, id)
        .map_err(|e| e.to_string())?
        .ok_or("no such change")?;
    if change.undone_at.is_some() {
        return Err("that change was undone before".into());
    }
    if !change.undoable {
        return Err(
            "that change cannot be undone: its earlier text held a secret or was too long to keep"
                .into(),
        );
    }
    let scope = if change.owner == "personal" {
        Scope::Personal
    } else {
        Scope::Project
    };
    if change.subject != "memory" || change.owner != owner(scope, cwd) {
        return Err("that change belongs to another project".into());
    }
    undo_in(
        &dir(scope, cwd)?,
        &change.name,
        change.before.as_deref(),
        change.after.as_deref(),
    )?;
    crate::app_db::journal::mark_undone(&conn, id, now_ms() as i64).map_err(|e| e.to_string())?;
    // The undo is itself a row (FR-2).
    record_row(new_row(
        "memory",
        &change.owner,
        &change.name,
        "undone",
        "undo",
        change.chat_id.as_deref(),
        change.after.clone(),
        change.before.clone(),
    ))
    .map(|_| ())
}

/// Everything for the Memory panel. `chat_memory` is false for a chat
/// started without memory (`--no-memory`, `--incognito`).
pub fn describe(cwd: &Path, chat_memory: bool) -> Result<Value, String> {
    let personal = dir(Scope::Personal, cwd)?;
    let project = dir(Scope::Project, cwd)?;
    let agent = crate::config::agent_dir().map_err(|e| e.to_string())?;
    Ok(json!({
        "enabled": enabled(),
        "chatMemory": chat_memory,
        "usePersonal": personal_enabled(cwd),
        "sensitive": sensitive_enabled(),
        "total": total_in(&agent),
        "personal": {
            "dir": personal,
            "items": list_dir(&personal).iter().map(|m| entry(m, Scope::Personal)).collect::<Vec<_>>(),
        },
        "project": {
            "dir": project,
            "folder": cwd,
            "items": list_dir(&project).iter().map(|m| entry(m, Scope::Project)).collect::<Vec<_>>(),
        },
    }))
}

/// How a save from the panel ended.
#[derive(Debug, Clone, PartialEq)]
pub enum Saved {
    Done(Value),
    /// The topic changed since the panel opened it: nothing was written, and
    /// this is the topic as it is now (`null` when it was deleted; FR-3).
    Conflict(Value),
}

/// Save from the Memory panel: the person wrote it. `opened` is the hash of
/// the topic when the panel opened it; a topic that changed since is not
/// overwritten (learning-layers FR-3). `None` overwrites (**Overwrite with
/// mine**, or a new topic).
#[allow(clippy::too_many_arguments)]
pub fn save(
    cwd: &Path,
    scope: Scope,
    name: &str,
    description: &str,
    kind: &str,
    content: &str,
    previous: Option<&str>,
    opened: Option<&str>,
) -> Result<Saved, String> {
    let folder = dir(scope, cwd)?;
    let opened_name = previous.unwrap_or(name);
    let file_of = |name: &str| {
        clean_name(name)
            .ok()
            .and_then(|name| std::fs::read_to_string(folder.join(format!("{name}.md"))).ok())
    };
    let opened_before = file_of(opened_name);
    if let Some(hash) = opened.filter(|hash| !hash.is_empty()) {
        let now = opened_before.as_deref().map(hash_text);
        if now.as_deref() != Some(hash) {
            let current = find_in(&folder, opened_name).map(|m| entry(&m, scope));
            return Ok(Saved::Conflict(current.unwrap_or(Value::Null)));
        }
    }
    let name_before = file_of(name);
    let m = save_in(
        &folder,
        name,
        description,
        kind,
        content,
        previous,
        Meta {
            by: Some("person"),
            sensitive: false,
        },
    )?;
    let owner = owner(scope, cwd);
    let chat = crate::office::chat_id();
    let after = std::fs::read_to_string(&m.path).ok();
    journal_row(&owner, &m.name, "person", chat, name_before, after)?;
    if let Some(old) = previous.and_then(|p| clean_name(p).ok()) {
        if old != m.name {
            journal_row(&owner, &old, "person", chat, opened_before, None)?;
        }
    }
    Ok(Saved::Done(entry(&m, scope)))
}

/// Delete from the panel: the person did it.
pub fn delete(cwd: &Path, scope: Scope, name: &str) -> Result<(), String> {
    let folder = dir(scope, cwd)?;
    let name = clean_name(name)?;
    let before = std::fs::read_to_string(folder.join(format!("{name}.md"))).ok();
    delete_in(&folder, &name)?;
    journal_row(
        &owner(scope, cwd),
        &name,
        "person",
        crate::office::chat_id(),
        before,
        None,
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name(" Prefers Tabs! ").unwrap(), "prefers-tabs");
        assert!(clean_name("  !! ").is_err());
        assert!(clean_name("MEMORY").is_err());
    }

    #[test]
    fn save_list_rename_and_delete_keep_the_index() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        save_in(
            d,
            "prefers-tabs",
            "Uses tabs",
            "user",
            "Tabs, width 4.",
            None,
            Meta::default(),
        )
        .unwrap();
        save_in(
            d,
            "release-freeze",
            "",
            "project",
            "Freeze from 2026-12-01.\nWhy: launch.",
            None,
            Meta::default(),
        )
        .unwrap();
        let all = list_dir(d);
        assert_eq!(all.len(), 2);
        let index = std::fs::read_to_string(d.join("MEMORY.md")).unwrap();
        assert!(
            index.contains("[prefers-tabs](prefers-tabs.md) — Uses tabs"),
            "{index}"
        );
        // An empty description falls back to the content's first words.
        assert!(index.contains("Freeze from 2026-12-01."), "{index}");
        // Saving under a new name with `previous` renames.
        save_in(
            d,
            "indent-style",
            "Indent",
            "user",
            "Tabs.",
            Some("prefers-tabs"),
            Meta::default(),
        )
        .unwrap();
        assert!(!d.join("prefers-tabs.md").exists());
        assert!(find_in(d, "indent-style").is_some());
        assert!(save_in(d, "x", "", "secret", "y", None, Meta::default()).is_err());
        delete_in(d, "indent-style").unwrap();
        delete_in(d, "release-freeze").unwrap();
        assert!(!d.join("MEMORY.md").exists());
        assert!(delete_in(d, "gone").is_err());
    }

    // docs/features/memory-topics-and-chat-recall/spec.md (FR-3).
    #[test]
    fn topic_update_replaces_text_that_occurs_once() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        save_in(
            d,
            "commit-style",
            "How to write commits",
            "feedback",
            "Small commits.\nConventional subjects.",
            None,
            Meta::default(),
        )
        .unwrap();
        let (m, diff) = update_in(
            d,
            "commit-style",
            Some("Small commits."),
            "Small commits, one task each.",
            Meta {
                by: Some("chat"),
                sensitive: false,
            },
        )
        .unwrap();
        assert_eq!(
            m.content,
            "Small commits, one task each.\nConventional subjects."
        );
        assert_eq!(diff.removed, ["Small commits."]);
        assert_eq!(diff.added, ["Small commits, one task each."]);
        // The rest of the topic is as it was.
        assert_eq!(
            (m.description.as_str(), m.kind.as_str(), m.by.as_deref()),
            ("How to write commits", "feedback", Some("chat"))
        );

        // No old text: the new text goes at the end.
        let (m, diff) =
            update_in(d, "commit-style", None, "No force pushes.", Meta::default()).unwrap();
        assert!(m
            .content
            .ends_with("Conventional subjects.\nNo force pushes."));
        assert!(diff.removed.is_empty());
        assert_eq!(diff.added, ["No force pushes."]);

        // Forgetting a fact: its text is replaced by nothing.
        let (m, diff) = update_in(
            d,
            "commit-style",
            Some("Conventional subjects.\n"),
            "",
            Meta::default(),
        )
        .unwrap();
        assert_eq!(m.content, "Small commits, one task each.\nNo force pushes.");
        assert_eq!(diff.removed, ["Conventional subjects."]);
        assert!(diff.added.is_empty());

        let index = std::fs::read_to_string(d.join("MEMORY.md")).unwrap();
        assert!(
            index.contains("[commit-style](commit-style.md) — How to write commits"),
            "{index}"
        );
    }

    #[test]
    fn topic_update_fails_without_changing_the_topic() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let original = "alpha beta\nalpha gamma";
        save_in(d, "t", "T", "project", original, None, Meta::default()).unwrap();
        let fails = |old: Option<&str>, new: &str| {
            update_in(d, "t", old, new, Meta::default()).unwrap_err()
        };
        assert!(fails(Some("delta"), "x").contains("not found"));
        assert!(fails(Some("alpha"), "x").contains("2 times"));
        assert!(fails(Some(original), "").contains("empty"));
        assert!(fails(None, "  ").contains("nothing to add"));
        assert!(update_in(d, "nope", None, "x", Meta::default())
            .unwrap_err()
            .contains("no memory named"));
        assert_eq!(find_in(d, "t").unwrap().content, original);
        // A write goes through a rename: nothing but topics and the index is left.
        let mut names: Vec<String> = std::fs::read_dir(d)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["MEMORY.md", "t.md"]);
    }

    // FR-1, NFR-3: memories saved before topics read as they are.
    #[test]
    fn older_memory_files_read_as_topics() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        std::fs::write(
            d.join("prefers-tabs.md"),
            "---\nname: prefers-tabs\ndescription: \"Uses tabs\"\ntype: user\nupdated: 5\n---\n\nTabs, width 4.\n",
        )
        .unwrap();
        let m = find_in(d, "prefers-tabs").unwrap();
        assert_eq!(
            (
                m.by,
                m.sensitive,
                m.content.as_str(),
                m.updated,
                m.kind.as_str()
            ),
            (None, false, "Tabs, width 4.", 5, "user")
        );
        // Saved again with who changed it and the sensitive mark.
        let m = save_in(
            d,
            "health",
            "",
            "user",
            "Has a bad back.",
            None,
            Meta {
                by: Some("person"),
                sensitive: true,
            },
        )
        .unwrap();
        assert_eq!((m.by.as_deref(), m.sensitive), (Some("person"), true));
        assert_eq!(list_dir(d).len(), 2);
    }

    fn topic(name: &str) -> Memory {
        Memory {
            name: name.into(),
            description: "what it holds".into(),
            kind: "user".into(),
            content: "text".into(),
            updated: 0,
            path: PathBuf::new(),
            by: None,
            sensitive: false,
            hash: String::new(),
        }
    }

    // FR-4, FR-17, FR-18: what the prompt tells the agent.
    #[test]
    fn memory_prompt_follows_settings() {
        let cwd = Path::new("/work/app");
        let personal = [topic("role")];
        let project = [topic("freeze")];
        let usual = prompt_text(cwd, Some(&personal), &project, false);
        assert!(usual.contains("## Personal topics\n- role (user): what it holds"));
        assert!(usual.contains("## Topics for this project (/work/app)\n- freeze (user)"));
        // Update a covering topic; add one only for a new subject.
        assert!(usual.contains("`action: update`"));
        assert!(usual.contains("only for a new subject"));
        assert!(usual.contains("remember, change or forget"));
        // The never-save list is there whatever the settings say.
        let never =
            "Never save identity numbers, financial account numbers, passwords, keys or tokens";
        assert!(usual.contains(never));
        assert!(usual.contains("Do not save sensitive subjects"));
        assert!(usual.contains("**Include sensitive subjects**"));

        let allowed = prompt_text(cwd, Some(&personal), &project, true);
        assert!(allowed.contains("`sensitive: true`"));
        assert!(!allowed.contains("Do not save sensitive subjects"));
        assert!(allowed.contains(never));
    }

    // docs/features/evo-mates/spec.md (FR-11, FR-51): a mate's own memory.
    #[test]
    fn a_mates_prompt_names_its_own_memory() {
        let cwd = Path::new("/work/app");
        let text = prompt_text_as(
            cwd,
            Some(&[topic("prefers-tabs")]),
            &[topic("launch")],
            false,
            Some("Writer"),
        );
        assert!(text.contains("## Your own topics as Writer"));
        assert!(text.contains("- launch (user)"));
        assert!(text.contains("never another mate's claims"));
        assert!(!text.contains("Topics for this project"));
        let plain = prompt_text(cwd, Some(&[topic("prefers-tabs")]), &[], false);
        assert!(
            plain.contains("## Topics for this project (/work/app)")
                || plain.contains("## Topics for this project (")
        );
        assert!(!plain.contains("Your own topics"));
    }

    // FR-7: a project that does without personal topics.
    #[test]
    fn personal_memory_switch_drops_personal_topics() {
        let cwd = Path::new("/work/app");
        let alone = prompt_text(cwd, None, &[topic("freeze")], false);
        assert!(!alone.contains("Personal topics"));
        assert!(alone.contains("use scope `project` only"));
        assert!(alone.contains("- freeze (user)"));

        let none: Vec<String> = Vec::new();
        let off = personal_off_after(&none, "C--work-app", false);
        assert_eq!(off, ["C--work-app"]);
        // Switching it off twice lists the folder once; other folders stay.
        let more = personal_off_after(
            &["C--other".into(), "C--work-app".into()],
            "C--work-app",
            false,
        );
        assert_eq!(more, ["C--other", "C--work-app"]);
        assert_eq!(personal_off_after(&more, "C--work-app", true), ["C--other"]);
        assert!(personal_off_after(&off, "C--work-app", true).is_empty());
    }

    // FR-29: undo puts a topic back, unless it changed since.
    #[test]
    fn undo_indexing_change_restores_the_topic() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let file = |name: &str| std::fs::read_to_string(d.join(format!("{name}.md"))).ok();

        // An update: back to the earlier text.
        save_in(
            d,
            "style",
            "Style",
            "feedback",
            "Small commits.",
            None,
            Meta::default(),
        )
        .unwrap();
        let before = file("style");
        update_in(
            d,
            "style",
            None,
            "No force pushes.",
            Meta {
                by: Some("index"),
                sensitive: false,
            },
        )
        .unwrap();
        let after = file("style");
        undo_in(d, "style", before.as_deref(), after.as_deref()).unwrap();
        assert_eq!(file("style"), before);
        assert_eq!(find_in(d, "style").unwrap().content, "Small commits.");

        // A topic indexing added: removed again, and out of the index.
        save_in(d, "added", "", "user", "New fact.", None, Meta::default()).unwrap();
        let added = file("added");
        undo_in(d, "added", None, added.as_deref()).unwrap();
        assert!(file("added").is_none());
        assert!(!std::fs::read_to_string(d.join("MEMORY.md"))
            .unwrap()
            .contains("added"));

        // A topic indexing deleted: back as it was.
        let kept = file("style");
        delete_in(d, "style").unwrap();
        undo_in(d, "style", kept.as_deref(), None).unwrap();
        assert_eq!(file("style"), kept);
        assert!(std::fs::read_to_string(d.join("MEMORY.md"))
            .unwrap()
            .contains("style"));

        // Changed since: refused, and nothing is written.
        update_in(d, "style", None, "Rebase before merging.", Meta::default()).unwrap();
        let newer = file("style");
        let error = undo_in(d, "style", before.as_deref(), after.as_deref()).unwrap_err();
        assert!(error.contains("changed since"), "{error}");
        assert_eq!(file("style"), newer);
        // The same for a topic that was deleted since, or re-created since.
        assert!(undo_in(d, "gone", Some("x"), Some("y")).is_err());
        assert!(undo_in(d, "style", Some("x"), None).is_err());

        // What the panel shows of a change: lines, not files.
        let shown = line_diff(
            &body_of(before.as_deref().unwrap()),
            &body_of(after.as_deref().unwrap()),
        );
        assert_eq!(shown.removed, Vec::<String>::new());
        assert_eq!(shown.added, ["No force pushes."]);
    }

    // FR-9: reset forgets every topic in every scope.
    #[test]
    fn reset_leaves_no_topic() {
        let agent_dir = tempfile::tempdir().unwrap();
        let agent = agent_dir.path();
        let personal = dir_in(agent, Scope::Personal, Path::new("/work/a"));
        let a = dir_in(agent, Scope::Project, Path::new("/work/a"));
        let b = dir_in(agent, Scope::Project, Path::new("/work/b"));
        for (dir, name) in [
            (&personal, "role"),
            (&personal, "style"),
            (&a, "freeze"),
            (&b, "plan"),
        ] {
            save_in(dir, name, "", "project", "Text.", None, Meta::default()).unwrap();
        }
        std::fs::write(a.join("notes.txt"), "not a topic").unwrap();
        assert_eq!(total_in(agent), 4);
        assert_eq!(reset_in(agent).unwrap(), 4);
        assert_eq!(total_in(agent), 0);
        for dir in [&personal, &a, &b] {
            assert!(!dir.join("MEMORY.md").exists());
            assert!(list_dir(dir).is_empty());
        }
        assert!(a.join("notes.txt").exists(), "only topics and indexes go");
        assert_eq!(reset_in(agent).unwrap(), 0);
        // An agent folder with no memory yet resets to nothing.
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(reset_in(empty.path()).unwrap(), 0);
    }

    #[test]
    fn project_memories_live_outside_the_project() {
        let agent = Path::new("/home/me/.evo/agent");
        let p = dir_in(agent, Scope::Project, Path::new("/work/app"));
        assert!(p.starts_with(agent));
        assert!(p.ends_with("memory"));
        assert_eq!(slug(Path::new("C:\\work\\app")), "C--work-app");
    }
}
