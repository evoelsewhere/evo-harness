//! Calls from a program to tools, over the tool program capability
//! (docs/features/code-mode/spec.md, FR-5, FR-7, FR-12, FR-13; plan 3.5).
//!
//! Each [`Call`] the interpreter sends becomes a nested tool call through
//! [`ToolProgram`], so it passes the whole `before_tool_call` /
//! `after_tool_call` chain like a direct call (FR-10). At most
//! [`CONCURRENCY`] run at once; the rest wait their turn.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use evo_agent::tool_program::ToolProgram;
use evo_agent::types::TextContentOrImage;
use serde_json::{json, Value};
use tokio::sync::Semaphore;

use super::runtime::Call;
use crate::tools::info::callable_from_code;

/// Calls one program runs at once: the tool program's own active limit.
pub const CONCURRENCY: usize = 8;

/// What a call keeps for the transcript's Code row (FR-15): string
/// arguments past this many characters are cut, output past this many is cut,
/// details larger than this many bytes are left out, and once the kept copies
/// of one program pass [`KEPT_BYTES`] only names and outcomes are kept.
const ARG_CHARS: usize = 1_000;
const OUTPUT_CHARS: usize = 4_000;
const DETAILS_BYTES: usize = 8 * 1024;
const KEPT_BYTES: usize = 256 * 1024;

/// One call a program made, in start order, with a bounded copy of what it
/// was given and returned for the transcript (FR-15).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallRecord {
    pub name: String,
    /// `None` while running.
    pub ok: Option<bool>,
    pub ms: u64,
    pub args: Value,
    pub output: String,
    pub details: Value,
}

impl CallRecord {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            ok: None,
            ms: 0,
            args: Value::Null,
            output: String::new(),
            details: Value::Null,
        }
    }
}

/// What the bridge saw.
#[derive(Debug, Default)]
pub struct Served {
    pub calls: Vec<CallRecord>,
    /// Images the calls returned, for the program's result (FR-8).
    pub images: Vec<TextContentOrImage>,
    /// Bytes of kept copies so far.
    pub(crate) kept: usize,
}

fn cut(text: &str, chars: usize) -> String {
    if text.chars().count() <= chars {
        text.to_owned()
    } else {
        format!("{}…[cut]", text.chars().take(chars).collect::<String>())
    }
}

/// Arguments with long strings cut and large nested values left out.
fn kept_args(args: &Value) -> Value {
    let Some(fields) = args.as_object() else {
        return Value::Null;
    };
    Value::Object(
        fields
            .iter()
            .map(|(key, value)| {
                let kept = match value {
                    Value::String(text) => Value::String(cut(text, ARG_CHARS)),
                    other if other.to_string().len() <= ARG_CHARS => other.clone(),
                    _ => Value::String("[cut]".into()),
                };
                (key.clone(), kept)
            })
            .collect(),
    )
}

/// Serve `calls` until the interpreter drops its sender and every call has
/// finished. `timeout_ms` is each nested call's limit; a shell command keeps
/// its own, shorter, timeout (FR-13).
pub async fn serve(
    program: Arc<ToolProgram>,
    mut calls: tokio::sync::mpsc::UnboundedReceiver<Call>,
    timeout_ms: u64,
) -> Served {
    let gate = Arc::new(Semaphore::new(CONCURRENCY));
    let served = Arc::new(Mutex::new(Served::default()));
    let mut running = tokio::task::JoinSet::new();
    while let Some(call) = calls.recv().await {
        let index = {
            let mut served = served.lock().unwrap();
            served.calls.push(CallRecord::new(&call.name));
            served.calls.len() - 1
        };
        running.spawn(one(
            program.clone(),
            gate.clone(),
            served.clone(),
            index,
            call,
            timeout_ms,
        ));
    }
    while running.join_next().await.is_some() {}
    let mut served = served.lock().unwrap();
    std::mem::take(&mut *served)
}

async fn one(
    program: Arc<ToolProgram>,
    gate: Arc<Semaphore>,
    served: Arc<Mutex<Served>>,
    index: usize,
    call: Call,
    timeout_ms: u64,
) {
    let started = Instant::now();
    let args = kept_args(&call.args);
    let (reply, images) = if callable_from_code(&call.name) {
        let _permit = gate.acquire_owned().await;
        dispatch(&program, &call.name, call.args, timeout_ms).await
    } else {
        (
            failure(format!("{} cannot be called from code", call.name)),
            Vec::new(),
        )
    };
    {
        let mut served = served.lock().unwrap();
        let output = cut(reply["text"].as_str().unwrap_or_default(), OUTPUT_CHARS);
        let details = Some(&reply["details"])
            .filter(|d| !d.is_null() && d.to_string().len() <= DETAILS_BYTES)
            .cloned()
            .unwrap_or(Value::Null);
        let size = args.to_string().len() + output.len() + details.to_string().len();
        let keep = served.kept + size <= KEPT_BYTES;
        if keep {
            served.kept += size;
        }
        if let Some(record) = served.calls.get_mut(index) {
            record.ok = Some(reply["ok"] == true);
            record.ms = started.elapsed().as_millis() as u64;
            if keep {
                record.args = args;
                record.output = output;
                record.details = details;
            }
        }
        served.images.extend(images);
    }
    let _ = call.reply.send((call.id, Ok(reply)));
}

/// The program's view of a failed call.
fn failure(text: String) -> Value {
    json!({ "ok": false, "text": text })
}

/// Start the call, wait for it, release it, and shape the reply.
async fn dispatch(
    program: &Arc<ToolProgram>,
    name: &str,
    args: Value,
    timeout_ms: u64,
) -> (Value, Vec<TextContentOrImage>) {
    let request = json!({
        "op": "start", "name": name, "arguments": args, "timeoutMs": timeout_ms,
    });
    // A call that just finished releases its slot a moment after its result
    // is published; wait for it rather than fail (FR-7).
    let mut tries = 0;
    let started = loop {
        match program.handle(request.clone()).await {
            Err(error) if error.contains("active request limit") && tries < 5_000 => {
                tries += 1;
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
            other => break other,
        }
    };
    let id = match started {
        Ok(reply) => reply["requestId"].as_str().unwrap_or_default().to_owned(),
        Err(error) if error.contains("effective active subset") => {
            return (
                failure(format!("{name} is not a tool in this chat")),
                Vec::new(),
            )
        }
        Err(error) => return (failure(error), Vec::new()),
    };
    let done = program.wait(&id).await;
    let _ = program
        .handle(json!({ "op": "dispose", "requestId": id }))
        .await;
    match done {
        Ok(done) => shape(&done),
        Err(error) => (failure(error), Vec::new()),
    }
}

/// `{ ok, text, details, images }` from a finished request; images go to the
/// program's result instead of into the program.
fn shape(done: &Value) -> (Value, Vec<TextContentOrImage>) {
    if done["status"] != "done" {
        let error = done["error"].as_str().unwrap_or("the call failed");
        return (failure(error.to_owned()), Vec::new());
    }
    let result = &done["result"];
    let mut texts = Vec::new();
    let mut images = Vec::new();
    for part in result["content"].as_array().into_iter().flatten() {
        match part["type"].as_str() {
            Some("text") => texts.push(part["text"].as_str().unwrap_or_default().to_owned()),
            Some("image") => {
                if let Ok(image) = serde_json::from_value(part.clone()) {
                    images.push(TextContentOrImage::Image(image));
                }
            }
            _ => {}
        }
    }
    let reply = json!({
        "ok": done["isError"] != true,
        "text": texts.join("\n"),
        "details": result.get("details").cloned().unwrap_or(Value::Null),
        "images": images.len(),
    });
    (reply, images)
}

#[cfg(test)]
#[path = "bridge_tests.rs"]
mod tests;
