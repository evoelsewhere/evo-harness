//! Keeping a controlled app running but off the user's screen, and putting
//! it back exactly where it was — also after evo crashed.

use super::*;

// ── Parking: keep the app running but off the user's screen ─────────────
//
// A minimized window cannot be captured or clicked (it has no size), so an
// app the user wants kept out of sight is moved just outside the virtual
// desktop instead. It keeps its taskbar button and keeps rendering; on
// release it gets its exact previous placement back, and a window that was
// minimized or maximized returns minimized (restoring to maximized) rather
// than being activated behind the user's back.
//
// A Chromium window (Edge, Chrome, their installed web apps, Electron) that
// is wholly off every monitor counts as occluded: it stops painting and its
// pictures go stale (research.md section 6). It is never hidden by
// transparency (spec FR-6); instead one pixel of it stays on a monitor, in a
// corner, on top, so Chromium keeps painting it (the "corner", spec FR-6,
// plan R1/R2). Chromium only re-evaluates occlusion when a window moves, so
// the corner is always reached by a move with `HWND_TOPMOST` in the same call.

pub(super) fn virtual_screen() -> RECT {
    unsafe {
        let left = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let top = GetSystemMetrics(SM_YVIRTUALSCREEN);
        RECT {
            left,
            top,
            right: left + GetSystemMetrics(SM_CXVIRTUALSCREEN),
            bottom: top + GetSystemMetrics(SM_CYVIRTUALSCREEN),
        }
    }
}

/// The monitors' rectangles, in physical pixels.
pub(super) fn monitor_rects() -> Vec<RECT> {
    unsafe extern "system" fn add(monitor: HMONITOR, _dc: HDC, _clip: *mut RECT, list: LPARAM) -> BOOL {
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
            let list = unsafe { &mut *(list.0 as *mut Vec<RECT>) };
            list.push(info.rcMonitor);
        }
        BOOL(1)
    }
    let mut rects: Vec<RECT> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(add), LPARAM(&mut rects as *mut Vec<RECT> as isize));
    }
    rects
}

/// How many pixels of `frame` lie on some monitor (monitors never overlap).
pub(super) fn visible_area(frame: RECT, monitors: &[RECT]) -> i64 {
    monitors
        .iter()
        .map(|monitor| {
            let width = i64::from(frame.right.min(monitor.right) - frame.left.max(monitor.left));
            let height = i64::from(frame.bottom.min(monitor.bottom) - frame.top.max(monitor.top));
            if width > 0 && height > 0 {
                width * height
            } else {
                0
            }
        })
        .sum()
}

/// Whether the person cannot see the window: wholly off every monitor, or
/// showing at most the one pixel of the corner.
pub(super) fn is_off_screen(hwnd: HWND) -> bool {
    visible_area(frame_rect(hwnd), &monitor_rects()) <= 1
}

/// Whether a parked window needs the corner pixel to keep painting: a
/// Chromium top-level window (Edge, Chrome, Electron). A WebView2 control
/// inside another app's window keeps painting off screen, and so does every
/// native window (`PrintWindow` reads it there).
pub(super) fn needs_corner(hwnd: HWND) -> bool {
    class_name(hwnd).starts_with("Chrome_WidgetWin")
}

/// Where a frame of `width` × `height` shows exactly one pixel, at a corner
/// of one of `monitors`: its top-left frame point. Top corners first, since
/// the taskbar usually sits at the bottom.
pub(super) fn corner_place(width: i32, height: i32, monitors: &[RECT]) -> Option<POINT> {
    let candidates = monitors.iter().flat_map(|m| {
        [
            POINT { x: m.right - 1, y: m.top + 1 - height },  // top-right
            POINT { x: m.left + 1 - width, y: m.top + 1 - height }, // top-left
            POINT { x: m.right - 1, y: m.bottom - 1 },         // bottom-right
            POINT { x: m.left + 1 - width, y: m.bottom - 1 },  // bottom-left
        ]
    });
    let mut ordered: Vec<(usize, POINT)> = candidates.enumerate().map(|(i, p)| (i % 4, p)).collect();
    ordered.sort_by_key(|(corner, _)| *corner / 2);
    ordered.into_iter().map(|(_, p)| p).find(|p| {
        let frame = RECT { left: p.x, top: p.y, right: p.x + width, bottom: p.y + height };
        visible_area(frame, monitors) == 1
    })
}

/// Windows this run put in the corner (topmost there), so release can take
/// the topmost bit away again.
static CORNERED: Lazy<Mutex<HashSet<isize>>> = Lazy::new(|| Mutex::new(HashSet::new()));

pub(super) fn is_cornered(hwnd: HWND) -> bool {
    CORNERED.lock().map(|set| set.contains(&(hwnd.0 as isize))).unwrap_or(false)
}

/// Move `hwnd` to the corner: one pixel on a monitor, on top, in one
/// `SetWindowPos`. Returns false when no corner shows exactly one pixel (it is
/// then moved wholly off screen instead).
pub(super) fn move_to_corner(hwnd: HWND) -> bool {
    let frame = frame_rect(hwnd);
    let (width, height) = (frame.right - frame.left, frame.bottom - frame.top);
    let Some(place) = corner_place(width, height, &monitor_rects()) else {
        move_off_screen(hwnd);
        return false;
    };
    let mut window = RECT::default();
    unsafe {
        let _ = GetWindowRect(hwnd, &mut window);
    }
    // The frame sits inside the window rectangle's invisible resize borders.
    let (dx, dy) = (frame.left - window.left, frame.top - window.top);
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND(-1isize as _)), // HWND_TOPMOST
            place.x - dx,
            place.y - dy,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        );
    }
    if let Ok(mut set) = CORNERED.lock() {
        set.insert(hwnd.0 as isize);
    }
    true
}

/// Whether a cornered window is still exactly in its corner and on top.
fn still_in_corner(hwnd: HWND) -> bool {
    let topmost = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32 & WS_EX_TOPMOST.0 != 0;
    topmost && visible_area(frame_rect(hwnd), &monitor_rects()) == 1
}

/// Put a parked window back out of view if it has drifted: the person
/// brought it back from the taskbar, the app moved itself, or something took
/// its corner's topmost place (spec FR-6, plan R2). A cornered window that
/// drifted is nudged by a move, which makes Chromium look again.
pub(super) fn keep_out_of_view(hwnd: HWND) -> bool {
    if needs_corner(hwnd) {
        if still_in_corner(hwnd) {
            return false;
        }
        move_to_corner(hwnd);
        return true;
    }
    if is_off_screen(hwnd) {
        return false;
    }
    move_off_screen(hwnd);
    true
}

/// Take the topmost bit away from a window this run put in the corner.
fn leave_corner(hwnd: HWND) {
    let was = CORNERED.lock().map(|mut set| set.remove(&(hwnd.0 as isize))).unwrap_or(false);
    if was {
        clear_topmost(hwnd);
    }
}

fn clear_topmost(hwnd: HWND) {
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND(-2isize as _)), // HWND_NOTOPMOST
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        );
    }
}

pub(super) fn move_off_screen(hwnd: HWND) {
    let screen = virtual_screen();
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            screen.right + 200,
            screen.top + 40,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        );
    }
}

// ── The stage: a fixed-size screen for a parked window ──────────────────
//
// Off every monitor, a parked window is sized like a screen of its own:
// the same size every time, and small enough that its screenshot reaches
// the agent pixel for pixel (see `screenshot_scale`). The agent's
// coordinates are then the window's, with no scaling to round them, and the
// app lays itself out the same way in every session instead of by however
// the user last sized it. On release it gets its own placement back.

/// The stage in physical pixels: 1280×800 — within the screenshot limits —
/// unless that is less than 1024×640 logical pixels on a scaled display,
/// where an app laid out in less (a ribbon, a dialog) starts to fold away.
pub(super) fn stage_size(dpi: u32) -> (i32, i32) {
    const STAGE: (i32, i32) = (1280, 800);
    const MIN_LOGICAL: (i32, i32) = (1024, 640);
    let scale = f64::from(dpi.max(96)) / 96.0;
    let at_least = |logical: i32| (f64::from(logical) * scale).round() as i32;
    (STAGE.0.max(at_least(MIN_LOGICAL.0)), STAGE.1.max(at_least(MIN_LOGICAL.1)))
}

/// Give a parked window the stage's size. Only a window the user could
/// resize themselves is resized: a fixed-size dialog or tool window lays
/// itself out for its own size and nothing else.
fn fit_to_stage(hwnd: HWND) {
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } as u32;
    if style & WS_THICKFRAME.0 == 0 {
        return;
    }
    let (width, height) = stage_size(unsafe { GetDpiForWindow(hwnd) });
    let mut window_rect = RECT::default();
    unsafe {
        let _ = GetWindowRect(hwnd, &mut window_rect);
    }
    // The stage is the visible frame; the window rectangle adds the
    // invisible resize borders around it.
    let frame = frame_rect(hwnd);
    let borders_x = (window_rect.right - window_rect.left) - (frame.right - frame.left);
    let borders_y = (window_rect.bottom - window_rect.top) - (frame.bottom - frame.top);
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            width + borders_x.max(0),
            height + borders_y.max(0),
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        );
    }
}

pub(super) fn park(hwnd: HWND) -> Option<WINDOWPLACEMENT> {
    let mut placement = WINDOWPLACEMENT {
        length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
        ..Default::default()
    };
    unsafe {
        GetWindowPlacement(hwnd, &mut placement).ok()?;
        // On disk first, so no moment passes with the window off-screen
        // and its place known only in memory.
        remember_parked(hwnd, placement);
        if IsIconic(hwnd).as_bool() || IsZoomed(hwnd).as_bool() {
            // Back to its normal size first: a maximized window is pinned to
            // its monitor and a minimized one has no size to render.
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            std::thread::sleep(Duration::from_millis(150));
        }
    }
    move_off_screen(hwnd);
    // Sized once it is off-screen: moving there can change its DPI, and the
    // app rescales itself to the new one first.
    std::thread::sleep(Duration::from_millis(100));
    fit_to_stage(hwnd);
    if needs_corner(hwnd) {
        // Last, once it has its stage size: the corner depends on the size.
        move_to_corner(hwnd);
    }
    Some(placement)
}

// ── Parked windows in the journal ───────────────────────────────────────
//
// Where a parked window belongs lived only in memory: if evo crashed or
// was killed, the app stayed off-screen, reachable only by keyboard tricks.
// Every parked window is also written to the journal (`evo.db`'s
// `computer_parked`, spec FR-8), and taken out of it once it is back; the
// next start puts back whatever a previous run left behind.

#[derive(Clone, Copy)]
pub(super) struct Stranded {
    pub(super) hwnd: isize,
    pid: u32,
    placement: WINDOWPLACEMENT,
    /// Put in the corner, so topmost by this run's doing.
    cornered: bool,
}

static STRANDED: Lazy<Mutex<Vec<Stranded>>> = Lazy::new(|| Mutex::new(Vec::new()));
static JOURNAL: std::sync::OnceLock<Box<dyn Journal>> = std::sync::OnceLock::new();

pub(super) fn stranded() -> std::sync::MutexGuard<'static, Vec<Stranded>> {
    STRANDED.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(super) fn placement_json(placement: &WINDOWPLACEMENT) -> Value {
    let normal = placement.rcNormalPosition;
    json!({
        "flags": placement.flags.0,
        "show": placement.showCmd,
        "min": [placement.ptMinPosition.x, placement.ptMinPosition.y],
        "max": [placement.ptMaxPosition.x, placement.ptMaxPosition.y],
        "normal": [normal.left, normal.top, normal.right, normal.bottom],
    })
}

pub(super) fn placement_from_json(value: &Value) -> Option<WINDOWPLACEMENT> {
    let numbers = |key: &str| -> Option<Vec<i32>> {
        value.get(key)?.as_array()?.iter().map(|n| n.as_i64().map(|n| n as i32)).collect()
    };
    let (min, max, normal) = (numbers("min")?, numbers("max")?, numbers("normal")?);
    if min.len() != 2 || max.len() != 2 || normal.len() != 4 {
        return None;
    }
    Some(WINDOWPLACEMENT {
        length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
        flags: WINDOWPLACEMENT_FLAGS(value.get("flags")?.as_u64()? as u32),
        showCmd: value.get("show")?.as_u64()? as u32,
        ptMinPosition: POINT { x: min[0], y: min[1] },
        ptMaxPosition: POINT { x: max[0], y: max[1] },
        rcNormalPosition: RECT { left: normal[0], top: normal[1], right: normal[2], bottom: normal[3] },
    })
}

fn remember_parked(hwnd: HWND, placement: WINDOWPLACEMENT) {
    let pid = window_pid(hwnd);
    let mut list = stranded();
    list.retain(|entry| entry.hwnd != hwnd.0 as isize);
    let cornered = needs_corner(hwnd);
    list.push(Stranded { hwnd: hwnd.0 as isize, pid, placement, cornered });
    if let Some(journal) = JOURNAL.get() {
        let mut recorded = placement_json(&placement);
        recorded["cornered"] = json!(cornered);
        journal.record(hwnd.0 as i64, i64::from(pid), &recorded);
    }
}

pub(super) fn forget_parked(hwnd: HWND) {
    let mut list = stranded();
    let before = list.len();
    list.retain(|entry| entry.hwnd != hwnd.0 as isize);
    if list.len() != before {
        if let Some(journal) = JOURNAL.get() {
            journal.forget(hwnd.0 as i64);
        }
    }
}

/// Start recording parked windows in `journal`, and put back any window
/// that a previous run left parked: still open, still the same process's,
/// still off-screen. Runs on a thread of its own, since moving a window
/// waits for its app.
pub(super) fn recover_stranded(journal: Box<dyn Journal>) {
    let leftovers = load_stranded(journal.as_ref());
    if JOURNAL.set(journal).is_err() {
        return;
    }
    // Recorded until put back, in case this run does not get that far.
    stranded().extend(leftovers.iter().copied());
    off_ui_thread(move || put_back(leftovers));
}

pub(super) fn load_stranded(journal: &dyn Journal) -> Vec<Stranded> {
    journal
        .load()
        .into_iter()
        .filter_map(|(window, pid, placement)| {
            Some(Stranded {
                hwnd: window as isize,
                pid: u32::try_from(pid).ok()?,
                placement: placement_from_json(&placement)?,
                cornered: placement.get("cornered").and_then(Value::as_bool).unwrap_or(false),
            })
        })
        .collect()
}

pub(super) fn put_back(leftovers: Vec<Stranded>) {
    for entry in leftovers {
        let hwnd = to_hwnd(entry.hwnd);
        // A window handle can be reused once its window is gone.
        let same = unsafe { IsWindow(Some(hwnd)) }.as_bool() && window_pid(hwnd) == entry.pid;
        if same && is_off_screen(hwnd) {
            eprintln!("computer app: putting back a window left off-screen by a previous run");
            if entry.cornered {
                clear_topmost(hwnd);
            }
            unpark(hwnd, entry.placement, false);
        } else {
            forget_parked(hwnd);
        }
    }
}

pub(super) fn unpark(hwnd: HWND, placement: WINDOWPLACEMENT, activate: bool) {
    if !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
        forget_parked(hwnd);
        return;
    }
    leave_corner(hwnd);
    let mut restore = placement;
    let was_maximized = placement.showCmd == SW_SHOWMAXIMIZED.0 as u32;
    let was_minimized = placement.showCmd == SW_SHOWMINIMIZED.0 as u32;
    restore.showCmd = match (activate, was_minimized, was_maximized) {
        (true, true, _) if placement.flags.0 & WPF_RESTORETOMAXIMIZED.0 != 0 => {
            SW_SHOWMAXIMIZED.0 as u32
        }
        (true, true, _) => SW_RESTORE.0 as u32,
        (true, false, _) => placement.showCmd,
        (false, true, _) => SW_SHOWMINNOACTIVE.0 as u32,
        (false, false, true) => {
            restore.flags = WINDOWPLACEMENT_FLAGS(restore.flags.0 | WPF_RESTORETOMAXIMIZED.0);
            SW_SHOWMINNOACTIVE.0 as u32
        }
        (false, false, false) => SW_SHOWNOACTIVATE.0 as u32,
    };
    unsafe {
        let _ = SetWindowPlacement(hwnd, &restore);
    }
    forget_parked(hwnd);
    bring_dialogs_back(hwnd, placement.rcNormalPosition);
    bring_moved_back(hwnd);
}

// ── Dialogs of a parked window ──────────────────────────────────────────
//
// A dialog is a top-level window of its own, and Windows (DS_CENTER) and
// frameworks (WinForms' CenterParent) keep it on a monitor: a parked app's
// dialogs opened on the user's screen. So did its menus, floating panes and
// palettes, opened by a key or a command as often as by a click. While a
// window is parked, every window it opens (see `opened_by`) leaves the
// user's screen the moment it is shown: a captioned window it owns is moved
// over it, anything else next to it (`bring_popups_along`). When the window
// is handed back, its dialogs are centred on it again and its panes and
// palettes go back where they opened.

pub(super) fn is_dialog_of(hwnd: HWND, top: HWND) -> bool {
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } as u32;
    hwnd != top
        && style & WS_CHILD.0 == 0
        && style & WS_CAPTION.0 == WS_CAPTION.0
        && (owned_by(hwnd, top, top) || owned_by_hidden_window_of(hwnd, top))
}

/// Whether `hwnd` is owned by a window of `top`'s process that is never
/// shown. Apps often own their dialogs by such a window rather than by the
/// window the dialog is about: a spreadsheet's Create Table dialog, owned
/// that way, opened on the user's screen while its workbook was parked. A
/// dialog owned by another window the user can see (a second document
/// window) is that window's, and is left alone.
pub(super) fn owned_by_hidden_window_of(hwnd: HWND, top: HWND) -> bool {
    match unsafe { GetWindow(hwnd, GW_OWNER) } {
        Ok(owner) if !owner.0.is_null() && owner != top => {
            !unsafe { IsWindowVisible(owner) }.as_bool() && window_pid(owner) == window_pid(top)
        }
        _ => false,
    }
}

/// Centre `window` on `over`, resized neither.
fn centre_on(window: HWND, over: RECT, min_x: i32) {
    let frame = frame_rect(window);
    let x = over.left + ((over.right - over.left) - (frame.right - frame.left)) / 2;
    let y = over.top + ((over.bottom - over.top) - (frame.bottom - frame.top)) / 2;
    unsafe {
        let _ = SetWindowPos(
            window,
            None,
            x.max(min_x),
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        );
    }
}

/// Move a dialog of the parked `top` over it, off the user's screen.
pub(super) fn park_dialog(dialog: HWND, top: HWND) {
    if !is_off_screen(dialog) {
        // Wholly past the screen's edge, even when wider than its owner.
        centre_on(dialog, frame_rect(top), virtual_screen().right + 40);
    }
}

/// `top` is back where the user left it (`normal`): bring its dialogs along.
fn bring_dialogs_back(top: HWND, normal: RECT) {
    let pid = window_pid(top);
    for dialog in top_level_windows() {
        if window_pid(dialog) == pid
            && unsafe { IsWindowVisible(dialog) }.as_bool()
            && is_off_screen(dialog)
            && is_dialog_of(dialog, top)
        {
            centre_on(dialog, normal, i32::MIN);
        }
    }
}

/// Start, once, the thread that parks dialogs as they open (see above).
pub(super) fn watch_for_dialogs() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        let _ = std::thread::Builder::new()
            .name("computer-app-dialogs".into())
            .spawn(|| unsafe {
                let hook = SetWinEventHook(
                    EVENT_OBJECT_SHOW,
                    EVENT_OBJECT_SHOW,
                    None,
                    Some(on_window_shown),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                );
                if hook.is_invalid() {
                    return;
                }
                let mut message = MSG::default();
                while GetMessageW(&mut message, None, 0, 0).as_bool() {
                    DispatchMessageW(&message);
                }
            });
    });
}

unsafe extern "system" fn on_window_shown(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    object: i32,
    child: i32,
    _thread: u32,
    _time: u32,
) {
    if object != OBJID_WINDOW.0 || child != CHILDID_SELF as i32 || hwnd.0.is_null() {
        return;
    }
    let parked: Vec<(String, HWND, u32)> = registry()
        .attached
        .iter()
        .filter(|(_, attached)| attached.parked.is_some())
        .map(|(session, attached)| (session.clone(), to_hwnd(attached.hwnd), attached.pid))
        .collect();
    if let Some((session, top, pid)) = parked.into_iter().find(|(_, top, pid)| opened_by(hwnd, *top, *pid)) {
        keep_along(&session, top, pid, hwnd);
    }
}
