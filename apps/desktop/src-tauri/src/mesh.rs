//! The session mesh hub (`crates/evo-mesh`, `docs/features/session-mesh-protocol/spec.md`).
//!
//! Every engine's stdout passes through [`on_line`]. The hub answers the
//! session tools (`start_session`, `message_session`, `list_sessions`), turns
//! a nested session's finished turn into a note for the chat that started it,
//! stores mail in the app database and writes it to the receiving engine,
//! whichever window shows that chat. A proposed session's card starts it with
//! [`mesh_start_session`] or dismisses it with [`mesh_dismiss_proposal`].
//!
//! When the hub needs an engine that runs nowhere — a session it just
//! created, a chat with mail waiting — the engine supervisor starts it, with
//! or without a window (`crate::supervisor`), and once it is set up tells the
//! hub, which sends what waits for it. An engine that crashes with mail
//! waiting is restarted within limits; at app start every chat with
//! undelivered mail is hosted (docs/features/shell-hosted-engines/spec.md,
//! FR-4).
//!
//! Mail between nodes of different engines (`send_message`, `ask`, `reply`
//! across engines, docs/features/session-roles-and-mail/spec.md, FR-6 – FR-8)
//! reaches the hub the same way, as mail ops on an engine's stdout; the hub
//! routes it and stores it here as session mail.

use std::path::{Path, PathBuf};

use evo_mesh::hub::{ChildInfo, CreatedSession, Effect, Hub, Mail, MailStore, SessionSpec};
use evo_mesh::wire::MailBody;
use evo_store::chats::{self, Chat, NewChat};
use evo_store::mail::{self, StoredMail};
use evo_store::rusqlite::Connection;
use evo_store::{transcripts, DbError};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::db::Db;

/// [`MailStore`] over the app database.
pub struct DbMailStore {
    app: AppHandle,
}

impl DbMailStore {
    fn db(&self) -> State<'_, Db> {
        self.app.state::<Db>()
    }
}

/// A new chat id, in the UUID v4 shape the UI uses.
pub(crate) fn new_chat_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut bytes = [0u8; 16];
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    for (index, chunk) in bytes.chunks_mut(8).enumerate() {
        // Each RandomState has its own random keys.
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u128(nanos);
        hasher.write_usize(index);
        chunk.copy_from_slice(&hasher.finish().to_le_bytes());
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32])
}

/// The folder a chat works in when it has no workspace: the home folder.
fn home() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

impl MailStore for DbMailStore {
    fn parent_of(&self, chat: &str) -> Option<String> {
        self.db().with(|conn| chats::get_chat(conn, chat)).ok().flatten()?.parent_chat_id
    }

    fn title_of(&self, chat: &str) -> Option<String> {
        self.db().with(|conn| chats::get_chat(conn, chat)).ok().flatten().map(|chat| chat.title)
    }

    fn insert(&self, mail: &Mail) -> Result<(), String> {
        let stored = StoredMail {
            id: mail.id.clone(),
            from_chat: mail.from_chat.clone(),
            to_chat: mail.to_chat.clone(),
            body: serde_json::to_string(&mail.body).map_err(|e| e.to_string())?,
            created_at: mail.created_at,
            delivered_at: None,
        };
        self.db().with(|conn| mail::insert_mail(conn, &stored))
    }

    fn pending(&self, chat: &str) -> Vec<Mail> {
        let stored = self.db().with(|conn| mail::pending_mail(conn, chat)).unwrap_or_default();
        stored
            .into_iter()
            .filter_map(|stored| {
                let body: MailBody = serde_json::from_str(&stored.body).ok()?;
                Some(Mail {
                    id: stored.id,
                    from_chat: stored.from_chat,
                    to_chat: stored.to_chat,
                    body,
                    created_at: stored.created_at,
                })
            })
            .collect()
    }

    fn mark_delivered(&self, id: &str) {
        let _ = self.db().with(|conn| mail::mark_mail_delivered(conn, id, chats::now_ms()));
    }

    fn children_of(&self, parent: &str) -> Vec<ChildInfo> {
        let chats = self.db().with(|conn| chats::list_chats(conn, false)).unwrap_or_default();
        chats
            .into_iter()
            .filter(|chat| chat.parent_chat_id.as_deref() == Some(parent))
            .map(|chat| ChildInfo { id: chat.id, title: chat.title, unread: chat.unread, preview: chat.preview })
            .collect()
    }

    fn create_session(&self, parent: &str, spec: &SessionSpec) -> Result<CreatedSession, String> {
        let db = self.db();
        let parent_row = db
            .with(|conn| chats::get_chat(conn, parent))?
            .ok_or_else(|| format!("no chat {parent}"))?;
        if spec.fork && parent_row.incognito {
            // An incognito chat leaves no copy of itself (`chats::fork_chat`).
            return Err("an incognito chat cannot be forked; start the session with context \"fresh\"".into());
        }
        let title = evo_mesh::hub::session_title(&spec.title);
        let id = new_chat_id();
        // Its own worktree of the parent's repository (fails outside a repository).
        let worktree = if spec.worktree {
            let base = match parent_row.workspace_id.as_deref() {
                Some(workspace) => db
                    .with(|conn| chats::get_workspace(conn, workspace))?
                    .map(|w| PathBuf::from(w.path))
                    .unwrap_or_else(home),
                None => home(),
            };
            let (path, cwd) = crate::worktrees::create_blocking(&db, &base, spec.branch.as_deref())?;
            Some((base, path, cwd))
        } else {
            None
        };
        let worktree_path = worktree.as_ref().map(|(_, path, _)| path.display().to_string());
        let row = db
            .with(|conn| session_row(conn, &db.transcripts, &parent_row, &id, &title, spec.fork, worktree_path.as_deref(), spec.role.as_ref()))
            .and_then(|chat| chat.ok_or_else(|| "the fork was not created".to_string()));
        let chat = match row {
            Ok(chat) => chat,
            Err(error) => {
                // No session was made: its worktree must not stay behind.
                if let Some((base, path, _)) = &worktree {
                    discard_worktree(&db, base, path);
                }
                return Err(error);
            }
        };
        db.log(evo_store::logs::NewLog::new("info", "mesh", format!("started nested session {id}")).chat(Some(parent)));
        // The windows' sidebars add it (session-store.ts listens).
        let _ = self.app.emit("chat://updated", json!({ "from": "shell", "chat": chat }));
        Ok(CreatedSession {
            id,
            title: chat.title,
            cwd: worktree.map(|(_, _, cwd)| cwd.display().to_string()),
        })
    }
}

/// The chat row of a nested session of `parent`: a copy of its conversation
/// (`fork`) or a fresh chat. Either way it is linked to the parent, has origin
/// `agent`, and follows the parent's folder, model and memory flags. A
/// session started with a role runs on the role's model and keeps the role
/// (docs/features/session-roles-and-mail/spec.md, FR-1, FR-2). `None`: the
/// parent cannot be copied (it is gone, or incognito).
fn session_row(
    conn: &mut Connection,
    transcripts_dir: &Path,
    parent: &Chat,
    id: &str,
    title: &str,
    fork: bool,
    worktree_path: Option<&str>,
    role: Option<&evo_mesh::wire::SessionRole>,
) -> Result<Option<Chat>, DbError> {
    let role_json = role.map(|role| serde_json::to_value(role).unwrap_or_default());
    // The role's model, when it names one; otherwise the parent's.
    let model = role.and_then(|role| Some((role.provider.clone()?, role.model.clone()?)));
    if !fork {
        let new = NewChat {
            id: id.to_string(),
            workspace_id: parent.workspace_id.clone(),
            title: title.to_string(),
            // The role's model, else the one the parent runs on.
            provider: model.as_ref().map(|(p, _)| p.clone()).or_else(|| parent.provider.clone()),
            model: model.as_ref().map(|(_, m)| m.clone()).or_else(|| parent.model.clone()),
            role: role_json,
            parent_chat_id: Some(parent.id.clone()),
            origin: Some("agent".into()),
            worktree_path: worktree_path.map(str::to_string),
            // A nested chat follows its parent: no memory, no history.
            memory_off: crate::chat_flags::ChatFlags::of(parent).memory_off,
            incognito: parent.incognito,
            ..Default::default()
        };
        return chats::create_chat(conn, &new).map(Some);
    }
    // The parent's engine is running: its transcript is copied as far as it
    // has been written. The copy has the parent's folder, model and memory
    // flag (`chats::fork_chat`); what makes it a nested session is added here.
    if chats::fork_chat(conn, transcripts_dir, &parent.id, id, title)?.is_none() {
        return Ok(None);
    }
    let nested = chats::set_parent(conn, id, Some(&parent.id))
        .and_then(|_| chats::set_origin(conn, id, "agent"))
        .and_then(|_| chats::set_worktree(conn, id, worktree_path))
        .and_then(|chat| match &role_json {
            Some(role) => chats::set_role(conn, id, Some(role)),
            None => Ok(chat),
        })
        .and_then(|chat| match &model {
            Some((provider, model)) => chats::update_chat(
                conn,
                id,
                &chats::ChatPatch { provider: Some(provider.clone()), model: Some(model.clone()), ..Default::default() },
            ),
            None => Ok(chat),
        });
    if !matches!(nested, Ok(Some(_))) {
        // Half made: no copy stays that is not a nested session.
        let _ = chats::delete_chat(conn, id);
        let _ = transcripts::path_in(transcripts_dir, id).and_then(|path| transcripts::remove(&path));
    }
    nested
}

/// Remove the worktree at `path` (of `base`'s repository), made for a session
/// that was not created after all.
fn discard_worktree(db: &Db, base: &Path, path: &Path) {
    // The same root as `worktrees.rs` creates them under.
    let root = db.path.parent().map(|dir| dir.join("worktrees")).unwrap_or_else(|| PathBuf::from("worktrees"));
    if let Err(error) = evo_worktrees::Worktrees::with_data_root(root).remove(base, path) {
        db.log(evo_store::logs::NewLog::new("warn", "mesh", format!("worktree {} not removed: {error}", path.display())));
    }
}

/// The hub, managed as Tauri state.
pub struct MeshHub(pub Hub<DbMailStore>);

impl MeshHub {
    pub fn new(app: AppHandle) -> Self {
        let hub = Hub::new(DbMailStore { app: app.clone() });
        // Settings → Sandbox, before any window is open (session-roles-and-mail
        // FR-5; shell-hosted-engines FR-8): later changes come through
        // `supervisor::settings::on_setting`.
        if let Some(db) = app.try_state::<crate::db::Db>() {
            hub.set_max_children(crate::engine::nested_session_limit(&db));
            hub.set_auto_start(crate::supervisor::settings::auto_start_sessions(&db));
        }
        MeshHub(hub)
    }
}

/// Whether the hub's answer to `line` reads or writes the database at length:
/// a session tool (it may create a chat and a worktree), or mail for a node
/// of another engine and the `peers` command, which read the chats and store
/// mail (docs/features/session-roles-and-mail/spec.md, FR-6, FR-7).
fn needs_the_database(line: &Value) -> bool {
    match line.get("type").and_then(Value::as_str) {
        Some("external_tool_request") => true,
        Some("mesh") => match (line.get("kind").and_then(Value::as_str), line.get("op").and_then(Value::as_str)) {
            (Some("mail"), Some(op)) => op != "message",
            (Some("cmd"), Some(op)) => op == "peers",
            _ => false,
        },
        _ => false,
    }
}

/// One stdout line of `chat`'s engine: let the hub react, then carry out
/// what it asks (writes to engines, events to the windows).
pub async fn on_line(app: &AppHandle, chat: &str, line: &Value) {
    let effects = if needs_the_database(line) {
        // Off the reader.
        let (app2, chat2, line2) = (app.clone(), chat.to_string(), line.clone());
        tauri::async_runtime::spawn_blocking(move || app2.state::<MeshHub>().0.on_line(&chat2, &line2))
            .await
            .unwrap_or_default()
    } else {
        app.state::<MeshHub>().0.on_line(chat, line)
    };
    apply(app, effects).await;
}

/// `chat`'s engine exited (`crashed`) or was stopped on purpose. Mail that
/// waits for it restarts a crashed engine, within the supervisor's limits;
/// a stopped one keeps its mail until new mail comes or a window opens the
/// chat (docs/features/shell-hosted-engines/spec.md, FR-4).
pub async fn on_exit(app: &AppHandle, chat: &str, crashed: bool) {
    let (wants_host, effects) = exit_effects(app.state::<MeshHub>().0.on_exit(chat));
    apply(app, effects).await;
    if wants_host && crashed {
        crate::supervisor::restart_for_mail(app, chat.to_string());
    }
}

/// The hub's answer to an engine's exit: whether it asks for a host (mail
/// waits for the chat), and the other effects.
fn exit_effects(effects: Vec<Effect>) -> (bool, Vec<Effect>) {
    let (hosts, rest): (Vec<Effect>, Vec<Effect>) = effects.into_iter().partition(|effect| matches!(effect, Effect::Host { .. }));
    (!hosts.is_empty(), rest)
}

pub(crate) async fn apply(app: &AppHandle, effects: Vec<Effect>) {
    for effect in effects {
        match effect {
            Effect::Send { chat, line } => {
                let engines = app.state::<crate::engine::EngineState>();
                if let Err(error) = engines.write(&chat, &line).await {
                    // It stays pending: the next `ready` of that engine sends it.
                    app.state::<Db>().log(
                        evo_store::logs::NewLog::new("warn", "mesh", format!("mail not sent: {error}"))
                            .chat(Some(&chat)),
                    );
                    app.state::<MeshHub>().0.on_start(&chat);
                }
            }
            // The supervisor starts and sets it up, in the effect's folder
            // when it names one (a new session's worktree), FR-4.
            Effect::Host { chat, cwd, restore } => crate::supervisor::host_for_mail(app, chat, cwd, restore),
            Effect::Prompted { chat, text } => {
                let _ = app.emit("mesh://prompted", json!({ "chat": chat, "text": text }));
            }
        }
    }
}

/// Undelivered mail per chat, for the UI.
#[tauri::command]
pub fn mesh_pending(db: State<'_, Db>, chat: String) -> Result<usize, String> {
    db.with(|conn| mail::pending_mail(conn, &chat)).map(|mail| mail.len())
}

/// The session tools' definitions, for a top-level chat's external tools.
#[tauri::command]
pub fn mesh_session_tools() -> Value {
    evo_mesh::hub::session_tools()
}

/// Start a nested session of `parent` (a proposal card's Start): returns its
/// id. With `proposal_id` the proposal starts once: a second call returns the
/// same session, and the parent's agent gets a queued note with the id.
#[tauri::command]
pub async fn mesh_start_session(
    app: AppHandle,
    parent: String,
    spec: SessionSpec,
    proposal_id: Option<String>,
) -> Result<String, String> {
    let app2 = app.clone();
    let (session, effects) = tauri::async_runtime::spawn_blocking(move || {
        let hub = &app2.state::<MeshHub>().0;
        match proposal_id.as_deref() {
            Some(proposal) => hub.start_proposed(&parent, &spec, proposal),
            None => hub.start_session(&parent, &spec).map(|(created, effects)| (created.id, effects)),
        }
    })
    .await
    .map_err(|e| e.to_string())??;
    apply(&app, effects).await;
    Ok(session)
}

/// A proposal card's Dismiss: the proposal can no longer be started, and the
/// parent's agent gets a queued note that the person dismissed it.
#[tauri::command]
pub async fn mesh_dismiss_proposal(app: AppHandle, parent: String, proposal_id: String) -> Result<(), String> {
    let app2 = app.clone();
    let effects =
        tauri::async_runtime::spawn_blocking(move || app2.state::<MeshHub>().0.dismiss_proposal(&parent, &proposal_id))
            .await
            .map_err(|e| e.to_string())??;
    apply(&app, effects).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A scratch folder with an app database and a transcripts folder, removed on drop.
    struct Fixture {
        root: PathBuf,
        conn: Connection,
        dir: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let root = std::env::temp_dir().join(format!("evo-mesh-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            std::fs::create_dir_all(&root).unwrap();
            let conn = evo_store::open_at(&root.join("evo.db")).unwrap();
            let dir = root.join("transcripts");
            Self { root, conn, dir }
        }

        /// A parent chat on a model the person picked.
        fn parent(&self, new: NewChat) -> Chat {
            let new = NewChat { id: "p".into(), title: "Parent".into(), provider: Some("acme".into()), model: Some("acme-large".into()), ..new };
            chats::create_chat(&self.conn, &new).unwrap()
        }

        fn child(&mut self, parent: &Chat, fork: bool, worktree: Option<&str>) -> Option<Chat> {
            let dir = self.dir.clone();
            session_row(&mut self.conn, &dir, parent, "c", "Child", fork, worktree, None).unwrap()
        }

        fn child_as(&mut self, parent: &Chat, fork: bool, role: &evo_mesh::wire::SessionRole) -> Chat {
            let dir = self.dir.clone();
            session_row(&mut self.conn, &dir, parent, "c", "Child", fork, None, Some(role)).unwrap().unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn a_fresh_session_follows_its_parents_model_and_flags() {
        let mut fx = Fixture::new();
        let parent = fx.parent(NewChat { memory_off: true, ..Default::default() });
        let child = fx.child(&parent, false, Some("/wt/c")).unwrap();
        assert_eq!((child.provider.as_deref(), child.model.as_deref()), (Some("acme"), Some("acme-large")));
        assert_eq!((child.parent_chat_id.as_deref(), child.origin.as_str()), (Some("p"), "agent"));
        assert_eq!((child.memory_off, child.incognito), (true, false));
        assert_eq!(child.worktree_path.as_deref(), Some("/wt/c"));
    }

    // docs/features/session-roles-and-mail/spec.md (FR-1, FR-2, NFR-4).
    #[test]
    fn a_session_with_a_role_runs_on_its_model_and_keeps_it() {
        let role = evo_mesh::wire::SessionRole {
            name: Some("reviewer".into()),
            access: "read-only".into(),
            provider: Some("acme".into()),
            model: Some("acme-small".into()),
            thinking: Some("low".into()),
            ..Default::default()
        };
        for fork in [false, true] {
            let mut fx = Fixture::new();
            let parent = fx.parent(NewChat::default());
            let child = fx.child_as(&parent, fork, &role);
            assert_eq!((child.provider.as_deref(), child.model.as_deref()), (Some("acme"), Some("acme-small")), "fork: {fork}");
            let stored: evo_mesh::wire::SessionRole = serde_json::from_value(child.role.clone().unwrap()).unwrap();
            assert_eq!(stored, role, "fork: {fork}");
            assert_eq!(child.parent_chat_id.as_deref(), Some("p"));
        }
        // A role that names no model keeps the parent's.
        let mut fx = Fixture::new();
        let parent = fx.parent(NewChat::default());
        let typed = fx.child_as(&parent, false, &evo_mesh::wire::SessionRole { name: Some("r".into()), ..Default::default() });
        assert_eq!(typed.model.as_deref(), Some("acme-large"));
    }

    // docs/features/session-roles-and-mail/spec.md (FR-6, FR-7).
    #[test]
    fn mail_across_engines_is_routed_off_the_reader_and_kept_with_its_nodes() {
        let line = |kind: &str, op: &str| json!({ "type": "mesh", "kind": kind, "op": op });
        assert!(needs_the_database(&json!({ "type": "external_tool_request" })));
        for op in ["send", "ask", "reply", "cancel_ask"] {
            assert!(needs_the_database(&line("mail", op)), "{op}");
        }
        assert!(needs_the_database(&line("cmd", "peers")));
        for (kind, op) in [("mail", "message"), ("evt", "node_report"), ("res", "ask"), ("cmd", "list")] {
            assert!(!needs_the_database(&line(kind, op)), "{kind} {op}");
        }
        assert!(!needs_the_database(&json!({ "type": "agent_start" })));

        // The stored body keeps what the hub routes by: kind, ask and the nodes.
        let fx = Fixture::new();
        chats::create_chat(&fx.conn, &NewChat { id: "b".into(), title: "B".into(), ..Default::default() }).unwrap();
        let body = MailBody {
            kind: Some(evo_mesh::wire::MailKind::Ask),
            ask: Some(evo_mesh::wire::AskInfo { id: "ask_1".into(), timeout_ms: Some(600_000) }),
            from_node: Some(evo_mesh::wire::Address::parse("s_a/a_3").unwrap()),
            ..MailBody::new("Which port?", evo_mesh::wire::Deliver::Interrupt)
        };
        let stored = StoredMail {
            id: "mail_1".into(),
            from_chat: "a".into(),
            to_chat: "b".into(),
            body: serde_json::to_string(&body).unwrap(),
            created_at: 1,
            delivered_at: None,
        };
        mail::insert_mail(&fx.conn, &stored).unwrap();
        let pending = mail::pending_mail(&fx.conn, "b").unwrap();
        let back: MailBody = serde_json::from_str(&pending[0].body).unwrap();
        assert_eq!(back, body);
        let mail = Mail { id: "mail_1".into(), from_chat: "a".into(), to_chat: "b".into(), body: back, created_at: 1 };
        assert_eq!((mail.envelope().from.as_str(), mail.envelope().to.as_str()), ("s_a/a_3", "s_b"));
    }

    // docs/features/shell-hosted-engines/spec.md, FR-4 (T5).
    #[test]
    fn an_exit_with_mail_waiting_asks_for_a_host_and_keeps_the_other_effects() {
        let host = Effect::Host { chat: "c".into(), cwd: None, restore: true };
        let send = Effect::Send { chat: "p".into(), line: json!({ "type": "prompt" }) };
        assert_eq!(exit_effects(vec![host.clone(), send.clone()]), (true, vec![send.clone()]));
        assert_eq!(exit_effects(vec![send.clone()]), (false, vec![send]));
        assert_eq!(exit_effects(Vec::new()), (false, Vec::new()));
    }

    #[test]
    fn a_forked_session_is_a_nested_session_on_its_parents_model() {
        let mut fx = Fixture::new();
        let parent = fx.parent(NewChat { memory_off: true, ..Default::default() });
        let child = fx.child(&parent, true, Some("/wt/c")).unwrap();
        assert_eq!((child.provider.as_deref(), child.model.as_deref()), (Some("acme"), Some("acme-large")));
        assert_eq!((child.parent_chat_id.as_deref(), child.origin.as_str()), (Some("p"), "agent"));
        assert_eq!((child.title.as_str(), child.memory_off), ("Child", true));
        assert_eq!(child.worktree_path.as_deref(), Some("/wt/c"));
    }

    #[test]
    fn an_incognito_parent_has_incognito_sessions_and_no_copy() {
        let mut fx = Fixture::new();
        let parent = fx.parent(NewChat { incognito: true, ..Default::default() });
        // No copy of an incognito chat: the caller removes the worktree it made.
        assert_eq!(fx.child(&parent, true, Some("/wt/c")), None);
        assert_eq!(chats::get_chat(&fx.conn, "c").unwrap(), None);
        let child = fx.child(&parent, false, None).unwrap();
        assert_eq!((child.incognito, child.memory_off, child.origin.as_str()), (true, true, "agent"));
    }
}
