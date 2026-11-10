//! Keeping a controlled app running but out of the user's sight, and
//! putting it back where it was.

use super::*;

// ── Parking: keep the app running but out of the user's sight ───────────
//
// macOS keeps at least a sliver of every window on a display, so a parked
// window goes to the bottom-right corner of the desktop with only a point of
// it showing — the way window managers hide windows. It keeps running and
// keeps its Dock icon; a minimized window cannot be captured, so it is
// brought back from the Dock first and returns there on release.

/// The bottom-right corner of the display furthest down and to the right.
/// Taken from one display, not the union of all: with displays of different
/// heights the union's corner may be on none of them, and macOS would pull
/// the window back onto one.
fn hidden_origin() -> (f64, f64) {
    displays()
        .into_iter()
        .max_by(|a, b| (a.right() + a.bottom()).total_cmp(&(b.right() + b.bottom())))
        .map(|display| (display.right() - 1.0, display.bottom() - 1.0))
        .unwrap_or((10_000.0, 10_000.0))
}

pub(super) fn move_out_of_sight(window: &Ax) {
    let (x, y) = hidden_origin();
    if let Some(value) = ax_point_value(x, y) {
        let _ = window.set("AXPosition", &value);
    }
}

pub(super) fn park(window: &Ax, web: bool) -> Option<Parked> {
    let minimized = window.flag("AXMinimized").unwrap_or(false);
    if minimized {
        let _ = window.set_flag("AXMinimized", false);
        pause(450);
    }
    let origin = window.position()?;
    let parked = Parked { origin: (origin.x, origin.y), minimized };
    // Recorded before the move, so a crash during it is covered too.
    if let (Some(window_id), Some(pid)) = (window.window_id(), window.pid()) {
        remember_parked(Stranded { window_id, pid, parked, web });
    }
    move_out_of_sight(window);
    Some(parked)
}

/// Put a parked window back.
///
/// While a Chromium app's `AXEnhancedUserInterface` is on, macOS animates
/// moves made through accessibility and may drop them — release moved the
/// window first and turned the flag off after, and Chrome or Electron
/// windows stayed in their corner. The flag is off during the move, and on
/// again after only when `keep_enhanced` (the app is still being driven).
/// Whether the window arrived.
pub(super) fn hand_back(app: &Ax, window: &Ax, parked: Parked, activate: bool, web: bool, keep_enhanced: bool) -> bool {
    if web {
        let _ = app.set_flag("AXEnhancedUserInterface", false);
        pause(100);
    }
    let arrived = unpark(window, parked, activate);
    if web && keep_enhanced {
        let _ = app.set_flag("AXEnhancedUserInterface", true);
    }
    arrived
}

/// Move a parked window back; whether it arrived. It stays recorded on
/// disk until it has: an app that is busy (still launching, say) may ignore
/// the move.
pub(super) fn unpark(window: &Ax, parked: Parked, activate: bool) -> bool {
    let (x, y) = parked.origin;
    let mut arrived = false;
    // Asked again when the first move did not take.
    for attempt in 0..2 {
        if let Some(value) = ax_point_value(x, y) {
            let _ = window.set("AXPosition", &value);
        }
        arrived = window
            .position()
            .is_some_and(|now| (now.x - x).abs() < 2.0 && (now.y - y).abs() < 2.0);
        if arrived || attempt == 1 {
            break;
        }
        pause(200);
    }
    if parked.minimized && !activate {
        let _ = window.set_flag("AXMinimized", true);
    }
    if arrived {
        if let Some(window_id) = window.window_id() {
            forget_parked(window_id);
        }
    }
    arrived
}

// ── Parked windows on disk ──────────────────────────────────────────────
//
// Where a parked window belongs lived only in memory: if evo crashed or was
// killed, the window stayed in its corner with a point of it showing. Every
// parked window is also recorded in the journal (`evo.db`'s
// `computer_parked`, spec FR-8), and taken out of it once it is back; the
// next start puts back whatever a previous run left behind.

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Stranded {
    pub(super) window_id: u32,
    pub(super) pid: i32,
    pub(super) parked: Parked,
    /// Chromium content, whose enhanced interface evo turned on.
    pub(super) web: bool,
}

static STRANDED: Lazy<Mutex<Vec<Stranded>>> = Lazy::new(|| Mutex::new(Vec::new()));
static JOURNAL: std::sync::OnceLock<Box<dyn Journal>> = std::sync::OnceLock::new();

pub(super) fn stranded() -> std::sync::MutexGuard<'static, Vec<Stranded>> {
    STRANDED.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Where and how the window was, as the journal keeps it.
pub(super) fn placement_json(entry: &Stranded) -> Value {
    json!({
        "origin": [entry.parked.origin.0, entry.parked.origin.1],
        "minimized": entry.parked.minimized,
        "web": entry.web,
    })
}

/// A journal row back as an entry; anything unreadable is skipped.
pub(super) fn stranded_from_row(window: i64, pid: i64, placement: &Value) -> Option<Stranded> {
    let origin = placement.get("origin")?.as_array()?;
    Some(Stranded {
        window_id: u32::try_from(window).ok()?,
        pid: i32::try_from(pid).ok()?,
        parked: Parked {
            origin: (origin.first()?.as_f64()?, origin.get(1)?.as_f64()?),
            minimized: placement.get("minimized")?.as_bool()?,
        },
        web: placement.get("web").and_then(Value::as_bool).unwrap_or(false),
    })
}

fn remember_parked(entry: Stranded) {
    let mut list = stranded();
    list.retain(|other| other.window_id != entry.window_id);
    list.push(entry);
    if let Some(journal) = JOURNAL.get() {
        journal.record(i64::from(entry.window_id), i64::from(entry.pid), &placement_json(&entry));
    }
}

pub(super) fn forget_parked(window_id: u32) {
    let mut list = stranded();
    let before = list.len();
    list.retain(|entry| entry.window_id != window_id);
    if list.len() != before {
        if let Some(journal) = JOURNAL.get() {
            journal.forget(i64::from(window_id));
        }
    }
}

/// Start recording parked windows in `journal`, and put back any window that
/// a previous run left parked: still open, still the same process's, still
/// off screen. Runs on a thread of its own, since moving a window waits for
/// its app.
pub(super) fn recover_stranded(journal: Box<dyn Journal>) {
    let rows = journal.load();
    if JOURNAL.set(journal).is_err() {
        return;
    }
    let leftovers: Vec<Stranded> =
        rows.iter().filter_map(|(window, pid, placement)| stranded_from_row(*window, *pid, placement)).collect();
    if leftovers.is_empty() {
        return;
    }
    // Recorded until put back, in case this run does not get that far.
    stranded().extend(leftovers.iter().copied());
    let _ = std::thread::Builder::new()
        .name("computer-app-recover".into())
        .spawn(move || put_back_stranded(leftovers));
}

pub(super) fn put_back_stranded(leftovers: Vec<Stranded>) {
    for entry in leftovers {
        // Window ids are not reused within a login session; after a restart
        // the pid and the position rule out another window with the id.
        let still_parked = cg_window(entry.window_id)
            .is_some_and(|window| window.pid == entry.pid && mostly_off_screen(&window.bounds));
        if !still_parked {
            forget_parked(entry.window_id);
            continue;
        }
        eprintln!("computer app: putting back a window left parked by a previous run");
        let reached = Ax::application(entry.pid)
            .and_then(|app| reach_window(&app, entry.window_id).map(|window| (app, window)));
        // Nothing drives the app yet in this run.
        let arrived = reached
            .is_some_and(|(app, window)| hand_back(&app, &window, entry.parked, false, entry.web, false));
        if !arrived {
            put_back_later(entry.pid, entry.window_id, entry.parked, entry.web);
        }
    }
}
