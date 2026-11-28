//! Team files (docs/features/evo-mates/spec.md, FR-20, FR-21, FR-27; plan
//! 3.5 "Team files"): a mate, or a channel with its mates, in one versioned
//! Markdown file. The front matter holds the channel and each mate's name,
//! role, face, model need ("capable", never a model ID), tools, connectors,
//! skills and approval mode; the body holds each mate's instructions under a
//! `## <name>` heading. A file never holds ids, folders, secrets, memories,
//! messages or keys.
//!
//! Import shows a preview first and creates nothing until the person
//! confirms. Imported mates start with the app's approval mode whatever the
//! file asks, every app off, skills off and Sending on your behalf = Ask
//! every time (FR-21).

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Manager};

use crate::db::Db;

/// The format version this Evo writes and reads.
pub const VERSION: u64 = 1;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamMate {
    pub name: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub avatar: Value,
    /// A need, not a model: `capable` (or none: the app's default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connectors: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,
    /// From the body, under `## <name>`.
    #[serde(skip)]
    pub instructions: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamChannel {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_rule: Option<String>,
    /// The lead, by mate name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub instructions: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TeamFile {
    pub channel: Option<TeamChannel>,
    pub mates: Vec<TeamMate>,
    /// Front matter keys this Evo does not know, listed in the preview.
    pub unknown: Vec<String>,
}

/// A mate's instructions without the person's setup answers: those are
/// about the person who answered them, not the mate (FR-20).
pub fn shared_instructions(instructions: &str) -> String {
    let mark = crate::agent_tools::mates::SETUP_MARK;
    match instructions.find(mark) {
        Some(at) => instructions[..at].trim_end().to_string(),
        None => instructions.trim_end().to_string(),
    }
}

/// The text of a team file for `mates` (and `channel`).
pub fn write(mates: &[evo_store::mates::Mate], channel: Option<(&evo_store::channels::Channel, Option<String>)>) -> String {
    let team_mates: Vec<TeamMate> = mates
        .iter()
        .map(|mate| TeamMate {
            name: mate.name.clone(),
            role: mate.role.clone(),
            avatar: mate.avatar.clone(),
            model: mate.model.as_ref().map(|_| "capable".to_string()),
            approval_mode: Some(mate.approval_mode.clone()),
            tools: mate.tools.clone(),
            connectors: mate.connectors.clone(),
            skills: mate.skills.clone(),
            instructions: shared_instructions(&mate.instructions),
        })
        .collect();
    let mut front = Map::new();
    front.insert("evoTeam".into(), json!(VERSION));
    if let Some((channel, lead)) = channel {
        let team = TeamChannel {
            name: channel.name.clone(),
            reply_rule: Some(channel.reply_rule.clone()),
            lead,
            instructions: channel.instructions.clone(),
        };
        front.insert("channel".into(), serde_json::to_value(team).unwrap_or_default());
    }
    front.insert("mates".into(), serde_json::to_value(&team_mates).unwrap_or_default());
    let yaml = yaml_serde::to_string(&Value::Object(front)).unwrap_or_default();
    let mut out = format!("---\n{}---\n", yaml.trim_start_matches("---\n"));
    for mate in &team_mates {
        out.push_str(&format!("\n## {}\n\n{}\n", mate.name, mate.instructions.trim()));
    }
    out
}

/// Read a team file. Refused: no front matter, a newer format, no mates.
pub fn parse(text: &str) -> Result<TeamFile, String> {
    let text = text.replace("\r\n", "\n");
    let rest = text.strip_prefix("---\n").ok_or("This is not an Evo team file: it has no front matter.")?;
    let end = rest.find("\n---").ok_or("This is not an Evo team file: its front matter does not end.")?;
    let (front, body) = (&rest[..end], &rest[end + 4..]);
    let front: Value = yaml_serde::from_str(front).map_err(|e| format!("The team file cannot be read: {e}"))?;
    let object = front.as_object().ok_or("The team file cannot be read.")?;
    let version = object.get("evoTeam").and_then(Value::as_u64).ok_or("This is not an Evo team file (no `evoTeam`).")?;
    if version > VERSION {
        return Err(format!("This team file is from a newer Evo (format {version}); update Evo to import it."));
    }
    let unknown: Vec<String> = object
        .keys()
        .filter(|key| !matches!(key.as_str(), "evoTeam" | "channel" | "mates"))
        .cloned()
        .collect();
    let channel = match object.get("channel") {
        Some(value) if !value.is_null() => {
            Some(serde_json::from_value::<TeamChannel>(value.clone()).map_err(|e| format!("The channel cannot be read: {e}"))?)
        }
        _ => None,
    };
    let mut mates: Vec<TeamMate> = serde_json::from_value(object.get("mates").cloned().unwrap_or(Value::Array(Vec::new())))
        .map_err(|e| format!("The mates cannot be read: {e}"))?;
    mates.retain(|m| !m.name.trim().is_empty());
    if mates.is_empty() {
        return Err("The team file has no mates.".into());
    }
    // Each mate's instructions: the section under `## <name>`.
    let mut current: Option<usize> = None;
    let mut sections: Vec<String> = vec![String::new(); mates.len()];
    for line in body.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            current = mates.iter().position(|m| m.name.trim().eq_ignore_ascii_case(heading.trim()));
            continue;
        }
        if let Some(i) = current {
            sections[i].push_str(line);
            sections[i].push('\n');
        }
    }
    for (mate, section) in mates.iter_mut().zip(sections) {
        mate.instructions = section.trim().to_string();
    }
    Ok(TeamFile { channel, mates, unknown })
}

/// What the import shows before it creates anything.
pub(crate) fn preview_value(file: &TeamFile, app_mode: &str, connected: &[String]) -> Value {
    let mates: Vec<Value> = file
        .mates
        .iter()
        .map(|mate| {
            let missing: Vec<&String> = mate
                .connectors
                .iter()
                .flatten()
                .filter(|c| !connected.iter().any(|k| k.eq_ignore_ascii_case(c)))
                .collect();
            json!({
                "name": mate.name,
                "role": mate.role,
                "avatar": mate.avatar,
                // Whatever the file asks: the app's mode (FR-21).
                "approvalMode": app_mode,
                "asked": mate.approval_mode,
                "tools": mate.tools,
                "connectors": mate.connectors,
                "missingConnectors": missing,
                "skills": mate.skills,
                "instructions": mate.instructions,
            })
        })
        .collect();
    json!({ "channel": file.channel, "mates": mates, "unknown": file.unknown })
}

fn connector_keys(app: &AppHandle) -> Vec<String> {
    let Some(state) = app.try_state::<crate::mcp::McpState>() else {
        return Vec::new();
    };
    state
        .0
        .snapshot()
        .get("servers")
        .and_then(Value::as_array)
        .map(|servers| servers.iter().filter_map(|s| s.get("key").and_then(Value::as_str).map(str::to_string)).collect())
        .unwrap_or_default()
}

/// What to export: one mate, or a channel with its mates.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportTarget {
    pub mate: Option<String>,
    pub channel: Option<String>,
}

/// Write a team file to `path` (FR-20).
#[tauri::command]
pub fn team_export(app: AppHandle, target: ExportTarget, path: String) -> Result<(), String> {
    let db = app.state::<Db>();
    let text = if let Some(id) = target.channel {
        let channel = db.with(|conn| evo_store::channels::get_channel(conn, &id))?.ok_or("No such channel.")?;
        let mut mates = Vec::new();
        for member in &channel.members {
            if let Some(mate) = db.with(|conn| evo_store::mates::get_mate(conn, &member.mate_id))? {
                mates.push(mate);
            }
        }
        let lead = channel.lead_mate_id.as_ref().and_then(|id| mates.iter().find(|m| &m.id == id)).map(|m| m.name.clone());
        write(&mates, Some((&channel, lead)))
    } else if let Some(id) = target.mate {
        let mate = db.with(|conn| evo_store::mates::get_mate(conn, &id))?.ok_or("No such mate.")?;
        write(&[mate], None)
    } else {
        return Err("Nothing to export.".into());
    };
    std::fs::write(&path, text).map_err(|e| format!("Could not write {path}: {e}"))
}

/// Read `path` and say what an import would add (FR-21); creates nothing.
#[tauri::command]
pub fn team_preview(app: AppHandle, path: String) -> Result<Value, String> {
    let text = std::fs::read_to_string(&path).map_err(|e| format!("Could not read {path}: {e}"))?;
    Ok(preview_file(&app, &parse(&text)?))
}

/// What importing `file` would add, with the app's approval mode (FR-21).
pub fn preview_file(app: &AppHandle, file: &TeamFile) -> Value {
    let mode = crate::engine::approval_mode(&app.state::<Db>()).to_string();
    preview_value(file, &mode, &connector_keys(app))
}

/// Import `path`, leaving out the mates named in `skip` (FR-21). Returns the
/// new mates' ids and the channel's id.
#[tauri::command]
pub async fn team_import(app: AppHandle, path: String, skip: Vec<String>) -> Result<Value, String> {
    let text = std::fs::read_to_string(&path).map_err(|e| format!("Could not read {path}: {e}"))?;
    import_file(app, parse(&text)?, skip).await
}

/// Create `file`'s mates (but those in `skip`) and its channel, with the
/// safe defaults of FR-21. Returns the new mates' ids and the channel's id.
pub async fn import_file(app: AppHandle, file: TeamFile, skip: Vec<String>) -> Result<Value, String> {
    // Connected apps are not switched off whole: their services are, in `apps`.
    let keys: Vec<String> = connector_keys(&app).into_iter().filter(|k| k != crate::connected_apps::gate::KEY).collect();
    let mut made: Vec<(String, evo_store::mates::Mate)> = Vec::new();
    for team_mate in file.mates.iter().filter(|m| !skip.iter().any(|s| s.eq_ignore_ascii_case(&m.name))) {
        let request = super::CreateMate {
            name: Some(team_mate.name.clone()),
            avatar: (!team_mate.avatar.is_null()).then(|| team_mate.avatar.clone()),
            role: Some(team_mate.role.clone()),
            instructions: Some(team_mate.instructions.clone()),
            // The app's mode, never the file's (FR-21).
            approval_mode: None,
            ..super::CreateMate::default()
        };
        let mate = super::mates_create(app.clone(), request)?;
        // Skills off and Ask every time before sending (FR-21, FR-44).
        let patch = evo_store::mates::MatePatch {
            skills: Some(Some(Vec::new())),
            sending: Some("ask".into()),
            tools: team_mate.tools.clone().map(Some),
            // Connected-apps services it names, each off (connected-apps FR-10).
            apps: Some(Some(crate::connected_apps::gate::named_services(team_mate.connectors.as_deref().unwrap_or_default()))),
            ..Default::default()
        };
        let mate = super::mates_update(app.clone(), mate.id.clone(), patch).await?.unwrap_or(mate);
        // Every app off for the mate (FR-21).
        if !keys.is_empty() {
            crate::agent_tools::mcp::agent_mcp_chat_config(app.clone(), mate.chat_id.clone(), keys.clone());
        }
        made.push((team_mate.name.clone(), mate));
    }
    let mut channel_id = Value::Null;
    if let Some(channel) = &file.channel {
        if !made.is_empty() {
            let lead = channel
                .lead
                .as_ref()
                .and_then(|name| made.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)))
                .map(|(_, m)| m.id.clone());
            let input = super::channels::CreateChannel {
                name: channel.name.clone(),
                instructions: channel.instructions.clone(),
                reply_rule: channel.reply_rule.clone().filter(|r| evo_store::channels::REPLY_RULES.contains(&r.as_str())),
                lead_mate_id: lead,
                folder: None,
                members: made.iter().map(|(_, m)| m.id.clone()).collect(),
            };
            let created = super::channels::channels_create(app.clone(), input).await?;
            channel_id = json!(created.id);
        }
    }
    super::changed(&app);
    Ok(json!({ "mates": made.iter().map(|(_, m)| m.id.clone()).collect::<Vec<_>>(), "channel": channel_id }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mate(name: &str, model: Option<&str>) -> evo_store::mates::Mate {
        serde_json::from_value(json!({
            "id": format!("id-{name}"), "name": name, "avatar": { "definition": "pebble", "hue": 210 },
            "role": format!("{name} role"), "instructions": format!("Be {name}.\nKeep it short."),
            "provider": model.map(|_| "acme"), "model": model, "thinking": null, "tools": ["read"],
            "connectors": ["gmail"], "skills": null, "apps": null, "approvalMode": "auto", "sending": "send",
            "notifications": true, "initiative": "medium", "unanswered": 0, "paused": false,
            "folder": "/secret/folder", "projectFolder": null, "chatId": "chat-x", "template": null,
            "pinned": false, "hidden": false, "position": 0, "createdAt": 0, "updatedAt": 0
        }))
        .unwrap()
    }

    fn channel() -> evo_store::channels::Channel {
        serde_json::from_value(json!({
            "id": "ch-1", "name": "Launch room", "instructions": "Ship by Friday.", "replyRule": "lead",
            "leadMateId": "id-Planner", "folder": null, "chatId": "rootchat-9", "pinned": false, "hidden": false,
            "position": 0, "members": [], "createdAt": 0, "updatedAt": 0
        }))
        .unwrap()
    }

    #[test]
    fn an_export_reads_back_with_no_ids_folders_or_model_ids() {
        let ch = channel();
        let text = write(&[mate("Planner", Some("big-model-7")), mate("Writer", None)], Some((&ch, Some("Planner".into()))));
        assert!(text.starts_with("---\n"), "{text}");
        assert!(text.contains("evoTeam: 1"), "{text}");
        for leak in ["id-Planner", "chat-x", "/secret/folder", "big-model-7", "acme", "ch-1", "rootchat-9"] {
            assert!(!text.contains(leak), "{leak} leaked into {text}");
        }
        assert!(text.contains("model: capable"));
        let file = parse(&text).unwrap();
        assert_eq!(file.mates.len(), 2);
        assert_eq!(file.mates[0].name, "Planner");
        assert_eq!(file.mates[0].instructions, "Be Planner.\nKeep it short.");
        assert_eq!(file.mates[1].model, None);
        let channel = file.channel.unwrap();
        assert_eq!((channel.name.as_str(), channel.lead.as_deref(), channel.reply_rule.as_deref()), ("Launch room", Some("Planner"), Some("lead")));
        assert!(file.unknown.is_empty());
    }

    #[test]
    fn the_persons_setup_answers_stay_home() {
        let text = "Be brief.

Setup answers (from the person):
- Call them: Hung";
        assert_eq!(shared_instructions(text), "Be brief.");
        assert_eq!(shared_instructions("Plain."), "Plain.");
        let mut m = mate("Writer", None);
        m.instructions = text.into();
        let file = write(&[m], None);
        assert!(!file.contains("Hung") && !file.contains("Setup answers"), "{file}");
    }

    #[test]
    fn newer_files_and_non_team_files_are_refused() {
        assert!(parse("---\nevoTeam: 2\nmates: [{name: A}]\n---\n").unwrap_err().contains("newer Evo"));
        assert!(parse("# just notes").is_err());
        assert!(parse("---\nmates: [{name: A}]\n---\n").unwrap_err().contains("evoTeam"));
        assert!(parse("---\nevoTeam: 1\nmates: []\n---\n").unwrap_err().contains("no mates"));
    }

    #[test]
    fn the_preview_shows_the_apps_mode_and_missing_connectors() {
        let file = parse("---\nevoTeam: 1\nextra: 3\nmates:\n  - name: A\n    approvalMode: auto\n    connectors: [gmail, slack]\n---\n## A\nHi\n").unwrap();
        assert_eq!(file.unknown, ["extra"]);
        let preview = preview_value(&file, "ask", &["gmail".to_string()]);
        assert_eq!(preview["mates"][0]["approvalMode"], "ask", "never the file's looser mode");
        assert_eq!(preview["mates"][0]["asked"], "auto");
        assert_eq!(preview["mates"][0]["missingConnectors"], json!(["slack"]));
        assert_eq!(preview["mates"][0]["instructions"], "Hi");
    }
}
