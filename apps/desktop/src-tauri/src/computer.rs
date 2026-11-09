//! The app side of computer app control (docs/features/computer-app-control,
//! plan 3.2): the journal of windows moved off screen kept in `evo.db`, putting
//! them back at start-up, and handing every window back when evo quits.
//!
//! The agent's tool and the native backend live in `agent_tools/computer`. This
//! file also holds what the card asks of the shell (FR-10 – FR-13): the app's
//! pictures while a card shows it, and the person's own commands — Stop, Allow
//! again, Show the app, Close — and their clicks and keys in the card. None of
//! these commands is the agent's.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tokio_util::sync::CancellationToken;

use crate::agent_tools::computer::{self, Action, Journal};
use crate::db::Db;

/// The journal in `evo.db`'s `computer_parked` table (spec FR-8).
struct DbJournal {
    app: AppHandle,
}

impl Journal for DbJournal {
    fn record(&self, window: i64, pid: i64, placement: &Value) {
        let result = self.app.state::<Db>().with(|conn| evo_store::computer::record(conn, window, pid, placement, None));
        if let Err(error) = result {
            eprintln!("computer app: could not record a window moved off screen: {error}");
        }
    }

    fn forget(&self, window: i64) {
        let _ = self.app.state::<Db>().with(|conn| evo_store::computer::forget(conn, window));
    }

    fn load(&self) -> Vec<(i64, i64, Value)> {
        self.app
            .state::<Db>()
            .with(|conn| evo_store::computer::all(conn))
            .map(|rows| rows.into_iter().map(|row| (row.window, row.pid, row.placement)).collect())
            .unwrap_or_default()
    }
}

/// At start-up: put back the windows a previous run left off screen, and
/// journal moved windows from now on.
pub fn setup(app: &AppHandle) {
    computer::set_app(app.clone());
    computer::recover_stranded(Box::new(DbJournal { app: app.clone() }));
}

/// evo is quitting: hand every controlled window back (FR-7). Windows still
/// out of reach stay in the journal for the next start.
pub fn shutdown() {
    computer::release_all();
}

/// Tells the windows the shell's tool list changed, so each ready chat's engine
/// gets it again (`stores/engine-store.ts`).
pub const TOOLS_CHANGED: &str = "agent-tools://changed";

/// A computer app control setting changed (FR-1): the tool is offered or
/// withdrawn on every chat's next call, and turning the feature off hands every
/// app back. The gate and the policy read their settings on every call.
pub fn settings_changed(app: &AppHandle, key: &str, value: &Value) {
    if key == computer::tool::ENABLED_KEY {
        if value != &json!(true) {
            computer::release_all();
        }
        let _ = app.emit(TOOLS_CHANGED, json!({ "reason": key }));
    }
}


// ── The card's pictures (FR-10, NFR-8) ──────────────────────────────────────

/// `computer://frame`: `{surface, seq, data (base64 JPEG), width, height}`,
/// the shape of `browser://frame` with the card's surface for its tab.
pub const FRAME_EVENT: &str = "computer://frame";
/// The widest picture sent to the card.
const FRAME_WIDTH: u32 = 1280;
/// At most this many pictures a second.
const MAX_FPS: u32 = 15;

/// The chats whose app streams pictures now, with how to stop each.
static STAGES: LazyLock<Mutex<HashMap<String, CancellationToken>>> = LazyLock::new(Default::default);

/// Start or stop the pictures of `chat`'s app: on while a card or the panel
/// shows it, at about `fps` a second; off otherwise, so nothing is captured
/// for an app no one sees (NFR-8). An unchanged picture is not sent again.
#[tauri::command]
pub fn computer_stage(app: AppHandle, chat: String, on: bool, fps: Option<u32>) {
    let mut stages = STAGES.lock().expect("stages lock");
    if let Some(running) = stages.remove(&chat) {
        running.cancel();
    }
    if !on {
        return;
    }
    let cancel = CancellationToken::new();
    stages.insert(chat.clone(), cancel.clone());
    let interval = Duration::from_millis(1000 / u64::from(fps.unwrap_or(8).clamp(1, MAX_FPS)));
    tauri::async_runtime::spawn(async move {
        let surface = computer::surface(&chat);
        let mut seq: u64 = 0;
        let mut last: Option<u64> = None;
        loop {
            if cancel.is_cancelled() {
                return;
            }
            match computer::preview_frame(chat.clone(), FRAME_WIDTH).await {
                Ok(frame) if frame.get("attached") == Some(&json!(false)) => {
                    // Gone, released or stopped: a stopped app keeps its card.
                    if frame.get("stopped") != Some(&json!(true)) {
                        computer::released(&chat, "gone");
                    }
                    STAGES.lock().expect("stages lock").remove(&chat);
                    return;
                }
                Ok(frame) => {
                    if let Some(data) = frame.get("data").and_then(Value::as_str) {
                        use std::hash::{Hash, Hasher};
                        let mut hasher = std::collections::hash_map::DefaultHasher::new();
                        data.hash(&mut hasher);
                        let hash = hasher.finish();
                        if last != Some(hash) {
                            last = Some(hash);
                            seq += 1;
                            let _ = app.emit(
                                FRAME_EVENT,
                                json!({
                                    "surface": surface,
                                    "chat": chat,
                                    "seq": seq,
                                    "data": data,
                                    "width": frame.get("width"),
                                    "height": frame.get("height"),
                                    "title": frame.get("title"),
                                }),
                            );
                        }
                    }
                }
                // A hung app or a failed capture: try again on the next tick.
                Err(_) => {}
            }
            tokio::select! {
                _ = tokio::time::sleep(interval) => {}
                _ = cancel.cancelled() => return,
            }
        }
    });
}

// ── The person's commands (FR-12, FR-13) ────────────────────────────────────

/// Whether `chat` has an app attached, and whether control is stopped: for a
/// card that mounts after a reload.
#[tauri::command]
pub async fn computer_status(chat: String) -> Value {
    computer::act(&chat, Action::Status, json!({}), |_| {}).await.unwrap_or(Value::Null)
}

/// **Stop**: revoke control for the chat; the action in progress ends where it
/// got to and the app goes back. The agent cannot lift it.
#[tauri::command]
pub fn computer_stop(chat: String) -> Result<Value, String> {
    computer::stop(&chat)
}

/// **Allow again** after Stop.
#[tauri::command]
pub fn computer_resume(chat: String) -> Result<Value, String> {
    computer::resume(&chat)
}

/// **Show the app**: the window comes back to the person's screen and to the
/// front, and control ends for the rest of the turn. Bringing it to the front
/// is the person's own click on evo, the foreground process, so Windows allows
/// it; the agent can never call this.
#[tauri::command]
pub async fn computer_reveal(chat: String) -> Result<Value, String> {
    let revealed = computer::reveal(&chat)?;
    computer::close(&chat).await;
    #[cfg(target_os = "windows")]
    if let Some(window) = revealed.get("hwnd").and_then(Value::as_i64) {
        bring_to_front(window);
    }
    Ok(revealed)
}

#[cfg(target_os = "windows")]
fn bring_to_front(window: i64) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{SetForegroundWindow, ShowWindow, SW_RESTORE};
    let handle = window as isize;
    // After the window is back where it was (the backend restores it off the
    // UI thread first).
    std::thread::spawn(move || unsafe {
        std::thread::sleep(Duration::from_millis(300));
        let hwnd = HWND(handle as *mut core::ffi::c_void);
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = SetForegroundWindow(hwnd);
    });
}

/// **Close**: hand the app back and refuse attaching until the turn ends.
#[tauri::command]
pub async fn computer_close(chat: String) {
    computer::close(&chat).await;
}

/// The person's click, scroll or key in the card (FR-13). Positions are
/// fractions of the picture. It goes through the same background channels as
/// the agent's input, so the window stays off screen; it asks no approval.
#[tauri::command]
pub async fn computer_input(chat: String, input: Value) -> Result<(), String> {
    let kind = input.get("kind").and_then(Value::as_str).unwrap_or("");
    let (action, params) = match kind {
        "click" => (
            Action::Click,
            json!({ "x": input["x"], "y": input["y"], "fraction": true,
                    "button": input.get("button").cloned().unwrap_or(json!("left")),
                    "clicks": input.get("clicks").cloned().unwrap_or(json!(1)) }),
        ),
        "scroll" => (
            Action::Scroll,
            json!({ "x": input["x"], "y": input["y"], "fraction": true,
                    "direction": input.get("direction").cloned().unwrap_or(json!("down")), "amount": 1 }),
        ),
        "key" => (Action::Key, json!({ "key": input["key"] })),
        "text" => (Action::Type, json!({ "text": input["text"] })),
        other => return Err(format!("unknown input {other}")),
    };
    computer::act(&chat, action, params, |_| {}).await.map(|_| ())
}

/// The apps the person may allow or block in Settings, with their icons (FR-1).
#[tauri::command]
pub async fn computer_list_apps() -> Result<Value, String> {
    computer::list_apps().await
}
