//! The code tool's interpreter: one QuickJS runtime per program, with no
//! standard library, no modules and no host functions except the tool bridge
//! (docs/features/code-mode/spec.md, FR-9, FR-13, NFR-1, NFR-4;
//! ADR 0025; plan 3.5).
//!
//! [`run`] is blocking: the code tool calls it on a blocking worker, so a busy
//! loop never starves the engine's async runtime. Each tool call the program
//! makes goes out as a [`Call`] on `calls`; its reply comes back on a channel
//! the worker owns and settles the JavaScript promise the call returned.
//! Between replies the worker runs QuickJS's job queue. The interrupt handler
//! stops the program when its time is up or when it is cancelled, and the
//! runtime's memory limit stops a program that allocates without bound.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rquickjs::{
    context::EvalOptions, CatchResultExt, CaughtError, Context, Ctx, Exception, Function,
    Persistent, Promise, Runtime, Value,
};
use serde_json::Value as Json;
use tokio_util::sync::CancellationToken;

/// The file name programs run under, as it appears in errors and stacks.
const FILE: &str = "code.js";
/// How often the worker wakes while waiting for a reply, to notice time and
/// cancellation.
const TICK: Duration = Duration::from_millis(20);

/// Hard limits for one program (FR-13).
#[derive(Clone, Debug)]
pub struct Limits {
    /// The runtime's memory limit.
    pub memory_bytes: usize,
    /// Running time, not counting time [`Program::paused`] reports.
    pub time: Duration,
    /// Tool calls one program may start.
    pub max_calls: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            memory_bytes: 64 * 1024 * 1024,
            time: Duration::from_secs(10 * 60),
            max_calls: 128,
        }
    }
}

/// One tool call a program started.
#[derive(Debug)]
pub struct Call {
    pub id: u64,
    pub name: String,
    pub args: Json,
    /// Where the reply goes: the call's id and its outcome.
    pub reply: mpsc::Sender<(u64, Reply)>,
}

/// A call's outcome: a JSON value for the program (the bridge's shape), or an
/// error message the call's promise rejects with.
pub type Reply = Result<Json, String>;

/// Which limit stopped a program.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Limit {
    Time,
    Memory,
    Calls,
    Cancelled,
}

impl Limit {
    pub fn message(self, limits: &Limits) -> String {
        match self {
            Limit::Time => format!(
                "The program ran out of time ({} s limit).",
                limits.time.as_secs()
            ),
            Limit::Memory => format!(
                "The program ran out of memory ({} MB limit).",
                limits.memory_bytes / (1024 * 1024)
            ),
            Limit::Calls => format!(
                "The program reached the limit of {} tool calls.",
                limits.max_calls
            ),
            Limit::Cancelled => "The program was stopped.".into(),
        }
    }
}

/// What a program produced.
#[derive(Debug, Default)]
pub struct Outcome {
    /// The value of the last expression, as JSON (`None` for `undefined`).
    pub value: Option<Json>,
    pub stdout: Vec<String>,
    pub stderr: Vec<String>,
    /// Why it failed, with its line when known.
    pub error: Option<ProgramError>,
    /// Tool calls it started.
    pub calls: usize,
    /// The limit that stopped it, if one did.
    pub limit: Option<Limit>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramError {
    pub message: String,
    pub line: Option<u32>,
}

/// A program and what it may use.
pub struct Program<'a> {
    pub code: &'a str,
    /// JavaScript run before the program, defining its API on top of the
    /// host functions (`__evo_call`, `__evo_log`).
    pub prelude: &'a str,
    pub limits: Limits,
    pub cancel: CancellationToken,
    /// Time the program spent waiting on the person (approval prompts), which
    /// does not count toward [`Limits::time`] (FR-11).
    pub paused: Arc<dyn Fn() -> Duration + Send + Sync>,
    /// Where tool calls go.
    pub calls: tokio::sync::mpsc::UnboundedSender<Call>,
}

/// Shared between the worker loop and the host functions.
struct Shared {
    next_id: u64,
    calls: usize,
    pending: HashMap<u64, (Persistent<Function<'static>>, Persistent<Function<'static>>)>,
    stdout: Vec<String>,
    stderr: Vec<String>,
    limit: Option<Limit>,
}

/// Run `program` to completion. Blocking.
pub fn run(program: Program<'_>) -> Outcome {
    let runtime = match Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => return failed(format!("Could not start the interpreter: {error}")),
    };
    runtime.set_memory_limit(program.limits.memory_bytes);
    runtime.set_max_stack_size(1024 * 1024);
    let started = Instant::now();
    let interrupt = Rc::new(RefCell::new(None::<Limit>));
    {
        let cancel = program.cancel.clone();
        let paused = program.paused.clone();
        let time = program.limits.time;
        let interrupt = interrupt.clone();
        runtime.set_interrupt_handler(Some(Box::new(move || {
            let hit = over_limits(&cancel, paused.as_ref(), time, started);
            if let Some(limit) = hit {
                interrupt.borrow_mut().get_or_insert(limit);
            }
            hit.is_some()
        })));
    }
    let context = match Context::full(&runtime) {
        Ok(context) => context,
        Err(error) => return failed(format!("Could not start the interpreter: {error}")),
    };
    let (reply_tx, reply_rx) = mpsc::channel::<(u64, Reply)>();
    let shared = Rc::new(RefCell::new(Shared {
        next_id: 1,
        calls: 0,
        pending: HashMap::new(),
        stdout: Vec::new(),
        stderr: Vec::new(),
        limit: None,
    }));

    let mut outcome = context.with(|ctx| {
        if let Err(error) = install_host(&ctx, &shared, &program, reply_tx) {
            return failed(format!("Could not start the interpreter: {error}"));
        }
        let mut prelude_options = EvalOptions::default();
        prelude_options.filename = Some("prelude.js".into());
        if let Err(error) = ctx
            .eval_with_options::<(), _>(program.prelude, prelude_options)
            .catch(&ctx)
        {
            return failed(format!("The code tool's prelude failed: {error}"));
        }
        let compile = |code: &str| {
            let mut options = EvalOptions::default();
            options.promise = true;
            options.filename = Some(FILE.into());
            ctx.eval_with_options::<Promise, _>(code, options)
                .catch(&ctx)
        };
        let promise: Promise = match compile(program.code) {
            Ok(promise) => promise,
            Err(error) => {
                // A program may `return` its result at the top level: run it
                // as the body of an async function, on the same lines, so its
                // returned value is the result (docs/features/code-mode/spec.md, FR-8).
                let outcome = error_outcome(&ctx, error);
                let top_level_return = outcome
                    .error
                    .as_ref()
                    .is_some_and(|e| e.message.contains("return not in a function"));
                if !top_level_return {
                    return outcome;
                }
                match compile(&format!("await (async () => {{{}\n}})()", program.code)) {
                    Ok(promise) => promise,
                    Err(error) => return error_outcome(&ctx, error),
                }
            }
        };
        drive(
            &ctx, &promise, &shared, &program, &reply_rx, &interrupt, started,
        )
    });

    // Persistent handles must go before the runtime does.
    let mut state = shared.borrow_mut();
    state.pending.clear();
    outcome.stdout = std::mem::take(&mut state.stdout);
    outcome.stderr = std::mem::take(&mut state.stderr);
    outcome.calls = state.calls;
    let limit = interrupt.borrow().or(state.limit).or_else(|| {
        outcome
            .error
            .as_ref()
            .filter(|e| e.message.contains("out of memory"))
            .map(|_| Limit::Memory)
    });
    if let Some(limit) = limit {
        outcome.limit = Some(limit);
        let message = limit.message(&program.limits);
        outcome.error = Some(ProgramError {
            line: outcome.error.as_ref().and_then(|e| e.line),
            message,
        });
        outcome.value = None;
    }
    outcome
}

/// Cancelled, or out of running time (paused time excluded).
fn over_limits(
    cancel: &CancellationToken,
    paused: &(dyn Fn() -> Duration + Send + Sync),
    time: Duration,
    started: Instant,
) -> Option<Limit> {
    if cancel.is_cancelled() {
        Some(Limit::Cancelled)
    } else if started.elapsed().saturating_sub(paused()) > time {
        Some(Limit::Time)
    } else {
        None
    }
}

fn over(program: &Program<'_>, started: Instant) -> Option<Limit> {
    over_limits(
        &program.cancel,
        program.paused.as_ref(),
        program.limits.time,
        started,
    )
}

fn failed(message: String) -> Outcome {
    Outcome {
        error: Some(ProgramError {
            message,
            line: None,
        }),
        ..Default::default()
    }
}

/// `__evo_call(name, argsJson) -> Promise<replyJson>` and
/// `__evo_log(stream, text)`: the only host functions a program has.
fn install_host<'js>(
    ctx: &Ctx<'js>,
    shared: &Rc<RefCell<Shared>>,
    program: &Program<'_>,
    reply_tx: mpsc::Sender<(u64, Reply)>,
) -> rquickjs::Result<()> {
    let globals = ctx.globals();
    let call_shared = shared.clone();
    let calls = program.calls.clone();
    let max_calls = program.limits.max_calls;
    let call = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'js>, name: String, args: String| -> rquickjs::Result<Promise<'js>> {
            let args: Json = serde_json::from_str(&args)
                .map_err(|e| Exception::throw_type(&ctx, &format!("arguments: {e}")))?;
            let mut state = call_shared.borrow_mut();
            if state.calls >= max_calls {
                state.limit.get_or_insert(Limit::Calls);
                return Err(Exception::throw_range(
                    &ctx,
                    &format!("The program reached the limit of {max_calls} tool calls."),
                ));
            }
            state.calls += 1;
            let id = state.next_id;
            state.next_id += 1;
            let (promise, resolve, reject) = ctx.promise()?;
            state.pending.insert(
                id,
                (
                    Persistent::save(&ctx, resolve),
                    Persistent::save(&ctx, reject),
                ),
            );
            let sent = calls.send(Call {
                id,
                name,
                args,
                reply: reply_tx.clone(),
            });
            if sent.is_err() {
                state.pending.remove(&id);
                return Err(Exception::throw_internal(&ctx, "the code tool has stopped"));
            }
            Ok(promise)
        },
    )?;
    globals.set("__evo_call", call)?;
    let log_shared = shared.clone();
    let log = Function::new(ctx.clone(), move |stream: i32, text: String| {
        let mut state = log_shared.borrow_mut();
        if stream == 2 {
            state.stderr.push(text);
        } else {
            state.stdout.push(text);
        }
    })?;
    globals.set("__evo_log", log)?;
    Ok(())
}

/// Run jobs and settle replies until `promise` settles, the program can no
/// longer progress, or a limit stops it.
fn drive<'js>(
    ctx: &Ctx<'js>,
    promise: &Promise<'js>,
    shared: &Rc<RefCell<Shared>>,
    program: &Program<'_>,
    replies: &mpsc::Receiver<(u64, Reply)>,
    interrupt: &Rc<RefCell<Option<Limit>>>,
    started: Instant,
) -> Outcome {
    loop {
        while ctx.execute_pending_job() {}
        match promise.result::<Value>() {
            Some(Ok(completion)) => return value_outcome(ctx, completion),
            Some(Err(_)) => {
                let thrown = ctx.catch();
                return thrown_outcome(ctx, thrown);
            }
            None => {}
        }
        if interrupt.borrow().is_some() {
            return Outcome::default();
        }
        if shared.borrow().pending.is_empty() {
            return failed(
                "The program is waiting for a promise that nothing will settle \
                 (no tool call is running)."
                    .into(),
            );
        }
        let reply = match replies.recv_timeout(TICK) {
            Ok(reply) => reply,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Let the interrupt handler see time and cancellation even
                // while no JavaScript runs.
                if let Some(limit) = over(program, started) {
                    interrupt.borrow_mut().get_or_insert(limit);
                    return Outcome::default();
                }
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return failed("The code tool stopped before the program finished.".into())
            }
        };
        let (id, reply) = reply;
        let handles = shared.borrow_mut().pending.remove(&id);
        let Some((resolve, reject)) = handles else {
            continue;
        };
        let settled = match reply {
            Ok(value) => resolve
                .restore(ctx)
                .and_then(|f| f.call::<_, ()>((value.to_string(),))),
            Err(message) => reject.restore(ctx).and_then(|f| {
                let error = Exception::from_message(ctx.clone(), &message)?;
                f.call::<_, ()>((error,))
            }),
        };
        if settled.is_err() {
            let _ = ctx.catch();
        }
    }
}

/// The completion object of an async eval is `{ value }`.
fn value_outcome<'js>(ctx: &Ctx<'js>, completion: Value<'js>) -> Outcome {
    let value = completion
        .as_object()
        .and_then(|o| o.get::<_, Value>("value").ok())
        .unwrap_or(completion);
    Outcome {
        value: to_json(ctx, value),
        ..Default::default()
    }
}

/// A value as JSON: strings as they are, `undefined` and functions as none.
pub(crate) fn to_json<'js>(ctx: &Ctx<'js>, value: Value<'js>) -> Option<Json> {
    if value.is_undefined() {
        return None;
    }
    if let Some(text) = value.as_string() {
        return text.to_string().ok().map(Json::String);
    }
    match ctx.json_stringify(value).catch(ctx) {
        Ok(Some(text)) => text
            .to_string()
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok()),
        Ok(None) => None,
        Err(error) => Some(Json::String(format!("[value could not be shown: {error}]"))),
    }
}

fn error_outcome<'js>(ctx: &Ctx<'js>, error: CaughtError<'js>) -> Outcome {
    match error {
        CaughtError::Exception(exception) => thrown_outcome(ctx, exception.into_value()),
        CaughtError::Value(value) => thrown_outcome(ctx, value),
        CaughtError::Error(error) => failed(error.to_string()),
    }
}

/// What a thrown value says, and the line of the program it came from.
fn thrown_outcome<'js>(_ctx: &Ctx<'js>, thrown: Value<'js>) -> Outcome {
    let (message, stack) = match thrown.as_exception() {
        Some(exception) => {
            let name = exception
                .get::<_, String>("name")
                .unwrap_or_else(|_| "Error".into());
            let message = exception.message().unwrap_or_default();
            (
                if message.is_empty() {
                    name
                } else {
                    format!("{name}: {message}")
                },
                exception.stack(),
            )
        }
        None => (
            thrown
                .as_string()
                .and_then(|s| s.to_string().ok())
                .or_else(|| thrown.as_number().map(|n| n.to_string()))
                .unwrap_or_else(|| "a non-Error value was thrown".into()),
            None,
        ),
    };
    Outcome {
        error: Some(ProgramError {
            line: stack.as_deref().and_then(line_in_program),
            message,
        }),
        ..Default::default()
    }
}

/// The first `code.js:LINE` in a stack.
fn line_in_program(stack: &str) -> Option<u32> {
    let at = stack.find(&format!("{FILE}:"))? + FILE.len() + 1;
    stack[at..]
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
