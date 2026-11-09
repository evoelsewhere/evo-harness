//! Keeping the person's front window while the agent acts (spec FR-30).
//!
//! A driven app may answer evo's input by activating a window of its own: a
//! new document, a dialog. While any hold is alive, and for a short grace
//! after the last one ends, evo holds the system's foreground lock, so no
//! program can take the front window. The person's own clicks and Alt+Tab
//! still change it. Holds are counted: chats act in parallel, and the page
//! channel holds one while an app starts.

use super::*;

/// How long the lock outlives an action: an app's dialog may open a moment
/// after the input that asked for it.
const GRACE: Duration = Duration::from_millis(1500);

/// Holds alive now.
static HOLDS: Lazy<Mutex<u32>> = Lazy::new(|| Mutex::new(0));

/// While alive, no program can take the front window.
pub(super) struct FrontHold(());

impl Drop for FrontHold {
    fn drop(&mut self) {
        release_after(GRACE);
    }
}

/// Hold the front window where it is.
pub(super) fn hold_front() -> FrontHold {
    take();
    FrontHold(())
}

/// Hold the front window for an action that sends input or opens an app.
pub(super) fn holds_front(action: Action) -> bool {
    action.is_input() || matches!(action, Action::OpenApp)
}

fn take() {
    let mut holds = HOLDS.lock().unwrap_or_else(|poison| poison.into_inner());
    *holds += 1;
    if *holds == 1 {
        set_lock(true);
    }
}

/// End one hold after `delay`, on a thread of its own: the action's result
/// does not wait for the grace.
fn release_after(delay: Duration) {
    let release = move || {
        std::thread::sleep(delay);
        give_back();
    };
    if std::thread::Builder::new().name("computer-app-front".into()).spawn(release).is_err() {
        give_back();
    }
}

fn give_back() {
    let mut holds = HOLDS.lock().unwrap_or_else(|poison| poison.into_inner());
    *holds = holds.saturating_sub(1);
    if *holds == 0 {
        set_lock(false);
    }
}

fn set_lock(lock: bool) {
    #[cfg(test)]
    if FAKE_LOCK.load(Ordering::SeqCst) || std::env::var_os("EVO_TEST_NO_FRONT_LOCK").is_some() {
        LOCKS.lock().unwrap().push(lock);
        return;
    }
    use windows::Win32::UI::WindowsAndMessaging::{LockSetForegroundWindow, LSFW_LOCK, LSFW_UNLOCK};
    unsafe {
        let _ = LockSetForegroundWindow(if lock { LSFW_LOCK } else { LSFW_UNLOCK });
    }
}

/// Unit tests record the lock changes instead of locking the machine; the
/// live tests and the benchmark take the real lock.
#[cfg(test)]
static FAKE_LOCK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(test)]
static LOCKS: Lazy<Mutex<Vec<bool>>> = Lazy::new(|| Mutex::new(Vec::new()));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lock_lifts_only_after_the_last_hold_and_its_grace() {
        FAKE_LOCK.store(true, Ordering::SeqCst);
        LOCKS.lock().unwrap().clear();
        let first = hold_front();
        let second = hold_front();
        drop(first);
        drop(second);
        // Both holds ended; the lock stays through the grace.
        assert_eq!(*LOCKS.lock().unwrap(), vec![true]);
        std::thread::sleep(GRACE + Duration::from_millis(500));
        assert_eq!(*LOCKS.lock().unwrap(), vec![true, false]);
        assert_eq!(*HOLDS.lock().unwrap(), 0);
    }

    #[test]
    fn actions_that_send_input_hold_the_front() {
        assert!(holds_front(Action::Click));
        assert!(holds_front(Action::Key));
        assert!(holds_front(Action::OpenApp));
        assert!(!holds_front(Action::Snapshot));
        assert!(!holds_front(Action::Screenshot));
    }
}
