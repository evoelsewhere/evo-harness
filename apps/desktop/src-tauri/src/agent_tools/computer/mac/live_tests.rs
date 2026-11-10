//! Drives a real TextEdit window, and captures a parked Chrome one. Opt-in because it opens a window on the
//! desktop it runs on, and it needs evo's test binary (or the terminal
//! running it) to have Accessibility and Screen Recording access:
//!
//! `bun run test:shell computer::mac::live_tests -- --ignored --nocapture`
//!
//! It opens its own document in TextEdit without bringing TextEdit forward,
//! and checks the claim this module is built on: the frontmost app and the
//! user's cursor do not change while the agent works.

use super::*;

/// Dispatch by wire name, the way the agent's calls arrive.
fn run_action(emit: &dyn Fn(Value), session: &str, action: &str, params: &Value) -> Result<Value, String> {
    dispatch(emit, session, action.parse()?, params)
}

fn cursor() -> CGPoint {
    CGEvent::new(CGEventSource::new(CGEventSourceStateID::CombinedSessionState).unwrap())
        .unwrap()
        .location()
}

fn assert_frame_has_detail(label: &str, data: &str) {
    let bytes = BASE64.decode(data).unwrap();
    let image = image::load_from_memory(&bytes).unwrap().to_rgb8();
    let mut darkest = u8::MAX;
    let mut lightest = u8::MIN;
    for pixel in image.pixels() {
        let luminance = ((u16::from(pixel[0]) + u16::from(pixel[1]) + u16::from(pixel[2])) / 3) as u8;
        darkest = darkest.min(luminance);
        lightest = lightest.max(luminance);
    }
    assert!(
        lightest.saturating_sub(darkest) > 40,
        "{label} is blank: luminance range {darkest}..{lightest}"
    );
}

/// What a live test leaves on the desktop, put away when the test ends — also
/// when it panics, so a failed run leaves no window parked in a corner and no
/// test browser running.
struct Leftovers {
    session: &'static str,
    dir: std::path::PathBuf,
    /// The driven app, once attached.
    pid: Option<i32>,
    /// End the process (a test's own Chrome) rather than close its windows.
    kill: bool,
    /// Windows of the app to close: their titles contain one of these.
    titles: Vec<String>,
}

/// Close a test document, answering "Don't Save" if the edited document asks.
fn close_without_saving(window: &Ax) {
    let Some(close) = window.element("AXCloseButton") else {
        return;
    };
    let _ = close.perform("AXPress");
    for _ in 0..10 {
        pause(150);
        let sheets: Vec<Ax> = window.elements("AXChildren").into_iter().filter(|child| child.role() == "AXSheet").collect();
        let button = sheets.iter().flat_map(|sheet| sheet.elements("AXChildren")).find(|child| {
            child.role() == "AXButton"
                && matches!(child.label().as_str(), "Don't Save" | "Delete" | "Revert Changes" | "Don’t Save")
        });
        if let Some(button) = button {
            let _ = button.perform("AXPress");
            return;
        }
    }
}

impl Drop for Leftovers {
    fn drop(&mut self) {
        let _ = dispatch(&|_| {}, self.session, Action::Detach, &json!({}));
        if let Some(pid) = self.pid {
            if self.kill {
                // SAFETY: kill(2) touches no memory. Waited for, so the
                // browser no longer writes into the folder removed below.
                unsafe { libc::kill(pid, libc::SIGTERM) };
                for _ in 0..50 {
                    if unsafe { libc::kill(pid, 0) } != 0 {
                        break;
                    }
                    pause(100);
                }
            } else if let Some(app) = Ax::application(pid) {
                for window in app.elements("AXWindows") {
                    let title = window.string("AXTitle").unwrap_or_default();
                    if self.titles.iter().any(|wanted| title.contains(wanted.as_str())) {
                        close_without_saving(&window);
                    }
                }
            }
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
#[ignore = "opens a TextEdit document on the local desktop"]
fn drives_textedit_in_the_background() {
    assert!(accessibility_trusted(false), "grant Accessibility to the terminal running the tests");
    let test_id = std::process::id();
    let dir = std::env::temp_dir().join(format!("evo-computer-app-{test_id}"));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("computer-app-probe-{test_id}.txt"));
    std::fs::write(&file, "start\n").unwrap();
    let front_before = workspace_frontmost_pid();
    let cursor_before = cursor();
    // -g: open without bringing TextEdit to the front.
    let status = std::process::Command::new("open")
        .args(["-g", "-a", "TextEdit"])
        .arg(&file)
        .status()
        .unwrap();
    assert!(status.success());
    let session = "live-test";
    let mut leftovers = Leftovers { session, dir: dir.clone(), pid: None, kill: false, titles: vec!["computer-app-probe".into(), "computer-app-other".into()] };
    let emit = |_: Value| {};
    let mut attached = Err(String::new());
    for _ in 0..40 {
        pause(250);
        attached = run_action(&emit, session, "attach", &json!({ "title": format!("computer-app-probe-{test_id}"), "hide": true }));
        if attached.is_ok() {
            break;
        }
    }
    let attached = attached.expect("attach to the TextEdit document");
    leftovers.pid = attached["window"]["pid"].as_i64().map(|pid| pid as i32);
    println!("attached: {attached}");
    // A second document, opened last, becomes TextEdit's main window; the
    // agent's input must still reach the attached one.
    let other = dir.join(format!("computer-app-other-{test_id}.txt"));
    std::fs::write(&other, "other\n").unwrap();
    let status = std::process::Command::new("open").args(["-g", "-a", "TextEdit"]).arg(&other).status().unwrap();
    assert!(status.success());
    pause(1500);

    let snapshot = run_action(&emit, session, "snapshot", &json!({})).unwrap();
    let snapshot = snapshot.as_str().unwrap().to_string();
    println!("{snapshot}");
    let text_area = snapshot
        .lines()
        .find(|line| line.contains("- TextArea"))
        .and_then(|line| line.split("[ref=").nth(1))
        .and_then(|rest| rest.split(']').next())
        .expect("the document's text area")
        .to_string();

    let typed = run_action(&emit, session, "type", &json!({ "ref": text_area, "text": "hello from evo" })).unwrap();
    println!("type: {typed}");
    assert_eq!(typed["confirmed"], json!(true));

    // The text is in the document, read back through accessibility. Saving
    // it would need ⌘S, a menu command TextEdit only runs while active:
    // `refuses_menu_commands_that_need_activation` covers that.
    let read_back = run_action(&emit, session, "snapshot", &json!({})).unwrap();
    assert!(read_back.as_str().unwrap().contains("hello from evo"), "the typed text is not in the document");

    let screenshot = run_action(&emit, session, "screenshot", &json!({})).unwrap();
    assert!(screenshot["width"].as_u64().unwrap() > 100);
    assert_frame_has_detail("screenshot", screenshot["data"].as_str().unwrap());
    let preview = preview_frame(session, 480).unwrap();
    assert_frame_has_detail("preview", preview["data"].as_str().unwrap());

    let closing = run_action(&emit, session, "find", &json!({ "query": "close" })).unwrap();
    println!("{closing}");
    run_action(&emit, session, "detach", &json!({})).unwrap();

    assert_eq!(workspace_frontmost_pid(), front_before, "the frontmost app changed");
    // Only reported: the agent never moves the real cursor, but a user
    // working while the test runs does.
    let cursor_after = cursor();
    println!(
        "cursor before ({}, {}), after ({}, {})",
        cursor_before.x, cursor_before.y, cursor_after.x, cursor_after.y
    );
    // Close the document windows the test opened.
    let app = Ax::application(
        attached["window"]["pid"].as_i64().unwrap() as i32,
    )
    .unwrap();
    for window in app.elements("AXWindows") {
        let title = window.string("AXTitle").unwrap_or_default();
        if title.contains("computer-app-probe") || title.contains("computer-app-other") {
            close_without_saving(&window);
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// FR-29: ⌘S on a background TextEdit, and its Save item invoked by ref, are
/// refused with the reason; the frontmost app never changes.
#[test]
#[ignore = "opens a TextEdit document on the local desktop"]
fn refuses_menu_commands_that_need_activation() {
    assert!(accessibility_trusted(false), "grant Accessibility to the terminal running the tests");
    let test_id = std::process::id();
    let dir = std::env::temp_dir().join(format!("evo-computer-app-menu-{test_id}"));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("computer-app-menu-{test_id}.txt"));
    std::fs::write(&file, "start\n").unwrap();
    let front_before = workspace_frontmost_pid();
    let status = std::process::Command::new("open").args(["-g", "-a", "TextEdit"]).arg(&file).status().unwrap();
    assert!(status.success());
    let session = "live-test-menu";
    let mut leftovers = Leftovers { session, dir: dir.clone(), pid: None, kill: false, titles: vec!["computer-app-menu".into()] };
    let emit = |_: Value| {};
    let mut attached = Err(String::new());
    for _ in 0..40 {
        pause(250);
        attached = run_action(&emit, session, "attach", &json!({ "title": format!("computer-app-menu-{test_id}"), "hide": true }));
        if attached.is_ok() {
            break;
        }
    }
    let attached = attached.expect("attach to the TextEdit document");
    leftovers.pid = attached["window"]["pid"].as_i64().map(|pid| pid as i32);
    let pid = attached["window"]["pid"].as_i64().unwrap() as i32;
    assert_ne!(workspace_frontmost_pid(), Some(pid), "TextEdit must be in the background for this test");

    let by_key = run_action(&emit, session, "key", &json!({ "key": "cmd+s" }));
    println!("cmd+s: {by_key:?}");
    let found = run_action(&emit, session, "find", &json!({ "query": "save" })).unwrap();
    println!("find save: {found}");
    let save_ref = found
        .as_str()
        .unwrap()
        .lines()
        .find(|line| line.contains("MenuItem \"Save") && line.contains("(menu bar)"))
        .and_then(|line| line.split("[ref=").nth(1))
        .and_then(|rest| rest.split(']').next())
        .map(str::to_string);
    let by_item = save_ref.map(|reference| run_action(&emit, session, "invoke", &json!({ "ref": reference })));
    println!("invoke Save: {by_item:?}");
    let front_after = workspace_frontmost_pid();

    run_action(&emit, session, "detach", &json!({})).unwrap();
    if let Some(app) = Ax::application(pid) {
        for window in app.elements("AXWindows") {
            if window.string("AXTitle").unwrap_or_default().contains("computer-app-menu") {
                close_without_saving(&window);
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(front_after, front_before, "the frontmost app changed");
    let error = by_key.expect_err("cmd+s on a background TextEdit must be refused");
    assert!(error.contains("never activates"), "{error}");
    if let Some(by_item) = by_item {
        let error = by_item.expect_err("the Save menu item must be refused too");
        assert!(error.contains("never activates"), "{error}");
    }
}

/// The share of the frame's pixels close to `color`.
fn share_of_color(data: &str, color: [u8; 3]) -> f64 {
    let bytes = BASE64.decode(data).unwrap();
    let image = image::load_from_memory(&bytes).unwrap().to_rgb8();
    let close = image
        .pixels()
        .filter(|pixel| (0..3).all(|channel| pixel[channel].abs_diff(color[channel]) < 40))
        .count();
    close as f64 / (image.width() * image.height()).max(1) as f64
}

#[test]
#[ignore = "opens a separate Google Chrome instance on the local desktop"]
fn captures_a_parked_chrome_window() {
    assert!(accessibility_trusted(false), "grant Accessibility to the terminal running the tests");
    let test_id = std::process::id();
    let dir = std::env::temp_dir().join(format!("evo-computer-app-chrome-{test_id}"));
    std::fs::create_dir_all(&dir).unwrap();
    let page = dir.join("probe.html");
    let title = format!("evo-chrome-probe-{test_id}");
    std::fs::write(
        &page,
        format!("<title>{title}</title><body style='margin:0;background:#1e6fd9'><h1 style='color:#fff'>probe</h1>"),
    )
    .unwrap();
    // Its own profile: a separate Chrome process the test can quit, which
    // leaves the user's Chrome alone.
    let status = std::process::Command::new("open")
        .args(["-g", "-n", "-a", "Google Chrome", "--args", "--no-first-run", "--no-default-browser-check"])
        .arg(format!("--user-data-dir={}", dir.join("profile").display()))
        .arg(page.to_str().unwrap())
        .status()
        .unwrap();
    assert!(status.success());
    let session = "live-test-chrome";
    let mut leftovers = Leftovers { session, dir: dir.clone(), pid: None, kill: true, titles: Vec::new() };
    let emit = |_: Value| {};
    let mut attached = Err(String::new());
    for _ in 0..60 {
        pause(250);
        attached = run_action(&emit, session, "attach", &json!({ "title": title, "hide": true }));
        if attached.is_ok() {
            break;
        }
    }
    let attached = attached.expect("attach to the Chrome probe window");
    leftovers.pid = attached["window"]["pid"].as_i64().map(|pid| pid as i32);
    println!("attached: {attached}");
    assert_eq!(attached["window"]["web_content"], json!(true));
    // Long enough for Chromium to stop painting the parked window.
    pause(2_000);

    let blue = [0x1e, 0x6f, 0xd9];
    let screenshot = run_action(&emit, session, "screenshot", &json!({})).unwrap();
    let screenshot_blue = share_of_color(screenshot["data"].as_str().unwrap(), blue);
    let started = Instant::now();
    let mut preview_blue = 0.0;
    for _ in 0..12 {
        let preview = preview_frame(session, 480).unwrap();
        preview_blue = share_of_color(preview["data"].as_str().unwrap(), blue);
    }
    println!(
        "screenshot {:.0}% page colour, preview {:.0}%, 12 preview frames in {:?}",
        screenshot_blue * 100.0,
        preview_blue * 100.0,
        started.elapsed()
    );

    let pid = attached["window"]["pid"].as_i64().unwrap();
    run_action(&emit, session, "detach", &json!({})).unwrap();
    let _ = std::process::Command::new("kill").arg(pid.to_string()).status();
    pause(500);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(screenshot_blue > 0.5, "the parked Chrome screenshot does not show the page");
    assert!(preview_blue > 0.5, "the parked Chrome preview does not show the page");
}

#[test]
#[ignore = "opens a TextEdit document on the local desktop"]
fn puts_back_a_window_parked_by_a_run_that_crashed() {
    assert!(accessibility_trusted(false), "grant Accessibility to the terminal running the tests");
    let test_id = std::process::id();
    let dir = std::env::temp_dir().join(format!("evo-parked-{test_id}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let journal = crate::agent_tools::computer::MemoryJournal::default();
    recover_stranded(Box::new(journal.clone()));
    let recorded = || -> Vec<Stranded> {
        journal.load().iter().filter_map(|(window, pid, placement)| stranded_from_row(*window, *pid, placement)).collect()
    };
    let document = dir.join(format!("computer-app-stranded-{test_id}.txt"));
    std::fs::write(&document, "stranded\n").unwrap();
    let status = std::process::Command::new("open").args(["-g", "-a", "TextEdit"]).arg(&document).status().unwrap();
    assert!(status.success());
    let session = "live-test-stranded";
    let mut leftovers = Leftovers { session, dir: dir.clone(), pid: None, kill: false, titles: vec!["computer-app-stranded".into()] };
    let emit = |_: Value| {};
    let mut attached = Err(String::new());
    for _ in 0..40 {
        pause(250);
        attached = run_action(&emit, session, "attach", &json!({ "title": format!("computer-app-stranded-{test_id}"), "hide": true }));
        if attached.is_ok() {
            break;
        }
    }
    let attached = attached.expect("attach to the TextEdit document");
    leftovers.pid = attached["window"]["pid"].as_i64().map(|pid| pid as i32);
    let window_id = attached["window"]["id"].as_u64().unwrap() as u32;
    let parked_at = cg_window(window_id).unwrap().bounds;
    assert!(mostly_off_screen(&parked_at), "the window was not parked: {parked_at:?}");
    let on_disk = recorded();
    assert!(on_disk.iter().any(|entry| entry.window_id == window_id), "the parked window was not recorded");

    // The crash: evo's memory is gone, the window is not back.
    registry().attached.remove(session);
    stranded().clear();
    // The next start.
    stranded().extend(on_disk.iter().copied());
    let pid = attached["window"]["pid"].as_i64().unwrap() as i32;
    let entry = on_disk.iter().find(|entry| entry.window_id == window_id).copied().unwrap();
    let reachable = Ax::application(pid).and_then(|app| ax_window(&app, window_id)).is_some();
    println!("recorded {entry:?}; reachable through accessibility: {reachable}");
    put_back_stranded(on_disk);
    let ax_position = Ax::application(pid).and_then(|app| ax_window(&app, window_id)).and_then(|w| w.position());
    println!("right after: accessibility says {ax_position:?}, window server says {:?}", cg_window(window_id).map(|w| w.bounds));

    // A window accessibility could not reach is put back by a retry.
    let mut back_at = cg_window(window_id).unwrap().bounds;
    for _ in 0..30 {
        if !mostly_off_screen(&back_at) {
            break;
        }
        pause(200);
        back_at = cg_window(window_id).unwrap().bounds;
    }
    println!("back at {back_at:?}");
    let still_recorded = recorded().iter().any(|entry| entry.window_id == window_id);
    if let Some(window) = Ax::application(attached["window"]["pid"].as_i64().unwrap() as i32)
        .and_then(|app| ax_window(&app, window_id))
    {
        if let Some(close) = window.element("AXCloseButton") {
            let _ = close.perform("AXPress");
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!mostly_off_screen(&back_at), "the window was left parked: {back_at:?}");
    assert!(!still_recorded, "a window put back is still recorded");
}

/// The first `[ref=…]` in a `find` result.
fn first_ref(found: &Value) -> Option<String> {
    let text = found.as_str()?;
    let start = text.find("[ref=")? + 5;
    let end = start + text[start..].find(']')?;
    Some(text[start..end].to_string())
}

/// Windows of `pid` other than `window_id` that the user can see: menus,
/// pop-ups and panels the app opened on the user's screen.
fn visible_popups(pid: i32, window_id: u32) -> Vec<CgWindow> {
    cg_windows(kCGWindowListOptionAll, kCGNullWindowID)
        .into_iter()
        .filter(|window| window.pid == pid && window.id != window_id && window.onscreen)
        .filter(|window| !window.bounds.is_empty() && !mostly_off_screen(&window.bounds))
        .collect()
}

/// Probes what a parked Chrome window takes through each input channel,
/// and whether any of it shows on the user's screen. Prints a report, then
/// fails on the scenarios that did not hold.
#[test]
#[ignore = "opens a separate Google Chrome instance on the local desktop"]
fn drives_a_parked_chrome_form() {
    assert!(accessibility_trusted(false), "grant Accessibility to the terminal running the tests");
    let test_id = std::process::id();
    let dir = std::env::temp_dir().join(format!("evo-computer-app-chrome-form-{test_id}"));
    std::fs::create_dir_all(&dir).unwrap();
    let page = dir.join("form.html");
    let title = format!("evo-chrome-form-{test_id}");
    std::fs::write(
        &page,
        format!(
            "<title>{title}</title><body style='font:16px sans-serif'>\
             <button onclick=\"out.textContent='clicks:'+(++window.n||(window.n=1))\">Press me</button>\
             <p id=out>clicks:0</p>\
             <form onsubmit=\"event.preventDefault();sent.textContent='submitted:'+who.value\">\
             <input id=who aria-label='Your name'></form><p id=sent>submitted:none</p>\
             <select aria-label='Colour'><option>Red<option>Green<option>Blue</select>"
        ),
    )
    .unwrap();
    let status = std::process::Command::new("open")
        .args(["-g", "-n", "-a", "Google Chrome", "--args", "--no-first-run", "--no-default-browser-check"])
        .arg(format!("--user-data-dir={}", dir.join("profile").display()))
        .arg(page.to_str().unwrap())
        .status()
        .unwrap();
    assert!(status.success());
    let front_before = workspace_frontmost_pid();
    let session = "live-test-chrome-form";
    let mut leftovers = Leftovers { session, dir: dir.clone(), pid: None, kill: true, titles: Vec::new() };
    let emit = |_: Value| {};
    let run = |action: &str, params: Value| run_action(&emit, session, action, &params);
    let mut attached = Err(String::new());
    for _ in 0..60 {
        pause(250);
        attached = run("attach", json!({ "title": title, "hide": true }));
        if attached.is_ok() {
            break;
        }
    }
    let attached = attached.expect("attach to the Chrome form");
    leftovers.pid = attached["window"]["pid"].as_i64().map(|pid| pid as i32);
    let pid = attached["window"]["pid"].as_i64().unwrap() as i32;
    let window_id = attached["window"]["id"].as_u64().unwrap() as u32;
    // Until the page's tree has filled in.
    for _ in 0..40 {
        let found = run("find", json!({ "query": "Press me" })).unwrap_or_default();
        if found.as_str().is_some_and(|text| text.contains("[ref=")) {
            break;
        }
        pause(250);
    }

    let mut report: Vec<(&str, bool, String)> = Vec::new();
    let find = |query: &str| run("find", json!({ "query": query })).unwrap_or_else(|error| json!(error));
    let shows = |query: &str, expected: &str| {
        let found = find(query);
        (found.as_str().unwrap_or_default().contains(expected), found.to_string())
    };

    // A button pressed by ref.
    let button = first_ref(&find("Press me"));
    let clicked = button.as_ref().map(|reference| run("click", json!({ "ref": reference })));
    pause(300);
    let (ok, detail) = shows("clicks:", "clicks:1");
    report.push(("click a button", ok, format!("{clicked:?} → {detail}")));

    // Text typed into a field, read back.
    let field = first_ref(&find("Your name"));
    let typed = field.as_ref().map(|reference| run("type", json!({ "ref": reference, "text": "Ada" })));
    pause(300);
    let (ok, detail) = shows("Your name", "Ada");
    report.push(("type into a field", ok, format!("{typed:?} → {detail}")));

    // Enter in the field submits its form.
    let entered = run("key", json!({ "key": "enter" }));
    pause(500);
    let (ok, detail) = shows("submitted:", "submitted:Ada");
    report.push(("enter submits the form", ok, format!("{entered:?} → {detail}")));

    // A <select>: Chrome opens a native menu of its own for it.
    let select = first_ref(&find("Colour"));
    let opened = select.as_ref().map(|reference| run("click", json!({ "ref": reference })));
    let mut popups = Vec::new();
    for _ in 0..6 {
        pause(200);
        popups.extend(visible_popups(pid, window_id));
    }
    let menu = find("Green");
    let _ = run("key", json!({ "key": "escape" }));
    pause(300);
    report.push((
        "a <select>'s menu stays off the user's screen",
        popups.is_empty(),
        format!("{opened:?}; visible: {:?}; menu: {menu}", popups.iter().map(|w| (w.id, w.layer, w.bounds)).collect::<Vec<_>>()),
    ));
    let set = select.as_ref().map(|reference| run("set_value", json!({ "ref": reference, "value": "Blue" })));
    pause(300);
    let (ok, detail) = shows("Colour", "Blue");
    let ok = ok && matches!(set, Some(Ok(_)));
    report.push(("set a <select>'s value", ok, format!("{set:?} → {detail}")));
    let select = first_ref(&find("Colour"));
    let picked = select.as_ref().map(|reference| run("click", json!({ "ref": reference, "menu_item": "green" })));
    pause(300);
    let (ok, detail) = shows("Colour", "Green");
    let ok = ok && matches!(picked, Some(Ok(_)));
    report.push(("pick a <select> option with menu_item", ok, format!("{picked:?} → {detail}")));
    let left_open = visible_popups(pid, window_id);
    report.push((
        "no <select> menu is left on the user's screen",
        left_open.is_empty(),
        format!("{:?}", left_open.iter().map(|w| (w.id, w.layer, w.bounds)).collect::<Vec<_>>()),
    ));

    // A context menu on the page.
    let context = button.as_ref().map(|reference| run("click", json!({ "ref": reference, "button": "right" })));
    let mut popups = Vec::new();
    for _ in 0..6 {
        pause(200);
        popups.extend(visible_popups(pid, window_id));
    }
    let _ = run("key", json!({ "key": "escape" }));
    pause(300);
    report.push((
        "a context menu stays off the user's screen",
        popups.is_empty(),
        format!("{context:?}; visible: {:?}", popups.iter().map(|w| (w.id, w.layer, w.bounds)).collect::<Vec<_>>()),
    ));
    // A context menu item picked in the same click: Reload resets the page.
    let button = first_ref(&find("Press me"));
    let reloaded = button.as_ref().map(|reference| run("click", json!({ "ref": reference, "button": "right", "menu_item": "Reload" })));
    pause(1_500);
    let (ok, detail) = shows("clicks:", "clicks:0");
    let ok = ok && matches!(reloaded, Some(Ok(_)));
    report.push(("pick a context menu item with menu_item", ok, format!("{reloaded:?} → {detail}")));

    let parked = cg_window(window_id).is_some_and(|window| mostly_off_screen(&window.bounds));
    report.push(("the window stays parked", parked, format!("{:?}", cg_window(window_id).map(|w| w.bounds))));
    let front_after = workspace_frontmost_pid();
    report.push(("the frontmost app does not change", front_after == front_before, format!("{front_before:?} → {front_after:?}")));

    let _ = run("detach", json!({}));
    let _ = std::process::Command::new("kill").arg(pid.to_string()).status();
    pause(500);
    let _ = std::fs::remove_dir_all(&dir);

    println!("\nparked Chrome form:");
    for (name, ok, detail) in &report {
        println!("  [{}] {name}\n        {detail}", if *ok { "ok" } else { "FAIL" });
    }
    let failed: Vec<&str> = report.iter().filter(|(_, ok, _)| !ok).map(|(name, _, _)| *name).collect();
    assert!(failed.is_empty(), "did not hold: {failed:?}");
}

#[test]
#[ignore = "reads the local Applications folders and running apps"]
fn lists_apps_with_icons() {
    let apps = list_apps();
    let apps = apps["apps"].as_array().unwrap();
    println!("{} apps", apps.len());
    assert!(apps.iter().any(|app| app["exe"] == json!("textedit")));
    assert!(apps.iter().filter(|app| app["icon"].is_string()).count() > apps.len() / 2);
}
