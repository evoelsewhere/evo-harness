//! Open questions per chat: what waits for the person
//! (docs/features/shell-hosted-engines/spec.md, FR-9; plan 3.5 "Open questions").
//!
//! Two kinds of question wait for an answer: an engine's (`ui_dialog_request`:
//! an approval, `ask_user`'s questions) and the shell's own tools'
//! (`ToolCtx::ask`: a connector tool set to ask, the browser's gate). The
//! shell keeps both here, per chat, whichever window shows the chat or none:
//! a window that attaches draws every open one, and a chat with an open
//! question counts as working, so its engine is never trimmed (FR-3).
//!
//! A question leaves when it is answered or ends another way: the engine's
//! `ui_dialog_closed`, a `ui_dialog_response` a window wrote, the person's
//! answer to a shell question, or its call being dropped. The shell never
//! answers one and none times out (FR-9).

use std::collections::HashMap;
use std::sync::Mutex;

use serde::Serialize;
use serde_json::Value;

/// Who asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// The chat's engine (`ui_dialog_request`).
    Engine,
    /// One of the shell's tools (`agent://ask` before this feature).
    Shell,
}

/// What kind of answer it wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Allow or deny a call.
    Permission,
    /// The agent's questions (`ask_user`).
    Questions,
    /// A pick from options.
    Choice,
}

/// One question waiting for the person (plan 3.3 `OpenQuestion`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenQuestion {
    pub request_id: String,
    pub source: Source,
    pub kind: Kind,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    pub options: Vec<String>,
    /// An engine question as the engine asked it (`ui`), so a window draws it
    /// exactly as from the live line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui: Option<Value>,
    /// The engine's tool call the question belongs to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// A shell question: the engine's request id of the tool call that asks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    /// When it was asked (ms since the epoch).
    pub at: u64,
}

impl OpenQuestion {
    /// The question of an engine's `ui_dialog_request` line.
    pub fn from_engine(line: &Value, at: u64) -> Option<Self> {
        let request_id = line.get("requestId").and_then(Value::as_str)?.to_string();
        let ui = line.get("ui").cloned().unwrap_or(Value::Null);
        let text = |key: &str| ui.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
        let kind = match ui.get("kind").and_then(Value::as_str) {
            Some("questions") => Kind::Questions,
            Some("select" | "choice") => Kind::Choice,
            _ => Kind::Permission,
        };
        let options = ui
            .get("options")
            .and_then(Value::as_array)
            .map(|options| {
                options
                    .iter()
                    .filter_map(|option| option.get("title").and_then(Value::as_str).or_else(|| option.as_str()).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        Some(OpenQuestion {
            request_id,
            source: Source::Engine,
            kind,
            title: text("title").or_else(|| text("question")).or_else(|| text("message")).unwrap_or_else(|| "The agent needs your input".to_string()),
            body: text("description").or_else(|| text("detail")),
            options,
            tool_call_id: line.get("toolCallId").and_then(Value::as_str).map(str::to_string),
            ui: Some(ui),
            call_id: None,
            at,
        })
    }

    /// A question one of the shell's tools asks (`ToolCtx::ask`).
    pub fn from_shell(request_id: &str, call_id: &str, title: &str, body: &str, options: Vec<String>, at: u64) -> Self {
        OpenQuestion {
            request_id: request_id.to_string(),
            source: Source::Shell,
            kind: Kind::Permission,
            title: title.to_string(),
            body: Some(body.to_string()).filter(|body| !body.is_empty()),
            options,
            ui: None,
            tool_call_id: None,
            call_id: Some(call_id.to_string()).filter(|call| !call.is_empty()),
            at,
        }
    }
}

/// What a line did to a chat's questions.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// A new question (`engine://question`).
    Opened(OpenQuestion),
    /// One left, by request id (`engine://question-closed`).
    Closed(String),
}

/// The open questions of every chat.
#[derive(Default)]
pub struct Questions(Mutex<HashMap<String, Vec<OpenQuestion>>>);

impl Questions {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Vec<OpenQuestion>>> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Keep `question` for `chat`; false when it was open already (a repeat
    /// replaces it and is not news).
    pub fn open(&self, chat: &str, question: OpenQuestion) -> bool {
        let mut chats = self.lock();
        let open = chats.entry(chat.to_string()).or_default();
        match open.iter_mut().find(|q| q.request_id == question.request_id) {
            Some(known) => {
                *known = question;
                false
            }
            None => {
                open.push(question);
                true
            }
        }
    }

    /// `request_id` of `chat` was answered or ended; true when it was open.
    pub fn close(&self, chat: &str, request_id: &str) -> bool {
        let mut chats = self.lock();
        let Some(open) = chats.get_mut(chat) else {
            return false;
        };
        let before = open.len();
        open.retain(|q| q.request_id != request_id);
        let closed = open.len() != before;
        if open.is_empty() {
            chats.remove(chat);
        }
        closed
    }

    /// Every open question of `chat`, oldest first.
    pub fn of(&self, chat: &str) -> Vec<OpenQuestion> {
        self.lock().get(chat).cloned().unwrap_or_default()
    }

    /// Whether `chat` has an open question (it counts as working, FR-3).
    pub fn has(&self, chat: &str) -> bool {
        self.lock().get(chat).is_some_and(|open| !open.is_empty())
    }

    /// `chat`'s engine is gone: its questions went with it, the shell's
    /// tools' too (they asked for a call of that engine, which no longer
    /// waits for the answer). The ids that closed.
    pub fn engine_gone(&self, chat: &str) -> Vec<String> {
        self.lock().remove(chat).unwrap_or_default().into_iter().map(|q| q.request_id).collect()
    }

    /// One stdout line of `chat`'s engine, at `at`.
    pub fn on_line(&self, chat: &str, line: &Value, at: u64) -> Option<Change> {
        match line.get("type").and_then(Value::as_str)? {
            "ui_dialog_request" => {
                let question = OpenQuestion::from_engine(line, at)?;
                self.open(chat, question.clone()).then_some(Change::Opened(question))
            }
            "ui_dialog_closed" => {
                let id = line.get("requestId").and_then(Value::as_str)?;
                self.close(chat, id).then(|| Change::Closed(id.to_string()))
            }
            _ => None,
        }
    }

    /// A command a window wrote to `chat`'s engine: an answer closes its question.
    pub fn on_sent(&self, chat: &str, line: &Value) -> Option<Change> {
        if line.get("type").and_then(Value::as_str) != Some("ui_dialog_response") {
            return None;
        }
        let id = line.get("requestId").and_then(Value::as_str)?;
        self.close(chat, id).then(|| Change::Closed(id.to_string()))
    }
}

// docs/features/shell-hosted-engines/spec.md, FR-9 (T12).
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn approval(id: &str) -> Value {
        json!({
            "type": "ui_dialog_request",
            "requestId": id,
            "toolCallId": "call_1",
            "ui": {
                "kind": "confirm",
                "title": "Run `cargo test`?",
                "description": "in /repo",
                "approval": { "kind": "command", "tool": "bash" },
                "options": [{ "title": "Allow" }, { "title": "Allow commands for this chat", "permission": "allow-chat" }, { "title": "Deny", "permission": "reject" }],
            },
        })
    }

    #[test]
    fn an_engine_request_opens_a_question_once_and_its_close_removes_it() {
        let questions = Questions::default();
        let Some(Change::Opened(question)) = questions.on_line("c", &approval("approve:1"), 7) else {
            panic!("not opened");
        };
        assert_eq!((question.source, question.kind), (Source::Engine, Kind::Permission));
        assert_eq!((question.title.as_str(), question.body.as_deref()), ("Run `cargo test`?", Some("in /repo")));
        assert_eq!(question.options, ["Allow", "Allow commands for this chat", "Deny"]);
        assert_eq!((question.tool_call_id.as_deref(), question.at), (Some("call_1"), 7));
        assert_eq!(question.ui.as_ref().unwrap()["approval"]["tool"], "bash");
        // Sent again (a second window subscribed): not news.
        assert_eq!(questions.on_line("c", &approval("approve:1"), 8), None);
        assert_eq!(questions.of("c").len(), 1);
        assert!(questions.has("c") && !questions.has("d"));
        assert_eq!(questions.on_line("c", &json!({ "type": "ui_dialog_closed", "requestId": "approve:1" }), 9), Some(Change::Closed("approve:1".into())));
        assert!(!questions.has("c"));
        // Closed twice: nothing more.
        assert_eq!(questions.on_line("c", &json!({ "type": "ui_dialog_closed", "requestId": "approve:1" }), 9), None);
    }

    #[test]
    fn an_answer_a_window_writes_removes_the_question() {
        let questions = Questions::default();
        questions.on_line("c", &approval("approve:1"), 1);
        questions.on_line("c", &approval("approve:2"), 2);
        // Another command, or an answer for another chat: nothing.
        assert_eq!(questions.on_sent("c", &json!({ "type": "prompt", "message": "hi" })), None);
        assert_eq!(questions.on_sent("d", &json!({ "type": "ui_dialog_response", "requestId": "approve:1" })), None);
        let answer = json!({ "type": "ui_dialog_response", "id": "ab-3", "requestId": "approve:1", "answer": { "value": "Allow" } });
        assert_eq!(questions.on_sent("c", &answer), Some(Change::Closed("approve:1".into())));
        assert_eq!(questions.of("c").iter().map(|q| q.request_id.as_str()).collect::<Vec<_>>(), ["approve:2"]);
    }

    #[test]
    fn the_agents_questions_are_kept_as_asked() {
        let questions = Questions::default();
        let line = json!({
            "type": "ui_dialog_request", "requestId": "ask:9", "toolCallId": "call_9",
            "ui": { "kind": "questions", "title": "Which port?", "questions": [{ "question": "Which port?", "options": [{ "label": "80" }] }], "options": [] },
        });
        let Some(Change::Opened(question)) = questions.on_line("c", &line, 1) else {
            panic!("not opened");
        };
        assert_eq!((question.kind, question.title.as_str()), (Kind::Questions, "Which port?"));
        assert!(question.options.is_empty());
        assert_eq!(question.ui.unwrap()["questions"][0]["options"][0]["label"], "80");
    }

    #[test]
    fn a_shell_question_registers_and_clears() {
        let questions = Questions::default();
        let asked = OpenQuestion::from_shell("agent-ask:1", "req-4", "Allow the browser to open example.com?", "", vec!["Allow".into(), "Deny".into()], 5);
        assert!(questions.open("c", asked.clone()));
        assert_eq!(questions.of("c"), vec![asked.clone()]);
        assert_eq!((asked.source, asked.body, asked.call_id.as_deref()), (Source::Shell, None, Some("req-4")));
        // The person answers it, or its call ends: once.
        assert!(questions.close("c", "agent-ask:1"));
        assert!(!questions.close("c", "agent-ask:1"));
        assert!(!questions.has("c"));
    }

    #[test]
    fn an_engine_that_goes_takes_every_question_of_its_chat() {
        let questions = Questions::default();
        questions.on_line("c", &approval("approve:1"), 1);
        questions.open("c", OpenQuestion::from_shell("agent-ask:2", "req", "Allow?", "", vec![], 2));
        assert_eq!(questions.engine_gone("c"), ["approve:1", "agent-ask:2"]);
        assert!(!questions.has("c"));
        assert!(questions.engine_gone("d").is_empty());
    }

    #[test]
    fn a_question_reads_as_a_window_expects() {
        let questions = Questions::default();
        questions.on_line("c", &approval("approve:1"), 3);
        let wire = serde_json::to_value(questions.of("c")).unwrap();
        assert_eq!(wire[0]["requestId"], "approve:1");
        assert_eq!(wire[0]["source"], "engine");
        assert_eq!(wire[0]["kind"], "permission");
        assert_eq!(wire[0]["toolCallId"], "call_1");
        assert!(wire[0].get("callId").is_none());
    }
}
