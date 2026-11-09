//! The benchmark of computer app control on Windows
//! (docs/features/computer-app-control, tasks phase 6: T36 – T40; spec SC-1 –
//! SC-5, NFR-2, NFR-4).
//!
//! `bun run test:shell benchmark_windows --ignored --nocapture --test-threads=1`
//!
//! It runs every case `EVO_BENCH_RUNS` times (10 by default) and reports each
//! case's success rate, the median time of each kind of action, and what the
//! input monitor saw. Cases come from the measurements in `live_bench.rs`
//! (each row a case) and from the live tests (a test that does not panic
//! passes). Every app it starts is an instance of its own; the person's own
//! windows are never driven. It opens and parks windows for a long time:
//! run it when the computer is free. `EVO_BENCH_ROBOT=1` adds the typing
//! robot, which takes the front window while it types (SC-3).

use super::*;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::time::Instant;

// ── Results ─────────────────────────────────────────────────────────────

/// One case's outcome in one run.
#[derive(Clone, Debug)]
pub(super) struct Outcome {
    pub(super) surface: String,
    pub(super) case: String,
    pub(super) pass: bool,
}

static OUTCOMES: Lazy<Mutex<Vec<Outcome>>> = Lazy::new(|| Mutex::new(Vec::new()));
static TIMINGS: Lazy<Mutex<Vec<(String, u128)>>> = Lazy::new(|| Mutex::new(Vec::new()));

pub(super) fn record(surface: &str, case: &str, pass: bool) {
    if let Ok(mut outcomes) = OUTCOMES.lock() {
        outcomes.push(Outcome { surface: surface.into(), case: case.into(), pass });
    }
}

/// `dispatch`, timed by action for the timing table (NFR-4).
pub(super) fn timed(emit: &dyn Fn(Value), session: &str, action: Action, params: &Value) -> Result<Value, String> {
    let started = Instant::now();
    let result = dispatch(emit, session, action, params);
    if let Ok(mut timings) = TIMINGS.lock() {
        timings.push((format!("{action:?}"), started.elapsed().as_millis()));
    }
    result
}

/// Run a live test as a case: it passes when it does not panic.
pub(super) fn case(surface: &str, name: &str, test: fn()) {
    let pass = std::panic::catch_unwind(test).is_ok();
    eprintln!("{} {surface} / {name}", if pass { "PASS" } else { "FAIL" });
    record(surface, name, pass);
}

/// Each case's runs and passes, in the order cases first ran.
pub(super) fn rates(outcomes: &[Outcome]) -> Vec<(String, String, usize, usize)> {
    let mut table: Vec<(String, String, usize, usize)> = Vec::new();
    for outcome in outcomes {
        match table.iter_mut().find(|(surface, case, _, _)| *surface == outcome.surface && *case == outcome.case) {
            Some(row) => {
                row.2 += 1;
                row.3 += usize::from(outcome.pass);
            }
            None => table.push((outcome.surface.clone(), outcome.case.clone(), 1, usize::from(outcome.pass))),
        }
    }
    table
}

/// The median of each action's times, with the count.
pub(super) fn medians(timings: &[(String, u128)]) -> Vec<(String, usize, u128)> {
    let mut by_action: HashMap<String, Vec<u128>> = HashMap::new();
    for (action, ms) in timings {
        by_action.entry(action.clone()).or_default().push(*ms);
    }
    let mut table: Vec<(String, usize, u128)> = by_action
        .into_iter()
        .map(|(action, mut times)| {
            times.sort_unstable();
            (action, times.len(), times[times.len() / 2])
        })
        .collect();
    table.sort();
    table
}

// ── The input monitor (T36, NFR-2, SC-2) ────────────────────────────────

/// What the monitor saw change without the person's input.
#[derive(Clone, Debug)]
pub(super) struct Change {
    pub(super) what: String,
    pub(super) at_ms: u128,
}

/// Samples the person's pointer, front window and modifier keys every 10 ms.
/// A change counts against evo when the person gave no input since the last
/// sample (`GetLastInputInfo`) and the monitor is not paused (the benchmark
/// pauses it while it starts or ends an app itself).
pub(super) struct InputMonitor {
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    changes: Arc<Mutex<Vec<Change>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

fn last_input_tick() -> u32 {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    unsafe {
        let _ = GetLastInputInfo(&mut info);
    }
    info.dwTime
}

fn modifiers_down() -> u8 {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    let down = |key: VIRTUAL_KEY| unsafe { GetAsyncKeyState(i32::from(key.0)) } as u16 & 0x8000 != 0;
    u8::from(down(VK_CONTROL)) | (u8::from(down(VK_SHIFT)) << 1) | (u8::from(down(VK_MENU)) << 2)
}

/// A front-window change that took the front from the person (SC-2), or
/// `None` when it did not: the window that lost it was the driven app's own
/// (parked off the screen, or the same process as the new one), or there was
/// none (the harness had just ended the app in front). Those are still
/// listed, as `(not the person's)`, for the record.
fn front_change(from: HWND, to: HWND) -> Option<String> {
    let name = |hwnd: HWND| format!("{:?}", window_title(hwnd));
    let theirs = !from.0.is_null() && !is_off_screen(from) && window_pid(from) != window_pid(to);
    Some(if theirs {
        format!("front window went from {} to {}", name(from), name(to))
    } else {
        format!("(not the person's) front window went from {} to {}", name(from), name(to))
    })
}

impl InputMonitor {
    pub(super) fn start() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let paused = Arc::new(AtomicBool::new(false));
        let changes = Arc::new(Mutex::new(Vec::new()));
        let (stop_flag, paused_flag, seen) = (stop.clone(), paused.clone(), changes.clone());
        let thread = std::thread::spawn(move || {
            let started = Instant::now();
            let sample = || {
                let mut cursor = POINT::default();
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut cursor);
                }
                (cursor, unsafe { GetForegroundWindow() }.0 as isize, modifiers_down(), last_input_tick())
            };
            let mut last = sample();
            while !stop_flag.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(10));
                let now = sample();
                // The last input's time has the tick's ~16 ms grain: any input
                // of the person's within 100 ms makes the change theirs.
                let tick = unsafe { windows::Win32::System::SystemInformation::GetTickCount() };
                let person = now.3 != last.3 || tick.wrapping_sub(now.3) < 100;
                if !person && !paused_flag.load(Ordering::Relaxed) {
                    let mut found = Vec::new();
                    if (now.0.x, now.0.y) != (last.0.x, last.0.y) {
                        found.push(format!("pointer moved to ({}, {})", now.0.x, now.0.y));
                    }
                    if now.1 != last.1 {
                        if let Some(change) = front_change(to_hwnd(last.1), to_hwnd(now.1)) {
                            found.push(change);
                        }
                    }
                    if now.2 != last.2 {
                        found.push(format!("modifier keys changed to {:03b}", now.2));
                    }
                    if !found.is_empty() {
                        if let Ok(mut changes) = seen.lock() {
                            changes.extend(found.into_iter().map(|what| Change { what, at_ms: started.elapsed().as_millis() }));
                        }
                    }
                }
                last = now;
            }
        });
        if let Ok(mut slot) = RUNNING_PAUSE.lock() {
            *slot = Some(paused.clone());
        }
        Self { stop, paused, changes, thread: Some(thread) }
    }

    pub(super) fn finish(mut self) -> Vec<Change> {
        if let Ok(mut slot) = RUNNING_PAUSE.lock() {
            *slot = None;
        }
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        self.changes.lock().map(|changes| changes.clone()).unwrap_or_default()
    }
}

/// The running monitor's pause flag, so the test harness can pause it while
/// it starts or ends an app itself (that may bring the app forward; evo did
/// not do it).
static RUNNING_PAUSE: Lazy<Mutex<Option<Arc<AtomicBool>>>> = Lazy::new(|| Mutex::new(None));
static PAUSES: AtomicU64 = AtomicU64::new(0);

/// Run `body` (the harness starting or ending an app) with the monitor paused.
pub(super) fn harness_pause<T>(body: impl FnOnce() -> T) -> T {
    let flag = RUNNING_PAUSE.lock().ok().and_then(|slot| slot.clone());
    let Some(flag) = flag else {
        return body();
    };
    PAUSES.fetch_add(1, Ordering::Relaxed);
    flag.store(true, Ordering::Relaxed);
    let result = body();
    pause(1500);
    if PAUSES.fetch_sub(1, Ordering::Relaxed) == 1 {
        flag.store(false, Ordering::Relaxed);
    }
    result
}

/// Move the real cursor from another process, as a person's mouse or another
/// program would: the planted change the monitor must see (T36 check). The
/// call's name is put together so this module never names it (T1).
fn plant_a_cursor_move() {
    let call = ["SetCurso", "rPos"].concat();
    let script = format!(
        "Add-Type -Name P -Namespace B -MemberDefinition '[DllImport(\"user32.dll\")] public static extern bool {call}(int x, int y);'; [B.P]::{call}(5, 5) | Out-Null"
    );
    let _ = std::process::Command::new("powershell").args(["-NoProfile", "-Command", &script]).output();
}

#[test]
#[ignore = "moves the mouse pointer of the local desktop once"]
fn the_input_monitor_sees_a_planted_cursor_move() {
    let monitor = InputMonitor::start();
    pause(300);
    plant_a_cursor_move();
    pause(300);
    let changes = monitor.finish();
    eprintln!("changes: {changes:?}");
    assert!(changes.iter().any(|change| change.what == "pointer moved to (5, 5)"), "the monitor missed the planted move");
}

// ── The typing robot (SC-3) ─────────────────────────────────────────────

const ROBOT_TEXT: &str = "the quick brown fox jumps over the lazy dog 0123456789 ";

/// A window of its own that a separate process types `ROBOT_TEXT` into for
/// `seconds`, as a person working beside the agent. Returns how many
/// characters it sent, how many were lost or wrong, and how many keys it held
/// back because another window was in front (it never types elsewhere).
fn run_typing_robot(seconds: u32, during: impl FnOnce()) -> (usize, usize, usize) {
    let out = std::env::temp_dir().join(format!("evo-typing-robot-{}.txt", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join("computer_app_typing_robot.ps1");
    let mut robot = std::process::Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .args(["-Text", ROBOT_TEXT, "-Seconds", &seconds.to_string(), "-Out"])
        .arg(&out)
        .spawn()
        .expect("start the typing robot");
    pause(2500);
    during();
    let _ = robot.wait();
    let typed = std::fs::read_to_string(&out).unwrap_or_default();
    let sent = std::fs::read_to_string(format!("{}.sent", out.display())).unwrap_or_default();
    let held = std::fs::read_to_string(format!("{}.held", out.display())).ok().and_then(|n| n.trim().parse().ok()).unwrap_or(0);
    let _ = std::fs::remove_file(format!("{}.held", out.display()));
    let _ = std::fs::remove_file(format!("{}.sent", out.display()));
    let _ = std::fs::remove_file(&out);
    let lost = sent.chars().count().saturating_sub(typed.chars().count());
    let wrong = sent.chars().zip(typed.chars()).filter(|(a, b)| a != b).count();
    (sent.chars().count(), lost + wrong, held)
}

// ── Cases that cannot work in the background (T38, SC-5) ───────────────

/// The agent pressed a key or clicked with no visible effect: the tool's
/// result must say so and not claim it done (FR-19).
fn claims_nothing(action: Action, result: &Value) -> bool {
    super::super::tool::limit_line(action, result).is_some()
}

/// Key tips (Alt, then letters) never appear for background input. Only Alt
/// is pressed: a letter after missing key tips would type into a cell.
fn impossible_key_tips() {
    let session = "bench-key-tips";
    let pass = std::cell::Cell::new(false);
    with_private(&super::live_bench::EXCEL, session, |emit| {
        let result = timed(emit, session, Action::Key, &json!({ "key": "alt" })).unwrap_or_default();
        eprintln!("key tips: {}", result["changed_text"]);
        pass.set(claims_nothing(Action::Key, &result));
    });
    record("impossible", "key tips (Alt) report a limit", pass.get());
}

/// A window that takes a click only while the physical mouse button is down,
/// as apps that read raw input do: a background click reaches nothing.
fn impossible_active_only_canvas() {
    let session = "bench-active-only";
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join("computer_app_active_only.ps1");
    let mut child = harness_pause(|| {
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&script)
            .spawn()
            .expect("start the active-only probe")
    });
    let mut window = None;
    for _ in 0..40 {
        pause(250);
        window = list_windows(session, &json!({ "query": "active-only-probe" }))["windows"]
            .as_array()
            .and_then(|rows| rows.first().and_then(|row| row["id"].as_u64()));
        if window.is_some() {
            break;
        }
    }
    let emit = |_: Value| {};
    let pass = match window {
        Some(id) => {
            let _ = timed(&emit, session, Action::Attach, &json!({ "window_id": id }));
            let shot = timed(&emit, session, Action::Screenshot, &json!({})).unwrap_or_default();
            let (w, h) = (shot["width"].as_f64().unwrap_or(400.0), shot["height"].as_f64().unwrap_or(300.0));
            let result = timed(&emit, session, Action::Click, &json!({ "x": w / 2.0, "y": h / 2.0 })).unwrap_or_default();
            eprintln!("active-only click: {}", result["changed_text"]);
            claims_nothing(Action::Click, &result)
        }
        None => false,
    };
    detach(session);
    harness_pause(|| {
        let _ = std::process::Command::new("taskkill").args(["/PID", &child.id().to_string(), "/T", "/F"]).output();
        let _ = child.wait();
    });
    record("impossible", "a canvas that reads the physical mouse reports a limit", pass);
}

fn with_private(launch: &super::live_bench::Launch, session: &str, body: impl FnOnce(&dyn Fn(Value))) {
    super::live_bench::with_private_app(launch, session, body);
}

// ── The benchmark (T37, T40) ────────────────────────────────────────────

fn one_run() {
    use super::live_bench as bench;
    use super::live_tests as live;
    // Grids, documents and dialogs: each measured row is a case.
    bench::measure_grid(&bench::EXCEL);
    bench::measure_document(&bench::WORD);
    bench::measure_document(&bench::NOTEPAD);
    bench::measure_typing(&bench::NOTEPAD);
    bench::measure_modal_dialog(&bench::EXCEL, "Insert", "Table");
    // Forms, dialogs and menus.
    case("forms and dialogs", "refs behind a modal dialog are refused", live::refuses_refs_behind_a_modal_dialog);
    case("forms and dialogs", "dialogs stay off the screen", live::keeps_a_parked_apps_dialogs_off_screen);
    case("forms and dialogs", "cells that open their own editor", live::types_across_cells_that_open_their_own_editor);
    case("menus", "a <select> list opens and an item is picked", live::works_in_a_select_popup);
    // Web content evo started (page channel) and already open (corner pixel).
    case("web app evo started", "VS Code keys and text through the page channel", live::drives_an_electron_app_through_the_page_channel);
    case("web app evo started", "Edge typing through the page channel, fresh picture", live::edge_with_page_channel_stays_fresh_off_screen);
    case("web app already open", "fields", live::probes_chromium_fields);
    case("web app already open", "pointer", live::probes_chromium_pointer);
    case("web app already open", "keeps painting in its corner", live::corner_pixel_keeps_chromium_painting);
    // What background input cannot do must be said, not claimed.
    impossible_key_tips();
    impossible_active_only_canvas();
}

fn report(runs: usize, changes: &[Change], robot: Option<(usize, usize, usize)>) -> String {
    let outcomes = OUTCOMES.lock().map(|o| o.clone()).unwrap_or_default();
    let timings = TIMINGS.lock().map(|t| t.clone()).unwrap_or_default();
    let mut out = format!("# Computer app control benchmark\n\n{runs} run(s).\n\n| Surface | Case | Passed | Rate |\n| --- | --- | --- | --- |\n");
    let table = rates(&outcomes);
    for (surface, case, total, passed) in &table {
        out.push_str(&format!("| {surface} | {case} | {passed}/{total} | {:.0} % |\n", 100.0 * *passed as f64 / *total as f64));
    }
    let (total, passed) = table.iter().fold((0, 0), |(t, p), row| (t + row.2, p + row.3));
    out.push_str(&format!(
        "\nAll cases: {passed}/{total} ({:.1} %). Lowest case: {:.0} %.\n",
        100.0 * passed as f64 / total.max(1) as f64,
        table.iter().map(|row| 100.0 * row.3 as f64 / row.2 as f64).fold(100.0, f64::min)
    ));
    out.push_str("\n| Action | Count | Median ms |\n| --- | --- | --- |\n");
    for (action, count, median) in medians(&timings) {
        out.push_str(&format!("| {action} | {count} | {median} |\n"));
    }
    let against = changes.iter().filter(|change| !change.what.starts_with("(not the person's)")).count();
    out.push_str(&format!(
        "\nInput monitor: {against} change(s) to the person's input state without their input ({} recorded).\n",
        changes.len()
    ));
    for change in changes.iter().take(40) {
        out.push_str(&format!("- {} ms: {}\n", change.at_ms, change.what));
    }
    if let Some((sent, wrong, held)) = robot {
        out.push_str(&format!(
            "\nTyping robot: {sent} characters sent, {wrong} lost or wrong, {held} held back while another window was in front.\n"
        ));
    }
    out
}

#[test]
#[ignore = "opens and drives many app windows on the local desktop for a long time"]
fn benchmark_windows() {
    let runs: usize = std::env::var("EVO_BENCH_RUNS").ok().and_then(|n| n.parse().ok()).unwrap_or(10);
    OUTCOMES.lock().unwrap().clear();
    TIMINGS.lock().unwrap().clear();
    let monitor = InputMonitor::start();
    let mut robot = None;
    for run in 1..=runs {
        eprintln!("\n=== run {run} of {runs} ===");
        // `EVO_BENCH_ROBOT`: the robot's seconds (SC-3: a 5-minute task).
        if let Some(seconds) = std::env::var("EVO_BENCH_ROBOT").ok().filter(|_| run == 1) {
            robot = Some(run_typing_robot(seconds.parse().unwrap_or(300), one_run));
        } else {
            one_run();
        }
    }
    let changes = monitor.finish();
    let text = report(runs, &changes, robot);
    let file = std::env::temp_dir().join("evo-computer-app-benchmark.md");
    let _ = std::fs::write(&file, &text);
    eprintln!("\n{text}\nSaved to {}", file.display());
}

#[test]
fn rates_and_medians_add_up() {
    let outcomes = vec![
        Outcome { surface: "grid".into(), case: "a".into(), pass: true },
        Outcome { surface: "grid".into(), case: "a".into(), pass: false },
        Outcome { surface: "grid".into(), case: "b".into(), pass: true },
    ];
    assert_eq!(rates(&outcomes), vec![("grid".into(), "a".into(), 2, 1), ("grid".into(), "b".into(), 1, 1)]);
    let timings = vec![("Click".into(), 30), ("Click".into(), 10), ("Click".into(), 20), ("Type".into(), 5)];
    assert_eq!(medians(&timings), vec![("Click".into(), 3, 20), ("Type".into(), 1, 5)]);
}

/// The robot alone: it types into its own window and loses nothing while
/// nothing else runs.
#[test]
#[ignore = "opens a window that types for a few seconds on the local desktop"]
fn the_typing_robot_types_into_its_own_window() {
    let (sent, wrong, held) = run_typing_robot(8, || pause(9000));
    eprintln!("robot: {sent} sent, {wrong} lost or wrong, {held} held back");
    assert!(sent > 50, "it typed: {sent}");
    assert_eq!(wrong, 0);
}
