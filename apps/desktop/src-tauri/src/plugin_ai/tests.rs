use evo_plugins::store::AiModel;
use serde_json::json;

use super::*;

fn body(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

#[test]
fn caps_count_what_was_spent_and_what_the_run_costs() {
    assert!(!over_cap((0.5, 3.0), 0.2, (None, None)), "no caps by default");
    assert!(over_cap((0.9, 3.0), 0.2, (Some(1.0), None)), "today's cap");
    assert!(over_cap((0.0, 9.95), 0.1, (None, Some(10.0))), "this month's cap");
    assert!(!over_cap((0.2, 0.2), 0.0, (Some(1.0), Some(10.0))));
    assert!(over_cap((1.0, 1.0), 0.0, (Some(1.0), None)), "a reached cap refuses the next run");
}

#[test]
fn the_engines_request_takes_only_what_a_plugin_may_set() {
    let settings = AiSettings {
        providers: Some(vec!["anthropic".into()]),
        default_model: Some(AiModel { provider: "anthropic".into(), model: "m".into() }),
        tiers: [("fast".to_string(), AiModel { provider: "anthropic".into(), model: "f".into() })].into(),
        ..Default::default()
    };
    let asked = body(json!({
        "messages": [{ "role": "user", "content": "hi" }],
        "need": { "tier": "fast" },
        "system": "Be brief.",
        "thread": "AAPL",
        "runId": "forged",
        "plugin": { "id": "other@m", "name": "other" },
        "allowed": { "providers": ["everything"] }
    }));
    let request = build_request("r1", "p@m", "p", &settings, &asked).unwrap();
    assert_eq!(request["type"], "ai_run");
    assert_eq!(request["runId"], "r1", "the shell names the run");
    assert_eq!(request["plugin"], json!({ "id": "p@m", "name": "p" }), "a plugin cannot speak for another");
    assert_eq!(request["allowed"], json!({ "providers": ["anthropic"] }), "the person's rules, not the plugin's");
    assert_eq!(request["defaultModel"]["model"], "m");
    assert_eq!(request["pins"]["fast"]["model"], "f");
    assert_eq!((request["need"]["tier"].as_str(), request["system"].as_str(), request["thread"].as_str()), (Some("fast"), Some("Be brief."), Some("AAPL")));

    assert_eq!(build_request("r", "p@m", "p", &settings, &body(json!({}))).unwrap_err().0, "bad_request");
    assert_eq!(build_request("r", "p@m", "p", &settings, &body(json!({ "messages": [] }))).unwrap_err().0, "bad_request");
    let long_thread = "x".repeat(200);
    assert_eq!(build_request("r", "p@m", "p", &settings, &body(json!({ "messages": [{ "role": "user", "content": "x" }], "thread": long_thread }))).unwrap_err().0, "bad_request");
    let huge = "A".repeat(MAX_REQUEST_BYTES + 1);
    assert_eq!(build_request("r", "p@m", "p", &settings, &body(json!({ "messages": [{ "role": "user", "content": huge }] }))).unwrap_err().0, "bad_request");
}

#[test]
fn no_rules_allow_every_connected_model() {
    let rules = rules(&AiSettings::default());
    assert_eq!(rules["allowed"], json!({}));
    assert!(rules.get("defaultModel").is_none());
}

#[test]
fn events_reach_every_follower_and_a_finished_run_is_forgotten() {
    let (a_tx, mut a_rx) = mpsc::unbounded_channel();
    let (b_tx, mut b_rx) = mpsc::unbounded_channel();
    state().runs.lock().unwrap().insert(
        "t-route".into(),
        RunEntry { plugin_id: "p@m".into(), view: None, thread: None, routes: HashMap::new(), followers: vec![Follower::Stream(a_tx)], started: Instant::now(), spent: (0.0, 0.0), caps: (None, None), cost: 0.0, model: None, cap_hit: false, apps: None },
    );
    route("t-route", json!({ "type": "started", "provider": "a", "model": "m" }));
    // A second follower joins mid-run (a view opened again).
    follow("p@m", "t-route", Follower::Stream(b_tx)).unwrap();
    assert!(follow("q@m", "t-route", Follower::View("x".into())).is_err(), "another plugin cannot follow it");
    route("t-route", json!({ "type": "text", "text": "hi" }));
    route("t-route", json!({ "type": "done", "result": { "text": "hi", "stopped": "end" } }));
    let got_a: Vec<String> = std::iter::from_fn(|| a_rx.try_recv().ok()).map(|e| e["type"].as_str().unwrap().to_string()).collect();
    let got_b: Vec<String> = std::iter::from_fn(|| b_rx.try_recv().ok()).map(|e| e["type"].as_str().unwrap().to_string()).collect();
    assert_eq!(got_a, ["started", "text", "done"]);
    assert_eq!(got_b, ["text", "done"]);
    assert!(!state().runs.lock().unwrap().contains_key("t-route"));
    assert_eq!(state().runs.lock().unwrap().get("t-route").map(|r| r.model.clone()), None);
}

#[test]
fn usage_past_a_cap_marks_the_run_once() {
    let (tx, _rx) = mpsc::unbounded_channel();
    state().runs.lock().unwrap().insert(
        "t-cap".into(),
        RunEntry { plugin_id: "p@m".into(), view: None, thread: None, routes: HashMap::new(), followers: vec![Follower::Stream(tx)], started: Instant::now(), spent: (0.95, 0.95), caps: (Some(1.0), None), cost: 0.0, model: None, cap_hit: false, apps: None },
    );
    route("t-cap", json!({ "type": "usage", "usage": { "costUsd": 0.01 } }));
    assert!(!state().runs.lock().unwrap()["t-cap"].cap_hit);
    // Over the cap: marked (and a cancel goes to the engine, once).
    let marked = {
        let mut runs = state().runs.lock().unwrap();
        let entry = runs.get_mut("t-cap").unwrap();
        entry.cost = 0.06;
        over_cap(entry.spent, entry.cost, entry.caps)
    };
    assert!(marked);
    state().runs.lock().unwrap().remove("t-cap");
}

#[test]
fn a_plugin_counts_only_its_own_runs() {
    let (tx, _rx) = mpsc::unbounded_channel();
    for (id, plugin) in [("t-c1", "count@m"), ("t-c2", "count@m"), ("t-c3", "other-count@m")] {
        state().runs.lock().unwrap().insert(
            id.into(),
            RunEntry { plugin_id: plugin.into(), view: None, thread: None, routes: HashMap::new(), followers: vec![Follower::Stream(tx.clone())], started: Instant::now(), spent: (0.0, 0.0), caps: (None, None), cost: 0.0, model: None, cap_hit: false, apps: None },
        );
    }
    let (mine, all) = running("count@m");
    assert_eq!(mine, 2);
    assert!(all >= 3);
    let going = runs("count@m");
    assert_eq!(going["going"].as_array().unwrap().len(), 2);
    for id in ["t-c1", "t-c2", "t-c3"] {
        state().runs.lock().unwrap().remove(id);
    }
}

fn server_tools() -> Vec<Value> {
    vec![
        json!({ "name": "plugin_stocks_api_quote", "description": "[stocks] The last price.", "inputSchema": { "type": "object", "properties": { "symbol": { "type": "string" } } }, "server": "plugin:stocks:api", "tool": "quote" }),
        json!({ "name": "plugin_stocks_api_news", "description": "[stocks] Headlines.", "inputSchema": { "type": "object" }, "server": "plugin:stocks:api", "tool": "news" }),
        json!({ "name": "plugin_other_api_quote", "description": "Someone else's.", "inputSchema": { "type": "object" }, "server": "plugin:other:api", "tool": "secret" }),
    ]
}

#[test]
fn a_views_tools_are_offered_and_routed_to_the_view() {
    let asked = body(json!({ "tools": [{ "name": "pick_chart", "description": "Pick a chart.", "inputSchema": { "type": "object" } }] }));
    let (offered, routes) = tools_for(&asked, true, "stocks", &server_tools()).unwrap();
    assert_eq!(offered, [json!({ "name": "pick_chart", "description": "Pick a chart.", "inputSchema": { "type": "object" }, "origin": "view" })]);
    assert_eq!(routes["pick_chart"], ToolRoute::View);
    // A backend (host API) cannot offer functions it has no way to answer.
    assert_eq!(tools_for(&asked, false, "stocks", &[]).unwrap_err().0, "bad_request");
    // Names the model can call, once each, never evo's own `respond`.
    for bad in [json!([{ "name": "respond" }]), json!([{ "name": "has space" }]), json!([{ "name": "a" }, { "name": "a" }]), json!({ "name": "a" })] {
        assert!(tools_for(&body(json!({ "tools": bad })), true, "stocks", &[]).is_err(), "{bad}");
    }
}

#[test]
fn backend_tools_come_only_from_the_plugins_own_servers() {
    let (offered, routes) = tools_for(&body(json!({ "backendTools": true })), false, "stocks", &server_tools()).unwrap();
    let names: Vec<&str> = offered.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["quote", "news"]);
    assert_eq!(offered[0]["description"], "The last price.", "the [plugin] tag is dropped");
    assert_eq!(offered[0]["origin"], "backend");
    assert_eq!(routes["quote"], ToolRoute::Backend { key: "plugin:stocks:api".into(), tool: "quote".into() });
    assert!(!routes.contains_key("secret"), "another plugin's tools are not offered");

    let (offered, _) = tools_for(&body(json!({ "backendTools": ["news"] })), false, "stocks", &server_tools()).unwrap();
    assert_eq!(offered.len(), 1);
    assert!(tools_for(&body(json!({ "backendTools": ["secret"] })), false, "stocks", &server_tools()).is_err(), "a missing name is an error");
    assert!(tools_for(&body(json!({ "backendTools": "all" })), false, "stocks", &server_tools()).is_err());
    let (offered, routes) = tools_for(&body(json!({})), true, "stocks", &server_tools()).unwrap();
    assert!(offered.is_empty() && routes.is_empty(), "no tools unless asked");

    // A view's own tool wins over a server tool of the same name.
    let both = body(json!({ "tools": [{ "name": "quote" }], "backendTools": true }));
    let (offered, routes) = tools_for(&both, true, "stocks", &server_tools()).unwrap();
    assert_eq!(offered.len(), 2);
    assert_eq!(routes["quote"], ToolRoute::View);
}

#[test]
fn a_server_tools_answer_reads_as_text() {
    let result = json!({ "content": [{ "type": "text", "text": "AAPL 187.2" }, { "type": "image", "data": "x", "mimeType": "image/png" }], "structuredContent": { "price": 187.2 } });
    let text = mcp_text(&result);
    assert!(text.starts_with("AAPL 187.2\n"));
    assert!(text.contains("\"image\"") && text.ends_with("{\"price\":187.2}"));
}

#[tokio::test]
async fn only_the_runs_plugin_answers_its_tools() {
    state().runs.lock().unwrap().insert(
        "t-tools".into(),
        RunEntry { plugin_id: "p@m".into(), view: Some("v".into()), thread: None, routes: HashMap::new(), followers: Vec::new(), started: Instant::now(), spent: (0.0, 0.0), caps: (None, None), cost: 0.0, model: None, cap_hit: false, apps: None },
    );
    assert_eq!(view_tool_result("q@m", "t-tools", "c1", json!(1), false).await.unwrap_err().0, "not_found");
    assert!(view_tool_result("p@m", "t-tools", "c1", json!(1), false).await.is_ok());
    state().runs.lock().unwrap().remove("t-tools");
}

fn row(role: &str, message: Value) -> evo_store::plugin_ai::ThreadMessage {
    evo_store::plugin_ai::ThreadMessage { role: role.into(), content_json: message.to_string(), provider_id: (role == "assistant").then(|| "anthropic".into()), model_id: (role == "assistant").then(|| "m".into()), at_ms: 5 }
}

#[test]
fn a_thread_reads_as_a_conversation_with_its_tool_steps() {
    let rows = [
        row("user", json!({ "role": "user", "content": [{ "type": "text", "text": "How is AAPL?" }, { "type": "text", "text": "[an image was attached]" }], "timestamp": 1 })),
        row("assistant", json!({ "role": "assistant", "content": [{ "type": "text", "text": "Checking." }, { "type": "toolCall", "id": "c1", "name": "quote", "arguments": { "symbol": "AAPL" } }] })),
        row("toolResult", json!({ "role": "toolResult", "toolCallId": "c1", "toolName": "quote", "content": [{ "type": "text", "text": "187.2" }], "isError": false })),
        row("assistant", json!({ "role": "assistant", "content": [{ "type": "text", "text": "AAPL is at 187.2." }] })),
        row("user", json!({ "role": "user", "content": "Thanks", "timestamp": 2 })),
        row("assistant", Value::String("not json".into())),
    ];
    let view = thread_view(&rows);
    assert_eq!(view.len(), 5);
    assert_eq!(view[0], json!({ "role": "user", "text": "How is AAPL?\n[an image was attached]", "at": 5 }));
    assert_eq!(view[1]["text"], "Checking.");
    assert_eq!(view[1]["tools"], json!([{ "id": "c1", "name": "quote", "input": { "symbol": "AAPL" }, "result": "187.2", "isError": false }]));
    assert_eq!((view[2]["text"].as_str(), view[2]["model"].as_str()), (Some("AAPL is at 187.2."), Some("m")));
    assert_eq!(view[3]["text"], "Thanks");
}

#[tokio::test]
async fn a_thread_with_a_run_going_cannot_be_deleted() {
    state().runs.lock().unwrap().insert(
        "t-thread".into(),
        RunEntry { plugin_id: "p@m".into(), view: None, thread: Some("AAPL".into()), routes: HashMap::new(), followers: Vec::new(), started: Instant::now(), spent: (0.0, 0.0), caps: (None, None), cost: 0.0, model: None, cap_hit: false, apps: None },
    );
    assert!(thread_going("p@m", "AAPL"));
    assert!(!thread_going("q@m", "AAPL"), "threads are per plugin");
    assert_eq!(delete_thread("p@m", "AAPL").await.unwrap_err().0, "busy");
    state().runs.lock().unwrap().remove("t-thread");
}

#[test]
fn evo_tools_need_their_group_permission() {
    let approved: Vec<String> = ["ai", "ai.tools:read", "ai.tools:command"].iter().map(|p| p.to_string()).collect();
    assert!(evo_groups(&body(json!({})), &approved).unwrap().is_empty());
    assert_eq!(evo_groups(&body(json!({ "evoTools": ["read", "command", "read"] })), &approved).unwrap(), ["read", "command"]);
    assert_eq!(evo_groups(&body(json!({ "evoTools": ["edit"] })), &approved).unwrap_err().0, "no_permission");
    assert_eq!(evo_groups(&body(json!({ "evoTools": ["apps"] })), &approved).unwrap_err().0, "no_permission");
    let with_apps: Vec<String> = ["ai", "ai.tools:apps"].iter().map(|p| p.to_string()).collect();
    assert_eq!(evo_groups(&body(json!({ "evoTools": ["apps"] })), &with_apps).unwrap(), ["apps"]);
    assert_eq!(evo_groups(&body(json!({ "evoTools": ["everything"] })), &approved).unwrap_err().0, "bad_request");
    assert_eq!(evo_groups(&body(json!({ "evoTools": "read" })), &approved).unwrap_err().0, "bad_request");
}

#[test]
fn a_plugin_run_gets_the_browser_and_terminals_only() {
    let names: Vec<String> = crate::agent_tools::static_definitions()
        .into_iter()
        .filter_map(|d| d["name"].as_str().map(str::to_string))
        .filter(|n| crate::agent_tools::PLUGIN_RUN_PREFIXES.iter().any(|p| n.starts_with(p)))
        .collect();
    for wanted in ["browser_navigate", "browser_read_page", "browser_screenshot", "terminal_run", "terminal_read"] {
        assert!(names.iter().any(|n| n == wanted), "{wanted} missing from {names:?}");
    }
    for chat_only in ["plugin_show_card", "app_panel", "send_file", "create_scheduled_task", "computer_app", "session_read"] {
        assert!(!names.iter().any(|n| n == chat_only), "{chat_only} is offered");
    }
    assert_eq!(crate::agent_tools::plugin_run_key("r1"), "plugin-ai:r1");
}

#[tokio::test]
async fn a_shell_question_is_answered_in_the_shell_and_closes_with_its_run() {
    let cancel = tokio_util::sync::CancellationToken::new();
    // No run: nobody is asked.
    assert_eq!(ask_in_run("t-gone", "p: x", "d", vec!["Allow".into()], &cancel).await, -1);
    state().runs.lock().unwrap().insert(
        "t-ask".into(),
        RunEntry { plugin_id: "p@m".into(), view: None, thread: None, routes: HashMap::new(), followers: Vec::new(), started: Instant::now(), spent: (0.0, 0.0), caps: (None, None), cost: 0.0, model: None, cap_hit: false, apps: None },
    );
    let asking = tokio::spawn({
        let cancel = cancel.clone();
        async move { ask_in_run("t-ask", "p: Navigate?", "detail", vec!["Allow".into(), "Allow for this run".into(), "Deny".into()], &cancel).await }
    });
    let id = loop {
        tokio::task::yield_now().await;
        let found = state().local.lock().unwrap().keys().next().cloned();
        if let Some(id) = found {
            break id;
        }
    };
    plugin_ai_answer(id, 1).await.unwrap();
    assert_eq!(asking.await.unwrap(), 1);
    // A question whose call is dropped answers -1.
    let asking = tokio::spawn({
        let cancel = cancel.clone();
        async move { ask_in_run("t-ask", "p: Run?", "detail", vec!["Allow".into(), "Deny".into()], &cancel).await }
    });
    tokio::task::yield_now().await;
    cancel.cancel();
    assert_eq!(asking.await.unwrap(), -1);
    assert!(state().questions.lock().unwrap().iter().all(|q| q["runId"] != "t-ask"), "its questions are closed");
    state().runs.lock().unwrap().remove("t-ask");
}
