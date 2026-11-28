//! A chat's or mate's call of a Connected apps tool
//! (docs/features/connected-apps/spec.md, FR-7 – FR-15; plan 3.5).
//!
//! `agent_tools::mcp` hands every call of the `connected_apps` connector
//! here instead of its own ask. By meta-tool:
//!
//! * search and schemas pass (a limited mate's search results lose what it
//!   may not use); the tools they describe are remembered for the gate;
//! * `MULTI_EXECUTE` goes through [`gate::prepare`]: checks, the approval
//!   card the class and the caller's settings call for, uploads, then the
//!   call; a missing-permission refusal gets its note (FR-15);
//! * `MANAGE_CONNECTIONS` becomes Evo's Connect card;
//! * the sandbox meta-tools are never a limited mate's, and otherwise follow
//!   their permission (blocked unless the person allowed them).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use evo_mcp::Permission;
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Emitter, Manager};

use super::accounts::{Account, Status};
use super::files::{LocalFile, Places};
use super::gate::{self, Approval, Apps, Checked, Class, Meta, Need, Refusal, Stop};
use crate::agent_tools::mcp::{shape_result, ChatTool};
use crate::agent_tools::{BoxFuture, Content, ToolCtx, ToolOutput};
use crate::mates::grants::{self, Pick, Plan};

// ── What Evo knows of the app tools ─────────────────────────────────────────

fn known() -> &'static Mutex<HashMap<String, Value>> {
    static KNOWN: std::sync::OnceLock<Mutex<HashMap<String, Value>>> = std::sync::OnceLock::new();
    KNOWN.get_or_init(Default::default)
}

/// The JSON of a tool result's text parts (Composio answers in JSON text).
pub fn json_parts(result: &Value) -> Vec<Value> {
    result
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|p| p.get("text").and_then(Value::as_str))
        .filter_map(|t| serde_json::from_str(t).ok())
        .chain(result.get("structuredContent").cloned())
        .collect()
}

/// Every app tool a result describes (an object with a slug and a schema).
pub fn tools_in(value: &Value, out: &mut HashMap<String, Value>) {
    match value {
        Value::Object(map) => {
            let slug = ["tool_slug", "slug", "name"]
                .iter()
                .find_map(|k| map.get(*k).and_then(Value::as_str));
            let schema = [
                "input_parameters",
                "input_schema",
                "inputSchema",
                "parameters",
            ]
            .iter()
            .find_map(|k| map.get(*k).filter(|s| s.is_object()));
            if let (Some(slug), Some(schema)) = (slug, schema) {
                if slug.contains('_') && slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                {
                    let mut tool = Value::Object(map.clone());
                    tool["input_parameters"] = schema.clone();
                    out.insert(slug.to_ascii_uppercase(), tool);
                }
            }
            // `{ "GMAIL_SEND_EMAIL": { schema } }`
            for (key, item) in map {
                if key.contains('_')
                    && key
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                    && item.get("properties").is_some()
                {
                    out.entry(key.clone())
                        .or_insert_with(|| json!({ "slug": key, "input_parameters": item }));
                }
                tools_in(item, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|i| tools_in(i, out)),
        _ => {}
    }
}

/// Remember the tools a search or schema result describes.
pub fn remember(result: &Value) {
    let mut found = HashMap::new();
    for part in json_parts(result) {
        tools_in(&part, &mut found);
    }
    if !found.is_empty() {
        known().lock().expect("known tools").extend(found);
    }
}

/// Ask Composio for the schemas of `slugs` Evo has not seen.
async fn fetch_schemas(app: &AppHandle, slugs: &[String]) {
    let missing: Vec<String> = {
        let known = known().lock().expect("known tools");
        slugs
            .iter()
            .filter(|s| !known.contains_key(*s))
            .cloned()
            .collect()
    };
    if missing.is_empty() {
        return;
    }
    let Some(mcp) = app.try_state::<crate::mcp::McpState>() else {
        return;
    };
    let pool = mcp.0.clone();
    let mut args = Map::new();
    args.insert("tool_slugs".into(), json!(missing));
    if let Ok(result) = pool
        .call_server(gate::KEY, gate::GET_TOOL_SCHEMAS, Some(args))
        .await
    {
        remember(&result);
    }
}

// ── Who calls ───────────────────────────────────────────────────────────────

fn mate_of(app: &AppHandle, chat: &str) -> Option<evo_store::mates::Mate> {
    let db = app.try_state::<crate::db::Db>()?;
    db.with(|conn| evo_store::mates::mate_of_chat(conn, chat))
        .ok()
        .flatten()
}

/// Whether a project switched connected apps off for its chats: its
/// `.evo/settings.json` has `"connectedApps": false` (FR-12). Switching a
/// capability off needs no project trust.
pub fn project_off(folder: &Path) -> bool {
    for dir in folder.ancestors() {
        let file = dir.join(".evo").join("settings.json");
        if let Ok(text) = std::fs::read_to_string(&file) {
            if let Ok(value) = serde_json::from_str::<Value>(&text) {
                if value.get("connectedApps") == Some(&Value::Bool(false)) {
                    return true;
                }
            }
        }
        if dir.join(".git").exists() {
            break;
        }
    }
    false
}

/// Whether a connector tool is offered: anything but connected apps is;
/// a connected-apps tool is not in a project that switched them off, and
/// its sandbox never reaches a mate with limited apps (plan 3.7).
pub fn offered_to(tool: &ChatTool, limited_mate: bool, project_off: bool) -> bool {
    if tool.server != gate::KEY {
        return true;
    }
    !project_off && !(limited_mate && gate::meta(&tool.tool) == Meta::Sandbox)
}

/// [`offered_to`] for `chat` working in `folder`.
pub fn offered(app: &AppHandle, chat: &str, folder: &Path, tool: &ChatTool) -> bool {
    if tool.server != gate::KEY {
        return true;
    }
    let limited = mate_of(app, chat).is_some_and(|m| Apps::of_mate(m.apps.as_ref()).limited());
    offered_to(tool, limited, project_off(folder))
}

/// The folders files may come from, and Evo's own data.
fn places(ctx: &ToolCtx, mate: Option<&evo_store::mates::Mate>) -> Places {
    let mut roots = vec![ctx.folder()];
    if let Ok(cache) = ctx.app.path().app_cache_dir() {
        roots.push(cache.join("pastes"));
    }
    let agent = crate::mcp::agent_dir();
    let mut evo_data = vec![agent.clone()];
    if let Some(parent) = agent
        .parent()
        .filter(|p| p.file_name().is_some_and(|n| n == ".evo"))
    {
        evo_data.push(parent.to_path_buf());
    }
    if let Ok(data) = ctx.app.path().app_data_dir() {
        evo_data.push(data);
    }
    let mate_dir = mate.map(|m| crate::mates::mate_dir(&m.id));
    if let Some(dir) = &mate_dir {
        roots.push(dir.clone());
    }
    Places {
        roots,
        evo_data,
        mate_dir,
    }
}

// ── Approval ────────────────────────────────────────────────────────────────

/// The exposed name a grant is kept under for an app tool.
pub fn grant_name(slug: &str) -> String {
    evo_mcp::pool::exposed_name(gate::LABEL, slug)
}

/// A mate's settings, for [`decide`].
pub struct MateSide<'a> {
    pub sending: &'a str,
    pub granted: &'a dyn Fn(&str) -> bool,
    pub here: &'a dyn Fn(&str) -> bool,
}

/// What the calls need before they run (FR-13): reading tools follow their
/// permission (default Allow) and may be always allowed; a sending tool
/// asks with Allow once / Deny — for a mate under its Sending on your
/// behalf setting, in a Harness chat always — and is never always allowed.
pub fn decide(
    checked: &[Checked],
    permission: &dyn Fn(&str) -> Option<Permission>,
    mate: Option<MateSide>,
) -> Plan {
    let read_only = checked.iter().all(|c| c.class == Class::Read);
    let asks = checked.iter().any(|c| {
        let default = if c.class == Class::Read {
            Permission::Allow
        } else {
            Permission::Ask
        };
        permission(&c.call.slug).unwrap_or(default) == Permission::Ask
    });
    match mate {
        Some(m) => {
            let for_mate = checked
                .iter()
                .all(|c| (m.granted)(&grant_name(&c.call.slug)));
            let here = checked.iter().all(|c| (m.here)(&grant_name(&c.call.slug)));
            grants::plan(read_only, asks, m.sending, for_mate, here)
        }
        None if !read_only => Plan::Ask {
            picks: vec![Pick::Once, Pick::Deny],
            sending: true,
        },
        None if asks => Plan::Ask {
            picks: vec![Pick::Once, Pick::Mate, Pick::Deny],
            sending: false,
        },
        None => Plan::Run,
    }
}

fn pick_label(pick: Pick, mate: Option<&str>) -> String {
    match (pick, mate) {
        (Pick::Mate, None) => "Always allow".into(),
        (pick, Some(name)) => pick.label(name),
        (pick, None) => pick.label(""),
    }
}

// ── The backend: the card and uploads ───────────────────────────────────────

struct CtxBackend {
    ctx: ToolCtx,
}

fn mimetype(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "txt" | "md" | "log" => "text/plain",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "json" => "application/json",
        "zip" => "application/zip",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        _ => "application/octet-stream",
    }
}

impl gate::Backend for CtxBackend {
    fn ask(&self, title: String, body: String, options: Vec<String>) -> BoxFuture<i32> {
        let ctx = self.ctx.clone();
        Box::pin(async move {
            let options: Vec<&str> = options.iter().map(String::as_str).collect();
            ctx.ask(&title, &body, &options).await
        })
    }

    fn upload(
        &self,
        toolkit: String,
        slug: String,
        file: LocalFile,
    ) -> BoxFuture<Result<Value, String>> {
        let app = self.ctx.app.clone();
        Box::pin(async move {
            let client = super::app::state(&app)
                .and_then(|s| s.service.client())
                .ok_or("No Composio key is saved.")?;
            let path: PathBuf = file.path.clone();
            let bytes = tokio::task::spawn_blocking(move || std::fs::read(path))
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?;
            let md5 = {
                use md5::Digest;
                md5::Md5::digest(&bytes)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            };
            let mime = mimetype(&file.name);
            let answer = client
                .upload_request(&toolkit, &slug, &file.name, mime, &md5)
                .await
                .map_err(|e| e.to_string())?;
            let key = ["/key", "/s3key", "/data/key"]
                .iter()
                .find_map(|p| answer.pointer(p).and_then(Value::as_str))
                .ok_or("Composio gave no place for the file")?
                .to_string();
            let url = [
                "/new_presigned_url",
                "/newPresignedUrl",
                "/presigned_url",
                "/url",
            ]
            .iter()
            .find_map(|p| answer.pointer(p).and_then(Value::as_str));
            let existing = answer.get("type").and_then(Value::as_str) == Some("existing");
            if !existing {
                client
                    .put_file(url.ok_or("Composio gave no upload address")?, bytes, mime)
                    .await?;
            }
            Ok(json!({ "name": file.name, "mimetype": mime, "s3key": key }))
        })
    }
}

// ── Running a call ──────────────────────────────────────────────────────────

async fn pool_call(ctx: &ToolCtx, name: &str, args: Value) -> Result<Value, String> {
    let pool = ctx
        .app
        .try_state::<crate::mcp::McpState>()
        .map(|s| s.0.clone())
        .ok_or("Connected apps is not available.")?;
    let arguments: Map<String, Value> = args.as_object().cloned().unwrap_or_default();
    let folder = ctx.folder();
    tokio::select! {
        result = pool.call(name, Some(arguments), Some(folder.as_path())) => result,
        _ = ctx.cancel.cancelled() => Err(String::new()),
    }
}

/// A call of `tool` (a tool of the `connected_apps` connector) by the chat
/// or mate of `ctx`.
pub async fn call(ctx: ToolCtx, tool: &ChatTool, name: &str, args: Value) -> ToolOutput {
    let mate = mate_of(&ctx.app, &ctx.chat);
    let apps = Apps::of_mate(mate.as_ref().and_then(|m| m.apps.as_ref()));
    match gate::meta(&tool.tool) {
        Meta::Unknown => ToolOutput::error(format!("{} is not a Connected apps tool Evo can check, so it was not run.", tool.tool)),
        Meta::Sandbox if apps.limited() => ToolOutput::error("This tool can reach any connected app, so it is not available to a mate limited to some apps."),
        Meta::Sandbox => {
            if tool.permission == Permission::Ask {
                let answer = ctx.ask(&format!("Allow Connected apps to run {}?", tool.tool), "It runs code or raw requests at Composio with your connected accounts.", &["Allow once", "Deny"]).await;
                if answer != 0 {
                    return if answer < 0 { ToolOutput::text("") } else { ToolOutput::error("The user declined this tool call.") };
                }
            }
            finish(pool_call(&ctx, name, args).await)
        }
        Meta::Pass => {
            let result = pool_call(&ctx, name, args).await;
            if let Ok(result) = &result {
                remember(result);
            }
            finish(result)
        }
        Meta::Search => {
            let result = pool_call(&ctx, name, args).await;
            match result {
                Ok(result) => {
                    remember(&result);
                    let known: Vec<String> = accounts(&ctx.app).iter().map(|a| a.toolkit.clone()).collect();
                    finish(Ok(filter_result(&result, &apps, &known)))
                }
                Err(e) => finish(Err(e)),
            }
        }
        Meta::Connect => {
            let toolkit = args.get("toolkits").and_then(Value::as_array).and_then(|t| t.first()).or_else(|| args.get("toolkit")).and_then(Value::as_str).unwrap_or("").to_ascii_lowercase();
            if toolkit.is_empty() {
                return ToolOutput::error("Name the app to connect in `toolkits`.");
            }
            connect_card(&ctx, mate.as_ref(), &toolkit, "", None).await
        }
        Meta::Execute => execute(ctx, mate, apps, name, args).await,
    }
}

fn accounts(app: &AppHandle) -> Vec<Account> {
    super::app::state(app)
        .map(|s| s.service.accounts_now())
        .unwrap_or_default()
}

/// A result whose JSON text loses the tools a limited caller may not use.
fn filter_result(result: &Value, apps: &Apps, known: &[String]) -> Value {
    if !apps.limited() {
        return result.clone();
    }
    let mut out = result.clone();
    if let Some(parts) = out.get_mut("content").and_then(Value::as_array_mut) {
        for part in parts {
            if let Some(text) = part.get("text").and_then(Value::as_str) {
                if let Ok(value) = serde_json::from_str::<Value>(text) {
                    part["text"] = json!(gate::filter_search(&value, apps, known).to_string());
                }
            }
        }
    }
    if let Some(structured) = out.get("structuredContent").cloned() {
        out["structuredContent"] = gate::filter_search(&structured, apps, known);
    }
    out
}

fn finish(result: Result<Value, String>) -> ToolOutput {
    match result {
        Ok(result) => shape_result(&result),
        Err(e) if e.is_empty() => ToolOutput::text(""),
        Err(e) => ToolOutput::error(e),
    }
}

async fn execute(
    ctx: ToolCtx,
    mate: Option<evo_store::mates::Mate>,
    apps: Apps,
    name: &str,
    args: Value,
) -> ToolOutput {
    let Some(state) = super::app::state(&ctx.app) else {
        return ToolOutput::error("Connected apps is not available.");
    };
    let service = state.service.clone();
    if service.accounts_now().is_empty() {
        let _ = service.refresh_accounts().await;
    }
    let accounts = service.accounts_now();
    // Learn the schemas of tools the agent did not search for.
    let slugs: Vec<String> = args
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|t| t.get("tool_slug").and_then(Value::as_str))
        .map(str::to_ascii_uppercase)
        .collect();
    fetch_schemas(&ctx.app, &slugs).await;
    let tools = known().lock().expect("known tools").clone();
    let names = super::app::names(&ctx.app).await;
    let places = places(&ctx, mate.as_ref());
    let gctx = gate::Context {
        accounts: &accounts,
        apps: &apps,
        tools: &tools,
        places: &places,
        names: &names,
    };
    let pool = ctx
        .app
        .try_state::<crate::mcp::McpState>()
        .map(|s| s.0.clone());
    let permission = |slug: &str| {
        pool.as_ref()
            .and_then(|p| p.app_permission(gate::LABEL, slug))
    };
    let db = ctx.app.try_state::<crate::db::Db>();
    let granted_list: Vec<String> = match (&mate, &db) {
        (Some(m), Some(db)) => db
            .with(|conn| evo_store::mates::list_grants(conn, &m.id))
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let granted = |n: &str| granted_list.iter().any(|g| g == n);
    let chat = ctx.chat.clone();
    let here = move |n: &str| grants::allowed_here(&chat, n);
    // A heartbeat turn sends nothing unasked (evo-mates FR-68).
    let sending = match &mate {
        Some(_) if crate::mates::initiative::in_heartbeat(&ctx.chat) => "ask".to_string(),
        Some(m) => m.sending.clone(),
        None => String::new(),
    };
    let mut picks = Vec::new();
    let mate_name = mate.as_ref().map(|m| m.name.clone());
    let approval = |checked: &[Checked]| {
        let side = mate.as_ref().map(|_| MateSide {
            sending: &sending,
            granted: &granted,
            here: &here,
        });
        match decide(checked, &permission, side) {
            Plan::Run => Approval::Run,
            Plan::Ask { picks: p, .. } => {
                picks = p;
                Approval::Ask(
                    picks
                        .iter()
                        .map(|p| pick_label(*p, mate_name.as_deref()))
                        .collect(),
                )
            }
        }
    };
    let backend = CtxBackend { ctx: ctx.clone() };
    let mut chosen = None;
    let run = |_: &[Checked], answer: i32| {
        chosen = Some(answer);
        true
    };
    // `run` cannot see `picks` (filled by `approval`): the answer is read below.
    let prepared = gate::prepare(&args, &gctx, approval, run, mate_name.as_deref(), &backend).await;
    let (forward, checked) = match prepared {
        Ok(ok) => ok,
        Err(Stop::Declined(None)) => return ToolOutput::text(""),
        Err(Stop::Declined(Some(text))) | Err(Stop::Refused(text)) => {
            return refusal_or_card(&ctx, mate.as_ref(), &gate::check(&args, &gctx).err(), text)
                .await;
        }
    };
    if let Some(answer) = chosen {
        let pick = picks.get(answer as usize).copied().unwrap_or(Pick::Deny);
        match pick {
            Pick::Deny => return ToolOutput::error("The user declined this tool call."),
            Pick::Once => {}
            Pick::Conversation => checked
                .iter()
                .for_each(|c| grants::allow_here(&ctx.chat, &grant_name(&c.call.slug))),
            Pick::Mate => match (&mate, &db) {
                (Some(m), Some(db)) => {
                    for c in &checked {
                        let _ = db.with(|conn| {
                            evo_store::mates::add_grant(conn, &m.id, &grant_name(&c.call.slug))
                        });
                    }
                    let _ = ctx.app.emit("mates://changed", json!({}));
                }
                _ => {
                    if let Some(pool) = &pool {
                        for c in &checked {
                            let _ = pool.set_permission(gate::KEY, &c.call.slug, Permission::Allow);
                        }
                    }
                }
            },
        }
    }
    let result = pool_call(&ctx, name, forward).await;
    let mut output = finish(result);
    // FR-15: a refusal for missing permissions says what to change.
    let text: String = output
        .content
        .iter()
        .filter_map(|c| {
            if let Content::Text(t) = c {
                Some(t.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if let Some(note) = super::errors::annotate(&text, &gctx.name(&checked[0].call.toolkit)) {
        output.content.push(Content::Text(note));
    }
    output
}

/// A refused call: for a mate whose service needs the person, the Connect
/// card (FR-11); otherwise the refusal as a tool error.
async fn refusal_or_card(
    ctx: &ToolCtx,
    mate: Option<&evo_store::mates::Mate>,
    refusal: &Option<Refusal>,
    text: String,
) -> ToolOutput {
    if let (Some(mate), Some(Refusal::Connect { toolkit, reason })) = (mate, refusal) {
        return connect_card(ctx, Some(mate), toolkit, "", Some(*reason)).await;
    }
    ToolOutput::error(text)
}

// ── The Connect card ────────────────────────────────────────────────────────

/// How long a Connect card waits (as the mates' own).
const WAIT_LIMIT: Duration = Duration::from_secs(15 * 60);

/// Where a connected service stands for a caller.
fn need_of(app: &AppHandle, mate: Option<&evo_store::mates::Mate>, toolkit: &str) -> Option<Need> {
    let accounts = accounts(app);
    let apps = Apps::of_mate(mate.and_then(|m| m.apps.as_ref()));
    let mine: Vec<&Account> = accounts.iter().filter(|a| a.toolkit == toolkit).collect();
    if mine.iter().any(|a| a.status == Status::Active) {
        return (!apps.service_allowed(toolkit)).then_some(Need::NotAllowed);
    }
    if mine.is_empty() {
        Some(Need::NotConnected)
    } else {
        Some(Need::Expired)
    }
}

/// The card for a service a caller needs (FR-11): Connect / Reconnect runs
/// the sign-in; Open Apps opens the mate's Apps tab; then it waits.
pub async fn connect_card(
    ctx: &ToolCtx,
    mate: Option<&evo_store::mates::Mate>,
    toolkit: &str,
    reason: &str,
    need: Option<Need>,
) -> ToolOutput {
    let Some(state) = super::app::state(&ctx.app) else {
        return ToolOutput::error(
            "Connected apps is not set up: the person adds a Composio key in Connected apps first.",
        );
    };
    let service = state.service.clone();
    if service.client().is_none() {
        return ToolOutput::error(
            "Connected apps is not set up: the person adds a Composio key in Connected apps first.",
        );
    }
    let _ = service.refresh_accounts().await;
    let names = super::app::names(&ctx.app).await;
    let name = names
        .get(toolkit)
        .cloned()
        .unwrap_or_else(|| toolkit.to_string());
    let Some(need) = need.or_else(|| need_of(&ctx.app, mate, toolkit)) else {
        return ToolOutput::text(format!(
            "{name} is already connected for you: go on with its tools."
        ));
    };
    let who = mate
        .map(|m| m.name.clone())
        .unwrap_or_else(|| "The agent".into());
    let (action, what) = match need {
        Need::NotConnected => ("Connect", format!("{name} is not connected yet.")),
        Need::Expired => ("Reconnect", format!("{name}'s sign-in has expired.")),
        Need::NotAllowed => (
            "Open Apps",
            format!("{name} is not among the apps {who} may use."),
        ),
    };
    let why = if reason.is_empty() {
        String::new()
    } else {
        format!(" {who} needs it to {}.", reason.trim_end_matches('.'))
    };
    let picked = ctx
        .ask(
            &format!("{action} {name}"),
            &format!("{what}{why} {who} never signs in by itself."),
            &[action, "Not now"],
        )
        .await;
    if picked < 0 {
        return ToolOutput::text("");
    }
    if picked != 0 {
        return ToolOutput::text(format!("The person chose not to connect {name} now. Go on without it, or ask them what to do instead; do not ask to connect it again in this turn."));
    }
    match need {
        Need::NotAllowed => {
            let _ = ctx.app.emit(
                "mates://open-panel",
                json!({ "chat": ctx.chat, "tab": "apps" }),
            );
        }
        Need::NotConnected | Need::Expired => {
            let expired = service
                .accounts_now()
                .into_iter()
                .find(|a| a.toolkit == toolkit && a.status != Status::Active);
            let started = match expired {
                Some(old) => super::app::apps_reconnect(ctx.app.clone(), old.id).await,
                None => super::app::apps_connect(ctx.app.clone(), toolkit.to_string(), None).await,
            };
            if let Err(error) = started {
                return ToolOutput::error(format!("{name} could not be connected: {error}"));
            }
        }
    }
    let mut waiting = ctx.clone();
    waiting.cancel = ctx.cancel.child_token();
    let hint = match need {
        Need::NotAllowed => format!("Allow {name} in {who}'s Apps tab."),
        _ => format!("Finish signing in to {name} in your browser."),
    };
    let (title, body) = (
        format!("Waiting for {name}"),
        format!("{hint} {who} goes on once it's connected."),
    );
    let ask = waiting.ask(&title, &body, &["Cancel"]);
    tokio::pin!(ask);
    let started = std::time::Instant::now();
    loop {
        tokio::select! {
            answer = &mut ask => {
                if answer < 0 && ctx.cancel.is_cancelled() {
                    return ToolOutput::text("");
                }
                return ToolOutput::text(format!("The person stopped connecting {name}. Go on without it, or ask them what to do."));
            }
            _ = tokio::time::sleep(Duration::from_secs(2)) => {
                let fresh = mate.and_then(|m| mate_of(&ctx.app, &ctx.chat).filter(|x| x.id == m.id));
                if need_of(&ctx.app, fresh.as_ref().or(mate), toolkit).is_none() {
                    waiting.cancel.cancel();
                    return ToolOutput::text(format!("{name} is connected: go on with its tools."));
                }
                if started.elapsed() > WAIT_LIMIT {
                    waiting.cancel.cancel();
                    return ToolOutput::text(format!("{name} was not connected within 15 minutes. Tell the person and go on without it."));
                }
            }
        }
    }
}

/// A mate's `connect_app` card for an app that is a connected-apps
/// service (FR-11): `None` when `asked` names no such service or no key
/// is saved, so the card for connectors runs instead.
pub async fn connect_named(
    ctx: &ToolCtx,
    mate: &evo_store::mates::Mate,
    asked: &str,
    reason: &str,
) -> Option<ToolOutput> {
    let state = super::app::state(&ctx.app)?;
    state.service.client()?;
    let names = super::app::names(&ctx.app).await;
    let wanted = asked.trim().to_ascii_lowercase();
    let toolkit = names
        .iter()
        .find(|(slug, name)| **slug == wanted || name.to_ascii_lowercase() == wanted)
        .map(|(slug, _)| slug.clone())?;
    Some(connect_card(ctx, Some(mate), &toolkit, reason, None).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connected_apps::gate::AppCall;

    fn checked(slug: &str, class: Class) -> Checked {
        Checked {
            call: AppCall {
                index: 0,
                slug: slug.into(),
                toolkit: "gmail".into(),
                arguments: json!({}),
                account: None,
            },
            account: Account {
                id: "ca_w".into(),
                toolkit: "gmail".into(),
                label: "work".into(),
                status: Status::Active,
                connected_at: String::new(),
            },
            class,
            files: vec![],
        }
    }

    #[test]
    fn a_send_tool_asks_under_ask_every_time() {
        let send = [checked("GMAIL_SEND_EMAIL", Class::Send)];
        let none = |_: &str| None;
        let yes = |_: &str| true;
        let no = |_: &str| false;
        // A mate on Ask every time: asks even when allowed before, never Always allow.
        let plan = decide(
            &send,
            &none,
            Some(MateSide {
                sending: "ask",
                granted: &yes,
                here: &yes,
            }),
        );
        assert_eq!(
            plan,
            Plan::Ask {
                picks: vec![Pick::Once, Pick::Deny],
                sending: true
            }
        );
        // Send without asking: the tool's own permission (Ask by default), no Always allow.
        let plan = decide(
            &send,
            &none,
            Some(MateSide {
                sending: "send",
                granted: &no,
                here: &no,
            }),
        );
        assert_eq!(
            plan,
            Plan::Ask {
                picks: vec![Pick::Once, Pick::Conversation, Pick::Deny],
                sending: false
            }
        );
        // A Harness chat: a sending tool always asks, Allow once or Deny.
        let allow = |_: &str| Some(Permission::Allow);
        assert_eq!(
            decide(&send, &allow, None),
            Plan::Ask {
                picks: vec![Pick::Once, Pick::Deny],
                sending: true
            }
        );
    }

    #[test]
    fn a_reading_tool_runs_and_may_be_always_allowed() {
        let read = [checked("GMAIL_FETCH_EMAILS", Class::Read)];
        let none = |_: &str| None;
        assert_eq!(
            decide(&read, &none, None),
            Plan::Run,
            "reading is allowed by default"
        );
        let ask = |_: &str| Some(Permission::Ask);
        assert_eq!(
            decide(&read, &ask, None),
            Plan::Ask {
                picks: vec![Pick::Once, Pick::Mate, Pick::Deny],
                sending: false
            }
        );
        let no = |_: &str| false;
        let plan = decide(
            &read,
            &ask,
            Some(MateSide {
                sending: "ask",
                granted: &no,
                here: &no,
            }),
        );
        assert_eq!(
            plan,
            Plan::Ask {
                picks: vec![Pick::Once, Pick::Conversation, Pick::Mate, Pick::Deny],
                sending: false
            }
        );
        assert_eq!(pick_label(Pick::Mate, None), "Always allow");
        assert_eq!(
            pick_label(Pick::Mate, Some("Writer")),
            "Always allow for Writer"
        );
        assert_eq!(
            grant_name("GMAIL_FETCH_EMAILS"),
            "mcp__connected_apps__GMAIL_FETCH_EMAILS"
        );
    }

    fn meta_tool(name: &str) -> ChatTool {
        ChatTool {
            name: grant_name(name),
            description: String::new(),
            input_schema: json!({"type": "object"}),
            permission: Permission::Allow,
            server: gate::KEY.into(),
            tool: name.into(),
            read_only: false,
        }
    }

    #[test]
    fn the_sandbox_is_closed_to_a_limited_mate() {
        let bash = meta_tool("COMPOSIO_REMOTE_BASH_TOOL");
        let run = meta_tool(gate::MULTI_EXECUTE);
        assert!(
            !offered_to(&bash, true, false),
            "a limited mate never gets the sandbox"
        );
        assert!(
            offered_to(&bash, false, false),
            "others follow its permission (blocked by default in the pool)"
        );
        assert!(offered_to(&run, true, false));
        let other = ChatTool {
            server: "github".into(),
            ..meta_tool("create_issue")
        };
        assert!(
            offered_to(&other, true, true),
            "other connectors are not touched"
        );
    }

    #[test]
    fn connected_apps_off_in_a_project_offers_none() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("proj");
        std::fs::create_dir_all(project.join(".git")).unwrap();
        std::fs::create_dir_all(project.join("src")).unwrap();
        assert!(!project_off(&project.join("src")));
        std::fs::create_dir_all(project.join(".evo")).unwrap();
        std::fs::write(
            project.join(".evo").join("settings.json"),
            r#"{"connectedApps": false}"#,
        )
        .unwrap();
        assert!(project_off(&project.join("src")));
        assert!(!offered_to(&meta_tool(gate::MULTI_EXECUTE), false, true));
        std::fs::write(
            project.join(".evo").join("settings.json"),
            r#"{"connectedApps": true}"#,
        )
        .unwrap();
        assert!(!project_off(&project));
    }

    #[test]
    fn tools_are_learned_from_search_and_schema_results() {
        let result = json!({"content": [{"type": "text", "text": json!({"data": {"results": [
            {"tool_slug": "GMAIL_SEND_EMAIL", "input_schema": {"type": "object", "properties": {"to": {"type": "string"}}}, "tags": ["important"]}
        ], "tool_schemas": {"GMAIL_FETCH_EMAILS": {"type": "object", "properties": {"q": {"type": "string"}}}}}}).to_string()}]});
        let mut found = HashMap::new();
        for part in json_parts(&result) {
            tools_in(&part, &mut found);
        }
        assert_eq!(
            found["GMAIL_SEND_EMAIL"]["input_parameters"]["properties"]["to"]["type"],
            "string"
        );
        assert_eq!(found["GMAIL_SEND_EMAIL"]["tags"], json!(["important"]));
        assert!(found.contains_key("GMAIL_FETCH_EMAILS"));
    }
}
