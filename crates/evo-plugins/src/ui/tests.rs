// docs/features/plugin-ui/spec.md (FR-1, FR-2; T3).
use super::*;
use serde_json::json;

fn plugin_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("ui")).unwrap();
    for file in [
        "ui/wall.html",
        "ui/notes.html",
        "ui/chart.html",
        "ui/snip.html",
        "ui/icon.svg",
        "ui/readme.txt",
    ] {
        std::fs::write(dir.path().join(file), "x").unwrap();
    }
    dir
}

#[test]
fn the_four_slots_are_read_with_their_fields() {
    let dir = plugin_dir();
    let (ui, problems) = parse_ui(
        dir.path(),
        Some(&json!({
            "sidebar": [{ "id": "wall", "title": "Sticky Wall", "entry": "ui/wall.html", "icon": "ui/icon.svg", "defaultShown": true }],
            "panels": [{ "id": "notes", "title": "Notes", "entry": "./ui/notes.html", "shortcut": "k" }],
            "cards": [{ "id": "chart", "title": "Chart", "entry": "ui/chart.html" }],
            "composer": [{ "id": "snip", "title": "Insert snippet", "entry": "ui/snip.html" }]
        })),
    );
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(ui.sidebar.len(), 1);
    let wall = &ui.sidebar[0];
    assert_eq!(
        (wall.id.as_str(), wall.title.as_str(), wall.entry.as_str()),
        ("wall", "Sticky Wall", "ui/wall.html")
    );
    assert_eq!(wall.icon.as_deref(), Some("ui/icon.svg"));
    assert!(wall.default_shown);
    assert_eq!(
        ui.panels[0].entry, "ui/notes.html",
        "a leading ./ is dropped"
    );
    assert_eq!(
        ui.panels[0].shortcut.as_deref(),
        Some("K"),
        "a shortcut is one upper-case key"
    );
    assert_eq!((ui.cards.len(), ui.composer.len()), (1, 1));
    assert!(!ui.is_empty());
}

#[test]
fn no_ui_section_means_no_views() {
    let dir = plugin_dir();
    let (ui, problems) = parse_ui(dir.path(), None);
    assert!(ui.is_empty() && problems.is_empty());
}

#[test]
fn a_view_that_is_wrong_is_dropped_with_a_reason() {
    let dir = plugin_dir();
    let (ui, problems) = parse_ui(
        dir.path(),
        Some(&json!({
            "sidebar": [
                { "id": "out", "title": "Out", "entry": "../secret.html" },
                { "id": "gone", "title": "Gone", "entry": "ui/missing.html" },
                { "id": "text", "title": "Text", "entry": "ui/readme.txt" },
                { "id": "Bad Id", "title": "x", "entry": "ui/wall.html" },
                { "id": "notitle", "entry": "ui/wall.html" },
                { "id": "wall", "title": "Wall", "entry": "ui/wall.html", "icon": "../x.svg" },
                { "id": "wall", "title": "Again", "entry": "ui/wall.html" }
            ],
            "panels": [{ "id": "p", "title": "P", "entry": "ui/notes.html", "shortcut": "ctrl+k" }],
            "toolbar": [{ "id": "x", "title": "X", "entry": "ui/wall.html" }]
        })),
    );
    // Only the first `wall` survives (without its icon); the panel loses its shortcut.
    assert_eq!(ui.sidebar.len(), 1);
    assert_eq!(ui.sidebar[0].title, "Wall");
    assert_eq!(ui.sidebar[0].icon, None);
    assert_eq!(ui.panels.len(), 1);
    assert_eq!(ui.panels[0].shortcut, None);
    let text = problems.join("\n");
    for expected in [
        "outside the plugin",
        "missing.html",
        "an .html file",
        "Bad Id",
        "title",
        "icon",
        "used twice",
        "shortcut",
        "`toolbar`",
    ] {
        assert!(
            text.contains(expected),
            "no problem mentions {expected:?}:\n{text}"
        );
    }
}

#[test]
fn permissions_from_the_fixed_list_and_https_origins() {
    let (permissions, problems) = parse_permissions(Some(&json!([
        "chat:read",
        "storage",
        "chat:read",
        "network:https://api.example.com",
        "network:https://api.example.com:8443/",
        "network:http://insecure.example.com",
        "network:https://x.com/path",
        "files:write",
        7
    ])));
    assert_eq!(
        permissions,
        vec![
            "chat:read",
            "storage",
            "network:https://api.example.com",
            "network:https://api.example.com:8443"
        ]
    );
    let text = problems.join("\n");
    for expected in [
        "http://insecure",
        "/path",
        "files:write",
        "expected a string",
    ] {
        assert!(
            text.contains(expected),
            "no problem mentions {expected:?}:\n{text}"
        );
    }
}

#[test]
fn every_known_permission_reads_in_plain_words() {
    for permission in PERMISSIONS {
        assert!(!describe_permission(permission).is_empty());
    }
    assert_eq!(
        describe_permission("network:https://api.example.com"),
        "Connect to https://api.example.com"
    );
}

#[test]
fn host_keys_are_hostname_labels_and_differ_per_plugin() {
    let a = host_key("sticky-wall@acme");
    let b = host_key("sticky-wall@other");
    assert_ne!(a, b);
    for key in [
        &a,
        &b,
        &host_key("Ünïcode Plugin@local"),
        &host_key(&"x".repeat(200)),
    ] {
        assert!(key.len() <= 63, "{key}");
        assert!(
            key.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "{key}"
        );
        assert!(!key.starts_with('-') && !key.ends_with('-'), "{key}");
    }
    assert_eq!(host_key("sticky-wall@acme"), a, "stable");
}

// docs/features/plugin-ai/spec.md (FR-13, FR-16).
#[test]
fn ai_permissions_are_known_and_say_what_they_cost() {
    let (infos, problems) = parse_permission_info(Some(&serde_json::json!([
        { "id": "ai", "reason": "Explain charts" },
        "ai.tools:read",
        "ai.tools:command",
        "ai.tools:everything"
    ])));
    assert_eq!(
        infos.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        ["ai", "ai.tools:read", "ai.tools:command"]
    );
    assert_eq!(problems.len(), 1, "an unknown AI tool group is refused");
    assert!(describe_permission("ai").contains("money"));
    assert!(describe_permission("ai.tools:command").contains("commands"));
}
