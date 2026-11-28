//! Linked conversations and followed threads
//! (docs/features/evo-mates-threads-board/spec.md, FR-16 – FR-22).
//!
//! The window records the person's shares here (`share_record`), lists a
//! conversation's links (`linked_of`), and reads and changes followed threads
//! (`threads_followed`, `thread_set`, `thread_read`). A mate's `share_to`
//! posts a share card into a channel it is in, through that channel's
//! engine, under the mate's **Share across conversations** setting
//! (`mates.share`: `ask`, the default; `allow`; `never`), and always asks
//! when it shares out of its direct conversation with the person.

use evo_store::links::{self, NewShare, Share, ThreadFollow};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::db::Db;

/// The event a window reads links and threads again on.
pub const CHANGED: &str = "mates://links";

fn changed(app: &AppHandle) {
    let _ = app.emit(CHANGED, json!({}));
}

#[tauri::command]
pub fn share_record(app: AppHandle, share: NewShare) -> Result<Share, String> {
    let share = NewShare { by: "person".into(), ..share };
    let out = app.state::<Db>().with(|conn| links::record_share(conn, &share))?;
    changed(&app);
    Ok(out)
}

#[tauri::command]
pub fn linked_of(app: AppHandle, chat: String) -> Result<Vec<Share>, String> {
    app.state::<Db>().with(|conn| links::linked(conn, &chat))
}

#[tauri::command]
pub fn threads_followed(app: AppHandle) -> Result<Vec<ThreadFollow>, String> {
    app.state::<Db>().with(|conn| links::followed_threads(conn))
}

#[tauri::command]
pub fn threads_of(app: AppHandle, chat: String) -> Result<Vec<ThreadFollow>, String> {
    app.state::<Db>().with(|conn| links::threads_of(conn, &chat))
}

#[tauri::command]
pub fn thread_set(app: AppHandle, chat: String, root: String, following: Option<bool>, resolved: Option<bool>) -> Result<(), String> {
    app.state::<Db>().with(|conn| links::thread_set(conn, &chat, &root, following, resolved))?;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub fn thread_read(app: AppHandle, chat: String, root: String) -> Result<(), String> {
    app.state::<Db>().with(|conn| links::thread_read(conn, &chat, &root, evo_store::chats::now_ms()))?;
    changed(&app);
    Ok(())
}

// ── share_to (FR-18) ────────────────────────────────────────────────────────

pub const SHARE_TOOL: &str = "share_to";

pub fn share_definition() -> Value {
    json!({
        "name": SHARE_TOOL,
        "description": "Share a short summary, or files, into another channel you are a member of, so its members and the person have it there. It shows as a card saying where it came from; nobody is woken by it. The person decides whether you may share: it may be refused, or they may be asked first. Never share private details from your conversation with the person unless they asked you to.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "to": { "type": "string", "description": "The channel's name, e.g. \"#marketing\"." },
                "text": { "type": "string", "description": "What to share, in a few lines." },
                "files": { "type": "array", "items": { "type": "string" }, "description": "Paths of files to share with it." }
            },
            "required": ["to", "text"],
            "additionalProperties": false
        }
    })
}

/// What `mates.share` says for a mate: `ask` (default), `allow` or `never`.
pub fn share_mode(settings: Option<&Value>, mate: &str) -> &'static str {
    match settings.and_then(|s| s.get(mate)).and_then(Value::as_str) {
        Some("allow") => "allow",
        Some("never") => "never",
        _ => "ask",
    }
}

/// Whether the person must be asked first (FR-18): always for a share out of
/// the mate's direct conversation with the person, else per its setting.
pub fn must_ask(mode: &str, from_direct: bool) -> bool {
    from_direct || mode == "ask"
}

/// The channel a word names among `channels` (name, with or without `#`).
pub fn channel_named<'a>(word: &str, channels: &'a [evo_store::channels::Channel]) -> Option<&'a evo_store::channels::Channel> {
    let want = word.trim().trim_start_matches('#').trim().to_lowercase();
    channels.iter().find(|c| c.name.trim().to_lowercase() == want)
}

/// Run `share_to` for `mate` from its chat `from`; the words the tool answers with.
pub async fn share_to(ctx: &crate::agent_tools::ToolCtx, mate: &evo_store::mates::Mate, args: &Value) -> Result<String, String> {
    let app = &ctx.app;
    let to = args.get("to").and_then(Value::as_str).unwrap_or_default();
    let text = args.get("text").and_then(Value::as_str).unwrap_or_default().trim().to_string();
    let files: Vec<String> = args
        .get("files")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default();
    if text.is_empty() && files.is_empty() {
        return Err("Nothing to share: give the text (and any files).".into());
    }
    let db = app.state::<Db>();
    let (mine, settings, from_place, from_direct) = db.with(|conn| {
        let all = evo_store::channels::list_channels(conn)?;
        let mine: Vec<_> = all.into_iter().filter(|c| c.members.iter().any(|m| m.mate_id == mate.id)).collect();
        let settings = evo_store::settings::get(conn, "mates.share")?;
        let seat_channel = super::reactions::channel_of_seat(conn, &ctx.chat);
        let (place, direct) = match seat_channel.and_then(|id| mine.iter().find(|c| c.id == id).cloned()) {
            Some(channel) => (format!("#{}", channel.name), false),
            None => (format!("your conversation with {}", mate.name), ctx.chat == mate.chat_id),
        };
        Ok((mine, settings, place, direct))
    })?;
    let Some(channel) = channel_named(to, &mine).cloned() else {
        return Err(format!(
            "You can share only into a channel you are in: {}.",
            if mine.is_empty() { "you are in none".to_string() } else { mine.iter().map(|c| format!("#{}", c.name)).collect::<Vec<_>>().join(", ") }
        ));
    };
    if from_place == format!("#{}", channel.name) {
        return Err("That is the channel you are in: just write it here.".into());
    }
    let mode = share_mode(settings.as_ref(), &mate.id);
    if mode == "never" {
        return Err("The person turned sharing across conversations off for you.".into());
    }
    if must_ask(mode, from_direct) {
        let excerpt: String = text.chars().take(400).collect();
        let more = if files.is_empty() { String::new() } else { format!("\n{} file(s): {}", files.len(), files.join(", ")) };
        let choice = ctx
            .ask(&format!("Share with #{}?", channel.name), &format!("{} wants to share from {from_place}:\n{excerpt}{more}", mate.name), &["Share", "Don't share"])
            .await;
        if choice != 0 {
            return Err("The person did not want that shared.".into());
        }
    }
    {
        use crate::supervisor::{AppHost, EngineSupervisor, EnsureRequest, Host, Reason};
        let host = AppHost(app.clone());
        let running = app.state::<crate::engine::EngineState>().running().await.iter().any(|c| *c == channel.chat_id);
        if !running {
            let request = EnsureRequest { chat: channel.chat_id.clone(), cwd: None, options: None, restore: true, restart: false, reason: Reason::Mail };
            app.state::<EngineSupervisor>().ensure(&host, request).await?;
        }
        host.request(&channel.chat_id, "channel_post_share", json!({ "by": mate.id, "from": from_place, "text": text, "files": files }))
            .await?;
    }
    let from_chat = ctx.chat.clone();
    let _ = db.with(|conn| {
        // A seat's share links its channel's chat.
        let from_chat = super::reactions::channel_of_seat(conn, &from_chat)
            .and_then(|id| evo_store::channels::get_channel(conn, &id).ok().flatten())
            .map(|c| c.chat_id)
            .unwrap_or(from_chat);
        links::record_share(
            conn,
            &NewShare { from_chat, excerpt: text.chars().take(200).collect(), to_chat: channel.chat_id.clone(), by: mate.id.clone(), ..NewShare::default() },
        )
    });
    changed(app);
    Ok(format!("Shared with #{}.", channel.name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sharing_asks_by_default_and_always_from_the_direct_chat() {
        let settings = json!({ "w": "allow", "n": "never" });
        assert_eq!(share_mode(Some(&settings), "w"), "allow");
        assert_eq!(share_mode(Some(&settings), "n"), "never");
        assert_eq!(share_mode(Some(&settings), "other"), "ask");
        assert_eq!(share_mode(None, "w"), "ask");
        assert!(must_ask("ask", false));
        assert!(!must_ask("allow", false));
        assert!(must_ask("allow", true), "a share out of the direct chat always asks");
    }
}
