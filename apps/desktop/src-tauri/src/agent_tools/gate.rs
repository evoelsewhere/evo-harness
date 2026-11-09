//! The approval gate of the agent tools the shell answers (ADR 0013).
//!
//! The classes are the engine's (ADR 0014). A read never asks. An edit (a click,
//! typing, a navigation) asks on a site that is not local, in "Ask every time",
//! once per site and chat. A command (an upload, storage, a mock, a script) asks
//! every time in "Ask every time" and "Approve edits". Nothing asks in "Approve
//! for me".
//! Spec: docs/features/agent-browser (FR-22).
//!
//! Controlling another app on the computer ([`check_control`], ADR 0015) has a
//! rule of its own: with its setting on "Ask every time" it asks in every mode,
//! "Approve for me" included; on "Allow without asking" it follows the mode like
//! a command. A refusal blocks the same actions for the rest of the run
//! (docs/features/computer-app-control, FR-24).

use std::collections::HashSet;
use std::future::Future;
use std::sync::Mutex;

use tauri::Url;

use super::Class;

/// What the gate decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Deny,
    /// The question became moot (the turn was stopped).
    Gone,
}

/// Pages of the person's own machine: editing them never asks.
pub fn is_local_origin(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    match parsed.scheme() {
        "file" | "about" | "data" | "blob" => true,
        _ => parsed.host_str().is_some_and(|host| {
            host == "localhost" || host == "127.0.0.1" || host == "[::1]" || host == "::1" || host.ends_with(".localhost")
        }),
    }
}

/// Scheme, host and port of `url`; empty when it is not an address.
pub fn origin_of(url: &str) -> String {
    match Url::parse(url) {
        Ok(parsed) if parsed.has_host() => {
            let host = parsed.host_str().unwrap_or_default();
            match parsed.port() {
                Some(port) => format!("{}://{host}:{port}", parsed.scheme()),
                None => format!("{}://{host}", parsed.scheme()),
            }
        }
        Ok(parsed) => format!("{}://", parsed.scheme()),
        Err(_) => String::new(),
    }
}

/// What a chat already allowed: `chat|edit|origin` and `chat|command|tool`. In memory only.
static GRANTS: Mutex<Option<HashSet<String>>> = Mutex::new(None);

fn granted(key: &str) -> bool {
    GRANTS.lock().expect("grants lock").as_ref().is_some_and(|set| set.contains(key))
}

fn grant(key: String) {
    GRANTS.lock().expect("grants lock").get_or_insert_with(HashSet::new).insert(key);
}

/// Forget every grant (tests).
#[cfg(test)]
pub fn reset_grants() {
    *GRANTS.lock().expect("grants lock") = None;
}

/// Actions the person refused, per chat (`chat|action`), until the chat's run
/// ends ([`forget_refusals`]). In memory only.
static REFUSED: Mutex<Option<HashSet<String>>> = Mutex::new(None);

fn refused(chat: &str, action: &str) -> bool {
    let key = format!("{chat}|{action}");
    REFUSED.lock().expect("refusals lock").as_ref().is_some_and(|set| set.contains(&key))
}

fn refuse(chat: &str, actions: &[String]) {
    let mut set = REFUSED.lock().expect("refusals lock");
    let set = set.get_or_insert_with(HashSet::new);
    for action in actions {
        set.insert(format!("{chat}|{action}"));
    }
}

/// The chat's run ended: actions refused in it may be asked about again.
pub fn forget_refusals(chat: &str) {
    let prefix = format!("{chat}|");
    if let Some(set) = REFUSED.lock().expect("refusals lock").as_mut() {
        set.retain(|key| !key.starts_with(&prefix));
    }
}

/// Decide whether a call that controls another app may run. `actions` are the
/// call's acting steps as the person reads them (`click e12`, `key ctrl+s`);
/// a call with none never asks. `always` is the feature's "Ask every time"
/// setting; `mode` the approval mode.
pub async fn check_control<F, Fut>(tool: &str, always: bool, mode: &str, chat: &str, actions: &[String], ask: F) -> Verdict
where
    F: Fn(String, String, Vec<String>) -> Fut,
    Fut: Future<Output = i32>,
{
    if actions.is_empty() {
        return Verdict::Allow;
    }
    if actions.iter().any(|action| refused(chat, action)) {
        return Verdict::Deny;
    }
    if !always && mode == "auto" {
        return Verdict::Allow;
    }
    let key = format!("{chat}|control|{tool}");
    if granted(&key) {
        return Verdict::Allow;
    }
    let answer = ask(
        "Let the agent act in an app on this computer?".to_string(),
        actions.iter().map(|action| format!("• {action}")).collect::<Vec<_>>().join("\n"),
        vec!["Allow".into(), "Allow for this chat".into(), "Deny".into()],
    )
    .await;
    match answer {
        -1 => Verdict::Gone,
        0 => Verdict::Allow,
        1 => {
            grant(key);
            Verdict::Allow
        }
        _ => {
            refuse(chat, actions);
            Verdict::Deny
        }
    }
}

/// Decide whether a call of `tool` (of `class`) may run. `mode` is the approval
/// mode (`ask`, `auto-edits`, `auto`); `ask(title, description, options)`
/// questions the person and returns the chosen index, or -1 when moot.
pub async fn check<F, Fut>(tool: &str, class: Class, mode: &str, chat: &str, url: &str, summary: &str, ask: F) -> Verdict
where
    F: Fn(String, String, Vec<String>) -> Fut,
    Fut: Future<Output = i32>,
{
    if class == Class::Read || mode == "auto" {
        return Verdict::Allow;
    }
    if class == Class::Edit {
        // Only "Ask every time" asks, and only for a site that is not local.
        if mode != "ask" || url.is_empty() || is_local_origin(url) {
            return Verdict::Allow;
        }
        let origin = match origin_of(url) {
            origin if origin.is_empty() => url.to_string(),
            origin => origin,
        };
        let key = format!("{chat}|edit|{origin}");
        if granted(&key) {
            return Verdict::Allow;
        }
        let answer = ask(
            format!("Let the agent work on {origin}?"),
            format!("{summary}\nSite: {origin}"),
            vec!["Allow".into(), "Deny".into()],
        )
        .await;
        return match answer {
            -1 => Verdict::Gone,
            0 => {
                grant(key);
                Verdict::Allow
            }
            _ => Verdict::Deny,
        };
    }

    // Command: "Ask every time" and "Approve edits" ask, every time unless allowed for the chat.
    let key = format!("{chat}|command|{tool}");
    if granted(&key) {
        return Verdict::Allow;
    }
    let site = match origin_of(url) {
        origin if origin.is_empty() && !url.is_empty() => format!("\nSite: {url}"),
        origin if origin.is_empty() => String::new(),
        origin => format!("\nSite: {origin}"),
    };
    let answer = ask(
        format!("Allow {tool}?"),
        format!("{summary}{site}"),
        vec!["Allow".into(), "Allow for this chat".into(), "Deny".into()],
    )
    .await;
    match answer {
        -1 => Verdict::Gone,
        1 => {
            grant(key);
            Verdict::Allow
        }
        0 => Verdict::Allow,
        _ => Verdict::Deny,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex as StdMutex};

    /// A fake person: answers with `answers` in turn and remembers the questions.
    #[derive(Clone, Default)]
    struct Person {
        answers: Arc<StdMutex<Vec<i32>>>,
        asked: Arc<StdMutex<Vec<(String, String, Vec<String>)>>>,
    }

    impl Person {
        fn answering(answers: &[i32]) -> Self {
            Self { answers: Arc::new(StdMutex::new(answers.to_vec())), ..Default::default() }
        }
        fn ask(&self) -> impl Fn(String, String, Vec<String>) -> std::pin::Pin<Box<dyn Future<Output = i32> + Send>> + '_ {
            move |title, description, options| {
                self.asked.lock().unwrap().push((title, description, options));
                let answer = {
                    let mut answers = self.answers.lock().unwrap();
                    if answers.is_empty() { 0 } else { answers.remove(0) }
                };
                Box::pin(async move { answer })
            }
        }
        fn count(&self) -> usize {
            self.asked.lock().unwrap().len()
        }
    }

    #[test]
    fn local_addresses_are_local_and_public_ones_are_not() {
        for url in ["http://localhost:3000/a", "http://127.0.0.1:8080", "http://[::1]:1/", "http://app.localhost/", "file:///C:/x.html", "about:blank"] {
            assert!(is_local_origin(url), "{url}");
        }
        for url in ["https://example.com/x", "http://192.168.1.4/", "https://localhost.evil.com/", "not a url"] {
            assert!(!is_local_origin(url), "{url}");
        }
    }

    #[test]
    fn the_origin_is_scheme_host_and_port() {
        assert_eq!(origin_of("https://example.com:8443/path?q=1"), "https://example.com:8443");
        assert_eq!(origin_of("https://example.com/x"), "https://example.com");
        assert_eq!(origin_of("not a url"), "");
    }

    // The grants are process-wide: each test uses its own chat id instead of resetting them.
    #[tokio::test]
    async fn observing_never_asks() {
        for mode in ["ask", "auto-edits", "auto"] {
            let person = Person::default();
            let verdict = check("browser_read_page", Class::Read, mode, "obs", "https://example.com", "x", person.ask()).await;
            assert_eq!(verdict, Verdict::Allow);
            assert_eq!(person.count(), 0);
        }
    }

    #[tokio::test]
    async fn acting_on_a_public_site_asks_once_per_site_and_chat_in_ask_every_time() {
        let person = Person::answering(&[0, 0]);
        let run = |url: &'static str| check("browser_click", Class::Edit, "ask", "act-1", url, "click", person.ask());
        assert_eq!(run("https://example.com/a").await, Verdict::Allow);
        assert_eq!(run("https://example.com/b").await, Verdict::Allow);
        assert_eq!(person.count(), 1);
        assert!(person.asked.lock().unwrap()[0].1.contains("https://example.com"));
        assert_eq!(run("https://other.org/").await, Verdict::Allow);
        assert_eq!(person.count(), 2);
    }

    #[tokio::test]
    async fn acting_never_asks_on_local_sites_or_outside_ask_every_time() {
        let person = Person::default();
        for mode in ["ask", "auto-edits", "auto"] {
            assert_eq!(check("browser_click", Class::Edit, mode, "act-2", "http://localhost:5173", "c", person.ask()).await, Verdict::Allow);
        }
        for mode in ["auto-edits", "auto"] {
            assert_eq!(check("browser_click", Class::Edit, mode, "act-2", "https://example.com", "c", person.ask()).await, Verdict::Allow);
        }
        assert_eq!(person.count(), 0);
    }

    #[tokio::test]
    async fn a_change_asks_every_time_in_ask_and_approve_edits_never_in_approve_for_me() {
        for (index, mode) in ["ask", "auto-edits"].into_iter().enumerate() {
            let person = Person::answering(&[0, 0]);
            let chat = format!("chg-{index}");
            check("browser_upload", Class::Command, mode, &chat, "http://localhost:3000", "upload", person.ask()).await;
            check("browser_upload", Class::Command, mode, &chat, "http://localhost:3000", "upload", person.ask()).await;
            assert_eq!(person.count(), 2, "{mode}");
        }
        let person = Person::default();
        assert_eq!(check("browser_upload", Class::Command, "auto", "chg-2", "https://example.com", "u", person.ask()).await, Verdict::Allow);
        assert_eq!(person.count(), 0);
    }

    #[tokio::test]
    async fn deny_refuses_and_a_moot_question_ends_quietly() {
        let deny = Person::answering(&[2]);
        assert_eq!(check("browser_click", Class::Edit, "ask", "deny", "https://example.com", "c", deny.ask()).await, Verdict::Deny);
        let moot = Person::answering(&[-1]);
        assert_eq!(check("browser_click", Class::Edit, "ask", "moot", "https://example.com", "c", moot.ask()).await, Verdict::Gone);
    }

    #[tokio::test]
    async fn allow_for_this_chat_stops_the_questions_of_that_kind_in_that_chat() {
        let person = Person::answering(&[1]);
        async fn change(chat: &str, p: &Person) -> Verdict {
            check("browser_storage", Class::Command, "ask", chat, "https://example.com", "clear", p.ask()).await
        }
        assert_eq!(change("grant-1", &person).await, Verdict::Allow);
        assert_eq!(change("grant-1", &person).await, Verdict::Allow);
        assert_eq!(person.count(), 1);
        assert_eq!(person.asked.lock().unwrap()[0].2, vec!["Allow", "Allow for this chat", "Deny"]);
        // Another chat is asked again.
        let other = Person::answering(&[0]);
        change("grant-2", &other).await;
        assert_eq!(other.count(), 1);
    }

    // ── Controlling another app (computer app control, FR-24) ──────────────

    fn acts(list: &[&str]) -> Vec<String> {
        list.iter().map(|a| a.to_string()).collect()
    }

    #[tokio::test]
    async fn ask_every_time_controls_ask_in_every_mode() {
        for (index, mode) in ["ask", "auto-edits", "auto"].into_iter().enumerate() {
            let chat = format!("ctl-always-{index}");
            let person = Person::answering(&[0]);
            let verdict = check_control("computer_app", true, mode, &chat, &acts(&["click e12"]), person.ask()).await;
            assert_eq!(verdict, Verdict::Allow, "{mode}");
            assert_eq!(person.count(), 1, "{mode}: Ask every time must ask");
            let (_, description, options) = person.asked.lock().unwrap()[0].clone();
            assert!(description.contains("click e12"), "the prompt lists the action: {description}");
            assert_eq!(options, vec!["Allow", "Allow for this chat", "Deny"]);
        }
    }

    #[tokio::test]
    async fn allow_without_asking_follows_the_mode_like_a_command() {
        let person = Person::default();
        assert_eq!(check_control("computer_app", false, "auto", "ctl-allow-auto", &acts(&["key ctrl+s"]), person.ask()).await, Verdict::Allow);
        assert_eq!(person.count(), 0, "Approve for me runs it straight away");
        for mode in ["ask", "auto-edits"] {
            let chat = format!("ctl-allow-{mode}");
            let person = Person::answering(&[0]);
            assert_eq!(check_control("computer_app", false, mode, &chat, &acts(&["key ctrl+s"]), person.ask()).await, Verdict::Allow);
            assert_eq!(person.count(), 1, "{mode} asks like a command");
        }
    }

    #[tokio::test]
    async fn a_refusal_blocks_the_same_actions_for_the_rest_of_the_run() {
        let chat = "ctl-refused";
        let person = Person::answering(&[2]);
        let verdict = check_control("computer_app", true, "ask", chat, &acts(&["click e12", "type \"hi\" (2 chars)"]), person.ask()).await;
        assert_eq!(verdict, Verdict::Deny);
        // The same action again: refused without asking.
        let again = Person::default();
        assert_eq!(check_control("computer_app", true, "ask", chat, &acts(&["click e12"]), again.ask()).await, Verdict::Deny);
        assert_eq!(again.count(), 0);
        // A different action asks again.
        let other = Person::answering(&[0]);
        assert_eq!(check_control("computer_app", true, "ask", chat, &acts(&["key escape"]), other.ask()).await, Verdict::Allow);
        assert_eq!(other.count(), 1);
        // Another chat is not affected; the run's end forgets the refusal.
        let elsewhere = Person::answering(&[0]);
        assert_eq!(check_control("computer_app", true, "ask", "ctl-refused-other", &acts(&["click e12"]), elsewhere.ask()).await, Verdict::Allow);
        forget_refusals(chat);
        let later = Person::answering(&[0]);
        assert_eq!(check_control("computer_app", true, "ask", chat, &acts(&["click e12"]), later.ask()).await, Verdict::Allow);
        assert_eq!(later.count(), 1);
    }

    #[tokio::test]
    async fn status_wait_and_detach_never_ask_and_a_chat_grant_holds() {
        let person = Person::default();
        assert_eq!(check_control("computer_app", true, "ask", "ctl-quiet", &[], person.ask()).await, Verdict::Allow);
        assert_eq!(person.count(), 0, "a call with no acting step never asks");

        let chat = "ctl-granted";
        let person = Person::answering(&[1]);
        assert_eq!(check_control("computer_app", true, "auto", chat, &acts(&["click e1"]), person.ask()).await, Verdict::Allow);
        let next = Person::default();
        assert_eq!(check_control("computer_app", true, "auto", chat, &acts(&["click e2"]), next.ask()).await, Verdict::Allow);
        assert_eq!(next.count(), 0, "Allow for this chat holds for the chat");
    }

    #[tokio::test]
    async fn a_dropped_call_is_gone() {
        let person = Person::answering(&[-1]);
        assert_eq!(check_control("computer_app", true, "ask", "ctl-gone", &acts(&["click e1"]), person.ask()).await, Verdict::Gone);
    }
}
