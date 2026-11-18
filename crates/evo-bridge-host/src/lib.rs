//! Evo Webbridge's native messaging host and the pieces the Evo app shares
//! with it: frame encoding, the pipe's name, and the rule that the host and
//! the app must run from the same folder
//! (docs/features/browser-bridge/spec.md FR-3, FR-19, NFR-1; plan 3.3).
//!
//! The browser starts the host (one per profile) for the Evo extension. The
//! host connects to the app's per-user pipe, checks that the process serving
//! it is Evo, tells the app which extension started it, and then relays
//! frames both ways without reading them.

pub mod frames;
pub mod peer;

/// The native messaging host name Chrome and Edge know the host by.
pub const HOST_NAME: &str = "dev.evo.webbridge";

/// Executable file names of the Evo app, newest first (the installed app,
/// then the development build).
pub const APP_EXECUTABLES: &[&str] = if cfg!(windows) {
    &["evo-harness.exe", "evo-desktop.exe"]
} else {
    &["evo-harness", "evo-desktop"]
};

/// The host's own executable name.
pub const HOST_EXECUTABLE: &str = if cfg!(windows) {
    "evo-bridge-host.exe"
} else {
    "evo-bridge-host"
};

/// The first frame the host sends the app, before relaying: who started it.
pub fn intro_frame(origin: &str) -> Vec<u8> {
    let origin = origin.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        r#"{{"t":"host","origin":"{origin}","pid":{}}}"#,
        std::process::id()
    )
    .into_bytes()
}

/// The frame the host sends the extension when it could not reach the app.
pub fn unreachable_frame(reason: &str) -> Vec<u8> {
    let reason = reason.replace('\\', "\\\\").replace('"', "\\\"");
    format!(r#"{{"t":"bye","reason":"{reason}"}}"#).into_bytes()
}

/// Whether the extension's first frame asks the host to start Evo (`"launch":true`).
pub fn wants_launch(hello: &[u8]) -> bool {
    let text = String::from_utf8_lossy(hello);
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    compact.contains(r#""launch":true"#)
}

/// The caller's origin among the host's arguments (`chrome-extension://<id>/`).
pub fn origin_arg(args: &[String]) -> Option<&str> {
    args.iter()
        .map(String::as_str)
        .find(|arg| arg.starts_with("chrome-extension://"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_origin_is_found_among_the_browser_arguments() {
        let args = vec![
            "evo-bridge-host.exe".to_string(),
            "chrome-extension://abc/".to_string(),
            "--parent-window=0".to_string(),
        ];
        assert_eq!(origin_arg(&args), Some("chrome-extension://abc/"));
        assert_eq!(origin_arg(&args[..1]), None);
    }

    #[test]
    fn only_a_hello_that_asks_starts_evo() {
        assert!(wants_launch(br#"{"t":"hello","launch": true}"#));
        assert!(!wants_launch(br#"{"t":"hello","launch":false}"#));
        assert!(!wants_launch(br#"{"t":"hello"}"#));
    }

    #[test]
    fn the_intro_frame_is_json() {
        let frame = String::from_utf8(intro_frame("chrome-extension://abc/")).unwrap();
        assert!(frame.starts_with(r#"{"t":"host","origin":"chrome-extension://abc/","pid":"#));
        assert!(frame.ends_with('}'));
    }
}
