//! The plugin-AI engine: `evo-engine --mode plugin-ai`
//! (docs/features/plugin-ai/spec.md; plan 3.1, 3.5; ADR 0023).
//!
//! One process, started by the desktop shell on a plugin's first AI call
//! and stopped after it has been idle. It serves many runs at once: each run
//! is a plugin's request for a model's answer, streamed back line by line
//! (`evo_protocol::plugin_ai`). The shell has already checked the plugin's
//! permissions, caps and concurrency; this process chooses the model among
//! the person's connected providers ([`choose`]), calls it, falls back to
//! the next fitting model once, records usage under the plugin's id and
//! keeps the run's result.
//!
//! It is not a chat: no session file, no hooks, skills, connectors or
//! project instructions, and no tools in this phase.

pub mod choose;
mod context;
mod run;
mod tools;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use evo_ai::types::InputModality;
use evo_protocol::plugin_ai::{AiCommand, AiOutput, Allowed, ModelInfo, Price, PROTOCOL};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::usage_log;

/// Tool calls waiting for the shell's answer, by `(run id, call id)`.
type Pending = HashMap<(String, String), tokio::sync::oneshot::Sender<(Value, bool)>>;
pub(crate) static TOOL_CALLS: std::sync::LazyLock<Mutex<Pending>> =
    std::sync::LazyLock::new(Mutex::default);

/// Questions to the person waiting for `ai_answer`, by id.
type Questions = HashMap<String, tokio::sync::oneshot::Sender<i32>>;
pub(crate) static QUESTIONS: std::sync::LazyLock<Mutex<Questions>> =
    std::sync::LazyLock::new(Mutex::default);
pub(crate) static NEXT_QUESTION: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

/// Lines to the shell, written in order by one task.
#[derive(Clone)]
pub(crate) struct Out(mpsc::UnboundedSender<String>);

impl Out {
    pub(crate) fn send(&self, output: &AiOutput) {
        if let Ok(line) = serde_json::to_string(output) {
            let _ = self.0.send(line);
        }
    }
}

/// The person's connected models, as plugins see them; deprecated ones are
/// left out.
pub(crate) fn connected() -> Vec<ModelInfo> {
    let Ok(resolved) = crate::provider::resolve(None, None, None, None, None) else {
        return Vec::new();
    };
    let dev = evo_runtime::models_dev::ModelsDev::bundled();
    crate::provider::authenticated_catalog(&resolved)
        .into_iter()
        .filter_map(|model| {
            let facts = dev.model(&model.provider, &model.id);
            if facts.is_some_and(|f| f.deprecated) {
                return None;
            }
            let rates = &model.cost.rates;
            Some(ModelInfo {
                vision: model.input.contains(&InputModality::Image),
                tools: facts.is_none_or(|f| f.tool_call),
                reasoning: model.reasoning,
                context: model.context_window,
                max_output: model.max_tokens,
                cost: (rates.input > 0.0 || rates.output > 0.0).then_some(Price {
                    input: rates.input,
                    output: rates.output,
                }),
                tiers: Vec::new(),
                name: model.name.clone(),
                provider: model.provider.clone(),
                model: model.id.clone(),
            })
        })
        .collect()
}

/// `models` that `allowed` lets a plugin use.
pub(crate) fn allowed(models: &[ModelInfo], allowed: &Allowed) -> Vec<ModelInfo> {
    models
        .iter()
        .filter(|m| {
            allowed
                .providers
                .as_ref()
                .is_none_or(|p| p.contains(&m.provider))
        })
        .filter(|m| {
            allowed.models.as_ref().is_none_or(|list| {
                list.iter()
                    .any(|r| r.provider == m.provider && r.model.eq_ignore_ascii_case(&m.model))
            })
        })
        .cloned()
        .collect()
}

/// `evo-engine --mode plugin-ai`: serve until stdin ends or `stop`.
pub async fn main() -> i32 {
    usage_log::start(None);
    if let Ok(conn) = evo_store::open() {
        let now = run::now_ms();
        let _ = evo_store::plugin_ai::end_orphans(&conn, now);
        let _ = evo_store::plugin_ai::prune(&conn, now);
    }
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let out = Out(tx);
    let writer = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(line) = rx.recv().await {
            if stdout.write_all(line.as_bytes()).await.is_err()
                || stdout.write_all(b"\n").await.is_err()
            {
                break;
            }
            let _ = stdout.flush().await;
        }
    });
    let catalog = Arc::new(RwLock::new(connected()));
    let runs: Arc<Mutex<HashMap<String, CancellationToken>>> = Arc::default();
    let env = Arc::new(run::RunEnv::real(out.clone()));
    out.send(&AiOutput::Ready {
        protocol: PROTOCOL.into(),
    });

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let command = match serde_json::from_str::<AiCommand>(&line) {
            Ok(command) => command,
            Err(error) => {
                let id = serde_json::from_str::<Value>(&line)
                    .ok()
                    .and_then(|v| v.get("id").cloned())
                    .unwrap_or(Value::Null);
                out.send(&AiOutput::Response {
                    id,
                    data: None,
                    error: Some(format!("unknown command: {error}")),
                });
                continue;
            }
        };
        match command {
            AiCommand::AiModels { id, allowed: rules } => {
                let mut models =
                    allowed(&catalog.read().unwrap_or_else(|e| e.into_inner()), &rules);
                choose::tiers_of(&mut models);
                out.send(&AiOutput::Response {
                    id,
                    data: serde_json::to_value(models).ok(),
                    error: None,
                });
            }
            AiCommand::AiRun(request) => {
                let cancel = CancellationToken::new();
                runs.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(request.run_id.clone(), cancel.clone());
                let models = allowed(
                    &catalog.read().unwrap_or_else(|e| e.into_inner()),
                    &request.allowed,
                );
                let (out, runs, env) = (out.clone(), runs.clone(), env.clone());
                tokio::spawn(async move {
                    let id = request.run_id.clone();
                    run::run(*request, models, out, cancel, env).await;
                    runs.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
                });
            }
            AiCommand::AiToolResult {
                run_id,
                call_id,
                result,
                is_error,
            } => {
                if let Some(tx) = TOOL_CALLS
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&(run_id, call_id))
                {
                    let _ = tx.send((result, is_error));
                }
            }
            AiCommand::AiAnswer { id, index } => {
                if let Some(tx) = QUESTIONS
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&id)
                {
                    let _ = tx.send(index);
                }
            }
            AiCommand::AiCancel { run_id } => {
                if let Some(cancel) = runs.lock().unwrap_or_else(|e| e.into_inner()).get(&run_id) {
                    cancel.cancel();
                }
            }
            AiCommand::ReloadProviders => {
                run::forget_broken();
                let fresh = tokio::task::spawn_blocking(connected)
                    .await
                    .unwrap_or_default();
                *catalog.write().unwrap_or_else(|e| e.into_inner()) = fresh;
            }
            AiCommand::Stop => break,
        }
    }
    for cancel in runs.lock().unwrap_or_else(|e| e.into_inner()).values() {
        cancel.cancel();
    }
    // Let cancelled runs write their end.
    for _ in 0..50 {
        if runs.lock().unwrap_or_else(|e| e.into_inner()).is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    usage_log::flush(Duration::from_secs(5));
    // The writer ends when every sender is gone: ours and the one `env`
    // keeps for tool calls (a run still finishing holds it a little longer).
    drop(out);
    drop(env);
    let _ = tokio::time::timeout(Duration::from_secs(2), writer).await;
    0
}
