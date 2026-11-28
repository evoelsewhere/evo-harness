//! What a mate may do at the computer (docs/features/evo-mates-computer/
//! spec.md, FR-3): the browser, app control and the person's own Chrome
//! (Webbridge), switched per mate in its profile and kept in the app setting
//! [`KEY`]: `{ "<mate id>": { "browser": bool, "app": bool, "chrome": bool } }`.
//! Missing means on. Off removes the tools from the mate's turns (its
//! conversation and its seats) and refuses a call that still names one.

use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::db::Db;

pub const KEY: &str = "mates.computer";

/// A mate's three switches; all on unless the setting says off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Switches {
    pub browser: bool,
    pub app: bool,
    pub chrome: bool,
}

impl Default for Switches {
    fn default() -> Self {
        Switches { browser: true, app: true, chrome: true }
    }
}

/// `mate`'s switches in the saved setting.
pub fn switches(saved: Option<&Value>, mate: &str) -> Switches {
    let Some(entry) = saved.and_then(|all| all.get(mate)) else {
        return Switches::default();
    };
    let on = |key: &str| entry.get(key).and_then(Value::as_bool).unwrap_or(true);
    Switches { browser: on("browser"), app: on("app"), chrome: on("chrome") }
}

/// Whether a tool is offered at all under `switches` (its definition).
pub fn offered(switches: Switches, tool: &str) -> bool {
    if tool.starts_with("browser_") {
        return switches.browser || switches.chrome;
    }
    if tool == "computer_app" {
        return switches.app;
    }
    true
}

/// Why a call is refused under `switches`, or `None` when it may run;
/// `webbridge_tab` says whether a tab id is one of the person's Chrome tabs.
pub fn refusal(switches: Switches, tool: &str, args: &Value, webbridge_tab: impl Fn(&str) -> bool) -> Option<&'static str> {
    if tool == "computer_app" && !switches.app {
        return Some("App control is switched off for this mate in its profile (Apps › At the computer).");
    }
    if !tool.starts_with("browser_") {
        return None;
    }
    let tab = args.get("tab").and_then(Value::as_str).unwrap_or_default();
    let in_chrome = !tab.is_empty() && webbridge_tab(tab);
    if in_chrome && !switches.chrome {
        return Some("Using the person's own Chrome is switched off for this mate in its profile (Apps › At the computer).");
    }
    if !in_chrome && !switches.browser {
        return Some("The browser is switched off for this mate in its profile (Apps › At the computer).");
    }
    None
}

/// The switches of the mate whose conversation or seat `chat` is; `None` for any other chat.
pub fn of_chat(app: &AppHandle, chat: &str) -> Option<Switches> {
    let db = app.try_state::<Db>()?;
    let mate = db.with(|conn| evo_store::mates::mate_of_chat(conn, chat)).ok().flatten()?;
    let saved = db.with(|conn| evo_store::settings::get(conn, KEY)).ok().flatten();
    Some(switches(saved.as_ref(), &mate.id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_means_on() {
        assert_eq!(switches(None, "m1"), Switches::default());
        let saved = json!({ "m1": { "browser": false } });
        assert_eq!(switches(Some(&saved), "m1"), Switches { browser: false, app: true, chrome: true });
        assert_eq!(switches(Some(&saved), "m2"), Switches::default());
    }

    #[test]
    fn off_removes_and_refuses() {
        let chrome = |tab: &str| tab.starts_with("chrome:");
        let none = Switches { browser: false, app: false, chrome: false };
        assert!(!offered(none, "browser_click") && !offered(none, "computer_app"));
        assert!(offered(none, "read") && offered(Switches::default(), "browser_click"));
        assert!(refusal(none, "computer_app", &json!({}), chrome).is_some());
        assert!(refusal(none, "browser_navigate", &json!({ "url": "x" }), chrome).is_some());

        // Chrome off, browser on: Evo's own pages only.
        let own = Switches { browser: true, app: true, chrome: false };
        assert!(refusal(own, "browser_click", &json!({ "tab": "t-1" }), chrome).is_none());
        assert!(refusal(own, "browser_click", &json!({ "tab": "chrome:p1:7" }), chrome).unwrap().contains("Chrome"));
        // Browser off, Chrome on: the person's tabs only.
        let theirs = Switches { browser: false, app: true, chrome: true };
        assert!(refusal(theirs, "browser_click", &json!({ "tab": "chrome:p1:7" }), chrome).is_none());
        assert!(refusal(theirs, "browser_navigate", &json!({}), chrome).unwrap().contains("browser is switched off"));
        assert!(refusal(Switches::default(), "browser_click", &json!({ "tab": "chrome:p1:7" }), chrome).is_none());
    }
}
