//! Drives a real Notepad window. Opt-in because it opens a window on the
//! desktop it runs on:
//!
//! `cargo test computer_app -- --ignored --nocapture`
//!
//! It launches its own minimized Notepad and never touches one that was
//! already open, and it checks the claim this module is built on: the
//! user's cursor and foreground window do not change while the agent works.

use super::*;
use std::cell::RefCell;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

/// Dispatch by wire name, the way the agent's calls arrive.
fn run_action(emit: &dyn Fn(Value), session: &str, action: &str, params: &Value) -> Result<Value, String> {
    super::benchmark::timed(emit, session, action.parse()?, params)
}

fn notepad_windows() -> Vec<(u64, u32)> {
    list_windows("live-test", &json!({ "query": "notepad" }))["windows"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter(|window| window["app"].as_str().unwrap_or("").eq_ignore_ascii_case("notepad.exe"))
        .map(|window| (window["id"].as_u64().unwrap_or(0), window["pid"].as_u64().unwrap_or(0) as u32))
        .collect()
}

#[test]
fn names_apps_from_their_captions() {
    assert_eq!(caption_app_name("notes.txt - Notepad").as_deref(), Some("Notepad"));
    assert_eq!(
        caption_app_name("Chat | Contoso | Microsoft Teams").as_deref(),
        Some("Microsoft Teams")
    );
    assert_eq!(caption_app_name("Calculator").as_deref(), Some("Calculator"));
    assert_eq!(caption_app_name("   ").as_deref(), None);
    assert_eq!(exe_stem("notepad.exe"), "Notepad");
}

#[test]
#[ignore = "reads the local Start menu and running apps"]
fn lists_apps_with_icons() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let started = std::time::Instant::now();
    let listed = list_apps();
    let apps = listed["apps"].as_array().unwrap();
    let with_icons = apps.iter().filter(|app| app["icon"].is_string()).count();
    eprintln!("{} apps ({with_icons} with icons) in {:?}", apps.len(), started.elapsed());
    for app in apps.iter().take(12) {
        eprintln!("  {} — {} running={}", app["exe"], app["name"], app["running"]);
    }
    assert!(!apps.is_empty());
    assert!(with_icons * 2 >= apps.len(), "most apps should have an icon");
    assert!(apps.iter().all(|app| !is_protected_process_name(app["exe"].as_str().unwrap())));
}

fn snapshot_text(emit: &dyn Fn(Value), session: &str) -> String {
    run_action(emit, session, "snapshot", &json!({})).unwrap().as_str().unwrap().to_string()
}

fn ref_for(emit: &dyn Fn(Value), session: &str, query: &str) -> String {
    let found = run_action(emit, session, "find", &json!({ "query": query })).unwrap();
    let text = found.as_str().unwrap();
    let start = text.find("[ref=").unwrap_or_else(|| panic!("{query} not found:\n{text}")) + 5;
    let end = start + text[start..].find(']').unwrap();
    text[start..end].to_string()
}

/// The probe page's report (see the fixture), parsed from the caption.
fn page_report(hwnd: HWND) -> HashMap<String, String> {
    window_title(hwnd)
        .split('|')
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn report_number(report: &HashMap<String, String>, key: &str) -> i64 {
    report.get(key).and_then(|value| value.parse().ok()).unwrap_or(0)
}

/// Screenshot coordinates of an element's centre, from a `find` line.
fn centre_of(emit: &dyn Fn(Value), session: &str, query: &str) -> (i64, i64) {
    let found = run_action(emit, session, "find", &json!({ "query": query })).unwrap();
    let line = found
        .as_str()
        .unwrap()
        .lines()
        .find(|line| line.contains("[ref="))
        .unwrap_or_else(|| panic!("{query} not found: {found}"))
        .to_string();
    let at = line.rfind('@').unwrap_or_else(|| panic!("{query} has no position: {line}"));
    let (position, size) = line[at + 1..].split_once(' ').unwrap();
    let (x, y) = position.split_once(',').unwrap();
    let (width, height) = size.trim().split_once('x').unwrap();
    let number = |text: &str| text.trim().parse::<i64>().unwrap();
    (number(x) + number(width) / 2, number(y) + number(height) / 2)
}

const EDGE: &str = r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe";

fn probe_url() -> String {
    let page = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("computer_app_probe.html");
    format!("file:///{}", page.display().to_string().replace('\\', "/"))
}

fn edge_profile() -> std::path::PathBuf {
    std::env::temp_dir().join("evo-computer-app-probe")
}

/// Wait until no Edge holds the probe profile. Each test starts Edge
/// right after the previous one killed its own, and Chromium runs one
/// browser per profile: an Edge started while the last one was still
/// dying handed its window to it and exited, leaving no probe window.
/// A running Chromium keeps `lockfile` in its profile open.
fn wait_for_free_profile() {
    let lockfile = edge_profile().join("lockfile");
    for _ in 0..50 {
        match std::fs::remove_file(&lockfile) {
            Ok(()) => return,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(_) => pause(200),
        }
    }
    panic!("an earlier Edge still holds the probe profile");
}

/// Start a process that must not inherit the test's output pipes, or the
/// harness waits on them for as long as any of its children lives.
/// Returns its process id.
fn spawn_quiet(command: &mut std::process::Command) -> u32 {
    super::benchmark::harness_pause(|| spawn_quiet_now(command))
}

fn spawn_quiet_now(command: &mut std::process::Command) -> u32 {
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("start the probe host")
        .id()
}

/// Kill a probe host with its whole process tree, and wait for it to be
/// gone.
fn kill_tree(pid: u32) {
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .output();
    for _ in 0..50 {
        let alive = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
            .map(|handle| unsafe { CloseHandle(handle) })
            .is_ok();
        if !alive {
            return;
        }
        pause(100);
    }
}

#[derive(Clone, Copy, Debug)]
enum ProbeHost {
    /// Edge in app mode: Chromium's own top-level window.
    Edge,
    /// A WebView2 control inside a Win32 host window — the layout of
    /// Teams and other WebView2 apps (see [`webview2_host`]).
    WebView2,
    /// A native window with only a small WebView2 pane in a corner.
    WebView2Pane,
}

/// Open the probe page in `host`, attach to it (parked off-screen when
/// `hide`), run `body`, and clean up whatever happens.
fn with_probe_page(
    session: &str,
    host: ProbeHost,
    hide: bool,
    body: impl FnOnce(&dyn Fn(Value), HWND),
) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    if matches!(host, ProbeHost::Edge) {
        wait_for_free_profile();
    }
    let launched = match host {
        ProbeHost::Edge => spawn_quiet(
            std::process::Command::new(EDGE)
                .arg(format!("--user-data-dir={}", edge_profile().display()))
                .args(["--no-first-run", "--no-default-browser-check", "--new-window"])
                .arg(format!("--app={}", probe_url())),
        ),
        // This same test binary, running only the host "test" below.
        ProbeHost::WebView2 | ProbeHost::WebView2Pane => {
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args(["agent_tools::computer::win::live_tests::webview2_host", "--exact", "--ignored"])
                .env("COMPUTER_APP_PROBE_URL", probe_url());
            if matches!(host, ProbeHost::WebView2Pane) {
                command.env("COMPUTER_APP_PROBE_PANE", "1");
            }
            spawn_quiet(&mut command)
        }
    };

    // The window of the process just started: "probe" alone also matched
    // what an earlier test left closing.
    let mut window = None;
    for _ in 0..60 {
        pause(250);
        window = list_windows(session, &json!({ "query": "probe" }))["windows"]
            .as_array()
            .and_then(|windows| windows.iter().find(|window| window["pid"] == json!(launched)).cloned());
        if window.is_some() {
            break;
        }
    }
    let window = window.expect("the probe page never opened");
    let window_id = window["id"].as_u64().unwrap();
    let pid = window["pid"].as_u64().unwrap();
    let hwnd = to_hwnd(window_id as isize);
    pause(1500);

    let emit = |_: Value| {};
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let foreground = unsafe { GetForegroundWindow() };
        let attached = run_action(&emit, session, "attach", &json!({ "window_id": window_id, "hide": hide })).unwrap();
        eprintln!("{host:?} attached: {attached}");
        let web = !matches!(host, ProbeHost::WebView2Pane);
        assert_eq!(attached["window"]["web_content"], json!(web), "web content detection");
        assert_eq!(is_off_screen(hwnd), hide, "parking did not follow hide={hide}");
        body(&emit, hwnd);
        // A just-launched probe window often *is* the foreground window,
        // and parking it hands the foreground on; the claim under test is
        // that the user's own window keeps it.
        if foreground != hwnd {
            let now = unsafe { GetForegroundWindow() };
            assert_eq!(
                foreground,
                now,
                "the foreground window changed from {:?} to {:?} (the probe is {:?})",
                window_title(foreground),
                window_title(now),
                window_title(hwnd)
            );
        }
    }));

    detach(session);
    kill_tree(pid as u32);
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

/// Fills every kind of field in a real Chromium page and reads back what
/// the page received, including whether `input` events fired — what
/// React-style apps such as Teams need to notice the text at all.
///
/// `cargo test probes_chromium -- --ignored --nocapture`
#[test]
#[ignore = "opens an Edge window on the local desktop"]
pub(super) fn probes_chromium_fields() {
    probe_fields(ProbeHost::Edge);
}

#[test]
#[ignore = "opens a WebView2 window on the local desktop"]
fn probes_webview2_fields() {
    probe_fields(ProbeHost::WebView2);
}

/// A `<select>` opens its list as a popup window of its own. It must
/// show up in the screenshot and the UI tree, and an item in it must be
/// pickable.
#[test]
#[ignore = "opens an Edge window on the local desktop"]
pub(super) fn works_in_a_select_popup() {
    let session = "probe-popup";
    // Parked, as every attached window is in evo 4 (spec FR-6): the list
    // opens next to the window, off the person's screen.
    with_probe_page(session, ProbeHost::Edge, true, |emit, hwnd| {
        let (x, y) = centre_of(emit, session, "Color select");
        let opened = run_action(emit, session, "click", &json!({ "x": x, "y": y })).unwrap();
        eprintln!("open select: {opened}");
        let mut popups = Vec::new();
        for _ in 0..20 {
            pause(150);
            popups = Target::resolve(session).unwrap().popups;
            if !popups.is_empty() {
                break;
            }
        }
        assert!(!popups.is_empty(), "the select's list never showed up as a popup");
        eprintln!("popups: {:?}", popups.iter().map(|popup| class_name(*popup)).collect::<Vec<_>>());

        let target = Target::resolve(session).unwrap();
        let shot = run_action(emit, session, "screenshot", &json!({})).unwrap();
        let (width, height) = target.screenshot_size();
        assert_eq!((shot["width"].as_u64(), shot["height"].as_u64()), (Some(width as u64), Some(height as u64)));

        let tree = snapshot_text(emit, session);
        let green = tree
            .lines()
            .find(|line| line.contains("\"Green\""))
            .and_then(|line| line.split("[ref=").nth(1))
            .and_then(|rest| rest.split(']').next())
            .unwrap_or_else(|| panic!("the popup's items are not in the tree:\n{tree}"))
            .to_string();
        let picked = run_action(emit, session, "click", &json!({ "ref": green })).unwrap();
        eprintln!("pick green: {picked}");
        pause(400);
        assert_eq!(page_report(hwnd).get("color").map(String::as_str), Some("Green"));
    });
}

/// Two buttons at the same spot: a click by coordinates reaches the one
/// drawn on top (later in the page), not the one it covers.
#[test]
#[ignore = "opens an Edge window on the local desktop"]
fn clicks_the_element_drawn_on_top() {
    let session = "probe-overlay";
    with_probe_page(session, ProbeHost::Edge, true, |emit, hwnd| {
        let (x, y) = centre_of(emit, session, "Covering button");
        let clicked = run_action(emit, session, "click", &json!({ "x": x, "y": y })).unwrap();
        eprintln!("click overlay: {clicked}");
        assert_eq!(clicked["delivered_via"], json!("ui_automation"), "the click fell back to posted input");
        assert_eq!(clicked["delivered_to"], json!("Covering button"));
        pause(300);
        let report = page_report(hwnd);
        assert_eq!(report_number(&report, "over"), 1, "the covering button was not clicked");
        assert_eq!(report_number(&report, "under"), 0, "the covered button was clicked");
    });
}

/// A native window with a small web pane stays a native app: attaching
/// reports no web content, so keys go to the app's own focus.
#[test]
#[ignore = "opens a WebView2 window on the local desktop"]
fn treats_a_small_web_pane_as_native() {
    with_probe_page("probe-pane", ProbeHost::WebView2Pane, false, |_, _| {});
}

fn probe_fields(host: ProbeHost) {
    let session = "probe-fields";
    with_probe_page(session, host, true, |emit, hwnd| {
        let caption = || window_title(hwnd);
        let name = ref_for(emit, session, "Name field");
        let clicked = run_action(emit, session, "click", &json!({ "ref": name })).unwrap();
        eprintln!("click name: {clicked}");
        let typed = run_action(emit, session, "type", &json!({ "text": "alpha" })).unwrap();
        eprintln!("type name: {typed}\n  caption: {}", caption());
        assert_eq!(typed["delivered_via"], json!("keyboard"));
        assert_ne!(typed["confirmed"], json!(false), "a hidden page's stale value was reported as a failure");

        // Tab moves the page's focus on; typing without a ref follows it
        // instead of going back into the field clicked before.
        run_action(emit, session, "key", &json!({ "key": "tab" })).unwrap();
        let typed = run_action(emit, session, "type", &json!({ "text": "tabbed" })).unwrap();
        eprintln!("type after tab: {typed}\n  caption: {}", caption());
        pause(300);
        let report = page_report(hwnd);
        assert_eq!(report.get("name").map(String::as_str), Some("alpha"), "typing after Tab went back into the name field");
        assert_eq!(report.get("notes").map(String::as_str), Some("tabbed"), "typing after Tab did not reach the next field");

        let notes = ref_for(emit, session, "Notes field");
        let set = run_action(emit, session, "set_value", &json!({ "ref": notes, "value": "beta" })).unwrap();
        eprintln!("set_value notes: {set}\n  caption: {}", caption());

        let editor = ref_for(emit, session, "Message editor");
        let typed = run_action(emit, session, "type", &json!({ "ref": editor, "text": "gamma" })).unwrap();
        eprintln!("type editor: {typed}\n  caption: {}", caption());

        // A second line in a chat editor is Shift+Enter, never a send.
        let typed = run_action(emit, session, "type", &json!({ "text": "\ndelta" })).unwrap();
        eprintln!("type editor line 2: {typed}\n  caption: {}", caption());

        let send = ref_for(emit, session, "Send");
        let clicked = run_action(emit, session, "click", &json!({ "ref": send })).unwrap();
        eprintln!("click send: {clicked}");
        pause(300);

        let report = page_report(hwnd);
        eprintln!("final report: {report:?}");
        assert_eq!(report.get("name").map(String::as_str), Some("alpha"), "input value");
        assert_eq!(report.get("notes").map(String::as_str), Some("beta"), "textarea value");
        assert_eq!(report.get("editor").map(String::as_str), Some("gamma/delta"), "contenteditable text");
        let events: Vec<i64> = report["ev"].split(',').map(|n| n.parse().unwrap_or(0)).collect();
        for (count, label) in events.iter().zip(["input", "textarea", "contenteditable"]) {
            assert!(*count > 0, "no input event reached the {label}");
        }
        assert_eq!(report_number(&report, "sent"), 1, "Send click");
    });
}

/// Hover, double-click, right-click, scroll, drag and a slider on a real
/// Chromium page, parked off-screen, each checked against what the page
/// itself saw.
#[test]
#[ignore = "opens an Edge window on the local desktop"]
pub(super) fn probes_chromium_pointer() {
    probe_pointer(ProbeHost::Edge);
}

#[test]
#[ignore = "opens a WebView2 window on the local desktop"]
fn probes_webview2_pointer() {
    probe_pointer(ProbeHost::WebView2);
}

/// Not a test on its own: the WebView2 host process that the WebView2
/// probes start, running this binary with only this "test" selected. A
/// tao window holding a wry (WebView2) view is laid out the way Teams is,
/// and the page title is copied to the window caption for the probes.
#[test]
#[ignore = "runs only as the WebView2 host child process of other live tests"]
fn webview2_host() {
    let Ok(url) = std::env::var("COMPUTER_APP_PROBE_URL") else {
        return;
    };
    use tao::event::Event;
    use tao::event_loop::{ControlFlow, EventLoopBuilder};
    use tao::platform::windows::EventLoopBuilderExtWindows;
    let event_loop = EventLoopBuilder::<String>::with_user_event()
        .with_any_thread(true)
        .build();
    // Never focused, by the window or by its WebView once ready: the
    // probes check that the user's foreground window keeps the
    // foreground, which a host that took it itself would make pass
    // without checking anything (and, when its WebView2 finished loading
    // mid-test, fail for its own reasons).
    let window = tao::window::WindowBuilder::new()
        .with_title("probe host")
        .with_inner_size(tao::dpi::LogicalSize::new(900.0, 1000.0))
        .with_focused(false)
        .build(&event_loop)
        .unwrap();
    let proxy = event_loop.create_proxy();
    let builder = wry::WebViewBuilder::new()
        .with_focused(false)
        .with_url(&url)
        .with_document_title_changed_handler(move |title| {
            let _ = proxy.send_event(title);
        });
    // A native window with only a small web pane in a corner, like an
    // Office add-in pane.
    let webview = if std::env::var("COMPUTER_APP_PROBE_PANE").is_ok() {
        builder
            .with_bounds(wry::Rect {
                position: wry::dpi::LogicalPosition::new(600.0, 0.0).into(),
                size: wry::dpi::LogicalSize::new(300.0, 300.0).into(),
            })
            .build_as_child(&window)
            .unwrap()
    } else {
        builder.build(&window).unwrap()
    };
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        let _ = &webview;
        if let Event::UserEvent(title) = event {
            window.set_title(&title);
        }
    });
}

fn probe_pointer(host: ProbeHost) {
    let session = "probe-pointer";
    let hide = std::env::var("PROBE_HIDE").map(|value| value != "0").unwrap_or(true);
    with_probe_page(session, host, hide, |emit, hwnd| {
        let step =|label: &str, action: &str, params: Value| {
            let result = run_action(emit, session, action, &params);
            // Hidden pages run timers about once a second.
            pause(1300);
            eprintln!("{label}: {result:?}\n  report: {:?}", page_report(hwnd));
        };
        let _ = run_action(emit, session, "snapshot", &json!({}));
        step("hover", "hover", json!({ "ref": ref_for(emit, session, "Hover target") }));
        step("double", "click", json!({ "ref": ref_for(emit, session, "Double target"), "clicks": 2 }));
        step("context", "click", json!({ "ref": ref_for(emit, session, "Context target"), "button": "right" }));
        step("scroll", "scroll", json!({ "ref": ref_for(emit, session, "Scroll box"), "direction": "down", "amount": 3 }));
        let (x, y) = centre_of(emit, session, "Drag handle");
        step("drag", "drag", json!({ "x": x, "y": y, "to_x": x + 150, "to_y": y }));
        assert_eq!(is_off_screen(hwnd), hide, "the drag changed where the page is");
        step("slider", "set_value", json!({ "ref": ref_for(emit, session, "Volume slider"), "value": "70" }));

        let report = page_report(hwnd);
        let mut failures = Vec::new();
        for key in ["hover", "dbl", "ctx", "scroll"] {
            if report_number(&report, key) <= 0 {
                failures.push(key);
            }
        }
        // The whole gesture, not just its first steps.
        if report_number(&report, "drag") < 120 {
            failures.push("drag");
        }
        if report_number(&report, "range") != 70 {
            failures.push("range");
        }
        assert!(failures.is_empty(), "not received by the page: {failures:?} — report {report:?}");
    });
}

/// Run `body` against the WinForms dialog probe (see the fixture),
/// attached and parked.
fn with_dialog_probe(body: impl FnOnce(&dyn Fn(Value), &str, HWND)) {
    with_winforms_probe("computer_app_dialogs.ps1", "dialog-probe", "live-dialogs", body);
}

/// Run `body` against a WinForms probe from `tests/fixtures`, found by
/// its title and attached parked.
fn with_winforms_probe(fixture: &str, title: &str, session: &str, body: impl FnOnce(&dyn Fn(Value), &str, HWND)) {
    use std::os::windows::process::CommandExt;
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(fixture);
    // The harness starting and ending the probe may move the front window;
    // the benchmark's input monitor does not count that against evo.
    let mut child = super::benchmark::harness_pause(|| {
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&script)
            // CREATE_NO_WINDOW: no console, only the form.
            .creation_flags(0x0800_0000)
            .spawn()
            .expect("start the WinForms probe")
    });
    let mut window = None;
    for _ in 0..50 {
        pause(200);
        window = list_windows(session, &json!({ "query": title }))["windows"]
            .as_array()
            .and_then(|windows| windows.first().cloned());
        if window.is_some() {
            break;
        }
    }
    let window_id = window.unwrap_or_else(|| panic!("{title} never opened"))["id"].as_u64().unwrap();
    let emit = |_: Value| {};
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_action(&emit, session, "attach", &json!({ "window_id": window_id, "hide": true })).unwrap();
        body(&emit, session, to_hwnd(window_id as isize));
    }));
    detach(session);
    super::benchmark::harness_pause(|| {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .output();
        let _ = child.wait();
    });
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

/// The window the session is driving now: the probe, or its dialog.
fn driven_title(session: &str) -> String {
    window_title(Target::resolve(session).unwrap().window)
}

fn wait_for_title(session: &str, title: &str) {
    for _ in 0..30 {
        if driven_title(session) == title {
            return;
        }
        pause(100);
    }
    panic!("expected to be driving {title:?}, driving {:?}", driven_title(session));
}

#[test]
#[ignore = "opens a WinForms window on the local desktop"]
pub(super) fn refuses_refs_behind_a_modal_dialog() {
    with_dialog_probe(|emit, session, _| {
        let open = ref_for(emit, session, "Open dialog 1");
        // A posted click: Invoke would not return while the dialog is open.
        let clicked = run_action(emit, session, "click", &json!({ "ref": open })).unwrap();
        assert_eq!(clicked["pattern"], json!("click_message"), "{clicked}");
        wait_for_title(session, "probe dialog 1");

        let behind = run_action(emit, session, "click", &json!({ "ref": open }));
        assert!(
            behind.as_ref().is_err_and(|error| error.contains("waiting on the dialog")),
            "a ref behind the modal dialog was accepted: {behind:?}"
        );
        let close = ref_for(emit, session, "Close dialog 1");
        run_action(emit, session, "click", &json!({ "ref": close })).unwrap();
        wait_for_title(session, "dialog-probe");
        let gone = run_action(emit, session, "click", &json!({ "ref": close }));
        assert!(
            gone.as_ref().is_err_and(|error| error.contains("has closed")),
            "a ref into the closed dialog was accepted: {gone:?}"
        );

        // A new snapshot retires the old refs instead of renumbering.
        snapshot_text(emit, session);
        let stale = run_action(emit, session, "click", &json!({ "ref": open }));
        assert!(
            stale.as_ref().is_err_and(|error| error.contains("Unknown ref")),
            "a ref from an earlier snapshot was accepted: {stale:?}"
        );
    });
}

/// A shown top-level window titled `title`, once it opens.
fn opened_window(title: &str) -> HWND {
    for _ in 0..30 {
        let found = top_level_windows()
            .into_iter()
            .find(|hwnd| unsafe { IsWindowVisible(*hwnd) }.as_bool() && window_title(*hwnd) == title);
        if let Some(hwnd) = found {
            return hwnd;
        }
        pause(100);
    }
    panic!("{title:?} never opened");
}

#[test]
#[ignore = "opens a WinForms window on the local desktop"]
pub(super) fn keeps_a_parked_apps_dialogs_off_screen() {
    with_dialog_probe(|emit, session, probe| {
        // WinForms keeps a dialog on a monitor; the dialog watcher moves
        // it over its parked owner as it opens, before any action runs.
        run_action(emit, session, "click", &json!({ "ref": ref_for(emit, session, "Open dialog 1") })).unwrap();
        let first = opened_window("probe dialog 1");
        pause(300);
        assert!(is_off_screen(first), "dialog 1 opened on the user's screen at {:?}", frame_rect(first));
        wait_for_title(session, "probe dialog 1");

        // A dialog opened from the dialog is followed, and parked too.
        run_action(emit, session, "click", &json!({ "ref": ref_for(emit, session, "Open dialog 2") })).unwrap();
        let second = opened_window("probe dialog 2");
        pause(300);
        assert!(is_off_screen(second), "dialog 2 opened on the user's screen at {:?}", frame_rect(second));
        wait_for_title(session, "probe dialog 2");
        run_action(emit, session, "click", &json!({ "ref": ref_for(emit, session, "Close dialog 2") })).unwrap();
        wait_for_title(session, "probe dialog 1");

        // Released while a dialog is open: it comes back with its window,
        // or the user would face an app blocked by a dialog they cannot see.
        detach(session);
        pause(300);
        assert!(!is_off_screen(probe), "the probe was left off-screen");
        assert!(!is_off_screen(first), "dialog 1 was left off-screen at {:?}", frame_rect(first));
    });
}

#[test]
#[ignore = "opens a WinForms window on the local desktop"]
fn keeps_a_parked_apps_panes_and_palettes_off_screen() {
    with_dialog_probe(|emit, session, probe| {
        // Opened by a key, not a click, at a fixed spot on the monitor:
        // the window watcher takes each off the user's screen as it is
        // shown, before the agent's next action.
        run_action(emit, session, "key", &json!({ "key": "f7" })).unwrap();
        let pane = opened_window("probe pane");
        pause(300);
        assert!(is_off_screen(pane), "the owned pane opened on the user's screen at {:?}", frame_rect(pane));

        run_action(emit, session, "key", &json!({ "key": "f9" })).unwrap();
        let palette = opened_window("probe palette");
        pause(300);
        assert!(is_off_screen(palette), "the palette opened on the user's screen at {:?}", frame_rect(palette));

        // Both are part of what the agent sees.
        let target = Target::resolve(session).unwrap();
        assert!(target.popups.contains(&pane) && target.popups.contains(&palette), "the capture leaves them out");

        // Handed back, they return to where they opened.
        detach(session);
        pause(300);
        assert!(!is_off_screen(probe), "the probe was left off-screen");
        assert!(!is_off_screen(pane), "the pane was left off-screen at {:?}", frame_rect(pane));
        assert!(!is_off_screen(palette), "the palette was left off-screen at {:?}", frame_rect(palette));
    });
}

/// The grid probe's title once it reads `wanted`, or what it read last.
fn grid_title(hwnd: HWND, wanted: &str) -> String {
    let mut title = String::new();
    for _ in 0..30 {
        title = window_title(hwnd);
        if title == wanted {
            break;
        }
        pause(100);
    }
    title
}

#[test]
#[ignore = "opens a WinForms window on the local desktop"]
pub(super) fn types_across_cells_that_open_their_own_editor() {
    with_winforms_probe("computer_app_grid.ps1", "grid-probe", "live-grid", |emit, session, probe| {
        // One type across cells: every Tab lands in a new cell, whose
        // first character opens its editor and takes the focus there.
        // (No spaces: DataGridView takes Space for itself.)
        let typed = run_action(
            emit,
            session,
            "type",
            &json!({ "text": "Americas\t188\tEurope\t154\tAsiaPacific\t129\n" }),
        )
        .unwrap();
        let cells = "Americas|188/Europe|154/AsiaPacific|129";
        assert_eq!(grid_title(probe, &format!("grid-probe {cells} goto:")), format!("grid-probe {cells} goto:"), "{typed}");

        // A shortcut that opens a dialog, then typing into the dialog:
        // the text must wait for the dialog rather than land behind it.
        run_action(emit, session, "key", &json!({ "key": "ctrl+g" })).unwrap();
        run_action(emit, session, "type", &json!({ "text": "B2" })).unwrap();
        run_action(emit, session, "key", &json!({ "key": "enter" })).unwrap();
        assert_eq!(grid_title(probe, &format!("grid-probe {cells} goto:B2")), format!("grid-probe {cells} goto:B2"));
    });
}

#[test]
#[ignore = "opens a Calculator window on the local desktop"]
fn names_store_apps_by_their_own_process() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let frames = || -> Vec<Value> {
        list_windows("live-store", &json!({ "query": "calculator" }))["windows"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    };
    let before: HashSet<u64> = frames().iter().filter_map(|window| window["id"].as_u64()).collect();
    std::process::Command::new("cmd").args(["/C", "start", "calc.exe"]).status().expect("start calculator");
    let mut opened = None;
    for _ in 0..50 {
        pause(200);
        opened = frames().into_iter().find(|window| !before.contains(&window["id"].as_u64().unwrap_or(0)));
        if opened.is_some() {
            break;
        }
    }
    let window = opened.expect("Calculator never opened");
    let hwnd = to_hwnd(window["id"].as_u64().unwrap() as isize);
    let outcome = std::panic::catch_unwind(|| {
        // Not ApplicationFrameHost.exe, the host every Store app shares.
        assert_eq!(window["app"], json!("CalculatorApp.exe"), "{window}");
        let apps = list_apps();
        let exes: Vec<&str> = apps["apps"].as_array().unwrap().iter().filter_map(|app| app["exe"].as_str()).collect();
        assert!(exes.contains(&"calculatorapp.exe"), "{exes:?}");
        assert!(!exes.contains(&"applicationframehost.exe"), "{exes:?}");
    });
    let _ = post(hwnd, windows::Win32::UI::WindowsAndMessaging::WM_CLOSE, 0, LPARAM(0));
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

#[test]
#[ignore = "opens a WinForms window on the local desktop"]
fn puts_back_a_window_parked_by_a_run_that_crashed() {
    let journal = super::super::MemoryJournal::default();
    recover_stranded(Box::new(journal.clone()));
    with_dialog_probe(|_, session, probe| {
        assert!(is_off_screen(probe));
        let recorded = load_stranded(&journal);
        assert!(recorded.iter().any(|entry| entry.hwnd == probe.0 as isize), "the parked window was not recorded");

        // The crash: evo's memory is gone, the window is not back.
        registry().attached.remove(session);
        stranded().clear();

        // The next start.
        stranded().extend(recorded.iter().copied());
        put_back(recorded);
        assert!(!is_off_screen(probe), "the window was left off-screen");
        assert!(load_stranded(&journal).is_empty(), "a window put back is still recorded");
    });
}

/// Excel, parked on the stage: sized to it, screenshotted pixel for pixel,
/// and each typed entry lands in the grid and shows in the next picture.
/// Starts an Excel instance of its own (`/x`) and ends only that one, so a
/// workbook the user has open is never touched. Leaves each step's
/// screenshot in the temp folder.
#[test]
#[ignore = "opens an Excel window on the local desktop"]
fn drives_excel_on_the_stage() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let Some((window_id, pid)) = start_private(&EXCEL) else {
        eprintln!("skipped: Excel is not installed or did not open");
        return;
    };
    let session = "live-test-excel";
    let emit = |_: Value| {};
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let attached = run_action(&emit, session, "attach", &json!({ "window_id": window_id, "hide": true })).unwrap();
        eprintln!("attached: {attached}");
        let excel = to_hwnd(window_id as isize);
        let frame = frame_rect(excel);
        assert_eq!(
            (frame.right - frame.left, frame.bottom - frame.top),
            stage_size(unsafe { GetDpiForWindow(excel) }),
            "Excel was not sized to the stage"
        );
        // `/e` starts without a workbook: open a blank one to type into.
        run_action(&emit, session, "key", &json!({ "key": "ctrl+n" })).unwrap();
        pause(3000);
        let folder = std::env::temp_dir();
        eprintln!("pictures in {}", folder.display());
        let mut previous = capture(excel).unwrap();
        for (step, text) in ["12345\n", "67890\n", "abc\t", "=A1+A2\n"].iter().enumerate() {
            let typed = run_action(&emit, session, "type", &json!({ "text": text })).unwrap();
            assert!(typed["delivered_to"].as_str().is_some_and(|class| class.starts_with("EXCEL")), "{typed}");
            pause(400);
            let shot = run_action(&emit, session, "screenshot", &json!({})).unwrap();
            assert_eq!(
                (shot["width"].as_i64().unwrap(), shot["height"].as_i64().unwrap()),
                (i64::from(frame.right - frame.left), i64::from(frame.bottom - frame.top)),
                "the screenshot of the stage was scaled"
            );
            let picture = capture(excel).unwrap();
            picture.save(folder.join(format!("evo-excel-step{step}.png"))).unwrap();
            assert!(picture.as_raw() != previous.as_raw(), "after {text:?} the picture did not change");
            previous = picture;
        }
    }));
    detach(session);
    let _ = std::process::Command::new("taskkill").args(["/PID", &pid.to_string(), "/F"]).status();
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

/// VS Code — an Electron app — parked off-screen and driven with posted
/// keys only: a new file, typed code, a command from the command palette,
/// a search in the Settings it opens, and closing that again. A private
/// instance (its own profile under %TEMP%) is started and only that one is
/// ended. Each step's screenshot and snapshot are left in
/// `EVO_TRACE_DIR` (default: %TEMP%\evo-vscode-trace);
/// `EVO_TRACE_SHOWN=1` keeps the window on screen.
///
/// `cargo test drives_vscode_by_its_keys -- --ignored --nocapture`
#[test]
#[ignore = "opens a private VS Code window on the local desktop"]
fn drives_vscode_by_its_keys() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let out = std::env::var("EVO_TRACE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("evo-vscode-trace"));
    std::fs::create_dir_all(&out).unwrap();
    let profile = std::env::temp_dir().join("evo-debug-code");
    if !std::path::Path::new(&std::env::var("LOCALAPPDATA").unwrap_or_default())
        .join(r"Programs\Microsoft VS Code\Code.exe")
        .exists()
    {
        eprintln!("skipped: VS Code (user install) is not installed");
        return;
    }
    let launch = Launch {
        exe: "Code.exe",
        run: r"%LOCALAPPDATA%\Programs\Microsoft VS Code\Code.exe",
        args: &[
            "--user-data-dir",
            r"%TEMP%\evo-debug-code",
            "--extensions-dir",
            r"%TEMP%\evo-debug-code-ext",
            "--new-window",
            "--disable-workspace-trust",
        ],
        blank: &[],
        cleanup: &[],
        title: "",
        blank_title: "",
    };
    let (window_id, pid) = start_private(&launch).expect("VS Code did not open");
    let session = "live-test-vscode";
    let emit = |_: Value| {};
    let hwnd = to_hwnd(window_id as isize);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut cursor_before = POINT::default();
        unsafe { GetCursorPos(&mut cursor_before) }.unwrap();
        let foreground_before = unsafe { GetForegroundWindow() };
        let hide = std::env::var("EVO_TRACE_SHOWN").is_err();
        let attached = run_action(&emit, session, "attach", &json!({ "window_id": window_id, "hide": hide })).unwrap();
        eprintln!("attach: {attached}");
        pause(3000);
        // Leaves the step's picture and tree behind; returns the tree.
        let record = |index: usize, label: &str| -> String {
            let shot = run_action(&emit, session, "screenshot", &json!({})).unwrap();
            let bytes = BASE64.decode(shot["data"].as_str().unwrap()).unwrap();
            std::fs::write(out.join(format!("{index:02}-{label}.png")), bytes).unwrap();
            let tree = snapshot_text(&emit, session);
            std::fs::write(out.join(format!("{index:02}-{label}.txt")), &tree).unwrap();
            let mut info = GUITHREADINFO { cbSize: std::mem::size_of::<GUITHREADINFO>() as u32, ..Default::default() };
            let thread = unsafe { GetWindowThreadProcessId(hwnd, None) };
            let _ = unsafe { GetGUIThreadInfo(thread, &mut info) };
            eprintln!(
                "[{index:02} {label}] title={:?} thread-focus={} active={}",
                window_title(hwnd),
                class_name(info.hwndFocus),
                class_name(info.hwndActive)
            );
            tree
        };
        record(0, "attached");
        // A fresh profile opens with a sign-in dialog that takes every key;
        // leave it without signing in, and never press Enter while it shows.
        // Its pages: sign in (skipped), then "Make It Yours" (Get Started).
        let onboarding = |tree: &str| tree.contains("Continue with GitHub") || tree.contains("Make It Yours");
        for _ in 0..4 {
            let tree = snapshot_text(&emit, session);
            if !onboarding(&tree) {
                break;
            }
            // The button named exactly so, not a Welcome page item that
            // merely contains the words.
            let Some((button, skip)) = ["Continue without Signing In", "Get Started"].into_iter().find_map(|name| {
                let line = tree.lines().find(|line| line.contains(&format!("Button \"{name}\" ")))?;
                let reference = line.split("[ref=").nth(1)?.split(']').next()?.to_string();
                Some((name, reference))
            }) else {
                break;
            };
            let skipped = run_action(&emit, session, "click", &json!({ "ref": skip }));
            eprintln!("onboarding {button}: {skipped:?}");
            pause(1500);
        }
        record(0, "onboarding-done");
        assert!(
            !onboarding(&snapshot_text(&emit, session)),
            "the onboarding dialog is still up; stopping before any key reaches it"
        );
        // Each step, and what the window's title or tree shows after it.
        enum Shows {
            Title(&'static str),
            Tree(&'static str),
            NoTree(&'static str),
            Nothing,
        }
        let steps: Vec<(&str, &str, Value, u64, Shows)> = vec![
            // A first run opens the Chat panel with the focus in it, where
            // Ctrl+N is "New Chat": close the panel first.
            ("chat-closed", "key", json!({ "key": "ctrl+alt+i" }), 1000, Shows::Nothing),
            ("ctrl-n", "key", json!({ "key": "ctrl+n" }), 1500, Shows::Title("Untitled-1")),
            ("typed-code", "type", json!({ "text": "let answer = 42;" }), 800, Shows::Title("let answer = 42;")),
            ("palette", "key", json!({ "key": "ctrl+shift+p" }), 1000, Shows::Tree("Type the name of a command to run.")),
            ("palette-typed", "type", json!({ "text": "Preferences: Open Settings (UI)" }), 1000, Shows::Tree(">Preferences: Open Settings (UI)")),
            // Settings opens as a window over the editor: the title stays.
            ("enter", "key", json!({ "key": "enter" }), 2500, Shows::Tree("Window \"Settings\"")),
            ("search-typed", "type", json!({ "text": "font size" }), 2000, Shows::Tree("Editor Font Size")),
            ("closed", "key", json!({ "key": "ctrl+w" }), 1500, Shows::NoTree("Window \"Settings\"")),
        ];
        for (index, (label, action, params, wait, shows)) in steps.into_iter().enumerate() {
            let result = run_action(&emit, session, action, &params);
            eprintln!("{action} {params} -> {result:?}");
            assert!(result.is_ok(), "{label}: {result:?}");
            pause(wait);
            let tree = record(index + 1, label);
            match shows {
                Shows::Title(text) => assert!(window_title(hwnd).contains(text), "{label}: title {:?}", window_title(hwnd)),
                Shows::Tree(text) => assert!(tree.contains(text), "{label}: {text:?} is not in the tree"),
                Shows::NoTree(text) => assert!(!tree.contains(text), "{label}: {text:?} is still in the tree"),
                Shows::Nothing => {}
            }
        }
        let mut cursor_after = POINT::default();
        unsafe { GetCursorPos(&mut cursor_after) }.unwrap();
        eprintln!("cursor {:?} -> {:?}", (cursor_before.x, cursor_before.y), (cursor_after.x, cursor_after.y));
        // A person may move the mouse meanwhile, so only the foreground is
        // asserted: the parked VS Code never took it.
        if foreground_before != hwnd {
            assert_eq!(foreground_before, unsafe { GetForegroundWindow() }, "the foreground window changed");
        }
    }));
    detach(session);
    // Only ever the private instance: its command line names the profile.
    let command_line = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &format!("(Get-CimInstance Win32_Process -Filter \"ProcessId={pid}\").CommandLine")])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
        .unwrap_or_default();
    if command_line.contains(&*profile.file_name().unwrap().to_string_lossy()) {
        super::benchmark::harness_pause(|| {
            let _ = std::process::Command::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).status();
        });
    } else {
        eprintln!("NOT killing pid {pid}: {command_line}");
    }
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

#[test]
#[ignore = "opens a Notepad window on the local desktop"]
fn drives_notepad_in_the_background() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let before = notepad_windows();
    std::process::Command::new("cmd")
        .args(["/C", "start", "/min", "notepad.exe"])
        .status()
        .expect("start notepad");
    let mut launched = None;
    for _ in 0..50 {
        pause(200);
        launched = notepad_windows().into_iter().find(|window| !before.contains(window));
        if launched.is_some() {
            break;
        }
    }
    let Some((window_id, pid)) = launched else {
        eprintln!("skipped: Notepad opened as a tab of an existing window, not a new one");
        return;
    };
    let already_running = before.iter().any(|(_, existing)| *existing == pid);

    let session = "live-test";
    let events = RefCell::new(Vec::<Value>::new());
    let emit = |payload: Value| events.borrow_mut().push(payload);

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let attached = run_action(&emit, session, "attach", &json!({ "window_id": window_id, "hide": false })).unwrap();
        eprintln!("attached: {attached}");
        // A second chat can neither take the window nor pick it blindly.
        let other = "live-test-other";
        let taken = run_action(&emit, other, "attach", &json!({ "window_id": window_id }));
        assert!(taken.is_err_and(|error| error.contains("another chat")), "a second chat attached the same window");
        let listed = run_action(&emit, other, "list_windows", &json!({})).unwrap();
        let row = listed["windows"].as_array().unwrap().iter().find(|row| row["id"] == json!(window_id)).cloned();
        assert_eq!(row.map(|row| row["controlled_elsewhere"].clone()), Some(json!(true)));
        pause(500);
        // Windows Notepad restores unsaved tabs from earlier sessions;
        // work in a fresh tab so none of them is touched.
        run_action(&emit, session, "key", &json!({ "key": "ctrl+n" })).unwrap();
        pause(700);

        let notepad = to_hwnd(window_id as isize);
        let notepad_was_foreground = unsafe { GetForegroundWindow() } == notepad;

        let typed = run_action(&emit, session, "type", &json!({ "text": "hello from evo\nsecond line" })).unwrap();
        eprintln!("typed: {typed}");
        assert_eq!(typed["confirmed"], json!(true), "typing was not confirmed: {typed}");
        // FR-17: the result reads back what changed in the window.
        let added = typed["changed"]["added"].to_string();
        assert!(added.contains("hello from evo"), "the read-back does not show the typed text: {}", typed["changed"]);
        pause(400);
        let first = snapshot_text(&emit, session);
        assert!(first.contains("hello from evo"), "text not in the UI tree:\n{first}");
        assert!(first.contains("second line"), "the line break did not type as Enter:\n{first}");

        run_action(&emit, session, "key", &json!({ "key": "ctrl+a" })).unwrap();
        run_action(&emit, session, "type", &json!({ "text": "replaced" })).unwrap();
        pause(400);
        let second = snapshot_text(&emit, session);
        assert!(second.contains("replaced"), "ctrl+a then typing failed:\n{second}");
        assert!(!second.contains("hello from evo"), "ctrl+a did not select all:\n{second}");

        let shot = run_action(&emit, session, "screenshot", &json!({})).unwrap();
        assert!(shot["data"].as_str().map(str::len).unwrap_or(0) > 1000);
        let (width, height) = (shot["width"].as_u64().unwrap(), shot["height"].as_u64().unwrap());
        let (x, y) = ((width / 2) as f64, (height / 2) as f64);
        let clicked = Target::resolve(session).unwrap().screen_point(x, y).unwrap();
        run_action(&emit, session, "click", &json!({ "x": x, "y": y })).unwrap();

        // A person may be using the mouse while this runs, so "the cursor
        // did not move" cannot be asserted — but a click that went through
        // the real cursor would have left it exactly on the clicked point.
        let mut cursor = POINT::default();
        unsafe { GetCursorPos(&mut cursor) }.unwrap();
        assert!(
            (cursor.x - clicked.x).abs() > 2 || (cursor.y - clicked.y).abs() > 2,
            "the real cursor sits on the clicked point {clicked:?}"
        );
        if !notepad_was_foreground {
            assert_ne!(unsafe { GetForegroundWindow() }, notepad, "Notepad was brought to the front");
        }
        let phases: Vec<String> = events
            .borrow()
            .iter()
            .filter_map(|event| event["phase"].as_str().map(str::to_string))
            .collect();
        assert!(phases.iter().any(|phase| phase == "click"), "no pointer events: {phases:?}");

        // Hidden mode: parked off-screen, still controllable, and put
        // back exactly where it was on detach.
        let mut original = WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        unsafe { GetWindowPlacement(notepad, &mut original) }.unwrap();
        let hidden = run_action(&emit, session, "attach", &json!({ "window_id": window_id, "hide": true })).unwrap();
        assert_eq!(hidden["window"]["hidden"], json!(true));
        assert!(is_off_screen(notepad), "Notepad is still on screen");
        // On the stage: a fixed size, streamed pixel for pixel.
        let frame = frame_rect(notepad);
        let stage = stage_size(unsafe { GetDpiForWindow(notepad) });
        eprintln!("stage {stage:?}, frame {}x{}", frame.right - frame.left, frame.bottom - frame.top);
        assert_eq!((frame.right - frame.left, frame.bottom - frame.top), stage, "not sized to the stage");
        let before_typing = capture(notepad).unwrap();
        assert_eq!((before_typing.width() as i32, before_typing.height() as i32), stage, "capture is not the frame");
        let shot = run_action(&emit, session, "screenshot", &json!({})).unwrap();
        assert_eq!(
            (shot["width"].as_i64().unwrap(), shot["height"].as_i64().unwrap()),
            (stage.0 as i64, stage.1 as i64),
            "the screenshot of the stage was scaled"
        );

        let typed_hidden = run_action(&emit, session, "type", &json!({ "text": " hidden ok" })).unwrap();
        assert_eq!(typed_hidden["confirmed"], json!(true), "typing while parked was not confirmed: {typed_hidden}");
        pause(400);
        let hidden_tree = snapshot_text(&emit, session);
        assert!(hidden_tree.contains("hidden ok"), "typing while parked failed:\n{hidden_tree}");
        assert!(capture(notepad).unwrap().as_raw() != before_typing.as_raw(), "the typed text is not in the picture");
        run_action(&emit, session, "detach", &json!({})).unwrap();
        pause(300);
        let mut restored = WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        unsafe { GetWindowPlacement(notepad, &mut restored) }.unwrap();
        assert_eq!(
            (original.rcNormalPosition.left, original.rcNormalPosition.top),
            (restored.rcNormalPosition.left, restored.rcNormalPosition.top),
            "detach did not restore the window's position"
        );
        assert_eq!(
            (original.rcNormalPosition.right, original.rcNormalPosition.bottom),
            (restored.rcNormalPosition.right, restored.rcNormalPosition.bottom),
            "detach did not restore the window's size"
        );
        assert!(
            !is_off_screen(notepad) || unsafe { IsIconic(notepad) }.as_bool(),
            "Notepad was left off-screen"
        );
    }));

    // Empty and close the test's own tab so Notepad has nothing of it to
    // restore next time, whether or not the assertions above held.
    let noop = |_: Value| {};
    let _ = run_action(&noop, session, "attach", &json!({ "window_id": window_id, "hide": false }));
    if Target::resolve(session).is_ok() {
        let _ = run_action(&noop, session, "key", &json!({ "key": "ctrl+a" }));
        let _ = run_action(&noop, session, "key", &json!({ "key": "delete" }));
        pause(200);
        let _ = run_action(&noop, session, "key", &json!({ "key": "ctrl+w" }));
        pause(500);
    }

    detach(session);
    if already_running {
        // A new window of the user's own Notepad: close just that window
        // (its last tab was emptied above, so nothing asks to save).
        let _ = post(to_hwnd(window_id as isize), windows::Win32::UI::WindowsAndMessaging::WM_CLOSE, 0, LPARAM(0));
    } else {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .status();
    }
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

/// A parked Chromium window keeps one pixel on top in a screen corner, so it
/// keeps painting: typing into the page changes the next picture
/// (spec FR-6, NFR-3; plan R1, R2). Release takes the topmost bit away and
/// puts the window back where it was.
#[test]
#[ignore = "opens an Edge window on the local desktop"]
pub(super) fn corner_pixel_keeps_chromium_painting() {
    let session = "probe-corner";
    let mut window = None;
    with_probe_page(session, ProbeHost::Edge, true, |emit, hwnd| {
        window = Some((hwnd, frame_rect(hwnd)));
        assert!(is_cornered(hwnd), "a parked Chromium window was not put in its corner");
        let topmost = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32 & WS_EX_TOPMOST.0 != 0;
        assert!(topmost, "the corner window is not on top");
        assert_eq!(visible_area(frame_rect(hwnd), &monitor_rects()), 1, "more than one pixel shows");

        let before = capture(hwnd).expect("a picture before typing");
        let name = ref_for(emit, session, "Name field");
        run_action(emit, session, "click", &json!({ "ref": name })).unwrap();
        run_action(emit, session, "type", &json!({ "text": "painted while parked" })).unwrap();
        pause(500);
        let after = capture(hwnd).expect("a picture after typing");
        assert_ne!(before.as_raw(), after.as_raw(), "the picture did not change: the page stopped painting");
    });
    let (hwnd, _) = window.expect("the probe ran");
    // `with_probe_page` detached and ended Edge; a window still there must
    // no longer be on top.
    if unsafe { IsWindow(Some(hwnd)) }.as_bool() {
        let topmost = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32 & WS_EX_TOPMOST.0 != 0;
        assert!(!topmost, "release left the window on top");
    }
}

/// The end of a turn hands the app back (spec FR-7): `release_chat`, which the
/// shell calls on `agent_end` and when a chat's engine stops, puts the window
/// back where it was and ends the chat's attachment.
#[test]
#[ignore = "opens a WinForms window on the local desktop"]
fn returns_the_window_at_the_end_of_the_turn() {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    with_dialog_probe(|_, session, probe| {
        assert!(is_off_screen(probe), "the probe was not parked");
        runtime.block_on(super::super::release_chat(session));
        assert!(!is_off_screen(probe), "the window is still off screen after the turn ended");
        assert!(!registry().attached.contains_key(session), "the chat still holds the window");
    });
}

/// A screenshot with marks numbers the controls by refs that work, and a zoom
/// enlarges a region of it (spec FR-14, FR-20).
#[test]
#[ignore = "opens a WinForms window on the local desktop"]
fn marks_controls_and_zooms_a_region() {
    with_dialog_probe(|emit, session, _probe| {
        let shot = run_action(emit, session, "screenshot", &json!({ "marks": true })).unwrap();
        let marks = shot["marks"].as_array().cloned().unwrap_or_default();
        eprintln!("marks: {marks:?}");
        assert!(!marks.is_empty(), "no control was marked");
        let line = marks[0].as_str().unwrap();
        let reference = line.split("[ref=").nth(1).and_then(|rest| rest.split(']').next()).unwrap().to_string();
        // The marked ref is live: find answers for it.
        let element = element_for(&Target::resolve(session).unwrap(), &reference);
        assert!(element.is_ok(), "the marked ref {reference} does not resolve");

        let zoomed = run_action(emit, session, "zoom", &json!({ "region": [10, 10, 200, 100] })).unwrap();
        assert_eq!((zoomed["width"].as_u64(), zoomed["height"].as_u64()), (Some(400), Some(200)));
        assert_eq!(zoomed["zoom"]["factor"], json!(2.0));
    });
}

/// Whether the person has been idle past Windows' foreground lock timeout,
/// when any starting app may bring itself to the front.
fn foreground_lock_expired() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    use windows::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETFOREGROUNDLOCKTIMEOUT, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS};
    let mut timeout: u32 = 0;
    let mut last = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    unsafe {
        let _ = SystemParametersInfoW(SPI_GETFOREGROUNDLOCKTIMEOUT, 0, Some((&mut timeout as *mut u32).cast()), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
        if !GetLastInputInfo(&mut last).as_bool() {
            return false;
        }
    }
    let idle = unsafe { windows::Win32::System::SystemInformation::GetTickCount() }.wrapping_sub(last.dwTime);
    eprintln!("idle {idle} ms, foreground lock timeout {timeout} ms");
    idle >= timeout
}

/// The window of process `pid`, once it shows (list_windows rows).
fn window_of_pid(session: &str, pid: u32) -> Option<u64> {
    for _ in 0..80 {
        pause(250);
        let found = list_windows(session, &json!({}))["windows"]
            .as_array()
            .and_then(|rows| rows.iter().find(|row| row["pid"] == json!(pid)).and_then(|row| row["id"].as_u64()));
        if found.is_some() {
            return found;
        }
    }
    None
}

/// An Electron app evo starts gets a page channel, and its keys reach the
/// page off screen: VS Code 1.145 ignored posted shortcuts, which go through
/// DevTools input here (spec FR-16, plan R4, R5).
#[test]
#[ignore = "opens a VS Code window (its own profile) on the local desktop"]
pub(super) fn drives_an_electron_app_through_the_page_channel() {
    let profile = std::env::temp_dir().join(format!("evo-page-code-{}", std::process::id()));
    let code = std::path::PathBuf::from(std::env::var("LOCALAPPDATA").unwrap()).join(r"Programs\Microsoft VS Code\Code.exe");
    assert!(code.is_file(), "VS Code is not installed for this user");
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let foreground = unsafe { GetForegroundWindow() };
    let pid = start_with_channel(
        code.to_str().unwrap(),
        &format!(
            "--user-data-dir=\"{0}\" --extensions-dir=\"{0}\\ext\" --new-window --skip-welcome --disable-workspace-trust",
            profile.display()
        ),
        None,
    )
    .expect("start VS Code with a page channel");
    let session = "live-page-code";
    let emit = |_: Value| {};
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let window_id = window_of_pid(session, pid).expect("VS Code did not open a window");
        assert!(channel_answers(pid), "the page channel does not answer");
        // Starting it minimized without activation left the person's window in front.
        let now = unsafe { GetForegroundWindow() };
        eprintln!("foreground before {:?} \"{}\", now {:?} \"{}\"", foreground, window_title(foreground), now, window_title(now));
        // Windows lets a starting app take the front only once the person has
        // been idle past the foreground lock timeout; while they are active it
        // must not happen.
        if !foreground_lock_expired() {
            assert_eq!(now, foreground, "starting the app took the foreground while the person was active");
        }
        run_action(&emit, session, "attach", &json!({ "window_id": window_id })).unwrap();
        pause(3000);
        let hwnd = to_hwnd(window_id as isize);
        // A fresh profile opens with a sign-in page that takes every key.
        for _ in 0..4 {
            let tree = snapshot_text(&emit, session);
            let Some(skip) = ["Continue without Signing In", "Get Started"].into_iter().find_map(|name| {
                let line = tree.lines().find(|line| line.contains(&format!("Button \"{name}\" ")))?;
                Some(line.split("[ref=").nth(1)?.split(']').next()?.to_string())
            }) else {
                break;
            };
            let _ = run_action(&emit, session, "click", &json!({ "ref": skip }));
            pause(1500);
        }
        // A first run opens the Chat panel with the focus in it, where Ctrl+N
        // is "New Chat": close the panel first (as drives_vscode_by_its_keys).
        run_action(&emit, session, "key", &json!({ "key": "ctrl+alt+i" })).unwrap();
        pause(1000);
        let pressed = run_action(&emit, session, "key", &json!({ "key": "ctrl+n" })).unwrap();
        eprintln!("ctrl+n: {pressed}");
        assert_eq!(pressed["delivered_via"], json!("page channel"));
        pause(1500);
        let picture = std::env::temp_dir().join("evo-page-code.png");
        eprintln!("page state: {} (picture {})", channel_for(pid).map(|c| c.debug_state(&window_title(hwnd), &picture)).unwrap_or_default(), picture.display());
        assert!(window_title(hwnd).contains("Untitled-1"), "ctrl+n did not open a new file: title {:?}", window_title(hwnd));
        let typed = run_action(&emit, session, "type", &json!({ "text": "let answer = 42;" })).unwrap();
        eprintln!("type: {typed}");
        pause(1000);
        assert!(window_title(hwnd).contains("let answer = 42;"), "the text did not reach the editor: title {:?}", window_title(hwnd));
        assert!(is_off_screen(hwnd), "the window came onto the screen");
        if !foreground_lock_expired() && now == foreground {
            assert_eq!(unsafe { GetForegroundWindow() }, foreground, "driving the app took the foreground");
        }
    }));
    detach(session);
    kill_tree(pid);
    let _ = std::fs::remove_dir_all(&profile);
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

/// Edge started by evo with a page channel and its occlusion flag: typed text
/// reaches the page through the channel and the picture shows it (FR-16, NFR-3).
#[test]
#[ignore = "opens an Edge window (its own profile) on the local desktop"]
pub(super) fn edge_with_page_channel_stays_fresh_off_screen() {
    let profile = std::env::temp_dir().join(format!("evo-page-edge-{}", std::process::id()));
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let foreground = unsafe { GetForegroundWindow() };
    let pid = start_with_channel(
        EDGE,
        &format!("--user-data-dir=\"{}\" --no-first-run --no-default-browser-check --app={}", profile.display(), probe_url()),
        None,
    )
    .expect("start Edge with a page channel");
    let session = "live-page-edge";
    let emit = |_: Value| {};
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let window_id = window_of_pid(session, pid).expect("Edge did not open a window");
        assert!(channel_answers(pid), "the page channel does not answer");
        assert_eq!(unsafe { GetForegroundWindow() }, foreground, "starting Edge took the foreground");
        run_action(&emit, session, "attach", &json!({ "window_id": window_id })).unwrap();
        pause(1500);
        let hwnd = to_hwnd(window_id as isize);
        let before = capture(hwnd).unwrap();
        let name = ref_for(&emit, session, "Name field");
        let typed = run_action(&emit, session, "type", &json!({ "ref": name, "text": "through the pipe" })).unwrap();
        assert_eq!(typed["delivered_via"], json!("page channel"));
        pause(800);
        let picture = std::env::temp_dir().join("evo-page-edge.png");
        eprintln!("page state: {}", channel_for(pid).map(|c| c.debug_state(&window_title(hwnd), &picture)).unwrap_or_default());
        assert_eq!(page_report(hwnd).get("name").map(String::as_str), Some("through the pipe"));
        assert_ne!(capture(hwnd).unwrap().as_raw(), before.as_raw(), "the picture did not change");
    }));
    detach(session);
    kill_tree(pid);
    let _ = std::fs::remove_dir_all(&profile);
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

/// `open_app` starts only what the catalog lists, in the background; `close_app`
/// asks the window to close; `kill_app` ends what did not (spec FR-23). Skipped
/// when Excel is already running, so a workbook of the person's is never touched.
#[test]
#[ignore = "opens an Excel window on the local desktop"]
fn opens_closes_and_kills_from_the_catalog_only() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let running = top_level_windows().into_iter().any(|hwnd| file_name(&process_image_path(app_process(hwnd, window_pid(hwnd))).unwrap_or_default()).eq_ignore_ascii_case("excel.exe"));
    if running {
        eprintln!("Excel is running: skipped so the person's workbooks are not touched");
        return;
    }
    let session = "live-lifecycle";
    let emit = |_: Value| {};
    let refused = run_action(&emit, session, "open_app", &json!({ "exe": r"C:\Windows\System32\cmd.exe" }));
    assert!(refused.is_err(), "a path was started: {refused:?}");

    let opened = run_action(&emit, session, "open_app", &json!({ "exe": "excel.exe" })).unwrap();
    eprintln!("opened: {opened}");
    let window_id = opened["window"]["id"].as_u64().expect("Excel opened no window");
    assert_eq!(opened["page"], json!(false), "Excel is not a web app");
    pause(2000);
    let closed = run_action(&emit, session, "close_app", &json!({ "window_id": window_id })).unwrap();
    eprintln!("closed: {closed}");
    pause(2000);
    let hwnd = to_hwnd(window_id as isize);
    if unsafe { IsWindow(Some(hwnd)) }.as_bool() {
        let killed = run_action(&emit, session, "kill_app", &json!({ "window_id": window_id })).unwrap();
        eprintln!("killed: {killed}");
        pause(1000);
    }
    assert!(!unsafe { IsWindow(Some(hwnd)) }.as_bool(), "the Excel window is still open");
    // Excel can linger without a window after it closed; end the copy this
    // test started (no other was running).
    if let Some(pid) = opened["window"]["pid"].as_u64() {
        kill_tree(pid as u32);
    }
}

/// Where an action's time goes (NFR-4): each step of each action on the
/// WinForms grid probe, with `EVO_COMPUTER_TRACE` spans.
#[test]
#[ignore = "opens a WinForms window on the local desktop"]
fn times_each_step_on_the_grid_probe() {
    std::env::set_var("EVO_COMPUTER_TRACE", "1");
    with_winforms_probe("computer_app_grid.ps1", "grid-probe", "live-timing", |emit, session, _| {
        let step = |label: &str, action: &str, params: Value| {
            eprintln!("{label}:");
            let started = std::time::Instant::now();
            let result = run_action(emit, session, action, &params);
            eprintln!("  total {} ms ({})", started.elapsed().as_millis(), result.as_ref().map(|_| "ok").unwrap_or("error"));
        };
        for round in 0..2 {
            eprintln!("── round {round}");
            step("snapshot", "snapshot", json!({}));
            step("find", "find", json!({ "query": "Region" }));
            step("click", "click", json!({ "x": 100, "y": 60 }));
            step("key down", "key", json!({ "key": "down" }));
            step("key ctrl+home", "key", json!({ "key": "ctrl+home" }));
            step("type", "type", json!({ "text": "ab" }));
            step("key escape", "key", json!({ "key": "escape" }));
        }
    });
}
