//! The two macOS permissions Computer App Control depends on:
//! Accessibility (to read and operate apps) and Screen Recording (to capture
//! their windows). Only the person's click in Settings makes macOS ask for
//! them (spec FR-28); the agent's actions only report what is missing.

use std::sync::atomic::{AtomicBool, Ordering};

use super::*;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

/// Whether evo may use the Accessibility API. With `prompt`, macOS shows
/// its "allow in System Settings" dialog the first time.
pub(super) fn accessibility_trusted(prompt: bool) -> bool {
    let key = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
    let value = if prompt { CFBoolean::true_value() } else { CFBoolean::false_value() };
    let options = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) != 0 }
}

pub(super) const ACCESSIBILITY_REFUSAL: &str = "macOS has not given evo Accessibility access, which computer app control needs to read and operate apps. Ask the person to press Allow next to Accessibility in evo's Settings → Computer apps, turn evo on in the pane that opens, then try again.";
pub(super) const SCREEN_RECORDING_REFUSAL: &str = "macOS has not given evo Screen Recording access, so app windows cannot be captured. Ask the person to press Allow next to Screen Recording in evo's Settings → Computer apps, turn evo on in the pane that opens, and restart evo. snapshot, find, invoke and set_value work without it.";

/// The person asked for Screen Recording in this run. macOS only reports it
/// granted after evo restarts, so until then Settings offers Restart evo.
static SCREEN_RECORDING_ASKED: AtomicBool = AtomicBool::new(false);

pub(super) fn screen_capture_allowed() -> bool {
    unsafe { CGPreflightScreenCaptureAccess() }
}

/// What Settings → Computer App Control shows: which of the two macOS
/// permissions evo has.
pub(super) fn permissions() -> Value {
    let screen_recording = screen_capture_allowed();
    json!({
        "required": true,
        "accessibility": accessibility_trusted(false),
        "screen_recording": screen_recording,
        "restart_needed": !screen_recording && SCREEN_RECORDING_ASKED.load(Ordering::Relaxed),
    })
}

/// Ask for one permission from Settings. The system call puts evo in
/// the pane's list (macOS only shows its own prompt the first time), and the
/// pane itself is opened so the user lands on the switch to turn on.
pub(super) fn request_permission(kind: &str) -> Result<Value, String> {
    let pane = match kind {
        "accessibility" => {
            accessibility_trusted(true);
            "Privacy_Accessibility"
        }
        "screen_recording" => {
            unsafe { CGRequestScreenCaptureAccess() };
            SCREEN_RECORDING_ASKED.store(true, Ordering::Relaxed);
            "Privacy_ScreenCapture"
        }
        other => return Err(format!("Unknown macOS permission {other:?}.")),
    };
    std::process::Command::new("open")
        .arg(format!("x-apple.systempreferences:com.apple.preference.security?{pane}"))
        .spawn()
        .map_err(|error| format!("Could not open System Settings: {error}"))?;
    Ok(permissions())
}
