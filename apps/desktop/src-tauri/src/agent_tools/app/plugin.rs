//! `plugin_show_card` and `plugin_open`: the agent shows a plugin's card in
//! the transcript, or opens a plugin's sidebar page or panel
//! (docs/features/plugin-ui/spec.md, FR-7, FR-8).
//!
//! A card is drawn from the call's result (`structuredContent: {plugin,
//! card, data}`), so a reopened chat draws it again; the webview renders it
//! (`features/plugin-ui/plugin-card.tsx`). A panel opens in the chat's own
//! tabs (`agent://panel` with `plugin:<id>/<view>`), a page in the chat's
//! window (`plugin-ui://open`). Both are offered only while an enabled
//! plugin has views of that kind (`offered` in agent_tools/mod.rs).

use std::sync::Arc;

use serde_json::{json, Value};
use tauri::{Emitter, Manager};

use super::super::{BoxFuture, Class, Tool, ToolCtx, ToolOutput};
use super::{optional_str, required_str};
use crate::plugin_ui::views::MAX_CARD_DATA;
use crate::plugin_ui::{Entry, PluginUiState};

/// The enabled plugin named by `name`: its id, or its name when one plugin has it.
fn find(entries: &[Arc<Entry>], name: &str) -> Result<Arc<Entry>, String> {
    if let Some(entry) = entries.iter().find(|entry| entry.id == name) {
        return Ok(entry.clone());
    }
    let named: Vec<&Arc<Entry>> = entries.iter().filter(|entry| entry.name.eq_ignore_ascii_case(name)).collect();
    match named.as_slice() {
        [one] => Ok((*one).clone()),
        [] => Err(format!("No plugin `{name}` with views is on.")),
        _ => Err(format!("Several plugins are called `{name}`; use its id ({}).", named.iter().map(|e| e.id.as_str()).collect::<Vec<_>>().join(", "))),
    }
}

/// What the agent can show or open, for an error that helps it pick.
fn catalog(entries: &[Arc<Entry>], slots: &[&str]) -> String {
    let mut lines = Vec::new();
    for entry in entries {
        for slot in slots {
            let views = match *slot {
                "cards" => &entry.ui.cards,
                "sidebar" => &entry.ui.sidebar,
                _ => &entry.ui.panels,
            };
            for view in views {
                lines.push(format!("- plugin `{}` {} `{}`: {}", entry.id, slot.trim_end_matches('s'), view.id, view.title));
            }
        }
    }
    if lines.is_empty() {
        "No enabled plugin has any.".into()
    } else {
        format!("Available:\n{}", lines.join("\n"))
    }
}

fn entries(ctx: &ToolCtx) -> Vec<Arc<Entry>> {
    ctx.app.try_state::<PluginUiState>().map(|state| state.entries()).unwrap_or_default()
}

fn card_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "plugin": { "type": "string", "description": "The plugin's id (name@marketplace) or name." },
            "card": { "type": "string", "description": "The id of one of the plugin's cards." },
            "data": { "type": "object", "description": "A JSON object the card shows (up to 64 KB); the card reads it with evo.view.data()." }
        },
        "required": ["plugin", "card"],
        "additionalProperties": false
    })
}

async fn show_card(ctx: ToolCtx, args: Value) -> ToolOutput {
    let all = entries(&ctx);
    let (plugin, card) = match (required_str(&args, "plugin"), required_str(&args, "card")) {
        (Ok(plugin), Ok(card)) => (plugin, card),
        (Err(message), _) | (_, Err(message)) => return ToolOutput::error(message),
    };
    let entry = match find(&all, plugin) {
        Ok(entry) => entry,
        Err(message) => return ToolOutput::error(format!("{message}\n{}", catalog(&all, &["cards"]))),
    };
    let Some(view) = entry.view("cards", card) else {
        return ToolOutput::error(format!("{} has no card `{card}`.\n{}", entry.name, catalog(&all, &["cards"])));
    };
    let data = card_data(args.get("data"));
    if data.to_string().len() > MAX_CARD_DATA {
        return ToolOutput::error("`data` is larger than 64 KB; send less, or let the card fetch the rest.");
    }
    ToolOutput::text(format!("Showing {}'s card \"{}\" in the chat.", entry.name, view.title))
        .with_structured(json!({ "plugin": entry.id, "card": view.id, "title": view.title, "data": data }))
}

/// The card's data: models often send JSON as a string, so a string that
/// holds a JSON object or list is read as that value.
fn card_data(value: Option<&Value>) -> Value {
    match value {
        Some(Value::String(text)) => match serde_json::from_str::<Value>(text.trim()) {
            Ok(parsed @ (Value::Object(_) | Value::Array(_))) => parsed,
            _ => Value::String(text.clone()),
        },
        Some(value) => value.clone(),
        None => Value::Null,
    }
}

fn open_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "plugin": { "type": "string", "description": "The plugin's id (name@marketplace) or name." },
            "view": { "type": "string", "description": "The id of one of its sidebar pages or panels." }
        },
        "required": ["plugin"],
        "additionalProperties": false
    })
}

async fn open(ctx: ToolCtx, args: Value) -> ToolOutput {
    let all = entries(&ctx);
    let plugin = match required_str(&args, "plugin") {
        Ok(plugin) => plugin,
        Err(message) => return ToolOutput::error(message),
    };
    let entry = match find(&all, plugin) {
        Ok(entry) => entry,
        Err(message) => return ToolOutput::error(format!("{message}\n{}", catalog(&all, &["sidebar", "panels"]))),
    };
    let wanted = match optional_str(&args, "view") {
        Ok(view) => view.map(str::trim).filter(|v| !v.is_empty()),
        Err(message) => return ToolOutput::error(message),
    };
    // The named view, or the plugin's first page or panel.
    let found = match wanted {
        Some(id) => entry.view("sidebar", id).map(|v| ("sidebar", v)).or_else(|| entry.view("panels", id).map(|v| ("panels", v))),
        None => entry.ui.sidebar.first().map(|v| ("sidebar", v)).or_else(|| entry.ui.panels.first().map(|v| ("panels", v))),
    };
    let Some((slot, view)) = found else {
        return ToolOutput::error(format!("{} has no such page or panel.\n{}", entry.name, catalog(&all, &["sidebar", "panels"])));
    };
    let window = ctx.window();
    if slot == "panels" {
        let panel = format!("plugin:{}/{}", entry.id, view.id);
        ctx.emit("agent://panel", json!({ "chat": ctx.chat, "panel": panel, "action": "open", "window": window }));
        ToolOutput::text(format!("Opened {}'s panel \"{}\" in this chat's side panel.", entry.name, view.title))
    } else {
        let _ = ctx.app.emit_to(window.as_str(), "plugin-ui://open", json!({ "plugin": entry.id, "view": view.id, "slot": slot }));
        ToolOutput::text(format!("Opened {}'s page \"{}\" in the window showing this chat.", entry.name, view.title))
    }
}

fn run_card(ctx: ToolCtx, args: Value) -> BoxFuture<ToolOutput> {
    Box::pin(show_card(ctx, args))
}

fn run_open(ctx: ToolCtx, args: Value) -> BoxFuture<ToolOutput> {
    Box::pin(open(ctx, args))
}

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "plugin_show_card",
            description: "Show one of an installed plugin's cards in this chat, under your message, with `data` for it to display (a chart, a table, a form). The card stays in the transcript. An unknown plugin or card answers with the cards available.",
            schema: card_schema,
            class: Class::Read,
            run: run_card,
        },
        Tool {
            name: "plugin_open",
            description: "Open an installed plugin's sidebar page (in the chat's window) or panel (in this chat's side panel). Without `view`, its first page or panel. An unknown plugin or view answers with what is available.",
            schema: open_schema,
            // Moves the app's view on; nothing the person keeps is altered, so it never asks.
            class: Class::Edit,
            run: run_open,
        },
    ]
}

/// Whether the tool `name` is offered now: only while an enabled plugin has
/// cards (`plugin_show_card`) or pages and panels (`plugin_open`).
pub fn offered(app: &tauri::AppHandle, name: &str) -> bool {
    let Some(state) = app.try_state::<PluginUiState>() else { return false };
    let all = state.entries();
    match name {
        "plugin_show_card" => all.iter().any(|entry| !entry.ui.cards.is_empty()),
        _ => all.iter().any(|entry| !entry.ui.sidebar.is_empty() || !entry.ui.panels.is_empty()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evo_plugins::ui::{PluginUi, View};

    fn entry(id: &str, name: &str, cards: &[&str]) -> Arc<Entry> {
        Arc::new(Entry {
            id: id.into(),
            name: name.into(),
            key: id.into(),
            root: Default::default(),
            data: Default::default(),
            ui: PluginUi {
                cards: cards.iter().map(|c| View { id: c.to_string(), title: c.to_uppercase(), entry: "c.html".into(), ..Default::default() }).collect(),
                ..Default::default()
            },
            permissions: vec![],
            developer_mode: false,
        })
    }

    #[test]
    fn plugins_are_found_by_id_or_a_unique_name() {
        let all = [entry("wall@local", "wall", &["chart"]), entry("wall@acme", "wall", &[]), entry("notes@acme", "Notes", &[])];
        assert_eq!(find(&all, "wall@acme").unwrap().id, "wall@acme");
        assert_eq!(find(&all, "notes").unwrap().id, "notes@acme", "a name, any case");
        assert!(find(&all, "wall").unwrap_err().contains("wall@local, wall@acme"), "an ambiguous name lists the ids");
        assert!(find(&all, "nope").is_err());
    }

    #[test]
    fn card_data_sent_as_a_json_string_is_read_as_json() {
        assert_eq!(card_data(Some(&json!("{\"title\":\"T\"}"))), json!({ "title": "T" }));
        assert_eq!(card_data(Some(&json!(" [1, 2] "))), json!([1, 2]));
        assert_eq!(card_data(Some(&json!("plain words"))), json!("plain words"));
        assert_eq!(card_data(Some(&json!("42"))), json!("42"), "only an object or a list is unwrapped");
        assert_eq!(card_data(Some(&json!({ "a": 1 }))), json!({ "a": 1 }));
        assert_eq!(card_data(None), Value::Null);
    }

    #[test]
    fn the_catalog_lists_what_can_be_shown() {
        let all = [entry("wall@local", "wall", &["chart", "table"])];
        let text = catalog(&all, &["cards"]);
        assert!(text.contains("plugin `wall@local` card `chart`: CHART"), "{text}");
        assert!(text.contains("`table`"), "{text}");
        assert_eq!(catalog(&[], &["cards"]), "No enabled plugin has any.");
    }
}
