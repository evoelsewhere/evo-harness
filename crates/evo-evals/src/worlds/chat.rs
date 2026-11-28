//! The chat world: the engine's own model loops on a scripted model —
//! the index run (a chat's new part read with the `memory` and
//! `set_summary` tools) and a learning step (one tool-free call). The
//! evidence is which tools each call offered, which the model called with
//! what, the prompts it was given, and what memory holds afterwards.

use std::sync::{Arc, Mutex};

use evo_ai::providers::faux::{faux_assistant_message, FauxProvider, FauxScript, FauxStep};
use evo_ai::types::{AssistantMessage, Context, StopReason};
use evo_engine::memory_index::{Chunk, IndexInput, IndexRun};
use serde_json::{json, Value};

use super::memory::{snapshot, World};

#[derive(Default)]
struct Seen {
    tools: Vec<Value>,
    prompts: Vec<String>,
    calls: Vec<Value>,
}

/// The model's answers, scripted: `{"call": name, "args": {...}}` or `{"text": "..."}`.
fn script(steps: &[Value], seen: &Arc<Mutex<Seen>>) -> FauxScript {
    let script = FauxScript::new();
    let responses = steps
        .iter()
        .map(|step| {
            let reply = match step.get("call").and_then(Value::as_str) {
                Some(name) => match FauxStep::tool_call(name, step.get("args").cloned().unwrap_or(json!({}))) {
                    FauxStep::Message(message) => message,
                    FauxStep::Factory(_) => faux_assistant_message("", StopReason::Stop),
                },
                None => faux_assistant_message(step.get("text").and_then(Value::as_str).unwrap_or("done"), StopReason::Stop),
            };
            let seen = seen.clone();
            let call = step.get("call").and_then(Value::as_str).map(|name| json!({ "name": name, "arguments": step.get("args").cloned().unwrap_or(json!({})) }));
            FauxStep::factory(move |context: &Context| {
                let mut seen = seen.lock().unwrap();
                if let Some(call) = &call {
                    seen.calls.push(call.clone());
                }
                seen.tools.push(json!(context.tools.iter().map(|tool| tool.name.clone()).collect::<Vec<_>>()));
                if let Some(system) = &context.system_prompt {
                    seen.prompts.push(system.clone());
                }
                for message in &context.messages {
                    if let evo_ai::types::Message::User(user) = message {
                        seen.prompts.push(format!("{:?}", user.content));
                    }
                }
                reply.clone()
            })
        })
        .collect();
    script.set_responses(responses);
    script
}

/// Run one step; its evidence.
pub async fn step(world: &World, step: &Value, today: &str) -> Result<Value, String> {
    let steps = step
        .get("model")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let seen: Arc<Mutex<Seen>> = Arc::default();
    let provider = FauxProvider::new(script(&steps, &seen));
    let model = provider.default_model().clone();
    let result = match step.get("do").and_then(Value::as_str).unwrap_or("") {
        "index" => {
            let person = step
                .get("person")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let chat = step
                .get("chat")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let tool = evo_engine::tools::memory::index_memory_tool(
                world.cwd.clone(),
                Arc::new(|_, _, _, _| {}),
            )
            .with_person(person);
            let memory = move |params: &Value| {
                tool.call(&{
                    let mut params = params.clone();
                    params["scope"] = json!("personal");
                    params
                })
            };
            let topics = || "## Personal topics\n(none yet)".to_string();
            let quiet = |_: &AssistantMessage| {};
            let run = IndexRun {
                provider: provider.clone(),
                model: &model,
                memory: Some(&memory),
                topics: &topics,
                sensitive: false,
                on_response: &quiet,
            };
            let input = IndexInput {
                chunks: vec![Chunk {
                    text: chat,
                    through: "e1".into(),
                }],
                through: Some("e1".into()),
                more: false,
            };
            let (summary, _, outcome) = run.read(&input, None).await;
            json!({ "summary": summary, "error": outcome.err() })
        }
        "learnCall" => {
            let provider: Arc<dyn evo_ai::provider::Provider> = provider.clone();
            let quiet = |_: &AssistantMessage| {};
            let prompt = evo_engine::learning::run::fenced(
                "chats",
                step.get("read").and_then(Value::as_str).unwrap_or(""),
            );
            match evo_engine::learning::run::ask(
                &provider,
                &model,
                evo_engine::learning::run::DATA_RULE,
                prompt,
                &quiet,
            )
            .await
            {
                Ok(answer) => json!({ "answer": answer }),
                Err(error) => json!({ "error": error }),
            }
        }
        other => return Err(format!("the chat world has no step `{other}`")),
    };
    let seen = seen.lock().unwrap();
    let mut evidence = snapshot(world, today);
    evidence["toolsOffered"] = Value::Array(seen.tools.clone());
    evidence["prompts"] = json!(seen.prompts);
    evidence["toolCalls"] = Value::Array(seen.calls.clone());
    evidence["result"] = result;
    Ok(evidence)
}
