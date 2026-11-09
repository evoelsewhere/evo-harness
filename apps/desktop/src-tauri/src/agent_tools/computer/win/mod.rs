//! Windows implementation of Computer App Control.
//!
//! Every function here addresses the attached window's own message queue or
//! its UI Automation tree. Nothing calls `SendInput`, `SetCursorPos` or
//! `SetForegroundWindow` on the agent's behalf: the user keeps their mouse,
//! keyboard and foreground window while the agent works.
//!
//! # Layout
//!
//! This file is the factory's Windows product ([`WindowsBackend`]) and the
//! dispatch of each [`Action`]; the work lives in one module per concern:
//!
//! | Module         | Concern                                                        |
//! |----------------|----------------------------------------------------------------|
//! | `registry`     | Which window each chat drives; Stop, Resume, Reveal; refs      |
//! | `parking`      | Off-screen stage for an app; putting it back (also on crash)   |
//! | `listing`      | Enumerating windows and processes; what may never be attached  |
//! | `catalog`      | Installed and running apps (Settings picker, `search_apps`)    |
//! | `front`        | The foreground lock held while an action runs (FR-30)          |
//! | `attachment`   | `status`, `attach`, `detach`                                   |
//! | `lifecycle`    | `search_apps`, `open_app`, `close_app`, `kill_app`             |
//! | `target`       | The window an action addresses; web-content hosts              |
//! | `popups`       | Menus, drop-downs and panes the app opens                      |
//! | `capture`      | `PrintWindow` capture, screenshots, preview frames             |
//! | `messages`     | Posting messages: target child, DPI, waiting for the app       |
//! | `pointer`      | Posted mouse input: `click`, `hover`, `scroll`, `drag`         |
//! | `typing`       | `type`: posted characters, read back                           |
//! | `keyboard`     | `key`: shortcuts with modifiers held in the app's key state    |
//! | `page`         | The page channel: a DevTools pipe into web apps evo started    |
//! | `page_tree`    | Getting Chromium/WebView2 to build its accessibility tree      |
//! | `uia_tree`     | UI Automation tree: `snapshot`, `find`, refs                   |
//! | `uia_patterns` | UI Automation patterns and hit-testing                         |
//! | `uia_input`    | Input through UI Automation: clicks, scrolls, `invoke`, `set_value` |
//! | `web_fill`     | Typing into web page fields with real keyboard events          |
//! | `text_surface` | Caret and selection in documents and edit controls (Text pattern) |
//!
//! Every module starts with `use super::*;` and this file re-exports each
//! module's `pub(super)` items, so they share one namespace: the imports
//! below are the single list of what the backend uses from Windows.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use image::{imageops, DynamicImage, RgbaImage};
use std::sync::LazyLock as Lazy;
use serde_json::{json, Value};
use windows::core::{Interface, BOOL, BSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HMONITOR, MONITORINFO,
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    GetWindowDC, ReleaseDC, ScreenToClient, SelectObject, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS,
    HBITMAP, HDC, SRCCOPY,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Threading::{
    AttachThreadInput, GetCurrentProcessId, GetCurrentThreadId, OpenProcess,
    QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{
    AccessibleObjectFromWindow, CUIAutomation, SetWinEventHook, HWINEVENTHOOK, ExpandCollapseState_Collapsed,
    ExpandCollapseState_PartiallyExpanded, IAccessible, IUIAutomation, IUIAutomationElement,
    IUIAutomationTextPattern, IUIAutomationTextRange, TextPatternRangeEndpoint_End,
    TextPatternRangeEndpoint_Start, TreeScope_Children, UIA_BoundingRectanglePropertyId,
    UIA_ControlTypePropertyId, UIA_IsGridPatternAvailablePropertyId,
    UIA_IsTablePatternAvailablePropertyId, UIA_IsTextPatternAvailablePropertyId, UIA_TextPatternId,
    IUIAutomationExpandCollapsePattern, IUIAutomationInvokePattern,
    IUIAutomationLegacyIAccessiblePattern, IUIAutomationSelectionItemPattern,
    IUIAutomationRangeValuePattern, IUIAutomationScrollPattern, IUIAutomationTogglePattern,
    IUIAutomationTreeWalker, IUIAutomationValuePattern, ScrollAmount_NoAmount,
    ScrollAmount_SmallDecrement, ScrollAmount_SmallIncrement, ToggleState_On,
    UIA_ExpandCollapsePatternId, UIA_InvokePatternId, UIA_RangeValuePatternId,
    UIA_ScrollPatternId,
    UIA_LegacyIAccessiblePatternId, UIA_SelectionItemPatternId, UIA_TogglePatternId,
    UIA_ValuePatternId,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyboardState, IsWindowEnabled, MapVirtualKeyW, SetKeyboardState, VkKeyScanW,
    MAPVK_VK_TO_VSC, VIRTUAL_KEY, VK_0, VK_1, VK_2, VK_3, VK_4, VK_5, VK_6, VK_7, VK_8, VK_9,
    VK_A, VK_ADD, VK_APPS, VK_B, VK_BACK, VK_C, VK_CAPITAL, VK_CONTROL, VK_D, VK_DECIMAL,
    VK_DELETE, VK_DIVIDE, VK_DOWN, VK_E, VK_END, VK_ESCAPE, VK_F, VK_F1, VK_F10, VK_F11, VK_F12,
    VK_F2, VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_G, VK_H, VK_HOME, VK_I, VK_INSERT,
    VK_J, VK_K, VK_L, VK_LCONTROL, VK_LEFT, VK_LMENU, VK_LSHIFT, VK_M, VK_MENU, VK_MULTIPLY, VK_N,
    VK_NEXT, VK_NUMLOCK, VK_O, VK_OEM_1, VK_OEM_2, VK_OEM_3, VK_OEM_4, VK_OEM_5, VK_OEM_6,
    VK_OEM_7, VK_OEM_COMMA, VK_OEM_MINUS, VK_OEM_PERIOD, VK_OEM_PLUS, VK_P, VK_PAUSE, VK_PRIOR,
    VK_Q, VK_R, VK_RCONTROL, VK_RETURN, VK_RIGHT, VK_RMENU, VK_S, VK_SCROLL, VK_SHIFT, VK_SNAPSHOT,
    VK_SPACE, VK_SUBTRACT, VK_T, VK_TAB, VK_U, VK_UP, VK_V, VK_W, VK_X, VK_Y, VK_Z,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BM_CLICK, ChildWindowFromPointEx, EnumWindows, GetAncestor, GetClassNameW, GetForegroundWindow,
    GetGUIThreadInfo, GetSystemMetrics, GetWindow, GetWindowLongPtrW, GetWindowPlacement,
    GetWindowRect, GetWindowTextW, IsZoomed, SetWindowPlacement, SetWindowPos,
    SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE,
    SWP_NOZORDER, SW_SHOWMAXIMIZED, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
    SW_SHOWMINIMIZED, SW_SHOWMINNOACTIVE, WINDOWPLACEMENT, WINDOWPLACEMENT_FLAGS,
    WPF_RESTORETOMAXIMIZED,
    GetWindowThreadProcessId, IsHungAppWindow, IsIconic, IsWindow, IsWindowVisible, PostMessageW,
    SendMessageTimeoutW, ShowWindow, CWP_SKIPDISABLED, CWP_SKIPINVISIBLE,
    CWP_SKIPTRANSPARENT, GA_ROOT, GUITHREADINFO, GWL_EXSTYLE, GW_ENABLEDPOPUP, GW_OWNER,
    GWL_STYLE, WS_CAPTION, WS_CHILD, WS_POPUP, WS_THICKFRAME, CHILDID_SELF, EVENT_OBJECT_SHOW, OBJID_WINDOW,
    WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, MSG, GetMessageW, DispatchMessageW,
    SMTO_ABORTIFHUNG, SW_RESTORE, SW_SHOWNOACTIVATE, WM_CHAR, WM_CONTEXTMENU, WM_KEYDOWN, WM_KEYUP,
    WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDBLCLK, WM_MBUTTONDOWN,
    WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCHITTEST, WM_NULL, WM_RBUTTONDBLCLK,
    WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP, WS_EX_TOOLWINDOW,
};

use super::action::Action;
use super::apps::{self, sorted, AppEntry, Catalog};
use super::backend::{ComputerAppBackend, STOPPED_REFUSAL};
use super::readback::{self, Probe};
use super::Journal;
use super::geometry::{pack_point, screenshot_scale};
use super::interrupt::interrupted;
use super::keys::{blocked_combo_reason, parse_key_combo, shifted_letter, KeyCombo};
use super::policy::is_protected_process_name;
use super::workers::{on_worker, post as post_to_worker};

mod attachment;
mod capture;
mod catalog;
mod front;
mod keyboard;
mod lifecycle;
mod listing;
mod messages;
mod page;
mod page_tree;
mod parking;
mod pointer;
mod popups;
mod registry;
mod target;
mod text_surface;
mod typing;
mod uia_input;
mod uia_patterns;
mod uia_tree;
mod web_fill;

#[cfg(test)]
mod benchmark;
#[cfg(test)]
mod live_bench;
#[cfg(test)]
use live_bench::*;
#[cfg(test)]
mod live_tests;
#[cfg(test)]
mod tests;

use attachment::*;
use capture::*;
use catalog::*;
use front::*;
use keyboard::*;
use lifecycle::*;
use listing::*;
use messages::*;
use page::*;
use page_tree::*;
use parking::*;
use pointer::*;
use popups::*;
use registry::*;
use target::*;
use text_surface::*;
use typing::*;
use uia_input::*;
use uia_patterns::*;
use uia_tree::*;
use web_fill::*;

// ── The factory's Windows product ───────────────────────────────────────

/// Computer App Control on Windows, as handed out by
/// [`super::backend::backend`].
pub(crate) struct WindowsBackend;

impl ComputerAppBackend for WindowsBackend {
    /// UI Automation objects are COM objects bound to the apartment that
    /// created them, so every worker joins the multithreaded apartment first.
    fn init_worker_thread(&self) {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
    }

    fn is_attached(&self, session_id: &str) -> bool {
        registry().attached.contains_key(session_id)
    }

    fn run_action(&self, emit: &dyn Fn(Value), session_id: &str, action: Action, params: &Value) -> Result<Value, String> {
        dispatch(emit, session_id, action, params)
    }

    fn list_apps(&self) -> Value {
        list_apps()
    }

    fn preview_frame(&self, session_id: &str, max_width: u32) -> Result<Value, String> {
        preview_frame(session_id, max_width)
    }

    fn stop(&self, session_id: &str) -> Result<(), String> {
        stop(session_id);
        Ok(())
    }

    fn resume(&self, session_id: &str) -> Result<(), String> {
        resume(session_id);
        Ok(())
    }

    fn reveal(&self, session_id: &str) -> Result<Value, String> {
        reveal(session_id)
    }

    fn release_all(&self) {
        release_all();
    }

    fn recover_stranded(&self, journal: Box<dyn Journal>) {
        recover_stranded(journal);
    }
}

// ── Dispatch ────────────────────────────────────────────────────────────

/// Actions whose effect is read back (FR-17): those that change the app.
/// A hover only moves the pointer; its effect (a tooltip) is not a change.
fn reads_back(action: Action) -> bool {
    action.is_input() && action != Action::Hover
}

/// Time one step of an action for the timing tests (`EVO_COMPUTER_TRACE`);
/// in the app it only runs the step.
pub(super) fn spanned<T>(label: &str, step: impl FnOnce() -> T) -> T {
    #[cfg(test)]
    if std::env::var_os("EVO_COMPUTER_TRACE").is_some() {
        let started = std::time::Instant::now();
        let value = step();
        eprintln!("  span {label}: {} ms", started.elapsed().as_millis());
        return value;
    }
    let _ = label;
    step()
}

/// The read-back "before" of the action running on this thread.
enum Before {
    /// To be read just ahead of the input: while the card's cursor travels.
    Pending(Target),
    Taken(Probe),
}

thread_local! {
    static BEFORE: RefCell<Option<Before>> = const { RefCell::new(None) };
}

/// Actions whose every way of sending input first moves the card's cursor
/// ([`Target::travel`]) or calls [`before_input`].
fn reads_before_during_travel(action: Action) -> bool {
    matches!(action, Action::Click | Action::Scroll | Action::Drag | Action::Invoke | Action::SetValue)
}

/// Read the running action's "before" now, ahead of its input, unless it was
/// read already. The cursor's travel on the card is a wait the person sees
/// anyway; the read fills it instead of adding to it (NFR-4).
pub(super) fn before_input() {
    let pending = BEFORE.with(|slot| {
        let mut slot = slot.borrow_mut();
        match slot.take() {
            Some(Before::Pending(target)) => Some(target),
            other => {
                *slot = other;
                None
            }
        }
    });
    if let Some(target) = pending {
        let probe = spanned("probe before", || probe_state(&target));
        BEFORE.with(|slot| *slot.borrow_mut() = Some(Before::Taken(probe)));
    }
}

/// How long the read-back waits for a change once the app handled the
/// input, the first time and, when nothing changed yet, once more: a
/// Chromium page updates its tree a moment after its window took the input.
const FIRST_LOOK_MS: u64 = 40;
const SECOND_LOOK_MS: u64 = 200;

fn dispatch(emit: &dyn Fn(Value), session_id: &str, action: Action, params: &Value) -> Result<Value, String> {
    // The app may answer with a window of its own; the person keeps the
    // front window (FR-30).
    let _front = holds_front(action).then(hold_front);
    if !reads_back(action) {
        return spanned("act", || dispatch_action(emit, session_id, action, params));
    }
    let target = spanned("resolve", || Target::resolve(session_id).ok());
    // A web page evo has no page channel into may stop painting; its picture
    // is compared too (NFR-3). A page-channel app keeps painting.
    let watch_picture = target.as_ref().filter(|t| t.web && channel_for(t.pid).is_none()).map(|t| t.window);
    let picture_before = spanned("picture before", || watch_picture.and_then(picture_hash));
    let before = target.map(|target| {
        if reads_before_during_travel(action) {
            Before::Pending(target)
        } else {
            Before::Taken(spanned("probe before", || probe_state(&target)))
        }
    });
    BEFORE.with(|slot| *slot.borrow_mut() = before);
    let result = spanned("act", || dispatch_action(emit, session_id, action, params));
    let before = BEFORE.with(|slot| slot.borrow_mut().take());
    let result = result?;
    // Not read (no window, or the input went out without a read first):
    // the result then says nothing about changes rather than something wrong.
    let Some(Before::Taken(before)) = before else {
        return Ok(result);
    };
    let look = || match Target::resolve(session_id) {
        Ok(target) => {
            // Let the app take the input in before looking.
            settle(target.window);
            probe_state(&target)
        }
        // The window went away: that is the change.
        Err(_) => Probe { title: "(closed)".into(), ..Probe::default() },
    };
    pause(FIRST_LOOK_MS);
    let mut after = spanned("probe after", look);
    if readback::diff(&before, &after).is_null() {
        pause(SECOND_LOOK_MS);
        after = spanned("probe after (second look)", look);
    }
    let changed = readback::diff(&before, &after);
    let mut text = readback::describe(&changed);
    let late = match (picture_before, spanned("picture after", || watch_picture.and_then(picture_hash))) {
        (Some(a), Some(b)) => readback::picture_late(&changed, a == b),
        _ => false,
    };
    if late {
        text.push_str("
  The window's picture did not change although its content did: screenshots may lag behind, so trust snapshot and find.");
    }
    set_picture_late(session_id, late);
    Ok(merge(result, json!({ "changed": changed, "changed_text": text, "picture_may_be_late": late })))
}

fn dispatch_action(emit: &dyn Fn(Value), session_id: &str, action: Action, params: &Value) -> Result<Value, String> {
    // The window is resolved per action, not per session: the agent may
    // have been detached, or the window closed, since the last one.
    let target = || -> Result<Target, String> {
        let target = Target::resolve(session_id)?;
        if action.is_input() {
            claim_page_focus(&target);
        }
        Ok(target)
    };
    match action {
        Action::Status => Ok(status(session_id)),
        Action::ListWindows => Ok(list_windows(session_id, params)),
        Action::Attach => attach(session_id, params),
        Action::Detach => Ok(detach(session_id)),
        Action::Screenshot => screenshot_with(&target()?, params),
        Action::Zoom => zoom_shot(&target()?, params),
        Action::Snapshot => snapshot(&target()?, params),
        Action::Find => find(&target()?, params),
        Action::Click => click(emit, &target()?, params),
        Action::Hover => hover(emit, &target()?, params),
        Action::Scroll => scroll(emit, &target()?, params),
        Action::Drag => drag(emit, &target()?, params),
        Action::Type => type_text(emit, &target()?, params),
        Action::Key => press_key(&target()?, params),
        Action::Invoke => invoke(emit, &target()?, params),
        Action::SetValue => set_value(emit, &target()?, params),
        Action::Restore => {
            let target = target()?;
            Ok(json!({ "restored": target.restored, "window": target.describe() }))
        }
        Action::SearchApps => Ok(search_apps(params)),
        Action::OpenApp => open_app(session_id, params),
        Action::CloseApp => close_app(session_id, params),
        Action::KillApp => kill_app(session_id, params),
    }
}

/// A quick fingerprint of the window's picture.
fn picture_hash(window: HWND) -> Option<u64> {
    use std::hash::{Hash, Hasher};
    let image = capture(window).ok()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    image.as_raw().hash(&mut hasher);
    Some(hasher.finish())
}

/// Chats whose last action showed a picture that may be late (NFR-3); their
/// screenshots say so until an action shows a fresh one.
static PICTURE_LATE: Lazy<Mutex<HashSet<String>>> = Lazy::new(|| Mutex::new(HashSet::new()));

fn set_picture_late(session_id: &str, late: bool) {
    if let Ok(mut set) = PICTURE_LATE.lock() {
        if late {
            set.insert(session_id.to_string());
        } else {
            set.remove(session_id);
        }
    }
}

pub(super) fn picture_may_be_late(session_id: &str) -> bool {
    PICTURE_LATE.lock().map(|set| set.contains(session_id)).unwrap_or(false)
}

/// `base` with the fields of `extra` added (or replaced).
fn merge(mut base: Value, extra: Value) -> Value {
    if let (Some(base), Value::Object(extra)) = (base.as_object_mut(), extra) {
        base.extend(extra);
    }
    base
}
