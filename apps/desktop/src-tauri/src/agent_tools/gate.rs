//! The approval gate of the agent tools the shell answers (ADR 0013).
//!
//! The classes are the engine's (ADR 0014). A read never asks. An edit (a click,
//! typing, a navigation) asks on a site that is not local, in "Ask every time",
//! once per site and chat. A command (an upload, storage, a mock, a script) asks
//! every time in "Ask every time" and "Approve edits". Nothing asks in "Approve
//! for me".
//! Spec: docs/features/agent-browser (FR-22).

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
}
