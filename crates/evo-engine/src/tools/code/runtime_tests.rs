//! The interpreter on its own, with a fake host for tool calls
//! (docs/features/code-mode/spec.md, FR-9, FR-13, NFR-1, NFR-4).

use super::*;
use serde_json::json;

/// Just enough API for these tests; the real prelude has its own tests.
const PRELUDE: &str = r#"
globalThis.console = { log: (...a) => __evo_log(1, a.join(" ")), error: (...a) => __evo_log(2, a.join(" ")) };
globalThis.call = async (name, args) => JSON.parse(await __evo_call(name, JSON.stringify(args ?? {})));
"#;

/// How the fake host answers a call.
type Answer = Arc<dyn Fn(&str, &Json) -> Reply + Send + Sync>;

fn echo() -> Answer {
    Arc::new(|name, args| Ok(json!({ "tool": name, "args": args })))
}

struct Run {
    limits: Limits,
    cancel: CancellationToken,
    paused: Arc<dyn Fn() -> Duration + Send + Sync>,
    answer: Answer,
    /// Answer calls in reverse order of arrival, in batches of this size.
    reverse_batch: usize,
}

impl Default for Run {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            cancel: CancellationToken::new(),
            paused: Arc::new(|| Duration::ZERO),
            answer: echo(),
            reverse_batch: 1,
        }
    }
}

fn run_code(code: &str, setup: Run) -> Outcome {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Call>();
    let answer = setup.answer.clone();
    let batch = setup.reverse_batch;
    let host = std::thread::spawn(move || {
        let mut held = Vec::new();
        while let Some(call) = rx.blocking_recv() {
            held.push(call);
            if held.len() >= batch {
                for call in held.drain(..).rev() {
                    let reply = answer(&call.name, &call.args);
                    let _ = call.reply.send((call.id, reply));
                }
            }
        }
    });
    let outcome = run(Program {
        code,
        prelude: PRELUDE,
        limits: setup.limits,
        cancel: setup.cancel,
        paused: setup.paused,
        calls: tx,
    });
    host.join().unwrap();
    outcome
}

fn ok(code: &str) -> Option<Json> {
    let outcome = run_code(code, Run::default());
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    outcome.value
}

#[test]
fn the_last_expression_is_the_result() {
    assert_eq!(ok("1 + 2"), Some(json!(3)));
    assert_eq!(ok("const a = 5;\na * 2"), Some(json!(10)));
    assert_eq!(ok("'text'"), Some(json!("text")));
    assert_eq!(ok("({ count: 3 })"), Some(json!({ "count": 3 })));
    assert_eq!(ok("await Promise.resolve(7)"), Some(json!(7)));
    assert_eq!(ok("undefined"), None);
}

#[test]
fn console_output_is_kept_apart() {
    let outcome = run_code("console.log('a', 1); console.error('b'); 0", Run::default());
    assert_eq!(outcome.stdout, vec!["a 1".to_string()]);
    assert_eq!(outcome.stderr, vec!["b".to_string()]);
}

// FR-9: no files, network, environment, processes or modules.
#[test]
fn a_program_has_no_ambient_abilities() {
    let kinds = ok(
        "[typeof require, typeof process, typeof fetch, typeof Deno, typeof std, typeof os, \
         typeof setTimeout, typeof XMLHttpRequest, typeof WebAssembly, typeof scriptArgs]",
    )
    .unwrap();
    assert!(
        kinds.as_array().unwrap().iter().all(|k| k == "undefined"),
        "{kinds}"
    );
    let import = run_code("await import('fs')", Run::default());
    assert!(import.error.is_some(), "dynamic import must fail");
    let static_import = run_code("import fs from 'fs';\n1", Run::default());
    assert!(static_import.error.is_some(), "static import must fail");
}

#[test]
fn a_syntax_error_runs_nothing_and_names_its_line() {
    let outcome = run_code("call('a');\nlet x = ;", Run::default());
    let error = outcome.error.expect("error");
    assert!(error.message.contains("SyntaxError"), "{error:?}");
    assert_eq!(error.line, Some(2));
    assert_eq!(outcome.calls, 0);
}

#[test]
fn a_throw_halfway_keeps_finished_calls_and_names_its_line() {
    let outcome = run_code(
        "await call('first');\nawait call('second');\nthrow new Error('boom');",
        Run::default(),
    );
    let error = outcome.error.expect("error");
    assert_eq!(error.message, "Error: boom");
    assert_eq!(error.line, Some(3));
    assert_eq!(outcome.calls, 2);
}

#[test]
fn tool_replies_reach_the_program_even_out_of_order() {
    let setup = Run {
        reverse_batch: 3,
        ..Default::default()
    };
    let outcome = run_code(
        "const r = await Promise.all(['a', 'b', 'c'].map(n => call(n, { n })));\nr.map(x => x.tool).join('')",
        setup,
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert_eq!(outcome.value, Some(json!("abc")));
    assert_eq!(outcome.calls, 3);
}

#[test]
fn a_failed_call_rejects_and_can_be_caught() {
    let setup = Run {
        answer: Arc::new(|name, _| {
            if name == "bad" {
                Err("blocked by a hook".into())
            } else {
                Ok(json!("fine"))
            }
        }),
        ..Default::default()
    };
    let outcome = run_code(
        "let caught = '';\ntry { await call('bad') } catch (e) { caught = e.message }\n[caught, await call('good')]",
        setup,
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert_eq!(outcome.value, Some(json!(["blocked by a hook", "fine"])));
}

#[test]
fn waiting_on_nothing_is_an_error() {
    let outcome = run_code("await new Promise(() => {})", Run::default());
    let error = outcome.error.expect("error");
    assert!(error.message.contains("nothing will settle"), "{error:?}");
}

// FR-13, NFR-4: a busy loop is stopped at the time limit.
#[test]
fn a_busy_loop_stops_at_the_time_limit() {
    let setup = Run {
        limits: Limits {
            time: Duration::from_millis(200),
            ..Limits::default()
        },
        ..Default::default()
    };
    let started = Instant::now();
    let outcome = run_code("while (true) {}", setup);
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(outcome.limit, Some(Limit::Time));
    assert!(outcome.error.unwrap().message.contains("ran out of time"));
}

// FR-11: time spent waiting on the person does not count.
#[test]
fn paused_time_does_not_count() {
    let setup = Run {
        limits: Limits {
            time: Duration::from_millis(100),
            ..Limits::default()
        },
        paused: Arc::new(|| Duration::from_secs(60)),
        ..Default::default()
    };
    let outcome = run_code(
        "const t = Date.now(); while (Date.now() - t < 300) {}\n'done'",
        setup,
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert_eq!(outcome.value, Some(json!("done")));
}

// FR-13, NFR-4: runaway allocation is stopped at the memory limit.
#[test]
fn runaway_memory_stops_at_the_memory_limit() {
    let setup = Run {
        limits: Limits {
            memory_bytes: 16 * 1024 * 1024,
            ..Limits::default()
        },
        ..Default::default()
    };
    let outcome = run_code(
        "const keep = []; while (true) { keep.push(new Array(100000).fill('x')) }",
        setup,
    );
    assert_eq!(outcome.limit, Some(Limit::Memory), "{:?}", outcome.error);
}

#[test]
fn the_call_cap_stops_the_program() {
    let setup = Run {
        limits: Limits {
            max_calls: 3,
            ..Limits::default()
        },
        ..Default::default()
    };
    let outcome = run_code("for (let i = 0; i < 10; i++) { await call('x') }", setup);
    assert_eq!(outcome.limit, Some(Limit::Calls));
    assert_eq!(outcome.calls, 3);
    assert!(outcome.error.unwrap().message.contains("3 tool calls"));
}

// FR-14: cancelling stops a program that is running or waiting.
#[test]
fn cancelling_stops_the_program() {
    let cancel = CancellationToken::new();
    let later = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        later.cancel();
    });
    let busy = run_code(
        "while (true) {}",
        Run {
            cancel,
            ..Default::default()
        },
    );
    assert_eq!(busy.limit, Some(Limit::Cancelled));

    // Waiting on a call that never answers.
    let cancel = CancellationToken::new();
    let later = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        later.cancel();
    });
    let waiting = run_code(
        "await call('slow')",
        Run {
            cancel,
            reverse_batch: 2,
            ..Default::default()
        },
    );
    assert_eq!(waiting.limit, Some(Limit::Cancelled));
}

// NFR-4: a rejection nobody handles does not crash the run.
#[test]
fn an_unhandled_rejection_does_not_crash() {
    assert_eq!(ok("Promise.reject(new Error('x'));\n1"), Some(json!(1)));
}

// NFR-1: starting a program is fast.
#[test]
fn a_program_starts_quickly() {
    let started = Instant::now();
    assert_eq!(ok("1"), Some(json!(1)));
    let took = started.elapsed();
    // Generous for debug builds on CI; release is a few milliseconds.
    assert!(took < Duration::from_millis(500), "{took:?}");
}

#[test]
fn line_numbers_come_from_the_program_file() {
    assert_eq!(
        line_in_program("Error\n    at <eval> (code.js:12:3)\n"),
        Some(12)
    );
    assert_eq!(line_in_program("at prelude.js:4:1"), None);
}

// FR-8: a program may `return` its result at the top level; lines stay the same.
#[test]
fn a_top_level_return_is_the_result() {
    assert_eq!(ok("const x = 2;\nreturn x * 3;"), Some(json!(6)));
    let outcome = run_code(
        "await call('a');\nif (true) throw new Error('late');\nreturn 1;",
        Run::default(),
    );
    let error = outcome.error.expect("error");
    assert_eq!(error.message, "Error: late");
    assert_eq!(error.line, Some(2));
}
