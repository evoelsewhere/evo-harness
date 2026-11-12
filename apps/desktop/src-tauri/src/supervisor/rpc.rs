//! Requests the shell itself sends to a chat's engine, and their answers
//! (docs/features/shell-hosted-engines/spec.md, FR-1; plan 3.3 "Shell → engine").
//!
//! The shell sets an engine up with the same RPC commands a window uses
//! (`set_model`, `configure_external_tools`, …). Its requests carry ids
//! `shell-<n>`; every window hears every engine line, and the windows drop
//! responses with these ids (`src/engine/client.ts`). A request that gets no
//! answer within its timeout fails, and so does every open request of an
//! engine that exits or is restarted.

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::oneshot;

/// The id prefix of the shell's own requests.
pub const ID_PREFIX: &str = "shell-";

/// How long the shell waits for an engine's answer.
pub const TIMEOUT: Duration = Duration::from_secs(15);

/// An answer: the response's `data` (`null` when it has none), or its error.
pub type Reply = Result<Value, String>;

/// The shell's open requests, by id.
#[derive(Default)]
pub struct ShellRpc {
    next: AtomicU64,
    /// Open requests: the chat whose engine was asked, and where its answer goes.
    pending: Mutex<HashMap<String, (String, oneshot::Sender<Reply>)>>,
}

impl ShellRpc {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, (String, oneshot::Sender<Reply>)>> {
        self.pending.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// A new request `command` to `chat`'s engine: its id, the line to write
    /// and where its answer arrives.
    pub fn open(&self, chat: &str, command: &str, payload: Value) -> (String, Value, oneshot::Receiver<Reply>) {
        let id = format!("{ID_PREFIX}{}", self.next.fetch_add(1, Ordering::Relaxed) + 1);
        let mut line = match payload {
            Value::Object(_) => payload,
            _ => json!({}),
        };
        line["id"] = json!(id);
        line["type"] = json!(command);
        let (tx, rx) = oneshot::channel();
        self.lock().insert(id.clone(), (chat.to_string(), tx));
        (id, line, rx)
    }

    /// Forget an open request (it could not be written, or timed out).
    pub fn cancel(&self, id: &str) {
        self.lock().remove(id);
    }

    /// One stdout line of `chat`'s engine. True when it answered one of the
    /// shell's requests (the answer is delivered); any other line is left to
    /// the other readers.
    pub fn on_line(&self, chat: &str, line: &Value) -> bool {
        if line.get("type").and_then(Value::as_str) != Some("response") {
            return false;
        }
        let Some(id) = line.get("id").and_then(Value::as_str).filter(|id| id.starts_with(ID_PREFIX)) else {
            return false;
        };
        let mut pending = self.lock();
        // Another engine's answer with a reused id is not this request's.
        if pending.get(id).is_none_or(|(asked, _)| asked != chat) {
            return false;
        }
        let Some((_, tx)) = pending.remove(id) else {
            return false;
        };
        drop(pending);
        let reply = if line.get("success").and_then(Value::as_bool).unwrap_or(false) {
            Ok(line.get("data").cloned().unwrap_or(Value::Null))
        } else {
            Err(line
                .get("error")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("{} failed", line.get("command").and_then(Value::as_str).unwrap_or("command"))))
        };
        let _ = tx.send(reply);
        true
    }

    /// `chat`'s engine exited or was replaced: its open requests fail with `reason`.
    pub fn fail_chat(&self, chat: &str, reason: &str) {
        let failed: Vec<oneshot::Sender<Reply>> = {
            let mut pending = self.lock();
            let ids: Vec<String> = pending.iter().filter(|(_, (asked, _))| asked == chat).map(|(id, _)| id.clone()).collect();
            ids.into_iter().filter_map(|id| pending.remove(&id).map(|(_, tx)| tx)).collect()
        };
        for tx in failed {
            let _ = tx.send(Err(reason.to_string()));
        }
    }

    /// How many requests wait for an answer.
    #[cfg(test)]
    pub fn open_requests(&self) -> usize {
        self.lock().len()
    }
}

/// Send `command` to `chat`'s engine through `write` and wait for its answer,
/// at most `timeout`.
pub async fn request<W, F>(rpc: &ShellRpc, chat: &str, command: &str, payload: Value, timeout: Duration, write: W) -> Reply
where
    W: FnOnce(Value) -> F,
    F: Future<Output = Result<(), String>>,
{
    let (id, line, rx) = rpc.open(chat, command, payload);
    if let Err(error) = write(line).await {
        rpc.cancel(&id);
        return Err(error);
    }
    match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(reply)) => reply,
        Ok(Err(_)) => Err("the engine stopped".to_string()),
        Err(_) => {
            rpc.cancel(&id);
            Err(format!("{command}: no answer from the engine in {} s", timeout.as_secs()))
        }
    }
}

// docs/features/shell-hosted-engines/spec.md, FR-1 (T2).
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn response(id: &str, success: bool, data: Value) -> Value {
        if success {
            json!({ "type": "response", "id": id, "command": "get_state", "success": true, "data": data })
        } else {
            json!({ "type": "response", "id": id, "command": "set_model", "success": false, "error": data })
        }
    }

    #[test]
    fn requests_carry_shell_ids_and_the_command() {
        let rpc = ShellRpc::default();
        let (first, line, _rx) = rpc.open("a", "set_model", json!({ "provider": "acme", "modelId": "m" }));
        let (second, _, _rx2) = rpc.open("a", "get_state", Value::Null);
        assert!(first.starts_with(ID_PREFIX) && second.starts_with(ID_PREFIX));
        assert_ne!(first, second);
        assert_eq!(line["type"], "set_model");
        assert_eq!(line["id"], json!(first));
        assert_eq!(line["provider"], "acme");
        assert_eq!(rpc.open_requests(), 2);
    }

    #[tokio::test]
    async fn an_answer_resolves_its_request_with_data_or_error() {
        let rpc = ShellRpc::default();
        let (ok_id, _, ok) = rpc.open("a", "get_state", json!({}));
        let (err_id, _, err) = rpc.open("a", "set_model", json!({}));
        assert!(rpc.on_line("a", &response(&ok_id, true, json!({ "provider": "acme" }))));
        assert!(rpc.on_line("a", &response(&err_id, false, json!("no such model"))));
        assert_eq!(ok.await.unwrap(), Ok(json!({ "provider": "acme" })));
        assert_eq!(err.await.unwrap(), Err("no such model".to_string()));
        assert_eq!(rpc.open_requests(), 0);
        // A response without data is null.
        let (id, _, rx) = rpc.open("a", "configure_external_tools", json!({}));
        assert!(rpc.on_line("a", &json!({ "type": "response", "id": id, "success": true })));
        assert_eq!(rx.await.unwrap(), Ok(Value::Null));
    }

    #[test]
    fn lines_that_are_not_the_shells_answers_are_left_alone() {
        let rpc = ShellRpc::default();
        let (id, _, _rx) = rpc.open("a", "get_state", json!({}));
        // A window's own request, another engine's line, other events.
        assert!(!rpc.on_line("a", &response("ab12cd-3", true, json!({}))));
        assert!(!rpc.on_line("b", &response(&id, true, json!({}))));
        assert!(!rpc.on_line("a", &json!({ "type": "agent_start", "id": id })));
        assert!(!rpc.on_line("a", &response("shell-999", true, json!({}))));
        assert_eq!(rpc.open_requests(), 1);
    }

    #[tokio::test]
    async fn a_request_without_an_answer_times_out_and_is_forgotten() {
        let rpc = ShellRpc::default();
        let reply = request(&rpc, "a", "get_state", json!({}), Duration::from_millis(20), |_| async { Ok(()) }).await;
        assert!(reply.unwrap_err().contains("no answer"));
        assert_eq!(rpc.open_requests(), 0);
    }

    #[tokio::test]
    async fn a_request_that_cannot_be_written_fails_at_once() {
        let rpc = ShellRpc::default();
        let reply = request(&rpc, "a", "get_state", json!({}), TIMEOUT, |_| async { Err("engine is not running".to_string()) }).await;
        assert_eq!(reply, Err("engine is not running".to_string()));
        assert_eq!(rpc.open_requests(), 0);
    }

    #[tokio::test]
    async fn a_written_request_resolves_when_its_answer_is_read() {
        let rpc = Arc::new(ShellRpc::default());
        let reader = rpc.clone();
        let reply = request(&rpc, "a", "get_state", json!({}), TIMEOUT, |line| async move {
            // The engine answers on its stdout, read by another task.
            let id = line["id"].as_str().unwrap().to_string();
            tokio::spawn(async move {
                reader.on_line("a", &response(&id, true, json!({ "modelId": "m" })));
            });
            Ok(())
        })
        .await;
        assert_eq!(reply, Ok(json!({ "modelId": "m" })));
    }

    #[tokio::test]
    async fn an_engine_that_exits_fails_its_open_requests() {
        let rpc = ShellRpc::default();
        let (_, _, a) = rpc.open("a", "get_state", json!({}));
        let (_, _, b) = rpc.open("b", "get_state", json!({}));
        rpc.fail_chat("a", "engine exited");
        assert_eq!(a.await.unwrap(), Err("engine exited".to_string()));
        assert_eq!(rpc.open_requests(), 1);
        drop(b);
    }
}
