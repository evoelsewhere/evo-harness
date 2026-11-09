//! Reading back what an action changed (docs/features/computer-app-control,
//! FR-17): the window's state is probed before and after every acting action
//! and the difference goes into the result, so the agent sees whether its
//! input landed instead of guessing from a picture.
//!
//! A probe is platform-neutral: the window's title, the dialog or popups in
//! front, and the accessibility tree as lines without refs or positions (so a
//! probe never retires the refs the agent holds, and a control that only moved
//! does not count as changed). The platform backend takes the probes; this
//! module compares them.

use serde_json::{json, Value};

/// At most this many added and removed lines are reported.
const MAX_LINES: usize = 6;

/// The window's state at one moment.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Probe {
    /// The attached window's title.
    pub title: String,
    /// The window acted on when it is not the attached one (a modal dialog).
    pub dialog: Option<String>,
    /// Open popups, menus and drop-downs, by class or title.
    pub popups: Vec<String>,
    /// The tree, one element per line (`- Edit "Name" value="x" [focused]`).
    pub lines: Vec<String>,
}

fn multiset_minus(a: &[String], b: &[String]) -> Vec<String> {
    let mut rest: Vec<&String> = b.iter().collect();
    let mut out = Vec::new();
    for line in a {
        if let Some(at) = rest.iter().position(|other| *other == line) {
            rest.swap_remove(at);
        } else {
            out.push(line.trim().to_string());
        }
    }
    out
}

/// What changed from `before` to `after`, for the action's result:
/// `{ "title": …, "dialog": …, "popups": …, "added": […], "removed": […] }`
/// with only the parts that changed, or `null` when nothing visible did.
pub(crate) fn diff(before: &Probe, after: &Probe) -> Value {
    let mut changed = serde_json::Map::new();
    if before.title != after.title {
        changed.insert("title".into(), json!(after.title));
    }
    if before.dialog != after.dialog {
        changed.insert("dialog".into(), json!(after.dialog.clone().unwrap_or_else(|| "closed".into())));
    }
    if before.popups.len() != after.popups.len() {
        changed.insert("popups".into(), json!(after.popups.len()));
    }
    let added = multiset_minus(&after.lines, &before.lines);
    let removed = multiset_minus(&before.lines, &after.lines);
    let cut = |mut lines: Vec<String>| {
        let more = lines.len().saturating_sub(MAX_LINES);
        lines.truncate(MAX_LINES);
        if more > 0 {
            lines.push(format!("… and {more} more"));
        }
        lines
    };
    if !added.is_empty() {
        changed.insert("added".into(), json!(cut(added)));
    }
    if !removed.is_empty() {
        changed.insert("removed".into(), json!(cut(removed)));
    }
    if changed.is_empty() {
        Value::Null
    } else {
        Value::Object(changed)
    }
}

/// Whether the picture after an action may be late (NFR-3): the tree shows
/// the action took effect but the window's picture is unchanged, as with a
/// web page that stopped painting.
pub(crate) fn picture_late(changed: &Value, same_picture: bool) -> bool {
    !changed.is_null() && same_picture
}

/// The diff as one line of the result the agent reads.
pub(crate) fn describe(changed: &Value) -> String {
    let Some(object) = changed.as_object() else {
        return "no visible change in the window".into();
    };
    let mut parts = Vec::new();
    if let Some(dialog) = object.get("dialog").and_then(Value::as_str) {
        parts.push(if dialog == "closed" { "the dialog closed".to_string() } else { format!("dialog \"{dialog}\" opened") });
    }
    if let Some(title) = object.get("title").and_then(Value::as_str) {
        parts.push(format!("title is now \"{title}\""));
    }
    if object.contains_key("popups") {
        parts.push("a menu or popup opened or closed".to_string());
    }
    let lines = |key: &str| -> Vec<String> {
        object.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
    };
    let (added, removed) = (lines("added"), lines("removed"));
    if !added.is_empty() {
        parts.push(format!("now shows: {}", added.join(" | ")));
    }
    if !removed.is_empty() {
        parts.push(format!("no longer shows: {}", removed.join(" | ")));
    }
    format!("changed: {}", parts.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(title: &str, lines: &[&str]) -> Probe {
        Probe { title: title.into(), lines: lines.iter().map(|l| l.to_string()).collect(), ..Default::default() }
    }

    #[test]
    fn a_picture_is_late_only_when_the_tree_changed_and_the_picture_did_not() {
        let changed = json!({ "title": "x" });
        assert!(picture_late(&changed, true));
        assert!(!picture_late(&changed, false));
        assert!(!picture_late(&Value::Null, true), "nothing changed: an unchanged picture is right");
    }

    #[test]
    fn nothing_visible_changed() {
        let a = probe("Report", &["- Edit \"Name\" value=\"\"", "- Button \"Save\""]);
        assert_eq!(diff(&a, &a.clone()), Value::Null);
        assert_eq!(describe(&Value::Null), "no visible change in the window");
    }

    #[test]
    fn a_value_and_the_focus_changed() {
        let before = probe("Report", &["- Edit \"Name\" value=\"\" [focused]", "- Button \"Save\""]);
        let after = probe("Report*", &["- Edit \"Name\" value=\"alpha\"", "- Button \"Save\" [focused]"]);
        let changed = diff(&before, &after);
        assert_eq!(changed["title"], "Report*");
        assert_eq!(changed["added"], json!(["- Edit \"Name\" value=\"alpha\"", "- Button \"Save\" [focused]"]));
        assert_eq!(changed["removed"], json!(["- Edit \"Name\" value=\"\" [focused]", "- Button \"Save\""]));
        let text = describe(&changed);
        assert!(text.starts_with("changed: title is now \"Report*\"; now shows: - Edit \"Name\" value=\"alpha\""), "{text}");
    }

    #[test]
    fn a_dialog_opened_and_closed() {
        let main = probe("Book1", &["- Button \"OK\""]);
        let with_dialog = Probe { dialog: Some("Save As".into()), ..main.clone() };
        assert_eq!(describe(&diff(&main, &with_dialog)), "changed: dialog \"Save As\" opened");
        assert_eq!(describe(&diff(&with_dialog, &main)), "changed: the dialog closed");
    }

    #[test]
    fn repeated_lines_count_once_each_and_long_lists_are_cut() {
        let before = probe("t", &["- ListItem \"x\"", "- ListItem \"x\""]);
        let after = probe("t", &["- ListItem \"x\""]);
        assert_eq!(diff(&before, &after)["removed"], json!(["- ListItem \"x\""]));

        let many: Vec<String> = (0..10).map(|i| format!("- Text \"{i}\"")).collect();
        let after = Probe { lines: many, ..Default::default() };
        let added = diff(&Probe::default(), &after)["added"].as_array().unwrap().clone();
        assert_eq!(added.len(), MAX_LINES + 1);
        assert_eq!(added.last().unwrap(), "… and 4 more");
    }
}
