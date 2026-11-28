//! The gate every connected-app call passes before it leaves Evo
//! (docs/features/connected-apps/spec.md, FR-8 – FR-11, FR-13, FR-14;
//! plan 3.5).
//!
//! The Connected apps connector offers Composio's meta-tools. Only
//! `COMPOSIO_MULTI_EXECUTE_TOOL` names app tools, in its arguments; each of
//! them is checked in turn — the mate's grant for the service and tool,
//! the account (named when the caller may use several, and one the caller
//! may use), the arguments against the tool's schema, and its local files
//! — and any refusal refuses the whole call. Then the call is classed
//! (read, or sending on the person's behalf), the approval card asks
//! when the class and the caller's settings call for it, and files are
//! uploaded only after that. Search results are filtered by the mate's
//! grants; `COMPOSIO_MANAGE_CONNECTIONS` becomes Evo's Connect card; the
//! sandbox and proxy meta-tools are blocked by default and never offered
//! to a mate with limited apps.

use std::collections::{BTreeMap, HashMap};

use evo_mcp::Permission;
use serde_json::{json, Value};

use super::accounts::{Account, Status};
use super::files::{self, LocalFile, Places};
use crate::agent_tools::BoxFuture;

/// The connector's name (tool names `mcp__connected_apps__…`).
pub const LABEL: &str = "connected_apps";
/// Its pool key.
pub const KEY: &str = "app:connected_apps";

pub const SEARCH_TOOLS: &str = "COMPOSIO_SEARCH_TOOLS";
pub const GET_TOOL_SCHEMAS: &str = "COMPOSIO_GET_TOOL_SCHEMAS";
pub const MULTI_EXECUTE: &str = "COMPOSIO_MULTI_EXECUTE_TOOL";
pub const MANAGE_CONNECTIONS: &str = "COMPOSIO_MANAGE_CONNECTIONS";
pub const WAIT_FOR_CONNECTIONS: &str = "COMPOSIO_WAIT_FOR_CONNECTIONS";
/// Meta-tools that run code or raw requests at Composio: they can reach any
/// connected app past a mate's grants.
pub const SANDBOX: [&str; 3] = [
    "COMPOSIO_REMOTE_WORKBENCH",
    "COMPOSIO_REMOTE_BASH_TOOL",
    "COMPOSIO_PROXY_EXECUTE",
];

/// The per-tool field a `MULTI_EXECUTE` item names its account with: the
/// alias or the account id (recorded in `testdata/live/mcp_tools_list.json`).
pub const ACCOUNT_FIELD: &str = "account";

/// The pool's defaults for the connector's tools: the sandbox blocked
/// (Customize may allow it), the rest by the usual rule.
pub fn default_permissions() -> BTreeMap<String, Permission> {
    SANDBOX
        .iter()
        .map(|t| (t.to_string(), Permission::Block))
        .collect()
}

/// What a meta-tool is to the gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Meta {
    /// Search: passes, its results filtered by the mate's grants.
    Search,
    /// Passes as it is (schemas, waiting for a sign-in).
    Pass,
    /// App tools: every one is gated.
    Execute,
    /// Becomes Evo's Connect card.
    Connect,
    /// Code or raw requests at Composio.
    Sandbox,
    /// A meta-tool this gate does not know: refused.
    Unknown,
}

pub fn meta(tool: &str) -> Meta {
    match tool {
        SEARCH_TOOLS => Meta::Search,
        GET_TOOL_SCHEMAS | WAIT_FOR_CONNECTIONS => Meta::Pass,
        MULTI_EXECUTE => Meta::Execute,
        MANAGE_CONNECTIONS => Meta::Connect,
        t if SANDBOX.contains(&t) => Meta::Sandbox,
        _ => Meta::Unknown,
    }
}

// ── A mate's grants ─────────────────────────────────────────────────────────

/// Which tools of a service a mate may use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tools {
    All,
    These(Vec<String>),
    None,
}

/// A mate's grant for one connected service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    pub tools: Tools,
    /// The account labels it may use; `None`: any.
    pub accounts: Option<Vec<String>>,
}

/// Which connected apps a caller may use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Apps {
    /// A Harness chat, or a mate on All connected apps.
    All,
    /// A mate on Only these (or None): the services it names, by toolkit.
    Only(BTreeMap<String, Grant>),
}

/// The `apps` entry prefix of a connected service: `composio:gmail`.
pub const SERVICE_PREFIX: &str = "composio:";

impl Apps {
    /// A mate's `apps` setting (`None`: All connected apps).
    pub fn of_mate(apps: Option<&Vec<Value>>) -> Self {
        let Some(list) = apps else {
            return Apps::All;
        };
        let mut grants = BTreeMap::new();
        for entry in list {
            let Some(toolkit) = entry
                .get("service")
                .and_then(Value::as_str)
                .and_then(|s| s.strip_prefix(SERVICE_PREFIX))
            else {
                continue;
            };
            let tools = match entry.get("tools") {
                Some(Value::Array(names)) => Tools::These(
                    names
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect(),
                ),
                Some(Value::String(s)) if s == "none" => Tools::None,
                _ => Tools::All,
            };
            let accounts = entry.get("accounts").and_then(Value::as_array).map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            });
            grants.insert(toolkit.to_ascii_lowercase(), Grant { tools, accounts });
        }
        Apps::Only(grants)
    }

    /// Limited to some apps (the sandbox is never theirs).
    pub fn limited(&self) -> bool {
        matches!(self, Apps::Only(_))
    }

    fn grant(&self, toolkit: &str) -> Option<Grant> {
        match self {
            Apps::All => Some(Grant {
                tools: Tools::All,
                accounts: None,
            }),
            Apps::Only(grants) => grants.get(toolkit).cloned(),
        }
    }

    /// Whether any tool of `toolkit` may be used.
    pub fn service_allowed(&self, toolkit: &str) -> bool {
        self.grant(toolkit).is_some_and(|g| g.tools != Tools::None)
    }

    /// Whether tool `slug` of `toolkit` may be used.
    pub fn allows(&self, toolkit: &str, slug: &str) -> bool {
        match self.grant(toolkit).map(|g| g.tools) {
            Some(Tools::All) => true,
            Some(Tools::These(names)) => names.iter().any(|n| n.eq_ignore_ascii_case(slug)),
            Some(Tools::None) | None => false,
        }
    }
}

/// The `apps` entries of the services a template or team file names, each
/// off: the person turns on what the mate may use (FR-10).
pub fn named_services(names: &[String]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for name in names {
        let toolkit: String = name
            .trim()
            .to_ascii_lowercase()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        let service = format!("{SERVICE_PREFIX}{toolkit}");
        if !toolkit.is_empty() && !out.iter().any(|e| e["service"] == service.as_str()) {
            out.push(json!({ "service": service, "tools": "none" }));
        }
    }
    out
}

// ── Parsing a MULTI_EXECUTE call ────────────────────────────────────────────

/// One app tool of a `MULTI_EXECUTE` call.
#[derive(Debug, Clone, PartialEq)]
pub struct AppCall {
    /// Its place in `tools`.
    pub index: usize,
    /// `GMAIL_SEND_EMAIL`.
    pub slug: String,
    /// `gmail`.
    pub toolkit: String,
    pub arguments: Value,
    /// The account as the call names it (label or id), if it does.
    pub account: Option<String>,
}

/// The toolkit of `slug`: the longest of `known` (lower-case slugs) it
/// starts with, else its first part.
pub fn toolkit_of(slug: &str, known: &[String]) -> String {
    let upper = slug.to_ascii_uppercase();
    known
        .iter()
        .filter(|k| upper.starts_with(&format!("{}_", k.to_ascii_uppercase())))
        .max_by_key(|k| k.len())
        .cloned()
        .unwrap_or_else(|| upper.split('_').next().unwrap_or("").to_ascii_lowercase())
}

/// Read the app tools of a `MULTI_EXECUTE` call. A shape this gate does not
/// know is refused rather than passed on.
pub fn parse_multi_execute(args: &Value, known: &[String]) -> Result<Vec<AppCall>, String> {
    let shape = |what: &str| {
        format!("{MULTI_EXECUTE} arguments are not in a shape Evo can check ({what}); the call was not sent.")
    };
    let tools = args
        .get("tools")
        .and_then(Value::as_array)
        .ok_or_else(|| shape("no `tools` list"))?;
    if tools.is_empty() {
        return Err(shape("`tools` is empty"));
    }
    tools
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let slug = item
                .get("tool_slug")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| shape(&format!("tools[{index}] has no `tool_slug`")))?;
            let arguments = match item.get("arguments") {
                None | Some(Value::Null) => json!({}),
                Some(Value::Object(map)) => Value::Object(map.clone()),
                Some(_) => {
                    return Err(shape(&format!("tools[{index}].arguments is not an object")))
                }
            };
            let account = [ACCOUNT_FIELD, "connected_account_id", "alias"]
                .iter()
                .find_map(|f| item.get(*f).and_then(Value::as_str))
                .map(str::trim)
                .filter(|a| !a.is_empty())
                .map(str::to_string);
            Ok(AppCall {
                index,
                slug: slug.trim().to_ascii_uppercase(),
                toolkit: toolkit_of(slug.trim(), known),
                arguments,
                account,
            })
        })
        .collect()
}

// ── Refusals ────────────────────────────────────────────────────────────────

/// Why a call cannot go on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// A tool error with this text.
    Text(String),
    /// The service needs the person: not connected, expired, or (for a
    /// mate) not allowed. A mate shows its Connect / Reconnect card.
    Connect { toolkit: String, reason: Need },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    NotConnected,
    Expired,
    NotAllowed,
}

impl Refusal {
    /// The refusal as the agent reads it.
    pub fn text(&self, service: &str) -> String {
        match self {
            Refusal::Text(text) => text.clone(),
            Refusal::Connect { reason: Need::NotConnected, .. } => format!("{service} is not connected. The person connects it in Connected apps; do not retry until they have."),
            Refusal::Connect { reason: Need::Expired, .. } => format!("{service}'s sign-in expired. The person reconnects it in Connected apps (Reconnect); do not retry until they have."),
            Refusal::Connect { reason: Need::NotAllowed, .. } => format!("{service} is not among the apps you may use. The person can allow it in your Apps setting."),
        }
    }
}

// ── Accounts ────────────────────────────────────────────────────────────────

/// The account a call runs under (FR-8): the one it names, which the caller
/// must be allowed; or, with none named, the caller's only active one.
pub fn choose_account(
    call: &AppCall,
    accounts: &[Account],
    grant: &Grant,
    service: &str,
) -> Result<Account, Refusal> {
    let mine: Vec<&Account> = accounts
        .iter()
        .filter(|a| a.toolkit == call.toolkit)
        .collect();
    let allowed = |a: &Account| {
        grant
            .accounts
            .as_ref()
            .is_none_or(|labels| labels.iter().any(|l| l.eq_ignore_ascii_case(&a.label)))
    };
    let usable: Vec<&Account> = mine
        .iter()
        .copied()
        .filter(|a| a.status == Status::Active && allowed(a))
        .collect();
    let labels = |list: &[&Account]| {
        list.iter()
            .map(|a| a.label.clone())
            .collect::<Vec<_>>()
            .join(", ")
    };
    if usable.is_empty() {
        let reason = if mine.iter().any(|a| a.status == Status::Active) {
            Need::NotAllowed
        } else if mine.iter().any(|a| {
            matches!(
                a.status,
                Status::Expired | Status::NeedsSignIn | Status::Failed
            )
        }) {
            Need::Expired
        } else {
            Need::NotConnected
        };
        return Err(Refusal::Connect {
            toolkit: call.toolkit.clone(),
            reason,
        });
    }
    match &call.account {
        Some(named) => {
            let found = mine.iter().find(|a| a.label.eq_ignore_ascii_case(named) || a.id == *named);
            match found {
                Some(a) if usable.iter().any(|u| u.id == a.id) => Ok((*a).clone()),
                Some(a) if a.status != Status::Active && allowed(a) => Err(Refusal::Text(format!("{service} \"{}\" needs the person to Reconnect it in Connected apps. Accounts you may use now: {}.", a.label, labels(&usable)))),
                _ => Err(Refusal::Text(format!("account \"{named}\" is not one you may use for {service}. Accounts you may use: {}. Name one in `{ACCOUNT_FIELD}`.", labels(&usable)))),
            }
        }
        None if usable.len() == 1 => Ok(usable[0].clone()),
        None => Err(Refusal::Text(format!("account required: {service} has several accounts — {}. Name one in `{ACCOUNT_FIELD}` of {}.", labels(&usable), call.slug))),
    }
}

// ── Schema ──────────────────────────────────────────────────────────────────

/// Check `args` against `schema` (FR-9): the first problem, said to be
/// about the call's shape, not access.
pub fn check_schema(slug: &str, schema: &Value, args: &Value) -> Result<(), String> {
    let shape = |problem: String| {
        format!("arguments do not match {slug}: {problem} — this is about the call's shape, not access.")
    };
    let validator = if schema.get("$schema").is_some() {
        jsonschema::validator_for(schema)
    } else {
        jsonschema::draft7::new(schema)
    };
    let validator = validator.map_err(|e| shape(format!("its schema cannot be read ({e})")))?;
    let problem = validator.iter_errors(args).next().map(|error| {
        let path = error.instance_path().to_string();
        if let jsonschema::error::ValidationErrorKind::Required { property } = error.kind() {
            format!(
                "missing {}",
                property
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| property.to_string())
            )
        } else if path.is_empty() {
            error.to_string()
        } else {
            format!("{error} at {path}")
        }
    });
    match problem {
        None => Ok(()),
        Some(text) => Err(shape(text)),
    }
}

// ── Class ───────────────────────────────────────────────────────────────────

/// Read or send (FR-13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Read,
    /// Sends, posts, publishes, pays or deletes: anything not a read.
    Send,
}

const READ_VERBS: [&str; 6] = ["GET", "LIST", "FETCH", "SEARCH", "FIND", "READ"];

/// Read when Composio marks the tool read-only (and not destructive), or
/// the action of its slug starts with a read verb; anything else sends.
pub fn class(slug: &str, toolkit: &str, tool: Option<&Value>) -> Class {
    let tags: Vec<String> = tool
        .and_then(|t| t.get("tags"))
        .and_then(Value::as_array)
        .map(|t| {
            t.iter()
                .filter_map(Value::as_str)
                .map(str::to_ascii_lowercase)
                .collect()
        })
        .unwrap_or_default();
    if tags.iter().any(|t| t == "destructivehint") {
        return Class::Send;
    }
    if tags.iter().any(|t| t == "readonlyhint")
        || tool
            .and_then(|t| t.pointer("/annotations/readOnlyHint"))
            .and_then(Value::as_bool)
            == Some(true)
    {
        return Class::Read;
    }
    let upper = slug.to_ascii_uppercase();
    let action = upper
        .strip_prefix(&format!("{}_", toolkit.to_ascii_uppercase()))
        .unwrap_or(&upper);
    let verb = action.split('_').next().unwrap_or("");
    if READ_VERBS.contains(&verb) {
        Class::Read
    } else {
        Class::Send
    }
}

// ── The whole check ─────────────────────────────────────────────────────────

/// What the gate knows about the caller and the services.
pub struct Context<'a> {
    pub accounts: &'a [Account],
    pub apps: &'a Apps,
    /// The known tools by slug: `{slug, input_parameters (schema), tags, …}`.
    pub tools: &'a HashMap<String, Value>,
    pub places: &'a Places,
    /// Service names by toolkit, for messages.
    pub names: &'a HashMap<String, String>,
}

impl Context<'_> {
    pub fn name(&self, toolkit: &str) -> String {
        self.names.get(toolkit).cloned().unwrap_or_else(|| {
            let mut chars = toolkit.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        })
    }
}

/// The schema of a known tool.
pub fn schema_of(tool: &Value) -> Value {
    tool.get("input_parameters")
        .or_else(|| tool.get("inputSchema"))
        .or_else(|| tool.get("input_schema"))
        .cloned()
        .unwrap_or_else(|| json!({"type": "object"}))
}

/// One app tool that passed.
#[derive(Debug, Clone, PartialEq)]
pub struct Checked {
    pub call: AppCall,
    pub account: Account,
    pub class: Class,
    pub files: Vec<LocalFile>,
}

/// Check every app tool of a `MULTI_EXECUTE` call: grants and account
/// first, then the schema, then the files. The first refusal refuses all.
pub fn check(args: &Value, ctx: &Context) -> Result<Vec<Checked>, Refusal> {
    let known: Vec<String> = ctx
        .accounts
        .iter()
        .map(|a| a.toolkit.clone())
        .chain(ctx.names.keys().cloned())
        .collect();
    let calls = parse_multi_execute(args, &known).map_err(Refusal::Text)?;
    let mut out = Vec::with_capacity(calls.len());
    let mut files_seen = 0;
    for call in calls {
        let service = ctx.name(&call.toolkit);
        let Some(grant) = ctx
            .apps
            .grant(&call.toolkit)
            .filter(|g| g.tools != Tools::None)
        else {
            return Err(Refusal::Connect {
                toolkit: call.toolkit.clone(),
                reason: Need::NotAllowed,
            });
        };
        if !ctx.apps.allows(&call.toolkit, &call.slug) {
            return Err(Refusal::Text(format!(
                "{} is not among the {service} tools you may use.",
                call.slug
            )));
        }
        let account = choose_account(&call, ctx.accounts, &grant, &service)?;
        let tool = ctx.tools.get(&call.slug).ok_or_else(|| {
            Refusal::Text(format!(
                "{} is not a {service} tool Evo knows; find it with {SEARCH_TOOLS} first.",
                call.slug
            ))
        })?;
        let schema = schema_of(tool);
        check_schema(&call.slug, &schema, &call.arguments).map_err(Refusal::Text)?;
        let found = files::local_paths(&schema, &call.arguments);
        files_seen += found.len();
        if files_seen > files::MAX_FILES {
            return Err(Refusal::Text(format!(
                "A call may send at most {} files.",
                files::MAX_FILES
            )));
        }
        let files = files::check_all(&found, ctx.places).map_err(Refusal::Text)?;
        let class = class(&call.slug, &call.toolkit, Some(tool));
        out.push(Checked {
            call,
            account,
            class,
            files,
        });
    }
    Ok(out)
}

/// The call as it goes to Composio: each tool's account named by id, and
/// each local file replaced by what its upload gave (`uploads` by
/// `(tool index, pointer)`).
pub fn forwarded(
    args: &Value,
    checked: &[Checked],
    uploads: &HashMap<(usize, String), Value>,
) -> Value {
    let mut out = args.clone();
    for c in checked {
        let Some(item) = out.pointer_mut(&format!("/tools/{}", c.call.index)) else {
            continue;
        };
        if let Some(map) = item.as_object_mut() {
            map.remove("connected_account_id");
            map.remove("alias");
            map.insert(ACCOUNT_FIELD.into(), json!(c.account.id));
        }
        for f in &c.files {
            if let (Some(value), Some(slot)) = (
                uploads.get(&(c.call.index, f.pointer.clone())),
                item.get_mut("arguments")
                    .and_then(|a| a.pointer_mut(&f.pointer)),
            ) {
                *slot = value.clone();
            }
        }
    }
    out
}

/// The approval card: a title and the body listing each tool with its
/// service, account, arguments and files.
pub fn card(checked: &[Checked], ctx: &Context, who: Option<&str>) -> (String, String) {
    let sending = checked.iter().any(|c| c.class == Class::Send);
    let first = &checked[0];
    let service = format!(
        "{} ({})",
        ctx.name(&first.call.toolkit),
        first.account.label
    );
    let what = if checked.len() == 1 {
        first.call.slug.clone()
    } else {
        format!("{} tools", checked.len())
    };
    let title = match (sending, who) {
        (true, Some(mate)) => format!("Send on your behalf? {mate} wants {service} to run {what}"),
        (true, None) => format!("Send on your behalf? {service} would run {what}"),
        (false, _) => format!("Allow {service} to run {what}?"),
    };
    let body = checked
        .iter()
        .map(|c| {
            let args = serde_json::to_string_pretty(&c.call.arguments).unwrap_or_default();
            let args: String = if args.chars().count() > 600 {
                args.chars().take(600).collect::<String>() + "\u{2026}"
            } else {
                args
            };
            let mut part = format!(
                "{} · {} · account \"{}\"\n{args}",
                ctx.name(&c.call.toolkit),
                c.call.slug,
                c.account.label
            );
            if !c.files.is_empty() {
                part.push_str(&format!(
                    "\nFiles to upload:\n{}",
                    files::card_lines(&c.files)
                ));
            }
            part
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    (title, body)
}

// ── Search results ──────────────────────────────────────────────────────────

/// A search result without the tools the caller may not use. Composio's
/// result lists tools under `data.results[].tool_slug` (and schemas under
/// `data.tool_schemas`); anything else is kept as it is.
pub fn filter_search(result: &Value, apps: &Apps, known: &[String]) -> Value {
    if !apps.limited() {
        return result.clone();
    }
    let keep = |slug: &str| apps.allows(&toolkit_of(slug, known), slug);
    let mut out = result.clone();
    filter_slugs(&mut out, &keep);
    filter_toolkits(&mut out, &|toolkit: &str| {
        apps.service_allowed(&toolkit.to_ascii_lowercase())
    });
    out
}

/// Drop `{toolkit, …}` items (connection statuses) of services the caller
/// may not use, and those services from `toolkits` lists.
fn filter_toolkits(value: &mut Value, keep: &dyn Fn(&str) -> bool) {
    match value {
        Value::Array(items) => {
            items.retain(|item| match item {
                Value::Object(map) if !map.contains_key("tool_slug") => {
                    map.get("toolkit").and_then(Value::as_str).is_none_or(keep)
                }
                _ => true,
            });
            for item in items.iter_mut() {
                filter_toolkits(item, keep);
            }
        }
        Value::Object(map) => {
            if let Some(Value::Array(list)) = map.get_mut("toolkits") {
                list.retain(|t| t.as_str().is_none_or(keep));
            }
            for item in map.values_mut() {
                filter_toolkits(item, keep);
            }
        }
        _ => {}
    }
}

fn filter_slugs(value: &mut Value, keep: &dyn Fn(&str) -> bool) {
    match value {
        Value::Array(items) => {
            items.retain(|item| match item {
                // `primary_tool_slugs`, `related_tool_slugs`.
                Value::String(s) if looks_like_slug(s) => keep(s),
                _ => item
                    .get("tool_slug")
                    .or_else(|| item.get("slug"))
                    .and_then(Value::as_str)
                    .is_none_or(keep),
            });
            for item in items.iter_mut() {
                filter_slugs(item, keep);
            }
        }
        Value::Object(map) => {
            map.retain(|key, _| !looks_like_slug(key) || keep(key));
            for item in map.values_mut() {
                filter_slugs(item, keep);
            }
        }
        _ => {}
    }
}

fn looks_like_slug(key: &str) -> bool {
    key.contains('_')
        && key
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

// ── Running a call ──────────────────────────────────────────────────────────

/// What the caller decides once a call passed the checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Approval {
    Run,
    /// Ask with these options; the index of the one that runs once, the
    /// ones that also remember, and the one that declines are the
    /// caller's to read.
    Ask(Vec<String>),
}

/// The side effects a call needs, behind a trait so the order — checks,
/// card, uploads, call — can be tested.
pub trait Backend: Send + Sync {
    /// The person's answer: an option's index, or -1 when the call was dropped.
    fn ask(&self, title: String, body: String, options: Vec<String>) -> BoxFuture<i32>;
    /// Upload one file; what to send instead of its path.
    fn upload(
        &self,
        toolkit: String,
        slug: String,
        file: LocalFile,
    ) -> BoxFuture<Result<Value, String>>;
}

/// How a run ended before the call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    /// The person declined, or the call was dropped (`None`: no answer).
    Declined(Option<String>),
    Refused(String),
}

/// Check, ask when `approval` says so (`run` decides from the chosen
/// option whether to go on), upload the files, and give the arguments to
/// send. Nothing leaves Evo before the person's answer.
pub async fn prepare(
    args: &Value,
    ctx: &Context<'_>,
    approval: impl FnOnce(&[Checked]) -> Approval,
    run: impl FnOnce(&[Checked], i32) -> bool,
    who: Option<&str>,
    backend: &dyn Backend,
) -> Result<(Value, Vec<Checked>), Stop> {
    let checked = check(args, ctx).map_err(|r| {
        let service = match &r {
            Refusal::Connect { toolkit, .. } => ctx.name(toolkit),
            Refusal::Text(_) => String::new(),
        };
        Stop::Refused(r.text(&service))
    })?;
    if let Approval::Ask(options) = approval(&checked) {
        let (title, body) = card(&checked, ctx, who);
        let answer = backend.ask(title, body, options).await;
        if answer < 0 {
            return Err(Stop::Declined(None));
        }
        if !run(&checked, answer) {
            return Err(Stop::Declined(Some(
                "The user declined this tool call.".into(),
            )));
        }
    }
    let mut uploads = HashMap::new();
    for c in &checked {
        for f in &c.files {
            let value = backend
                .upload(c.call.toolkit.clone(), c.call.slug.clone(), f.clone())
                .await
                .map_err(|e| Stop::Refused(format!("{} could not be uploaded: {e}", f.name)))?;
            uploads.insert((c.call.index, f.pointer.clone()), value);
        }
    }
    Ok((forwarded(args, &checked, &uploads), checked))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    fn account(id: &str, toolkit: &str, label: &str, status: Status) -> Account {
        Account {
            id: id.into(),
            toolkit: toolkit.into(),
            label: label.into(),
            status,
            connected_at: String::new(),
        }
    }

    fn two_gmail() -> Vec<Account> {
        vec![
            account("ca_p", "gmail", "personal", Status::Active),
            account("ca_w", "gmail", "work", Status::Active),
        ]
    }

    fn known_tools() -> HashMap<String, Value> {
        let send = json!({"slug": "GMAIL_SEND_EMAIL", "input_parameters": {
            "type": "object",
            "properties": {
                "recipient_email": {"type": "string"},
                "body": {"type": "string"},
                "attachment": {"type": "string", "file_uploadable": true}
            },
            "required": ["recipient_email", "body"]
        }});
        let fetch = json!({"slug": "GMAIL_FETCH_EMAILS", "input_parameters": {"type": "object", "properties": {"query": {"type": "string"}}}, "tags": ["readOnlyHint"]});
        let issues = json!({"slug": "GITHUB_LIST_ISSUES", "input_parameters": {"type": "object"}});
        HashMap::from([
            ("GMAIL_SEND_EMAIL".to_string(), send),
            ("GMAIL_FETCH_EMAILS".to_string(), fetch),
            ("GITHUB_LIST_ISSUES".to_string(), issues),
        ])
    }

    struct World {
        accounts: Vec<Account>,
        apps: Apps,
        tools: HashMap<String, Value>,
        places: Places,
        names: HashMap<String, String>,
    }

    impl World {
        fn new(accounts: Vec<Account>, apps: Apps) -> Self {
            Self {
                accounts,
                apps,
                tools: known_tools(),
                places: Places::default(),
                names: HashMap::from([("gmail".to_string(), "Gmail".to_string())]),
            }
        }
        fn ctx(&self) -> Context<'_> {
            Context {
                accounts: &self.accounts,
                apps: &self.apps,
                tools: &self.tools,
                places: &self.places,
                names: &self.names,
            }
        }
    }

    fn fetch(account: Option<&str>) -> Value {
        let mut item =
            json!({"tool_slug": "GMAIL_FETCH_EMAILS", "arguments": {"query": "invoice"}});
        if let Some(a) = account {
            item[ACCOUNT_FIELD] = json!(a);
        }
        json!({"tools": [item], "thought": "find invoices", "sync_response_to_workbench": false})
    }

    #[test]
    fn an_account_is_required_with_two() {
        let world = World::new(two_gmail(), Apps::All);
        let Err(Refusal::Text(text)) = check(&fetch(None), &world.ctx()) else {
            panic!("refused")
        };
        assert!(text.starts_with("account required"), "{text}");
        assert!(text.contains("personal, work"), "{text}");
        let ok = check(&fetch(Some("work")), &world.ctx()).unwrap();
        assert_eq!(ok[0].account.id, "ca_w");
        // One account: it is used.
        let world = World::new(
            vec![account("ca_p", "gmail", "personal", Status::Active)],
            Apps::All,
        );
        assert_eq!(
            check(&fetch(None), &world.ctx()).unwrap()[0].account.id,
            "ca_p"
        );
        // The call goes out naming the account by id.
        let checked = check(&fetch(Some("PERSONAL")), &world.ctx()).unwrap();
        let out = forwarded(&fetch(Some("PERSONAL")), &checked, &HashMap::new());
        assert_eq!(out["tools"][0][ACCOUNT_FIELD], "ca_p");
        assert_eq!(out["thought"], "find invoices");
    }

    #[test]
    fn a_bad_argument_is_refused_before_sending() {
        let world = World::new(two_gmail(), Apps::All);
        let args = json!({"tools": [{"tool_slug": "GMAIL_SEND_EMAIL", "arguments": {"body": "hi"}, ACCOUNT_FIELD: "work"}]});
        let Err(Refusal::Text(text)) = check(&args, &world.ctx()) else {
            panic!("refused")
        };
        assert!(
            text.contains("arguments do not match GMAIL_SEND_EMAIL: missing recipient_email"),
            "{text}"
        );
        assert!(text.contains("not access"), "{text}");
        let args = json!({"tools": [{"tool_slug": "GMAIL_SEND_EMAIL", "arguments": {"recipient_email": 5, "body": "hi"}, ACCOUNT_FIELD: "work"}]});
        let Err(Refusal::Text(text)) = check(&args, &world.ctx()) else {
            panic!("refused")
        };
        assert!(text.contains("/recipient_email"), "{text}");
    }

    #[test]
    fn an_unknown_shape_is_refused() {
        let world = World::new(two_gmail(), Apps::All);
        for args in [
            json!({}),
            json!({"tools": []}),
            json!({"tools": [{"arguments": {}}]}),
            json!({"tools": [{"tool_slug": "GMAIL_FETCH_EMAILS", "arguments": "x"}]}),
        ] {
            let Err(Refusal::Text(text)) = check(&args, &world.ctx()) else {
                panic!("refused {args}")
            };
            assert!(text.contains("not in a shape Evo can check"), "{text}");
        }
        let args = json!({"tools": [{"tool_slug": "GMAIL_DELETE_EVERYTHING", "arguments": {}, ACCOUNT_FIELD: "work"}]});
        let Err(Refusal::Text(text)) = check(&args, &world.ctx()) else {
            panic!("refused")
        };
        assert!(text.contains("not a Gmail tool Evo knows"), "{text}");
    }

    #[test]
    fn accounts_that_need_the_person_say_so() {
        let world = World::new(vec![], Apps::All);
        assert_eq!(
            check(&fetch(None), &world.ctx()).unwrap_err(),
            Refusal::Connect {
                toolkit: "gmail".into(),
                reason: Need::NotConnected
            }
        );
        let world = World::new(
            vec![account("ca_w", "gmail", "work", Status::Expired)],
            Apps::All,
        );
        assert_eq!(
            check(&fetch(None), &world.ctx()).unwrap_err(),
            Refusal::Connect {
                toolkit: "gmail".into(),
                reason: Need::Expired
            }
        );
        let mut accounts = two_gmail();
        accounts[0].status = Status::Expired;
        let world = World::new(accounts, Apps::All);
        let Err(Refusal::Text(text)) = check(&fetch(Some("personal")), &world.ctx()) else {
            panic!()
        };
        assert!(text.contains("Reconnect"), "{text}");
    }

    #[test]
    fn reading_and_sending_tools_are_told_apart() {
        let tools = known_tools();
        assert_eq!(
            class(
                "GMAIL_FETCH_EMAILS",
                "gmail",
                tools.get("GMAIL_FETCH_EMAILS")
            ),
            Class::Read
        );
        assert_eq!(
            class("GMAIL_SEND_EMAIL", "gmail", tools.get("GMAIL_SEND_EMAIL")),
            Class::Send
        );
        assert_eq!(class("GITHUB_LIST_ISSUES", "github", None), Class::Read);
        assert_eq!(class("GITHUB_CREATE_ISSUE", "github", None), Class::Send);
        assert_eq!(
            class(
                "GITHUB_LIST_X",
                "github",
                Some(&json!({"tags": ["destructiveHint"]}))
            ),
            Class::Send
        );
        assert_eq!(
            class(
                "SLACK_POST_MESSAGE",
                "slack",
                Some(&json!({"tags": ["readOnlyHint"]}))
            ),
            Class::Read
        );
        assert_eq!(
            toolkit_of(
                "GOOGLE_DRIVE_LIST_FILES",
                &["google".into(), "google_drive".into()]
            ),
            "google_drive"
        );
        assert_eq!(toolkit_of("GMAIL_SEND_EMAIL", &[]), "gmail");
    }

    #[test]
    fn meta_tools_have_a_policy() {
        assert_eq!(meta(SEARCH_TOOLS), Meta::Search);
        assert_eq!(meta(MULTI_EXECUTE), Meta::Execute);
        assert_eq!(meta(MANAGE_CONNECTIONS), Meta::Connect);
        assert_eq!(meta("COMPOSIO_REMOTE_BASH_TOOL"), Meta::Sandbox);
        assert_eq!(meta("COMPOSIO_SOMETHING_NEW"), Meta::Unknown);
        let defaults = default_permissions();
        assert!(SANDBOX.iter().all(|t| defaults[*t] == Permission::Block));
        assert!(!defaults.contains_key(MULTI_EXECUTE));
    }

    struct Fake {
        answer: i32,
        asked: Mutex<Vec<(String, String, Vec<String>)>>,
        uploads: AtomicUsize,
        order: Mutex<Vec<&'static str>>,
    }

    impl Fake {
        fn new(answer: i32) -> Arc<Self> {
            Arc::new(Self {
                answer,
                asked: Mutex::new(vec![]),
                uploads: AtomicUsize::new(0),
                order: Mutex::new(vec![]),
            })
        }
    }

    impl Backend for Arc<Fake> {
        fn ask(&self, title: String, body: String, options: Vec<String>) -> BoxFuture<i32> {
            self.order.lock().unwrap().push("ask");
            self.asked.lock().unwrap().push((title, body, options));
            let answer = self.answer;
            Box::pin(async move { answer })
        }
        fn upload(
            &self,
            _toolkit: String,
            _slug: String,
            file: LocalFile,
        ) -> BoxFuture<Result<Value, String>> {
            self.order.lock().unwrap().push("upload");
            self.uploads.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Ok(json!({"name": file.name, "s3key": "k/1"})) })
        }
    }

    fn send_with_file(path: &str) -> Value {
        json!({"tools": [{"tool_slug": "GMAIL_SEND_EMAIL", ACCOUNT_FIELD: "work", "arguments": {"recipient_email": "a@example.com", "body": "Report attached", "attachment": path}}]})
    }

    #[tokio::test]
    async fn files_upload_only_after_approval() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("report.pdf");
        std::fs::write(&file, "pdf").unwrap();
        let mut world = World::new(two_gmail(), Apps::All);
        world.places.roots = vec![dir.path().to_path_buf()];
        let args = send_with_file(&file.to_string_lossy());
        let ask = |_: &[Checked]| Approval::Ask(vec!["Allow once".into(), "Deny".into()]);
        // Declined: nothing uploaded.
        let fake = Fake::new(1);
        let stop = prepare(
            &args,
            &world.ctx(),
            ask,
            |_, i| i == 0,
            Some("Writer"),
            &fake,
        )
        .await
        .unwrap_err();
        assert_eq!(
            stop,
            Stop::Declined(Some("The user declined this tool call.".into()))
        );
        assert_eq!(fake.uploads.load(Ordering::SeqCst), 0);
        let (title, body, options) = fake.asked.lock().unwrap()[0].clone();
        assert_eq!(
            title,
            "Send on your behalf? Writer wants Gmail (work) to run GMAIL_SEND_EMAIL"
        );
        assert!(
            body.contains("report.pdf") && body.contains("3 bytes"),
            "the card lists the file: {body}"
        );
        assert!(body.contains("a@example.com"), "{body}");
        assert_eq!(options, ["Allow once", "Deny"]);
        // Allowed: asked first, then uploaded, and the call carries the upload.
        let fake = Fake::new(0);
        let (out, _) = prepare(
            &args,
            &world.ctx(),
            ask,
            |_, i| i == 0,
            Some("Writer"),
            &fake,
        )
        .await
        .unwrap();
        assert_eq!(*fake.order.lock().unwrap(), ["ask", "upload"]);
        assert_eq!(out["tools"][0]["arguments"]["attachment"]["s3key"], "k/1");
        assert_eq!(out["tools"][0][ACCOUNT_FIELD], "ca_w");
        // A dropped call: no answer, no upload.
        let fake = Fake::new(-1);
        assert_eq!(
            prepare(&args, &world.ctx(), ask, |_, i| i == 0, None, &fake)
                .await
                .unwrap_err(),
            Stop::Declined(None)
        );
        assert_eq!(fake.uploads.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_refused_file_stops_the_call_before_any_card() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".ssh")).unwrap();
        let key = dir.path().join(".ssh").join("id_rsa");
        std::fs::write(&key, "private").unwrap();
        let mut world = World::new(two_gmail(), Apps::All);
        world.places.roots = vec![dir.path().to_path_buf()];
        let fake = Fake::new(0);
        let stop = prepare(
            &send_with_file(&key.to_string_lossy()),
            &world.ctx(),
            |_| Approval::Ask(vec!["Allow once".into()]),
            |_, _| true,
            None,
            &fake,
        )
        .await
        .unwrap_err();
        let Stop::Refused(text) = stop else { panic!() };
        assert!(text.contains("hidden"), "{text}");
        assert!(fake.asked.lock().unwrap().is_empty());
        assert_eq!(fake.uploads.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn search_results_lose_tools_a_limited_mate_may_not_use() {
        let mut grants = BTreeMap::new();
        grants.insert(
            "gmail".to_string(),
            Grant {
                tools: Tools::These(vec!["GMAIL_FETCH_EMAILS".into()]),
                accounts: None,
            },
        );
        let apps = Apps::Only(grants);
        let result = json!({"data": {"results": [
            {"tool_slug": "GMAIL_FETCH_EMAILS"}, {"tool_slug": "GMAIL_SEND_EMAIL"}, {"tool_slug": "GITHUB_LIST_ISSUES"}
        ], "tool_schemas": {"GMAIL_FETCH_EMAILS": {}, "GMAIL_SEND_EMAIL": {}}}});
        let out = filter_search(&result, &apps, &["gmail".into(), "github".into()]);
        assert_eq!(out["data"]["results"].as_array().unwrap().len(), 1);
        assert!(out["data"]["tool_schemas"]
            .get("GMAIL_SEND_EMAIL")
            .is_none());
        assert_eq!(filter_search(&result, &Apps::All, &[]), result);
    }

    fn live(name: &str) -> Value {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/connected_apps/testdata/live")
            .join(name);
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    // T1: the shapes recorded from Composio.
    #[test]
    fn a_forwarded_call_fits_composios_recorded_schema() {
        let tools = live("mcp_tools_list.json");
        let schema = tools["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == MULTI_EXECUTE)
            .unwrap()["inputSchema"]
            .clone();
        let validator = jsonschema::draft7::new(&schema).unwrap();
        let world = World::new(two_gmail(), Apps::All);
        let agent = fetch(Some("work"));
        assert!(
            validator.is_valid(&agent),
            "the agent's call passes the engine's check"
        );
        let checked = check(&agent, &world.ctx()).unwrap();
        let out = forwarded(&agent, &checked, &HashMap::new());
        assert!(validator.is_valid(&out), "{out}");
        assert_eq!(out["tools"][0]["account"], "ca_w");
        let wrong = json!({"tools": [{"tool_slug": "GMAIL_FETCH_EMAILS", "arguments": {}, "connected_account_id": "ca_w"}]});
        assert!(
            !validator.is_valid(&wrong),
            "Composio's items take no other field"
        );
    }

    #[test]
    fn the_recorded_search_is_filtered_for_a_limited_mate() {
        let result = live("mcp_search_tools.json");
        let data = &result["content"][0]["text"];
        assert!(data["data"]["tool_schemas"]
            .get("GOOGLEDRIVE_ADD_FILE_SHARING_PREFERENCE")
            .is_some());
        let apps = Apps::of_mate(Some(&vec![
            json!({"service": "composio:gmail", "tools": ["GMAIL_FETCH_EMAILS"]}),
        ]));
        let out = filter_search(data, &apps, &["gmail".into(), "googledrive".into()]);
        let text = out.to_string();
        assert!(
            !text.contains("GOOGLEDRIVE_ADD_FILE_SHARING_PREFERENCE"),
            "{text}"
        );
        assert!(
            !text.contains("GMAIL_SEND_EMAIL\""),
            "only the tools it may use"
        );
        assert!(text.contains("GMAIL_FETCH_EMAILS"));
        for status in out["data"]["toolkit_connection_statuses"]
            .as_array()
            .unwrap()
        {
            assert_eq!(
                status["toolkit"].as_str().unwrap().to_ascii_lowercase(),
                "gmail"
            );
        }
        // The schemas it describes are learned by the gate as input_schema.
        assert!(data["data"]["tool_schemas"]["GMAIL_FETCH_EMAILS"]["input_schema"].is_object());
    }

    #[test]
    fn a_mate_on_only_these_cannot_use_personal() {
        let apps = Apps::of_mate(Some(&vec![
            json!({"service": "composio:gmail", "tools": "all", "accounts": ["work"]}),
        ]));
        let world = World::new(two_gmail(), apps);
        let Err(Refusal::Text(text)) = check(&fetch(Some("personal")), &world.ctx()) else {
            panic!("refused")
        };
        assert!(
            text.contains("not one you may use") && text.contains("work"),
            "{text}"
        );
        assert!(
            !text.contains("personal,"),
            "only the accounts it may use are offered: {text}"
        );
        // Its one account is used without naming it.
        assert_eq!(
            check(&fetch(None), &world.ctx()).unwrap()[0].account.label,
            "work"
        );
        // Other services' tools are not its own.
        let args = json!({"tools": [{"tool_slug": "GITHUB_LIST_ISSUES", "arguments": {}}]});
        let mut accounts = two_gmail();
        accounts.push(account("ca_g", "github", "personal", Status::Active));
        let world = World::new(accounts, world.apps.clone());
        assert_eq!(
            check(&args, &world.ctx()).unwrap_err(),
            Refusal::Connect {
                toolkit: "github".into(),
                reason: Need::NotAllowed
            }
        );
    }

    #[test]
    fn a_new_service_reaches_mates_on_all() {
        let notion = vec![account("ca_n", "notion", "personal", Status::Active)];
        let mut tools = known_tools();
        tools.insert(
            "NOTION_SEARCH".into(),
            json!({"slug": "NOTION_SEARCH", "input_parameters": {"type": "object"}}),
        );
        let args = json!({"tools": [{"tool_slug": "NOTION_SEARCH", "arguments": {}}]});
        let mut world = World::new(notion.clone(), Apps::of_mate(None));
        world.tools = tools.clone();
        assert!(
            check(&args, &world.ctx()).is_ok(),
            "All connected apps: a service connected later is its"
        );
        let mut world = World::new(
            notion,
            Apps::of_mate(Some(&vec![json!({"service": "composio:gmail"})])),
        );
        world.tools = tools;
        assert_eq!(
            check(&args, &world.ctx()).unwrap_err(),
            Refusal::Connect {
                toolkit: "notion".into(),
                reason: Need::NotAllowed
            },
            "Only these: not until the person adds it"
        );
        // From a team file: the services it names, each off.
        let named = named_services(&["Gmail".into(), "slack".into(), "gmail".into()]);
        assert_eq!(
            named,
            vec![
                json!({"service": "composio:gmail", "tools": "none"}),
                json!({"service": "composio:slack", "tools": "none"})
            ]
        );
        assert!(!Apps::of_mate(Some(&named)).service_allowed("gmail"));
        assert!(
            Apps::of_mate(Some(&vec![])).limited(),
            "a gallery role's mate starts with none"
        );
    }

    #[test]
    fn mate_apps_read_from_its_setting() {
        assert_eq!(Apps::of_mate(None), Apps::All);
        let apps = Apps::of_mate(Some(&vec![
            json!({"service": "composio:gmail", "tools": ["GMAIL_FETCH_EMAILS"], "accounts": ["work"]}),
            json!({"service": "composio:slack", "tools": "none"}),
            json!({"service": "github"}),
        ]));
        assert!(apps.allows("gmail", "GMAIL_FETCH_EMAILS"));
        assert!(!apps.allows("gmail", "GMAIL_SEND_EMAIL"));
        assert!(!apps.allows("slack", "SLACK_POST_MESSAGE"));
        assert!(!apps.allows("notion", "NOTION_CREATE_PAGE"));
        assert!(apps.limited());
    }
}
