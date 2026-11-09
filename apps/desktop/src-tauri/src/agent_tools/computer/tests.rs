//! Rules that hold for the whole computer app control module.

use std::path::{Path, PathBuf};

/// Calls that would use the person's own pointer, keyboard or foreground
/// window (ADR 0015, spec FR-5). The module must never name them outside a
/// comment. Showing the app to the person on their click lives outside this
/// module (`src/computer.rs`).
const FORBIDDEN: &[&str] = &[
    "SendInput",
    "SetCursorPos",
    "SetForegroundWindow",
    "BlockInput",
    "keybd_event",
    "mouse_event",
    "SetPhysicalCursorPos",
    "AllowSetForegroundWindow",
    "SwitchToThisWindow",
    "InjectSyntheticPointerInput",
    "InjectTouchInput",
];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read the module folder").flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// The places in `source` (outside `//` comments) that name a forbidden call,
/// as `line: name`.
pub(super) fn forbidden_uses(source: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (number, line) in source.lines().enumerate() {
        let code = match line.find("//") {
            Some(at) => &line[..at],
            None => line,
        };
        for name in FORBIDDEN {
            let named = code.match_indices(name).any(|(at, _)| {
                let before = code[..at].chars().next_back();
                let after = code[at + name.len()..].chars().next();
                let ident = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
                !ident(before) && !ident(after)
            });
            if named {
                found.push(format!("{}: {name}", number + 1));
            }
        }
    }
    found
}

#[test]
fn spots_a_planted_forbidden_call() {
    let planted = "fn f() {\n    // SendInput is never used\n    unsafe { SendInput(&inputs, size) };\n    let SendInputs = 1;\n}\n";
    assert_eq!(forbidden_uses(planted), vec!["3: SendInput".to_string()]);
}

#[test]
fn never_uses_the_persons_input_or_foreground() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/agent_tools/computer");
    let mut files = Vec::new();
    rust_files(&root, &mut files);
    let this = Path::new(file!()).file_name().expect("file name").to_owned();
    let mut offences = Vec::new();
    for file in files {
        if file.file_name() == Some(this.as_os_str()) {
            continue;
        }
        let source = std::fs::read_to_string(&file).expect("read a module file");
        for offence in forbidden_uses(&source) {
            offences.push(format!("{}:{offence}", file.display()));
        }
    }
    assert!(offences.is_empty(), "the computer module must not use the person's input (ADR 0015):\n{}", offences.join("\n"));
}

/// FR-30: the agent hears once each time the app comes to hold the front
/// window; the card hears every change.
#[test]
fn the_agent_is_told_once_each_time_the_app_takes_the_front() {
    let chat = "front-test-chat";
    assert!(!super::observe_front(chat, false), "nothing to tell the card at first");
    assert!(!super::take_front_news(chat), "not in front: nothing to tell the agent");
    assert!(super::observe_front(chat, true), "the card hears the app took the front");
    assert!(super::take_front_news(chat), "the agent hears it once");
    assert!(!super::observe_front(chat, true));
    assert!(!super::take_front_news(chat), "and not again while it lasts");
    assert!(super::observe_front(chat, false), "the card hears it ended");
    assert!(super::observe_front(chat, true));
    assert!(super::take_front_news(chat), "a second time is told again");
    super::forget_front(chat);
}

#[test]
fn the_front_warning_names_the_app_and_what_to_tell_the_person() {
    let text = super::front_text("EXCEL.EXE");
    assert!(text.starts_with("Warning: EXCEL.EXE is now the person's front window"), "{text}");
    assert!(text.contains("Tell the person right away"), "{text}");
    assert!(super::front_text("").contains("The app is now"));
}
