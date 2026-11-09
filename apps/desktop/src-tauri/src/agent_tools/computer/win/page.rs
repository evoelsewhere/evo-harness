//! The page channel (docs/features/computer-app-control FR-16, NFR-6; plan R4).
//!
//! A web-based app evo starts itself — Chromium (Edge, Chrome) or an Electron
//! app — is started with a DevTools pipe: two anonymous pipes whose child ends
//! only that process inherits (`PROC_THREAD_ATTRIBUTE_HANDLE_LIST`), named to it
//! with `--remote-debugging-pipe --remote-debugging-io-pipes=<read>,<write>`.
//! No network port is opened. Keys and text then go straight into the page as
//! DevTools input events, which reach it with the window off screen and never
//! focused: posted shortcuts did not reach VS Code 1.145 (tasks Phase 1), these
//! do. The app is also told not to stop painting when hidden
//! (`--disable-features=CalculateNativeWinOcclusion`).
//!
//! The channel is internal: the agent never sends DevTools commands itself.
//! Only key, text and screenshot methods are used here — no script
//! evaluation, storage or network (spec NFR-6). It ends with the app.

use super::*;

use std::fs::File;
use std::io::{Read, Write};
use std::os::windows::io::FromRawHandle;
use std::sync::atomic::AtomicU64;

use windows::Win32::Foundation::{SetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT, HANDLE_FLAGS};
use windows::Win32::Security::SECURITY_ATTRIBUTES;
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, InitializeProcThreadAttributeList, UpdateProcThreadAttribute,
    EXTENDED_STARTUPINFO_PRESENT, LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
    STARTF_USESHOWWINDOW, STARTUPINFOEXW, STARTUPINFOW,
};

/// How long one DevTools command may take.
const CALL_WAIT: Duration = Duration::from_secs(10);

// ── Which programs get a page channel ───────────────────────────────────────

/// Chromium browsers by executable name.
const CHROMIUM_BROWSERS: &[&str] = &["msedge.exe", "chrome.exe", "brave.exe", "vivaldi.exe", "chromium.exe"];

/// Whether `program` is a Chromium browser or an Electron app (its folder has
/// `resources\app.asar` or `resources\app`), so it can take a page channel.
pub(super) fn is_chromium_program(program: &str) -> bool {
    let path = std::path::Path::new(program);
    let exe = path.file_name().and_then(|name| name.to_str()).unwrap_or("").to_lowercase();
    if CHROMIUM_BROWSERS.contains(&exe.as_str()) {
        return true;
    }
    let Some(folder) = path.parent() else {
        return false;
    };
    let resources = folder.join("resources");
    resources.join("app.asar").is_file() || resources.join("app").is_dir()
}

/// A Start menu shortcut's program, arguments and working folder.
pub(super) struct Shortcut {
    pub(super) program: String,
    pub(super) arguments: String,
    pub(super) folder: Option<String>,
}

pub(super) fn read_shortcut(link: &str) -> Option<Shortcut> {
    use windows::core::HSTRING;
    use windows::Win32::System::Com::{IPersistFile, STGM_READ};
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    let text = |buffer: &[u16]| {
        let len = buffer.iter().position(|&unit| unit == 0).unwrap_or(buffer.len());
        String::from_utf16_lossy(&buffer[..len])
    };
    unsafe {
        let shell_link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let file: IPersistFile = shell_link.cast().ok()?;
        file.Load(&HSTRING::from(link), STGM_READ).ok()?;
        let mut program = [0u16; 1024];
        shell_link.GetPath(&mut program, std::ptr::null_mut(), 0).ok()?;
        let mut arguments = [0u16; 2048];
        let _ = shell_link.GetArguments(&mut arguments);
        let mut folder = [0u16; 1024];
        let _ = shell_link.GetWorkingDirectory(&mut folder);
        let folder = text(&folder);
        Some(Shortcut { program: text(&program), arguments: text(&arguments), folder: (!folder.is_empty()).then_some(folder) })
    }
}

// ── The channel ─────────────────────────────────────────────────────────────

/// One app's DevTools pipe.
pub(super) struct Channel {
    writer: Mutex<File>,
    pending: Mutex<HashMap<u64, mpsc::Sender<Value>>>,
    next: AtomicU64,
    /// The page target's flat session, once attached.
    page: Mutex<Option<String>>,
    /// The reader saw the pipe close: the app is gone.
    closed: std::sync::atomic::AtomicBool,
}

/// Channels by the process id of the app's main (browser) process.
static CHANNELS: Lazy<Mutex<HashMap<u32, Arc<Channel>>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// The page channel of the app whose window belongs to `pid`, while it lives.
pub(super) fn channel_for(pid: u32) -> Option<Arc<Channel>> {
    let mut channels = CHANNELS.lock().ok()?;
    let channel = channels.get(&pid).cloned()?;
    if channel.closed.load(Ordering::Relaxed) {
        channels.remove(&pid);
        return None;
    }
    Some(channel)
}

/// Forget `pid`'s channel without a pipe to close it (the app was killed).
pub(super) fn forget_channel(pid: u32) {
    if let Ok(mut channels) = CHANNELS.lock() {
        channels.remove(&pid);
    }
}

impl Channel {
    fn start(reader: File, writer: File) -> Arc<Self> {
        let channel = Arc::new(Channel {
            writer: Mutex::new(writer),
            pending: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
            page: Mutex::new(None),
            closed: std::sync::atomic::AtomicBool::new(false),
        });
        let weak = Arc::downgrade(&channel);
        let _ = std::thread::Builder::new().name("computer-app-page".into()).spawn(move || {
            let mut reader = reader;
            let mut buffer = Vec::new();
            let mut chunk = [0u8; 64 * 1024];
            loop {
                let read = match reader.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => read,
                };
                buffer.extend_from_slice(&chunk[..read]);
                while let Some(end) = buffer.iter().position(|&byte| byte == 0) {
                    let message: Vec<u8> = buffer.drain(..=end).collect();
                    let Some(channel) = weak.upgrade() else {
                        return;
                    };
                    if let Ok(value) = serde_json::from_slice::<Value>(&message[..message.len() - 1]) {
                        if let Some(id) = value.get("id").and_then(Value::as_u64) {
                            if let Some(reply) = channel.pending.lock().ok().and_then(|mut pending| pending.remove(&id)) {
                                let _ = reply.send(value);
                            }
                        }
                    }
                }
            }
            if let Some(channel) = weak.upgrade() {
                channel.closed.store(true, Ordering::Relaxed);
                if let Ok(mut pending) = channel.pending.lock() {
                    pending.clear();
                }
            }
        });
        channel
    }

    /// Send one DevTools command and wait for its answer.
    fn call(&self, method: &str, params: Value, session: Option<&str>) -> Result<Value, String> {
        if self.closed.load(Ordering::Relaxed) {
            return Err("The app's page channel closed.".into());
        }
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (reply, answer) = mpsc::channel();
        self.pending.lock().map_err(|_| "page channel lock")?.insert(id, reply);
        let mut message = json!({ "id": id, "method": method, "params": params });
        if let Some(session) = session {
            message["sessionId"] = json!(session);
        }
        let mut bytes = serde_json::to_vec(&message).map_err(|error| error.to_string())?;
        bytes.push(0);
        self.writer
            .lock()
            .map_err(|_| "page channel lock")?
            .write_all(&bytes)
            .map_err(|error| format!("The app's page channel closed: {error}"))?;
        let answer = answer.recv_timeout(CALL_WAIT).map_err(|_| format!("The app did not answer {method}."))?;
        match answer.get("error") {
            Some(error) => Err(format!("{method} failed: {}", error.get("message").and_then(Value::as_str).unwrap_or("error"))),
            None => Ok(answer.get("result").cloned().unwrap_or(Value::Null)),
        }
    }

    /// Test diagnostics only: what the page has focused, and a picture of it.
    #[cfg(test)]
    pub(super) fn debug_state(&self, title: &str, picture: &std::path::Path) -> String {
        let Ok(session) = self.page_session(title) else {
            return "no page session".into();
        };
        let focus = self
            .call(
                "Runtime.evaluate",
                json!({ "expression": "(document.activeElement ? document.activeElement.tagName + '.' + document.activeElement.className.toString().slice(0,60) : 'none') + ' hasFocus=' + document.hasFocus() + ' visibility=' + document.visibilityState", "returnByValue": true }),
                Some(&session),
            )
            .map(|r| r["result"]["value"].to_string())
            .unwrap_or_else(|e| e);
        if let Ok(shot) = self.call("Page.captureScreenshot", json!({ "format": "png" }), Some(&session)) {
            use base64::Engine as _;
            if let Some(bytes) = shot["data"].as_str().and_then(|d| base64::engine::general_purpose::STANDARD.decode(d).ok()) {
                let _ = std::fs::write(picture, bytes);
            }
        }
        focus
    }

    /// The app's page targets (type, title, url), for tests and diagnostics.
    pub(super) fn targets(&self) -> Value {
        self.call("Target.getTargets", json!({}), None).map(|t| t["targetInfos"].clone()).unwrap_or(Value::Null)
    }

    /// The flat session of the page shown in the window titled `title`: the
    /// page target of that title, else the first page that is not DevTools
    /// itself. Attached once, and told it has focus: an app started without
    /// activation never had any, and Chromium drops keys and text to a page
    /// without focus. Focus emulation changes nothing outside the page.
    fn page_session(&self, title: &str) -> Result<String, String> {
        if let Some(session) = self.page.lock().map_err(|_| "page channel lock")?.clone() {
            return Ok(session);
        }
        let mut target = None;
        for _ in 0..20 {
            let targets = self.call("Target.getTargets", json!({}), None)?;
            let pages: Vec<&Value> = targets["targetInfos"]
                .as_array()
                .map(|infos| {
                    infos
                        .iter()
                        .filter(|info| info["type"] == "page" && !info["url"].as_str().unwrap_or("").starts_with("devtools://"))
                        .collect()
                })
                .unwrap_or_default();
            let titled = pages.iter().find(|info| {
                let page_title = info["title"].as_str().unwrap_or("");
                !page_title.is_empty() && (page_title == title || title.contains(page_title))
            });
            target = titled.or(pages.first()).and_then(|info| info["targetId"].as_str().map(str::to_string));
            if target.is_some() {
                break;
            }
            pause(250);
        }
        let target = target.ok_or("The app has no page to send input to.")?;
        let attached = self.call("Target.attachToTarget", json!({ "targetId": target, "flatten": true }), None)?;
        let session = attached["sessionId"].as_str().ok_or("The app's page did not attach.")?.to_string();
        let _ = self.call("Emulation.setFocusEmulationEnabled", json!({ "enabled": true }), Some(&session));
        *self.page.lock().map_err(|_| "page channel lock")? = Some(session.clone());
        Ok(session)
    }

    /// A key or shortcut (`ctrl+n`, `shift+enter`), `repeat` times.
    pub(super) fn press(&self, title: &str, combo: &KeyCombo, repeat: u64) -> Result<(), String> {
        let session = self.page_session(title)?;
        let key = dom_key(&combo.key, combo.shift).ok_or_else(|| format!("Unknown key: {}", combo.key))?;
        let modifiers = (combo.alt as i64) | ((combo.ctrl || combo.cmd) as i64) << 1 | (combo.shift as i64) << 3;
        // A key that types (no Ctrl or Alt) carries its text, as a keyboard's would.
        let text = if combo.ctrl || combo.alt || combo.cmd { None } else { key.text.clone() };
        for _ in 0..repeat {
            interrupted()?;
            let mut down = json!({
                "type": if text.is_some() { "keyDown" } else { "rawKeyDown" },
                "modifiers": modifiers,
                "key": key.key,
                "code": key.code,
                "windowsVirtualKeyCode": key.vk,
                "nativeVirtualKeyCode": key.vk,
            });
            if let Some(text) = &text {
                down["text"] = json!(text);
                down["unmodifiedText"] = json!(text);
            }
            self.call("Input.dispatchKeyEvent", down, Some(&session))?;
            self.call(
                "Input.dispatchKeyEvent",
                json!({ "type": "keyUp", "modifiers": modifiers, "key": key.key, "code": key.code,
                        "windowsVirtualKeyCode": key.vk, "nativeVirtualKeyCode": key.vk }),
                Some(&session),
            )?;
        }
        Ok(())
    }

    /// A mouse click at screen point `point` of the app's top-level window
    /// `window`, as DevTools mouse events in the page's CSS pixels: the page
    /// gets real pointer events and its focus moves where a person's click
    /// would put it (an accessibility press left Edge's focus on the body).
    pub(super) fn click(&self, window: HWND, point: POINT, button: &str, clicks: u64) -> Result<(), String> {
        let session = self.page_session(&window_title(window))?;
        let (x, y) = self.page_point(window, point, &session)?;
        let base = json!({ "x": x, "y": y, "button": button, "modifiers": 0 });
        let mut moved = base.clone();
        moved["type"] = json!("mouseMoved");
        moved["button"] = json!("none");
        self.call("Input.dispatchMouseEvent", moved, Some(&session))?;
        for count in 1..=clicks {
            interrupted()?;
            for kind in ["mousePressed", "mouseReleased"] {
                let mut event = base.clone();
                event["type"] = json!(kind);
                event["clickCount"] = json!(count);
                self.call("Input.dispatchMouseEvent", event, Some(&session))?;
            }
        }
        Ok(())
    }

    /// A screen point as the page's CSS pixels: from the window's client
    /// origin, scaled by the page's device pixel ratio, below whatever browser
    /// toolbar sits above the page.
    fn page_point(&self, window: HWND, point: POINT, session: &str) -> Result<(f64, f64), String> {
        use windows::Win32::Graphics::Gdi::ClientToScreen;
        use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
        let mut origin = POINT::default();
        let mut client = RECT::default();
        unsafe {
            let _ = ClientToScreen(window, &mut origin);
            let _ = GetClientRect(window, &mut client);
        }
        let metrics = self.call("Page.getLayoutMetrics", json!({}), Some(session))?;
        let viewport = &metrics["cssLayoutViewport"];
        let css_width = viewport["clientWidth"].as_f64().filter(|w| *w > 0.0).ok_or("The page reported no size.")?;
        let css_height = viewport["clientHeight"].as_f64().unwrap_or(0.0);
        let ratio = f64::from(client.right - client.left) / css_width;
        let toolbar = (f64::from(client.bottom - client.top) - css_height * ratio).max(0.0);
        Ok((
            f64::from(point.x - origin.x) / ratio,
            (f64::from(point.y - origin.y) - toolbar) / ratio,
        ))
    }

    /// Text into the page's focused field. A line break is Shift+Enter, so a
    /// chat message is not sent (as on the accessibility path).
    pub(super) fn insert_text(&self, title: &str, text: &str) -> Result<usize, String> {
        let session = self.page_session(title)?;
        let mut typed = 0;
        for (index, line) in text.split('\n').enumerate() {
            interrupted()?;
            if index > 0 {
                self.press(title, &KeyCombo { ctrl: false, alt: false, shift: true, win: false, cmd: false, key: "enter".into() }, 1)?;
                typed += 1;
            }
            let line = line.trim_end_matches('\r');
            if !line.is_empty() {
                self.call("Input.insertText", json!({ "text": line }), Some(&session))?;
                typed += line.chars().count();
            }
        }
        Ok(typed)
    }
}

/// A key as DevTools input wants it.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct DomKey {
    pub(super) key: String,
    pub(super) code: String,
    pub(super) vk: i64,
    pub(super) text: Option<String>,
}

/// The DOM `key`, `code`, virtual-key code and typed text of a key name
/// (`a`, `7`, `f5`, `enter`, `pageup`, `,`), with Shift for a letter's case.
pub(super) fn dom_key(name: &str, shift: bool) -> Option<DomKey> {
    let lower = name.to_lowercase();
    let make = |key: &str, code: &str, vk: i64, text: Option<&str>| DomKey { key: key.into(), code: code.into(), vk, text: text.map(str::to_string) };
    let mut chars = lower.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_ascii_lowercase() {
            let shown = if shift { c.to_ascii_uppercase() } else { c }.to_string();
            return Some(make(&shown, &format!("Key{}", c.to_ascii_uppercase()), c.to_ascii_uppercase() as i64, Some(&shown)));
        }
        if c.is_ascii_digit() {
            let s = c.to_string();
            return Some(make(&s, &format!("Digit{c}"), c as i64, Some(&s)));
        }
        let punctuation = match c {
            ',' => Some(("Comma", 188)),
            '.' => Some(("Period", 190)),
            '/' => Some(("Slash", 191)),
            ';' => Some(("Semicolon", 186)),
            '\'' => Some(("Quote", 222)),
            '[' => Some(("BracketLeft", 219)),
            ']' => Some(("BracketRight", 221)),
            '\\' => Some(("Backslash", 220)),
            '-' => Some(("Minus", 189)),
            '=' => Some(("Equal", 187)),
            '`' => Some(("Backquote", 192)),
            _ => None,
        };
        if let Some((code, vk)) = punctuation {
            let s = c.to_string();
            return Some(make(&s, code, vk, Some(&s)));
        }
    }
    if let Some(number) = lower.strip_prefix('f').and_then(|n| n.parse::<i64>().ok()).filter(|n| (1..=24).contains(n)) {
        let name = format!("F{number}");
        return Some(make(&name, &name, 111 + number, None));
    }
    Some(match lower.as_str() {
        "enter" | "return" => make("Enter", "Enter", 13, Some("\r")),
        "tab" => make("Tab", "Tab", 9, None),
        "escape" | "esc" => make("Escape", "Escape", 27, None),
        "backspace" => make("Backspace", "Backspace", 8, None),
        "delete" | "del" => make("Delete", "Delete", 46, None),
        "space" => make(" ", "Space", 32, Some(" ")),
        "up" | "arrowup" => make("ArrowUp", "ArrowUp", 38, None),
        "down" | "arrowdown" => make("ArrowDown", "ArrowDown", 40, None),
        "left" | "arrowleft" => make("ArrowLeft", "ArrowLeft", 37, None),
        "right" | "arrowright" => make("ArrowRight", "ArrowRight", 39, None),
        "home" => make("Home", "Home", 36, None),
        "end" => make("End", "End", 35, None),
        "pageup" | "pgup" => make("PageUp", "PageUp", 33, None),
        "pagedown" | "pgdn" => make("PageDown", "PageDown", 34, None),
        "insert" | "ins" => make("Insert", "Insert", 45, None),
        "plus" => make("+", "Equal", 187, Some("+")),
        _ => return None,
    })
}

// ── Starting an app with its channel ────────────────────────────────────────

/// `"program" arguments`, quoted the way `CommandLineToArgvW` reads it.
fn quote(program: &str) -> String {
    if program.contains(' ') || program.contains('\t') {
        format!("\"{program}\"")
    } else {
        program.to_string()
    }
}

/// An anonymous pipe: (read, write).
fn pipe() -> Result<(HANDLE, HANDLE), String> {
    let attributes = SECURITY_ATTRIBUTES { nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: std::ptr::null_mut(), bInheritHandle: BOOL(1) };
    let (mut read, mut write) = (HANDLE::default(), HANDLE::default());
    unsafe { CreatePipe(&mut read, &mut write, Some(&attributes), 0) }.map_err(|error| format!("could not create a pipe: {error}"))?;
    Ok((read, write))
}

/// Start `program` with `arguments` in `folder`, minimized and without taking
/// the foreground, with a page channel. Returns its process id; the channel is
/// kept by that id ([`channel_for`]).
pub(super) fn start_with_channel(program: &str, arguments: &str, folder: Option<&str>) -> Result<u32, String> {
    use windows::core::{HSTRING, PCWSTR, PWSTR};
    // Commands: evo writes `to_app_write`, the app reads `to_app_read`.
    let (to_app_read, to_app_write) = pipe()?;
    let (from_app_read, from_app_write) = pipe()?;
    // evo's own ends are not inherited by anyone.
    unsafe {
        let _ = SetHandleInformation(to_app_write, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0));
        let _ = SetHandleInformation(from_app_read, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0));
    }
    let command = format!(
        "{} {} --remote-debugging-pipe --remote-debugging-io-pipes={},{} --disable-features=CalculateNativeWinOcclusion",
        quote(program),
        arguments.trim(),
        to_app_read.0 as usize,
        from_app_write.0 as usize
    );
    let mut command: Vec<u16> = command.encode_utf16().chain(std::iter::once(0)).collect();

    // Only the two pipe ends are inherited, nothing else of evo's.
    let handles = [to_app_read, from_app_write];
    let mut size = 0usize;
    unsafe {
        let _ = InitializeProcThreadAttributeList(None, 1, None, &mut size);
    }
    let mut attribute_buffer = vec![0u8; size];
    // Electron ignores the start-up show state and brings its first window to
    // the front, which Windows allowed even with the person active (Phase 3
    // live test). While the app starts, no one may take the front: the lock
    // is lifted once the app's window shows (or after the wait), and the
    // person pressing Alt or clicking lifts it too. Nothing here takes the
    // front; it only keeps it where the person has it (FR-30).
    let front = hold_front();
    let attributes = LPPROC_THREAD_ATTRIBUTE_LIST(attribute_buffer.as_mut_ptr().cast());
    let started = (|| -> Result<PROCESS_INFORMATION, String> {
        unsafe {
            InitializeProcThreadAttributeList(Some(attributes), 1, None, &mut size).map_err(|error| error.to_string())?;
            UpdateProcThreadAttribute(
                attributes,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                Some(handles.as_ptr().cast()),
                std::mem::size_of_val(&handles),
                None,
                None,
            )
            .map_err(|error| error.to_string())?;
            let startup = STARTUPINFOEXW {
                StartupInfo: STARTUPINFOW {
                    cb: std::mem::size_of::<STARTUPINFOEXW>() as u32,
                    dwFlags: STARTF_USESHOWWINDOW,
                    wShowWindow: SW_SHOWMINNOACTIVE.0 as u16,
                    ..Default::default()
                },
                lpAttributeList: attributes,
            };
            let folder = folder.map(HSTRING::from);
            let mut process = PROCESS_INFORMATION::default();
            CreateProcessW(
                &HSTRING::from(program),
                Some(PWSTR(command.as_mut_ptr())),
                None,
                None,
                true,
                EXTENDED_STARTUPINFO_PRESENT,
                None,
                folder.as_ref().map(|folder| PCWSTR(folder.as_ptr())).unwrap_or(PCWSTR::null()),
                &startup.StartupInfo,
                &mut process,
            )
            .map_err(|error| format!("Windows could not start {program}: {error}"))?;
            Ok(process)
        }
    })();
    unsafe {
        DeleteProcThreadAttributeList(attributes);
        // The app has its ends now; evo keeps only its own.
        let _ = CloseHandle(to_app_read);
        let _ = CloseHandle(from_app_write);
    }
    let process = match started {
        Ok(process) => process,
        Err(error) => {
            unsafe {
                let _ = CloseHandle(to_app_write);
                let _ = CloseHandle(from_app_read);
            }
            return Err(error);
        }
    };
    unsafe {
        let _ = CloseHandle(process.hThread);
        let _ = CloseHandle(process.hProcess);
    }
    release_front_when_shown(process.dwProcessId, front);
    let writer = unsafe { File::from_raw_handle(to_app_write.0) };
    let reader = unsafe { File::from_raw_handle(from_app_read.0) };
    let channel = Channel::start(reader, writer);
    if let Ok(mut channels) = CHANNELS.lock() {
        channels.insert(process.dwProcessId, channel);
    }
    Ok(process.dwProcessId)
}

/// End `hold` once a window of `pid` shows, or after the wait for one; the
/// hold's grace covers the app's own focus request right after its window
/// shows (spec FR-30).
fn release_front_when_shown(pid: u32, hold: FrontHold) {
    let wait = move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while std::time::Instant::now() < deadline {
            if top_level_windows().into_iter().any(|hwnd| window_pid(hwnd) == pid && unsafe { IsWindowVisible(hwnd) }.as_bool()) {
                break;
            }
            pause(100);
        }
        drop(hold);
    };
    // Without a thread the hold ends now, with its grace.
    let _ = std::thread::Builder::new().name("computer-app-front".into()).spawn(wait);
}

/// Whether the app answers on its channel (it took the pipes rather than
/// handing the start to a copy already running).
pub(super) fn channel_answers(pid: u32) -> bool {
    channel_for(pid).is_some_and(|channel| channel.call("Browser.getVersion", json!({}), None).is_ok())
}

#[cfg(test)]
mod page_tests {
    use super::*;

    #[test]
    fn maps_key_names_to_dom_keys() {
        assert_eq!(dom_key("n", false).unwrap(), DomKey { key: "n".into(), code: "KeyN".into(), vk: 78, text: Some("n".into()) });
        assert_eq!(dom_key("N", true).unwrap().key, "N");
        assert_eq!(dom_key("f5", false).unwrap(), DomKey { key: "F5".into(), code: "F5".into(), vk: 116, text: None });
        assert_eq!(dom_key("Enter", false).unwrap().text.as_deref(), Some("\r"));
        assert_eq!(dom_key("pagedown", false).unwrap().vk, 34);
        assert_eq!(dom_key(",", false).unwrap().code, "Comma");
        assert!(dom_key("hyper", false).is_none());
    }

    #[test]
    fn recognizes_chromium_browsers_by_name() {
        assert!(is_chromium_program(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"));
        assert!(!is_chromium_program(r"C:\Windows\notepad.exe"));
    }
}
