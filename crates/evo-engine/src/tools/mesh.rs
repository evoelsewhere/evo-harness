//! Peer messaging between the conversations this engine hosts (session mesh
//! phase 4, `docs/features/session-mesh-protocol/spec.md`): the `send_message`,
//! `ask`, `reply`, `list_nodes` and `cancel_node` tools.
//!
//! The session has one instance of each (bound to the session's node); every
//! subagent gets its own, bound to its node (`tools/agent.rs` passes them as
//! the child's tool replacements), so a call knows who is calling. The
//! session's instances refuse calls from any other child (plugin children).
//!
//! - **Who may talk to whom.** A node reaches its parent, its children and,
//!   with the `mail.siblings` capability, its siblings — never another tree.
//! - **Delivery.** Mail is a user-role `<mail …>` message. A running receiver
//!   gets it before its next model call; an idle session gets it with its next
//!   run and a wake request (`meta.wakeReasons` = `mail`); an idle agent gets
//!   it when it is continued — except for an `ask`, which continues it at once.
//! - **ask / reply.** `ask` blocks the caller until the receiver calls
//!   `reply` with the ask's id, its run ends (its final text is the answer),
//!   or the timeout. The router's wait-for graph refuses an ask that would
//!   close a cycle (including asking the session that waits on you in a
//!   foreground `agent` call), and a pair of nodes may exchange at most
//!   [`crate::mesh::MAX_PAIR_MAIL`] messages per session run.
//! - **Other engines** (docs/features/session-roles-and-mail/spec.md, FR-6 –
//!   FR-8). A target this engine does not host — `parent` from the session,
//!   a nested or sibling session by title or id, a node of its engine — goes
//!   to the hub as a mail op when the session is linked to one
//!   ([`crate::mesh::LocalRouter::link_hub`]); the hub holds reach, the pair
//!   limit and the wait-for graph across engines (an `ask` reports the local
//!   waits that end at its asker). A remote ask waits for a `reply` mail and
//!   sends `cancel_ask` when it stops waiting; an ask from another engine is
//!   answered with `reply` or by the end of the asked node's run. `list_nodes`
//!   adds the hub's `peers`.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use evo_agent::agent_tool::AgentTool;
use evo_agent::error::AgentError;
use evo_agent::types::{AgentToolResult, TextContentOrImage, ToolExecutionMode, ToolResultPartial};
use evo_agent::AgentMessage;
use evo_ai::types::{Schema, Tool};
use evo_harness::agent_harness::{AgentHarness, WeakAgentHarness};
use serde_json::{json, Value};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::mesh::{self, Address, ListScope, Node, NodeKind, NodeState};
use evo_protocol::mesh::{
    AskInfo, Deliver, Envelope, MailBody, MailKind, MailOp, MeshCommand, ROUTER,
};

/// The session's harness, bound once it exists.
pub type HarnessCell = Arc<std::sync::OnceLock<WeakAgentHarness>>;

/// Names of the tools, for agent types that narrow the tool list.
pub const TOOL_NAMES: [&str; 5] = ["send_message", "ask", "reply", "list_nodes", "cancel_node"];
const DEFAULT_ASK_SECS: u64 = 600;
const MAX_ASK_SECS: u64 = 1800;
const MAX_TEXT: usize = 20_000;
/// How long a mail op waits for the hub to route it.
const ROUTE_WAIT: Duration = Duration::from_secs(15);
/// How long `list_nodes` waits for the hub's `peers`.
const PEERS_WAIT: Duration = Duration::from_secs(3);

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

fn text_result(text: impl Into<String>, details: Value) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextContentOrImage::text(text.into())],
        details,
        ..Default::default()
    }
}

// ── Pending asks ─────────────────────────────────────────────────────────────

struct PendingAsk {
    id: String,
    from: Address,
    to: Address,
    answer: oneshot::Sender<String>,
}

static ASKS: LazyLock<Mutex<Vec<PendingAsk>>> = LazyLock::new(Default::default);

/// Asks of this engine's nodes to nodes of other engines that wait for a
/// `reply` mail, by ask id (docs/features/session-roles-and-mail/spec.md, FR-6).
static WAITING: LazyLock<Mutex<HashMap<String, oneshot::Sender<String>>>> =
    LazyLock::new(Default::default);

/// An ask from another engine to a node of this one, not answered yet.
struct AskedRemotely {
    /// The asker (a node of another engine).
    from: Address,
    /// The node asked here.
    to: Address,
}

/// Asks from other engines by ask id (FR-6, mesh FR-29).
static ASKED: LazyLock<Mutex<HashMap<String, AskedRemotely>>> = LazyLock::new(Default::default);

/// `ask` mail from another engine arrived for `to` (the session or a node it
/// hosts): `reply` and the end of `to`'s run answer it.
pub(crate) fn asked_remotely(id: &str, from: &Address, to: &Address) {
    lock(&ASKED).insert(
        id.to_string(),
        AskedRemotely {
            from: from.clone(),
            to: to.clone(),
        },
    );
}

/// A `reply` mail for ask `id`: resolves the call that waits for it. False
/// when none waits (it timed out, or the engine restarted).
pub(crate) fn answer_remote(id: &str, text: &str) -> bool {
    match lock(&WAITING).remove(id) {
        Some(waiter) => waiter.send(mail_text(text)).is_ok(),
        None => false,
    }
}

/// What a `<mail …>` message says, without its tags.
fn mail_text(rendered: &str) -> String {
    let rendered = rendered.trim();
    let inner = rendered
        .strip_suffix("</mail>")
        .filter(|_| rendered.starts_with("<mail "))
        .and_then(|open| open.split_once('\n'))
        .map(|(_, inner)| inner.trim());
    inner.unwrap_or(rendered).to_string()
}

/// The session whose engine runs `node`.
fn session_of(node: &Address) -> Address {
    Address::session(node.session_id())
}

/// What a question gets when its receiver's run ends without a reply.
fn final_answer(text: &str) -> String {
    if text.trim().is_empty() {
        "(it finished without answering)".to_string()
    } else {
        format!(
            "(it did not call reply; its final message)\n{}",
            text.trim()
        )
    }
}

/// `node`'s run ended: questions to it that it did not answer get `text`
/// (its final reply) as their answer — those from other engines as `reply`
/// mail through the hub.
pub fn run_ended(node: &Address, text: &str) {
    let answered: Vec<PendingAsk> = {
        let mut asks = lock(&ASKS);
        let (done, open): (Vec<_>, Vec<_>) = asks.drain(..).partition(|ask| ask.to == *node);
        *asks = open;
        done
    };
    for ask in answered {
        let _ = ask.answer.send(final_answer(text));
    }
    let remote: Vec<(String, AskedRemotely)> = {
        let mut asked = lock(&ASKED);
        let ids: Vec<String> = asked
            .iter()
            .filter(|(_, ask)| ask.to == *node)
            .map(|(id, _)| id.clone())
            .collect();
        ids.into_iter()
            .filter_map(|id| asked.remove(&id).map(|ask| (id, ask)))
            .collect()
    };
    for (id, ask) in remote {
        let me = node_of(node);
        let envelope = Envelope::mail_op(
            mesh::router().next_id(),
            node.as_str(),
            ask.from.as_str(),
            MailOp::Reply,
            &reply_body(&me, &id, &final_answer(text)),
        );
        // The hub answers; nobody needs to wait for it.
        mesh::router().to_hub(&session_of(node), envelope);
    }
}

/// Open questions to `node`, oldest first (`list_nodes` shows them).
fn asks_to(node: &Address) -> Vec<Value> {
    let mut asks: Vec<Value> = lock(&ASKS)
        .iter()
        .filter(|ask| ask.to == *node)
        .map(|ask| json!({ "askId": ask.id, "from": ask.from }))
        .collect();
    asks.extend(
        lock(&ASKED)
            .iter()
            .filter(|(_, ask)| ask.to == *node)
            .map(|(id, ask)| json!({ "askId": id, "from": ask.from })),
    );
    asks
}

// ── Context ──────────────────────────────────────────────────────────────────

struct Ctx {
    harness: HarnessCell,
    /// The node these instances act for; `None`: the session itself.
    me: Option<Address>,
}

impl Ctx {
    fn session(&self) -> Result<AgentHarness, AgentError> {
        self.harness
            .get()
            .and_then(WeakAgentHarness::upgrade)
            .ok_or_else(|| AgentError::Tool("the session is not ready".into()))
    }

    fn session_address(&self) -> Result<Address, AgentError> {
        Ok(Address::session(
            &self.session()?.session().storage().metadata().id,
        ))
    }

    /// Who is calling.
    fn caller(&self) -> Result<Address, AgentError> {
        match &self.me {
            Some(me) => Ok(me.clone()),
            None if evo_agent::tool_execution_scope::is_child_execution() => Err(AgentError::Tool(
                "this tool is not available to this kind of child agent".into(),
            )),
            None => self.session_address(),
        }
    }
}

/// The node record of `address`, or a stand-in for a session the router
/// does not know (an engine with no RPC client: tests, headless).
fn node_of(address: &Address) -> Node {
    mesh::router().get(address).unwrap_or_else(|| {
        let mut node = Node::new(address.clone(), None, "", mesh::Origin::User);
        node.state = NodeState::Idle;
        node.caps = vec!["mail.children".into()];
        node
    })
}

fn title_of(node: &Node) -> String {
    match node.kind {
        NodeKind::Session => "the session".into(),
        _ if node.title.is_empty() => node.id.to_string(),
        _ => node.title.clone(),
    }
}

/// The nodes `me` may address: parent, children, and siblings (capability).
fn reachable(me: &Node) -> Vec<Node> {
    let router = mesh::router();
    let mut out = Vec::new();
    if let Some(parent) = &me.parent {
        out.push(node_of(parent));
        if me.caps.iter().any(|cap| cap == "mail.siblings") {
            let siblings = router
                .list(ListScope::Children, Some(parent))
                .unwrap_or_default();
            out.extend(
                siblings
                    .into_iter()
                    .filter(|n| n.id != me.id && n.kind == NodeKind::Agent),
            );
        }
    }
    let children = router
        .list(ListScope::Children, Some(&me.id))
        .unwrap_or_default();
    out.extend(children.into_iter().filter(|n| n.kind == NodeKind::Agent));
    out
}

/// `to` as given by a model: `parent`, an address, an agent id, or a title.
fn resolve(me: &Node, to: &str) -> Result<Node, AgentError> {
    let to = to.trim();
    let candidates = reachable(me);
    let found = if to.eq_ignore_ascii_case("parent") {
        me.parent
            .as_ref()
            .and_then(|p| candidates.iter().find(|n| n.id == *p))
    } else {
        candidates.iter().find(|n| {
            n.id.as_str() == to
                || n.id.as_str().ends_with(&format!("/a_{to}"))
                || (!n.title.is_empty() && n.title.eq_ignore_ascii_case(to))
        })
    };
    found.cloned().ok_or_else(|| {
        let names: Vec<String> = candidates
            .iter()
            .map(|n| format!("{} ({})", n.id, title_of(n)))
            .collect();
        AgentError::Validation(format!(
            "`{to}` is not a node you can reach; you can reach: {}",
            if names.is_empty() {
                "nobody".to_string()
            } else {
                names.join(", ")
            }
        ))
    })
}

/// The `<mail …>` message the receiver's model reads.
fn render(id: &str, from: &Node, ask: bool, text: &str) -> String {
    let kind = if ask {
        MailKind::Ask
    } else {
        MailKind::Message
    };
    render_mail(id, &from.id, &title_of(from), kind, text)
}

/// [`render`] for any kind, with the sender's title as given.
fn render_mail(id: &str, from: &Address, title: &str, kind: MailKind, text: &str) -> String {
    let kind_name = match kind {
        MailKind::Message => "message",
        MailKind::Ask => "ask",
        MailKind::Reply => "reply",
    };
    let mut body = format!(
        "<mail id=\"{id}\" from=\"{from}\" title=\"{}\" kind=\"{kind_name}\">\n{}\n",
        title.replace('"', "'"),
        text.trim()
    );
    if kind == MailKind::Ask {
        body.push_str(&format!(
            "(A question for you: answer it with the reply tool, askId \"{id}\". The asker waits for your answer.)\n"
        ));
    }
    body.push_str("</mail>");
    body
}

// ── Other engines ────────────────────────────────────────────────────────────
// docs/features/session-roles-and-mail/spec.md, FR-6 – FR-8.

/// How a node of this engine is named to another engine's model: a
/// subagent by its title, the session by its title or address (the
/// receiver does not know it as "the session").
fn remote_title(node: &Node) -> String {
    match node.kind {
        NodeKind::Session if node.title.is_empty() => node.id.to_string(),
        NodeKind::Session => node.title.clone(),
        _ => title_of(node),
    }
}

/// The sender's title for the hub (the receiver's wake label); the hub
/// puts the chat's own title for the session.
fn hub_title(node: &Node) -> Option<String> {
    (node.kind != NodeKind::Session).then(|| title_of(node))
}

/// The body of a mail op from `me`.
fn remote_body(me: &Node, id: &str, kind: MailKind, text: &str) -> MailBody {
    MailBody {
        kind: Some(kind),
        title: hub_title(me),
        ..MailBody::new(
            render_mail(id, &me.id, &remote_title(me), kind, text),
            Deliver::Interrupt,
        )
    }
}

/// The body of `me`'s answer to the ask `ask`.
fn reply_body(me: &Node, ask: &str, answer: &str) -> MailBody {
    MailBody {
        reply_to: Some(ask.to_string()),
        ..remote_body(me, ask, MailKind::Reply, answer)
    }
}

/// Where mail goes: a node this engine hosts, or the hub.
enum Target {
    Local(Box<Node>),
    /// `to` as the hub resolves it; `hint` is why it is not a local node.
    Remote {
        to: String,
        hint: Option<String>,
    },
}

/// `to` for `me`: a local node it may reach, or — when its session is
/// linked to the hub — anything else, which the hub routes or refuses.
fn target(me: &Node, to: &str) -> Result<Target, AgentError> {
    let session = session_of(&me.id);
    match resolve(me, to) {
        Ok(node) if node.kind == NodeKind::Session && !node.id.is_hosted_by(&session) => {
            Ok(Target::Remote {
                to: node.id.to_string(),
                hint: None,
            })
        }
        Ok(node) => Ok(Target::Local(Box::new(node))),
        Err(error) if mesh::router().has_hub(&session) => Ok(Target::Remote {
            to: to.trim().to_string(),
            hint: Some(error.to_string()),
        }),
        Err(error) => Err(error),
    }
}

/// Send a mail op of `me` to the hub and wait until it is routed: the
/// hub's answer (`deliveredTo`, `title`), or its refusal as a tool error.
async fn route(
    me: &Node,
    to: &str,
    op: MailOp,
    id: String,
    body: &MailBody,
    hint: Option<&str>,
) -> Result<Value, AgentError> {
    let envelope = Envelope::mail_op(id, me.id.as_str(), to, op, body);
    mesh::router()
        .ask_hub(&session_of(&me.id), envelope, ROUTE_WAIT)
        .await
        .map_err(|error| {
            let message = match hint {
                Some(hint) if error.code == mesh::ErrorCode::NotFound => {
                    format!("{}. Here: {hint}", error.message)
                }
                _ => error.message,
            };
            AgentError::Tool(message)
        })
}

/// The hub's name for where mail went.
fn routed_title(answer: &Value, to: &str) -> String {
    answer["title"]
        .as_str()
        .filter(|title| !title.is_empty())
        .unwrap_or(to)
        .to_string()
}

/// Wait for the `reply` to ask `id` of `me` (another engine answers it), up
/// to `wait` or until `signal`; when it stops waiting without an answer it
/// tells the hub (`cancel_ask`), so the ask leaves the wait-for graph.
async fn wait_remote(
    me: &Address,
    id: &str,
    answer: oneshot::Receiver<String>,
    wait: Duration,
    signal: &CancellationToken,
) -> Result<String, String> {
    let outcome = tokio::select! {
        answer = answer => answer.map_err(|_| "the question was dropped".to_string()),
        _ = tokio::time::sleep(wait) => Err(format!("no answer within {} seconds", wait.as_secs())),
        _ = signal.cancelled() => Err("cancelled".to_string()),
    };
    if outcome.is_err() {
        lock(&WAITING).remove(id);
        let body = MailBody {
            ask: Some(AskInfo {
                id: id.to_string(),
                timeout_ms: None,
            }),
            ..MailBody::default()
        };
        let cancel = Envelope::mail_op(
            mesh::router().next_id(),
            me.as_str(),
            ROUTER,
            MailOp::CancelAsk,
            &body,
        );
        mesh::router().to_hub(&session_of(me), cancel);
    }
    outcome
}

/// The sessions of other engines `me`'s session may mail, from the hub;
/// none without a hub or an answer.
async fn peers(me: &Node) -> Vec<Value> {
    let session = session_of(&me.id);
    if !mesh::router().has_hub(&session) {
        return Vec::new();
    }
    let command = Envelope::command(
        mesh::router().next_id(),
        me.id.as_str(),
        &MeshCommand::Peers {},
    );
    let answer = mesh::router()
        .ask_hub(&session, command, PEERS_WAIT)
        .await
        .unwrap_or_default();
    answer["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|node| {
            json!({
                "id": node["id"], "title": node["title"], "kind": "session",
                "relation": node["relation"], "state": node["state"], "status": null,
                "agentType": null,
            })
        })
        .collect()
}

/// Put `text` in front of `to`'s model. Returns how it was delivered.
async fn deliver(
    ctx: &Ctx,
    from: &Node,
    to: &Node,
    text: String,
    ask: bool,
) -> Result<&'static str, AgentError> {
    match to.kind {
        NodeKind::Session => {
            let session = ctx.session()?;
            if to.id != ctx.session_address()? {
                return Err(AgentError::Tool(format!(
                    "{} runs in another engine",
                    to.id
                )));
            }
            let message = AgentMessage::from(evo_ai::types::UserMessage::new(text, now_ms()));
            let lane = session.lane("main");
            if session.has_active_run() {
                lane.steer(message)
                    .await
                    .map_err(|e| AgentError::Tool(e.to_string()))?;
                Ok("delivered into its current turn")
            } else {
                lane.next_run(message)
                    .await
                    .map_err(|e| AgentError::Tool(e.to_string()))?;
                mesh::router().request_wake_for(&to.id, &from.id, Some(&title_of(from)), "mail");
                Ok("queued for its next turn; it is being woken")
            }
        }
        NodeKind::Agent => crate::tools::agent::deliver(&to.id, text, ask)
            .await
            .map_err(AgentError::Tool),
        NodeKind::Side => Err(AgentError::Tool("side questions take no messages".into())),
    }
}

// ── Tools ────────────────────────────────────────────────────────────────────

fn schema(name: &str, description: &str, parameters: Value) -> Tool {
    Tool {
        name: name.into(),
        description: description.into(),
        parameters: Schema::new(parameters),
        constrained_sampling: None,
    }
}

/// The five tools for the session (`me: None`) or for the agent at `me`.
pub fn create(harness: HarnessCell, me: Option<Address>) -> Vec<Arc<dyn AgentTool>> {
    let ctx = Arc::new(Ctx { harness, me });
    vec![
        Arc::new(SendMessage { ctx: ctx.clone(), schema: schema(
            "send_message",
            "Send a message to another agent of this session: your parent (`to`: \"parent\"), an agent you started, or — for a subagent — another subagent of the same parent. It also reaches the sessions related to this one (the session that started it, the sessions it started, and sessions started by the same parent), by title or id. It does not wait for an answer (use ask for that). A running receiver reads it before its next step; an idle session is woken to read it; an idle agent reads it when it is continued. Find who you can reach with list_nodes.",
            json!({
                "type": "object",
                "properties": {
                    "to": { "type": "string", "description": "\"parent\", or a node's id, agent id or title (from list_nodes)." },
                    "message": { "type": "string", "description": "What to tell it." }
                },
                "required": ["to", "message"],
                "additionalProperties": false
            }),
        ) }),
        Arc::new(Ask { ctx: ctx.clone(), schema: schema(
            "ask",
            "Ask another agent of this session, or a related session (whom send_message reaches), a question and wait for its answer (it answers with the reply tool; an agent or session that ends its run without replying answers with its final message). Use it when you cannot go on without the answer; otherwise use send_message. An idle agent is continued to answer. Asking someone who is waiting on you is refused (it would never end), e.g. a subagent asking the session that waits for its report: put the question in your report instead.",
            json!({
                "type": "object",
                "properties": {
                    "to": { "type": "string", "description": "\"parent\", or a node's id, agent id or title (from list_nodes)." },
                    "question": { "type": "string", "description": "The question, with what the receiver needs to answer it." },
                    "timeoutSeconds": { "type": "integer", "minimum": 10, "maximum": MAX_ASK_SECS, "description": "How long to wait (default 600)." }
                },
                "required": ["to", "question"],
                "additionalProperties": false
            }),
        ) }),
        Arc::new(Reply { ctx: ctx.clone(), schema: schema(
            "reply",
            "Answer a question another agent asked you (a <mail kind=\"ask\"> message): it is waiting for this answer.",
            json!({
                "type": "object",
                "properties": {
                    "askId": { "type": "string", "description": "The question's id (the mail's id)." },
                    "answer": { "type": "string", "description": "Your answer." }
                },
                "required": ["askId", "answer"],
                "additionalProperties": false
            }),
        ) }),
        Arc::new(ListNodes { ctx: ctx.clone(), schema: schema(
            "list_nodes",
            "List the agents and sessions you can talk to (your parent, the agents you started, your sibling subagents, and the related sessions) with their id, title, kind and state (running, settled, …), and the questions waiting for your reply.",
            json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        ) }),
        Arc::new(CancelNode { ctx, schema: schema(
            "cancel_node",
            "Stop an agent you started (and anything it started). Its run ends as cancelled.",
            json!({
                "type": "object",
                "properties": { "node": { "type": "string", "description": "The agent's id, node id or title." } },
                "required": ["node"],
                "additionalProperties": false
            }),
        ) }),
    ]
}

fn string_arg(params: &Value, key: &str) -> Result<String, AgentError> {
    let value = params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if value.is_empty() || value.len() > MAX_TEXT {
        return Err(AgentError::Validation(format!(
            "`{key}` must contain 1-{MAX_TEXT} bytes"
        )));
    }
    Ok(value.to_string())
}

macro_rules! tool_basics {
    ($name:literal) => {
        fn schema(&self) -> &Tool {
            &self.schema
        }
        fn label(&self) -> &str {
            $name
        }
        fn execution_mode(&self) -> ToolExecutionMode {
            ToolExecutionMode::Parallel
        }
    };
}

struct SendMessage {
    ctx: Arc<Ctx>,
    schema: Tool,
}

#[async_trait]
impl AgentTool for SendMessage {
    tool_basics!("send_message");

    async fn execute(
        &self,
        _call: &str,
        params: Value,
        _signal: CancellationToken,
        _update: Arc<dyn Fn(ToolResultPartial) + Send + Sync>,
    ) -> Result<AgentToolResult, AgentError> {
        let me = node_of(&self.ctx.caller()?);
        let to = string_arg(&params, "to")?;
        let message = string_arg(&params, "message")?;
        let to = match target(&me, &to)? {
            Target::Local(node) => *node,
            Target::Remote { to, hint } => {
                let id = mesh::router().next_id();
                let body = remote_body(&me, &id, MailKind::Message, &message);
                let answer =
                    route(&me, &to, MailOp::Send, id.clone(), &body, hint.as_deref()).await?;
                return Ok(text_result(
                    format!(
                        "Sent to {} (a session of its own; it reads it before its next step, or is woken to read it).",
                        routed_title(&answer, &to)
                    ),
                    json!({ "mail": {
                        "id": id, "from": me.id, "to": answer["deliveredTo"], "delivery": "another session",
                    } }),
                ));
            }
        };
        mesh::router()
            .count_mail(&me.id, &to.id)
            .map_err(|e| AgentError::Tool(e.message))?;
        let id = mesh::router().next_id();
        let how = deliver(
            &self.ctx,
            &me,
            &to,
            render(&id, &me, false, &message),
            false,
        )
        .await?;
        Ok(text_result(
            format!("Sent to {} ({how}).", title_of(&to)),
            json!({ "mail": { "id": id, "from": me.id, "to": to.id, "delivery": how } }),
        ))
    }
}

struct Ask {
    ctx: Arc<Ctx>,
    schema: Tool,
}

#[async_trait]
impl AgentTool for Ask {
    tool_basics!("ask");

    async fn execute(
        &self,
        _call: &str,
        params: Value,
        signal: CancellationToken,
        _update: Arc<dyn Fn(ToolResultPartial) + Send + Sync>,
    ) -> Result<AgentToolResult, AgentError> {
        let me = node_of(&self.ctx.caller()?);
        let to = string_arg(&params, "to")?;
        let question = string_arg(&params, "question")?;
        let secs = params
            .get("timeoutSeconds")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_ASK_SECS)
            .clamp(10, MAX_ASK_SECS);
        let router = mesh::router();
        let to = match target(&me, &to)? {
            Target::Local(node) => *node,
            Target::Remote { to, hint } => {
                let id = router.next_id();
                // The local waits that end at the asker: the hub's graph
                // then sees cycles through this engine too (FR-8).
                let body = MailBody {
                    ask: Some(AskInfo {
                        id: id.clone(),
                        timeout_ms: Some(secs * 1000),
                    }),
                    waits: router.waits_ending_at(&me.id),
                    ..remote_body(&me, &id, MailKind::Ask, &question)
                };
                let (answer_tx, answer_rx) = oneshot::channel();
                lock(&WAITING).insert(id.clone(), answer_tx);
                let routed =
                    match route(&me, &to, MailOp::Ask, id.clone(), &body, hint.as_deref()).await {
                        Ok(routed) => routed,
                        Err(error) => {
                            lock(&WAITING).remove(&id);
                            return Err(error);
                        }
                    };
                let title = routed_title(&routed, &to);
                // This node waits on that one (a local `ask` to it sees it).
                let _wait = routed["deliveredTo"]
                    .as_str()
                    .and_then(|to| Address::parse(to).ok())
                    .map(|to| router.add_wait(&me.id, &to));
                let answer =
                    wait_remote(&me.id, &id, answer_rx, Duration::from_secs(secs), &signal)
                        .await
                        .map_err(|reason| {
                            AgentError::Tool(format!("{title} did not answer: {reason}"))
                        })?;
                return Ok(text_result(
                    format!("{title} answered:\n{answer}"),
                    json!({ "ask": { "id": id, "from": me.id, "to": routed["deliveredTo"] } }),
                ));
            }
        };
        // Waiting on someone who waits on us would never end.
        let _wait = router
            .try_wait(&me.id, &to.id)
            .map_err(|e| AgentError::Tool(e.message))?;
        router
            .count_mail(&me.id, &to.id)
            .map_err(|e| AgentError::Tool(e.message))?;
        let id = router.next_id();
        let (answer_tx, answer_rx) = oneshot::channel();
        lock(&ASKS).push(PendingAsk {
            id: id.clone(),
            from: me.id.clone(),
            to: to.id.clone(),
            answer: answer_tx,
        });
        let forget = |id: &str| lock(&ASKS).retain(|ask| ask.id != id);
        if let Err(error) =
            deliver(&self.ctx, &me, &to, render(&id, &me, true, &question), true).await
        {
            forget(&id);
            return Err(error);
        }
        let outcome = tokio::select! {
            answer = answer_rx => answer.map_err(|_| "the question was dropped".to_string()),
            _ = tokio::time::sleep(Duration::from_secs(secs)) => Err(format!("no answer within {secs} seconds")),
            _ = signal.cancelled() => Err("cancelled".to_string()),
        };
        forget(&id);
        let answer = outcome.map_err(|reason| {
            AgentError::Tool(format!("{} did not answer: {reason}", title_of(&to)))
        })?;
        Ok(text_result(
            format!("{} answered:\n{answer}", title_of(&to)),
            json!({ "ask": { "id": id, "from": me.id, "to": to.id } }),
        ))
    }
}

struct Reply {
    ctx: Arc<Ctx>,
    schema: Tool,
}

#[async_trait]
impl AgentTool for Reply {
    tool_basics!("reply");

    async fn execute(
        &self,
        _call: &str,
        params: Value,
        _signal: CancellationToken,
        _update: Arc<dyn Fn(ToolResultPartial) + Send + Sync>,
    ) -> Result<AgentToolResult, AgentError> {
        let me = self.ctx.caller()?;
        let id = string_arg(&params, "askId")?;
        let answer = string_arg(&params, "answer")?;
        let local = lock(&ASKS).iter().any(|ask| ask.id == id);
        if !local {
            return reply_remotely(&me, &id, &answer).await;
        }
        let ask = {
            let mut asks = lock(&ASKS);
            let Some(index) = asks.iter().position(|ask| ask.id == id) else {
                return Err(AgentError::Validation(format!(
                    "no open question `{id}` (it was answered, timed out or never asked)"
                )));
            };
            if asks[index].to != me {
                return Err(AgentError::Validation(format!(
                    "question `{id}` was not asked to you"
                )));
            }
            asks.remove(index)
        };
        let to = ask.from.clone();
        let _ = ask.answer.send(answer);
        Ok(text_result(
            format!("Answered {to}."),
            json!({ "reply": { "askId": id, "to": to } }),
        ))
    }
}

/// `me` answers ask `id` of another engine (FR-6): only the node it was
/// asked of, once; the answer goes back through the hub as `reply` mail.
async fn reply_remotely(
    me: &Address,
    id: &str,
    answer: &str,
) -> Result<AgentToolResult, AgentError> {
    let asked = {
        let mut asked = lock(&ASKED);
        match asked.get(id) {
            None => {
                return Err(AgentError::Validation(format!(
                    "no open question `{id}` (it was answered, timed out or never asked)"
                )))
            }
            Some(ask) if ask.to != *me => {
                return Err(AgentError::Validation(format!(
                    "question `{id}` was not asked to you"
                )))
            }
            Some(_) => asked.remove(id).expect("present"),
        }
    };
    let node = node_of(me);
    let body = reply_body(&node, id, answer);
    let to = asked.from.to_string();
    if let Err(error) = route(
        &node,
        &to,
        MailOp::Reply,
        mesh::router().next_id(),
        &body,
        None,
    )
    .await
    {
        // Not sent: it can be answered again.
        lock(&ASKED).insert(id.to_string(), asked);
        return Err(error);
    }
    Ok(text_result(
        format!("Answered {to}."),
        json!({ "reply": { "askId": id, "to": to } }),
    ))
}

struct ListNodes {
    ctx: Arc<Ctx>,
    schema: Tool,
}

#[async_trait]
impl AgentTool for ListNodes {
    tool_basics!("list_nodes");

    async fn execute(
        &self,
        _call: &str,
        _params: Value,
        _signal: CancellationToken,
        _update: Arc<dyn Fn(ToolResultPartial) + Send + Sync>,
    ) -> Result<AgentToolResult, AgentError> {
        let me = node_of(&self.ctx.caller()?);
        let mut nodes: Vec<Value> = reachable(&me)
            .into_iter()
            .map(|node| {
                let relation = if Some(&node.id) == me.parent.as_ref() {
                    "parent"
                } else if node.parent.as_ref() == Some(&me.id) {
                    "child"
                } else {
                    "sibling"
                };
                json!({
                    "id": node.id, "title": title_of(&node), "kind": node.kind, "relation": relation,
                    "state": node.state, "status": node.status, "agentType": node.meta.get("agentType"),
                })
            })
            .collect();
        // Sessions of other engines the hub says this session may mail (FR-6).
        nodes.extend(peers(&me).await);
        let listing = json!({ "you": me.id, "nodes": nodes, "questionsForYou": asks_to(&me.id) });
        Ok(text_result(listing.to_string(), listing))
    }
}

struct CancelNode {
    ctx: Arc<Ctx>,
    schema: Tool,
}

#[async_trait]
impl AgentTool for CancelNode {
    tool_basics!("cancel_node");

    async fn execute(
        &self,
        _call: &str,
        params: Value,
        _signal: CancellationToken,
        _update: Arc<dyn Fn(ToolResultPartial) + Send + Sync>,
    ) -> Result<AgentToolResult, AgentError> {
        let me = node_of(&self.ctx.caller()?);
        let target = resolve(&me, &string_arg(&params, "node")?)?;
        if target.parent.as_ref() != Some(&me.id) {
            return Err(AgentError::Validation(format!(
                "{} was not started by you",
                title_of(&target)
            )));
        }
        let cancelled = mesh::router()
            .cancel(&target.id, true)
            .map_err(|e| AgentError::Tool(e.message))?;
        let text = if cancelled.is_empty() {
            format!("{} was not running.", title_of(&target))
        } else {
            format!("Stopped {}.", title_of(&target))
        };
        Ok(text_result(text, json!({ "cancelled": cancelled })))
    }
}

// docs/features/session-roles-and-mail/spec.md, FR-6 – FR-8.
#[cfg(test)]
mod cross_engine_mail;
