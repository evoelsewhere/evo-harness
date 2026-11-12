//! Mail between nodes of different engines, as this engine sees it
//! (docs/features/session-roles-and-mail/spec.md, FR-6 – FR-8, NFR-1).
//!
//! No shell here: each test links its session to a channel that stands for
//! the hub, reads what the engine sends there and answers it the way the hub
//! does (`res` envelopes, `message` mail).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

use evo_ai::providers::faux::{FauxProvider, FauxScript};
use evo_harness::session::memory::{InMemorySessionStorage, SystemClock};
use evo_harness::session::session::DefaultIdGenerator;
use evo_harness::session::types::SessionMetadata;
use evo_harness::session::Session;
use evo_harness::types::{AgentHarnessOptions, HarnessTool};
use evo_protocol::mesh::{Deliver, Envelope, Kind, MailBody, MailKind, MailOp, Origin, ROUTER};
use tokio::sync::mpsc;

use super::*;

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A session of its own with the mesh tools, linked to a stand-in hub.
struct Linked {
    harness: AgentHarness,
    cell: HarnessCell,
    session: Address,
    hub: mpsc::UnboundedReceiver<Envelope>,
}

async fn linked() -> Linked {
    let id = format!("xe-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let provider = FauxProvider::new(FauxScript::new());
    let cell: HarnessCell = Arc::new(OnceLock::new());
    let tools: Vec<HarnessTool> = create(cell.clone(), None)
        .into_iter()
        .map(HarnessTool::new)
        .collect();
    let active_tool_names = tools
        .iter()
        .map(|tool| tool.tool.schema().name.clone())
        .collect();
    let storage = InMemorySessionStorage::new(
        SessionMetadata {
            id: id.clone(),
            created_at: 0,
            parent_session_id: None,
        },
        Arc::new(SystemClock),
        Arc::new(DefaultIdGenerator::new()),
    );
    let harness = AgentHarness::create(AgentHarnessOptions {
        model: provider.default_model().clone(),
        models: vec![provider],
        tools,
        active_tool_names,
        session: Session::new(Arc::new(storage), None),
        ..Default::default()
    })
    .await
    .unwrap();
    let _ = cell.set(harness.downgrade());
    let session = Address::session(&id);
    let hub = mesh::router().link_hub(&session);
    Linked {
        harness,
        cell,
        session,
        hub,
    }
}

impl Linked {
    /// The tool `name`, bound to `me` (`None`: the session).
    fn tool(&self, me: Option<&Address>, name: &str) -> Arc<dyn AgentTool> {
        create(self.cell.clone(), me.cloned())
            .into_iter()
            .find(|tool| tool.schema().name == name)
            .unwrap()
    }

    /// Run `name` with `params` in the background.
    fn spawn(
        &self,
        me: Option<&Address>,
        name: &str,
        params: Value,
        signal: CancellationToken,
    ) -> tokio::task::JoinHandle<Result<AgentToolResult, AgentError>> {
        let tool = self.tool(me, name);
        let noop: Arc<dyn Fn(ToolResultPartial) + Send + Sync> = Arc::new(|_| {});
        tokio::spawn(async move { tool.execute("call", params, signal, noop).await })
    }

    /// The next envelope the engine sent the hub.
    async fn next(&mut self) -> Envelope {
        tokio::time::timeout(Duration::from_secs(5), self.hub.recv())
            .await
            .expect("the engine wrote to the hub")
            .expect("linked")
    }

    /// A subagent of this session, known to the router.
    fn agent(&self, title: &str) -> Address {
        let id = format!("{title}-{}", NEXT.fetch_add(1, Ordering::Relaxed));
        let address = self.session.child(NodeKind::Agent, &id);
        mesh::router().spawn(address.clone(), title, Origin::Agent, json!({}), None);
        address
    }

    /// Mail the hub delivers to this engine; the engine's answer body.
    async fn receive(&self, from: &str, to: &Address, body: MailBody) -> Value {
        let id = format!("mail_xe_{}", NEXT.fetch_add(1, Ordering::Relaxed));
        let envelope = Envelope::mail(id, from, to.as_str(), &body);
        mesh::receive_mail(&self.harness, &envelope).await.body
    }
}

/// Answer `envelope` as the hub does.
fn answer(envelope: &Envelope, body: Value) {
    let res = Envelope::response("hub_r".into(), envelope, Ok(body));
    assert!(mesh::router().on_answer(&res), "someone waited for it");
}

fn text(result: &AgentToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|c| match c {
            TextContentOrImage::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect()
}

fn ask_body(id: &str) -> MailBody {
    MailBody {
        kind: Some(MailKind::Ask),
        ask: Some(evo_protocol::mesh::AskInfo {
            id: id.into(),
            timeout_ms: Some(600_000),
        }),
        ..MailBody::new(
            format!("<mail id=\"{id}\" from=\"s_p/a_3\" title=\"Lead\" kind=\"ask\">\nWhich port?\n</mail>"),
            Deliver::Interrupt,
        )
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_target_is_forwarded_to_the_hub() {
    let mut s = linked().await;
    let call = s.spawn(
        None,
        "send_message",
        json!({ "to": "API builder", "message": "The port is 8080." }),
        CancellationToken::new(),
    );
    let out = s.next().await;
    assert_eq!((out.kind, out.op.as_str()), (Kind::Mail, "send"));
    assert_eq!(
        (out.from.as_str(), out.to.as_str()),
        (s.session.as_str(), "API builder")
    );
    let (op, body) = out.as_mail_op().unwrap();
    assert_eq!(op, MailOp::Send);
    assert_eq!(
        (body.kind, body.deliver),
        (Some(MailKind::Message), Deliver::Interrupt)
    );
    assert!(
        body.text.starts_with(&format!("<mail id=\"{}\"", out.id)),
        "{}",
        body.text
    );
    assert!(body.text.contains("kind=\"message\"") && body.text.contains("8080"));
    answer(
        &out,
        json!({ "deliveredTo": "s_b1", "title": "API builder", "kind": "message" }),
    );
    let result = call.await.unwrap().unwrap();
    assert!(text(&result).contains("API builder"), "{}", text(&result));
    assert_eq!(result.details["mail"]["to"], "s_b1");

    // The hub's refusal says who can be reached.
    let call = s.spawn(
        None,
        "send_message",
        json!({ "to": "Nobody", "message": "hi" }),
        CancellationToken::new(),
    );
    let out = s.next().await;
    answer(
        &out,
        json!({ "error": { "code": "not_found", "message": "`Nobody` is not a session you can reach",
            "reachable": [{ "id": "s_p", "title": "Parent", "relation": "parent" }] } }),
    );
    let error = call.await.unwrap().unwrap_err().to_string();
    assert!(error.contains("not a session you can reach"), "{error}");
    assert!(error.contains("s_p (Parent)"), "{error}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_remote_ask_carries_its_waits_blocks_and_resolves_on_its_reply() {
    let mut s = linked().await;
    // The session waits on its foreground agent, which asks another session.
    let agent = s.agent("Worker");
    let _foreground = mesh::router().add_wait(&s.session, &agent);
    let call = s.spawn(
        Some(&agent),
        "ask",
        json!({ "to": "API builder", "question": "Which port?", "timeoutSeconds": 60 }),
        CancellationToken::new(),
    );
    let out = s.next().await;
    assert_eq!(
        (out.op.as_str(), out.from.as_str()),
        ("ask", agent.as_str())
    );
    let (_, body) = out.as_mail_op().unwrap();
    let ask = body.ask.clone().unwrap();
    assert_eq!(ask.id, out.id);
    assert_eq!(ask.timeout_ms, Some(60_000));
    assert_eq!(body.waits, vec![[s.session.clone(), agent.clone()]]);
    assert_eq!(body.title.as_deref(), Some("Worker"));
    assert!(body.text.contains("kind=\"ask\"") && body.text.contains("reply tool"));
    answer(
        &out,
        json!({ "deliveredTo": "s_b1", "title": "API builder", "kind": "ask" }),
    );
    // Still waiting: the hub only routed it.
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!call.is_finished());
    // The answer comes back as mail; it resolves the call and is not shown.
    let reply = MailBody {
        kind: Some(MailKind::Reply),
        reply_to: Some(ask.id.clone()),
        ..MailBody::new(
            "<mail id=\"m_9\" from=\"s_b1\" title=\"API builder\" kind=\"reply\">\n8080\n</mail>",
            Deliver::Interrupt,
        )
    };
    let taken = s.receive("s_b1", &agent, reply).await;
    assert_eq!(taken["answered"], true, "{taken}");
    let result = call.await.unwrap().unwrap();
    assert_eq!(text(&result), "API builder answered:\n8080");
    mesh::router().transition(&agent, NodeState::Closed, None, None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_timeout_or_a_cancel_sends_cancel_ask() {
    let mut s = linked().await;
    // The timeout (the tool's shortest is 10 s: the wait itself, shorter).
    let (_keep, answer_rx) = tokio::sync::oneshot::channel::<String>();
    let outcome = wait_remote(
        &s.session,
        "k-timeout",
        answer_rx,
        Duration::from_millis(50),
        &CancellationToken::new(),
    )
    .await;
    assert!(outcome.unwrap_err().contains("no answer within"));
    let cancel = s.next().await;
    assert_eq!(
        (cancel.op.as_str(), cancel.to.as_str()),
        ("cancel_ask", ROUTER)
    );
    let (_, body) = cancel.as_mail_op().unwrap();
    assert_eq!(body.ask.unwrap().id, "k-timeout");

    // A cancelled call stops waiting and tells the hub too.
    let signal = CancellationToken::new();
    let call = s.spawn(
        None,
        "ask",
        json!({ "to": "Client", "question": "Ready?" }),
        signal.clone(),
    );
    let out = s.next().await;
    answer(
        &out,
        json!({ "deliveredTo": "s_b2", "title": "Client", "kind": "ask" }),
    );
    signal.cancel();
    let error = call.await.unwrap().unwrap_err().to_string();
    assert!(error.contains("cancelled"), "{error}");
    let cancel = s.next().await;
    assert_eq!(cancel.op, "cancel_ask");
    assert_eq!(cancel.as_mail_op().unwrap().1.ask.unwrap().id, out.id);
    // A late reply finds no waiting call: it is shown as mail instead.
    let late = MailBody {
        kind: Some(MailKind::Reply),
        reply_to: Some(out.id.clone()),
        ..MailBody::new("late", Deliver::Interrupt)
    };
    let taken = s.receive("s_b2", &s.session, late).await;
    assert_eq!(taken["queued"], "next_turn", "{taken}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reply_with_no_waiting_call_becomes_a_chip() {
    let s = linked().await;
    let reply = MailBody {
        kind: Some(MailKind::Reply),
        reply_to: Some("ask_from_before_a_restart".into()),
        title: Some("Client".into()),
        ..MailBody::new(
            "<mail id=\"m_1\" from=\"s_b2\" title=\"Client\" kind=\"reply\">\nDone.\n</mail>",
            Deliver::Interrupt,
        )
    };
    let taken = s.receive("s_b2", &s.session, reply).await;
    assert_eq!(
        (taken["queued"].as_str(), taken["wake"].as_bool()),
        (Some("next_turn"), Some(true)),
        "{taken}"
    );
    assert!(taken.get("answered").is_none());
    // Woken for mail, labelled with the sender.
    let node = mesh::router().get(&s.session).unwrap();
    assert_eq!(node.meta["wakeReasons"]["s_b2"], "mail");
    assert_eq!(node.meta["wakeTitles"]["s_b2"], "Client");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_received_ask_is_answered_by_reply_or_by_the_end_of_the_run() {
    let mut s = linked().await;
    let asker = Address::parse("s_p/a_3").unwrap();
    let id = format!("k-in-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let taken = s.receive(asker.as_str(), &s.session, ask_body(&id)).await;
    assert_eq!(taken["queued"], "next_turn", "{taken}");
    // list_nodes shows it as a question for the session.
    let call = s.spawn(None, "list_nodes", json!({}), CancellationToken::new());
    let peers = s.next().await;
    answer(&peers, json!({ "nodes": [] }));
    let listed = call.await.unwrap().unwrap();
    assert_eq!(listed.details["questionsForYou"][0]["askId"], id.as_str());
    assert_eq!(listed.details["questionsForYou"][0]["from"], "s_p/a_3");
    // Only the node it was asked of answers it.
    let agent = s.agent("Helper");
    let error = s
        .spawn(
            Some(&agent),
            "reply",
            json!({ "askId": id, "answer": "x" }),
            CancellationToken::new(),
        )
        .await
        .unwrap()
        .unwrap_err()
        .to_string();
    assert!(error.contains("not asked to you"), "{error}");
    let call = s.spawn(
        None,
        "reply",
        json!({ "askId": id, "answer": "8080" }),
        CancellationToken::new(),
    );
    let out = s.next().await;
    assert_eq!((out.op.as_str(), out.to.as_str()), ("reply", "s_p/a_3"));
    let (_, body) = out.as_mail_op().unwrap();
    assert_eq!(
        (body.kind, body.reply_to.as_deref()),
        (Some(MailKind::Reply), Some(id.as_str()))
    );
    assert!(
        body.text.contains("kind=\"reply\"") && body.text.contains("8080"),
        "{}",
        body.text
    );
    answer(
        &out,
        json!({ "deliveredTo": "s_p/a_3", "title": "Parent", "kind": "reply" }),
    );
    assert!(text(&call.await.unwrap().unwrap()).contains("s_p/a_3"));
    let again = s
        .spawn(
            None,
            "reply",
            json!({ "askId": id, "answer": "x" }),
            CancellationToken::new(),
        )
        .await
        .unwrap()
        .unwrap_err()
        .to_string();
    assert!(again.contains("no open question"), "{again}");

    // A question the run did not answer gets its final text.
    let second = format!("{id}-b");
    s.receive(asker.as_str(), &s.session, ask_body(&second))
        .await;
    run_ended(&s.session, "The port is 8080.");
    let out = s.next().await;
    assert_eq!((out.op.as_str(), out.to.as_str()), ("reply", "s_p/a_3"));
    let (_, body) = out.as_mail_op().unwrap();
    assert_eq!(body.reply_to.as_deref(), Some(second.as_str()));
    assert!(
        body.text.contains("did not call reply") && body.text.contains("8080"),
        "{}",
        body.text
    );
    mesh::router().transition(&agent, NodeState::Closed, None, None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn list_nodes_shows_peers() {
    let mut s = linked().await;
    let call = s.spawn(None, "list_nodes", json!({}), CancellationToken::new());
    let out = s.next().await;
    assert_eq!(
        (out.kind, out.op.as_str(), out.to.as_str()),
        (Kind::Cmd, "peers", ROUTER)
    );
    answer(
        &out,
        json!({ "nodes": [
            { "id": "s_p", "title": "Parent", "kind": "session", "relation": "parent", "state": "idle" },
            { "id": "s_b2", "title": "Client", "kind": "session", "relation": "sibling", "state": "running" },
        ] }),
    );
    let listed = call.await.unwrap().unwrap();
    let nodes = listed.details["nodes"].as_array().unwrap().clone();
    let rows: Vec<(String, String)> = nodes
        .iter()
        .map(|n| {
            (
                n["id"].as_str().unwrap().into(),
                n["relation"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("s_p".to_string(), "parent".to_string()),
            ("s_b2".into(), "sibling".into())
        ]
    );
    assert_eq!(nodes[1]["state"], "running");
    assert_eq!(nodes[1]["kind"], "session");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_a_hub_an_unknown_target_is_refused_at_once() {
    let Linked {
        harness: _harness,
        cell,
        hub,
        ..
    } = linked().await;
    drop(hub);
    let send = create(cell, None)
        .into_iter()
        .find(|tool| tool.schema().name == "send_message")
        .unwrap();
    let error = tokio::time::timeout(
        Duration::from_secs(2),
        send.execute(
            "c",
            json!({ "to": "nobody", "message": "hi" }),
            CancellationToken::new(),
            Arc::new(|_| {}),
        ),
    )
    .await
    .expect("no wait for a hub")
    .unwrap_err()
    .to_string();
    assert!(error.contains("you can reach: nobody"), "{error}");
}

#[test]
fn the_capability_is_announced() {
    let caps = mesh::capabilities();
    assert_eq!(caps["crossEngineMail"], true);
    let ops: Vec<&str> = caps["mail"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(ops, ["message", "send", "ask", "reply", "cancel_ask"]);
}
