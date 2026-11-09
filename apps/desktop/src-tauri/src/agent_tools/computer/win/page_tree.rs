//! Getting a web page's accessibility tree out of Chromium, Electron and
//! WebView2, which build it only when asked and only while it shows.

use super::*;

/// Chromium/Electron/WebView2 keep their accessibility tree minimal until an
/// assistive client asks for it through MSAA on the render widget window.
///
/// Returns the render hosts found. Their page tree does not appear as a
/// descendant of the top-level window, so callers walk them as roots of
/// their own. The first activation of a host waits briefly: Chromium builds
/// the tree asynchronously, and the first query would otherwise see an empty
/// page.
fn activate_chromium_accessibility(top: HWND) -> Vec<HWND> {
    thread_local! {
        static ACTIVATED: RefCell<HashSet<isize>> = RefCell::new(HashSet::new());
    }
    let hosts = render_hosts(top);
    let mut fresh = false;
    for host in &hosts {
        let mut object: *mut core::ffi::c_void = std::ptr::null_mut();
        unsafe {
            // OBJID_CLIENT; the interface is released straight away, only the
            // activation side effect matters.
            if AccessibleObjectFromWindow(*host, 0xFFFF_FFFC, &IAccessible::IID, &mut object).is_ok()
                && !object.is_null()
            {
                drop(IAccessible::from_raw(object));
            }
        }
        fresh |= ACTIVATED.with(|activated| activated.borrow_mut().insert(host.0 as isize));
    }
    if fresh {
        pause(600);
    }
    hosts
}

/// The Chromium render host windows inside `top` (at most four).
pub(super) fn render_hosts(top: HWND) -> Vec<HWND> {
    unsafe extern "system" fn find_render_host(hwnd: HWND, lparam: LPARAM) -> BOOL {
        if class_name(hwnd) == "Chrome_RenderWidgetHostHWND" {
            let found = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
            found.push(hwnd);
        }
        BOOL(1)
    }
    let mut hosts: Vec<HWND> = Vec::new();
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::EnumChildWindows(
            Some(top),
            Some(find_render_host),
            LPARAM(&mut hosts as *mut Vec<HWND> as isize),
        );
    }
    hosts.truncate(4);
    hosts
}

/// Make Chromium see a window it has never seen showing, so it creates the
/// render host its page's accessibility tree hangs from.
///
/// An Edge or Chrome window that opened completely covered counts as
/// occluded from the start and has no render host, and Chromium works out
/// again whether a window shows only when the window moves. A parked window
/// is moved into its corner again (one pixel on top, see `parking.rs`); a
/// window that is not parked is left alone: it is never made transparent
/// (spec FR-6).
fn wake_page(window: HWND) {
    if is_cornered(window) {
        move_off_screen(window);
        move_to_corner(window);
    }
}

/// Activate a Chromium window's accessibility and wait (up to ~4 s) until a
/// render host's page tree has content.
pub(super) fn wait_for_page_tree(window: HWND) {
    let Ok(automation) = automation() else {
        return;
    };
    let Ok(walker) = (unsafe { automation.ControlViewWalker() }) else {
        return;
    };
    let mut woken = false;
    for _ in 0..20 {
        if !woken && render_hosts(window).is_empty() && class_name(window).starts_with("Chrome_WidgetWin") {
            woken = true;
            wake_page(window);
        }
        let hosts = activate_chromium_accessibility(window);
        let ready = hosts.iter().any(|host| {
            unsafe { automation.ElementFromHandle(*host) }
                .and_then(|root| unsafe { walker.GetFirstChildElement(&root) })
                .and_then(|document| unsafe { walker.GetFirstChildElement(&document) })
                .is_ok()
        });
        // No render host at all (a composition-hosted WebView2 such as new
        // Teams): its tree hangs off the window itself; nothing to wait for.
        // Chromium's own window (Edge, Chrome, Electron) always gets one, but
        // only once its page starts rendering: returning early there parked
        // a window that was still starting before its page had a tree, and
        // the tree then stayed empty.
        let chromium_window = class_name(window).starts_with("Chrome_WidgetWin");
        if ready || (hosts.is_empty() && !chromium_window) {
            return;
        }
        pause(200);
    }
}

/// Where UI Automation walks start for this window: the window itself, then
/// each Chromium render host inside it (see [`activate_chromium_accessibility`]).
pub(super) fn automation_roots(automation: &IUIAutomation, window: HWND) -> Result<Vec<IUIAutomationElement>, String> {
    let hosts = activate_chromium_accessibility(window);
    let root = unsafe { automation.ElementFromHandle(window) }
        .map_err(|error| format!("UI Automation cannot read this window: {error}"))?;
    let mut roots = vec![root];
    for host in hosts {
        if let Ok(element) = unsafe { automation.ElementFromHandle(host) } {
            roots.push(element);
        }
    }
    Ok(roots)
}
