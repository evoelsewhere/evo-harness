// docs/features/plugin-settings/spec.md (FR-2 – FR-4; T5).
use super::*;
use serde_json::json;

fn parse(settings: Value) -> (Vec<Setting>, Vec<String>) {
    parse_settings(Some(&settings), None)
}

#[test]
fn every_kind_is_read_with_its_fields() {
    let (settings, problems) = parse(json!({
        "workspace": { "kind": "text", "title": "Workspace", "required": true, "scope": "workspace", "pattern": "[a-z-]+" },
        "notes": { "kind": "multiline" },
        "retries": { "kind": "number", "min": 0, "max": 5, "default": 3, "group": "Advanced" },
        "verbose": { "kind": "boolean", "default": false },
        "region": { "kind": "select", "options": ["eu", "us"], "default": "eu" },
        "out": { "kind": "folder" },
        "key": { "kind": "file" },
        "apiToken": { "kind": "secret", "description": "From the service" }
    }));
    assert!(problems.is_empty(), "{problems:?}");
    let kinds: Vec<Kind> = settings.iter().map(|s| s.kind).collect();
    assert_eq!(
        kinds,
        [
            Kind::Text,
            Kind::Multiline,
            Kind::Number,
            Kind::Boolean,
            Kind::Select,
            Kind::Folder,
            Kind::File,
            Kind::Secret
        ]
    );
    assert_eq!(settings[0].scope, Scope::Workspace);
    assert!(settings[0].required);
    assert_eq!(settings[2].default, Some(json!(3.0)));
    assert_eq!(settings[2].group.as_deref(), Some("Advanced"));
    assert_eq!(
        settings[1].title, "notes",
        "the key titles a setting without one"
    );
}

#[test]
fn older_user_config_reads_as_settings_and_sensitive_is_a_secret() {
    let (settings, problems) = parse_settings(
        Some(&json!({ "token": { "kind": "text", "title": "New" } })),
        Some(&json!({
            "token": { "type": "string", "title": "Old" },
            "password": { "type": "string", "sensitive": true },
            "dir": { "type": "directory", "required": true }
        })),
    );
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(settings.len(), 3);
    assert_eq!(settings[0].title, "New", "settings win over userConfig");
    assert_eq!(settings[1].kind, Kind::Secret);
    assert_eq!(settings[2].kind, Kind::Folder);
}

#[test]
fn wrong_settings_are_dropped_or_corrected_with_a_reason() {
    let (settings, problems) = parse(json!({
        "bad key": { "kind": "text" },
        "pick": { "kind": "select" },
        "what": { "kind": "colour" },
        "re": { "kind": "text", "pattern": "([" },
        "n": { "kind": "number", "max": 2, "default": 9 },
        "s": { "kind": "secret", "default": "x" },
        "sc": { "kind": "text", "scope": "global" }
    }));
    let keys: Vec<&str> = settings.iter().map(|s| s.key.as_str()).collect();
    assert_eq!(keys, ["re", "n", "s", "sc"]);
    assert!(
        settings[0].pattern.is_none()
            && settings[1].default.is_none()
            && settings[2].default.is_none()
    );
    let text = problems.join("\n");
    for expected in [
        "bad key",
        "needs `options`",
        "colour",
        "regular expression",
        "at most 2",
        "secret cannot have a default",
        "global",
    ] {
        assert!(
            text.contains(expected),
            "no problem mentions {expected:?}:\n{text}"
        );
    }
}

#[test]
fn values_are_checked_against_their_setting() {
    let (settings, _) = parse(json!({
        "name": { "kind": "text", "pattern": "[a-z]+", "max": 8 },
        "n": { "kind": "number", "min": 1, "max": 3 },
        "region": { "kind": "select", "options": ["eu", "us"] },
        "on": { "kind": "boolean" },
        "token": { "kind": "secret" }
    }));
    let problems = validate(
        &settings,
        json!({ "name": "Ab", "n": 7, "region": "asia", "on": "yes", "token": "t", "extra": 1 })
            .as_object()
            .unwrap(),
    );
    let mut keys: Vec<&str> = problems.iter().map(|(k, _)| k.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["extra", "n", "name", "on", "region", "token"]);
    assert!(validate(
        &settings,
        json!({ "name": "ok", "n": "2", "region": "eu", "on": true })
            .as_object()
            .unwrap()
    )
    .is_empty());
    assert_eq!(
        check(&settings[1], &json!(" 2 ")).unwrap(),
        json!(2.0),
        "numbers from text"
    );
    assert_eq!(
        check(&settings[0], &json!("toolongname")).unwrap_err(),
        "must have at most 8 characters"
    );
}

#[test]
fn required_settings_without_a_value_hold_the_plugin_back() {
    let (settings, _) = parse(json!({
        "workspace": { "kind": "text", "required": true },
        "region": { "kind": "select", "options": ["eu"], "default": "eu", "required": true },
        "token": { "kind": "secret", "required": true },
        "optional": { "kind": "text" }
    }));
    assert_eq!(
        missing_required(&settings, &Map::new(), &[]),
        ["workspace", "token"]
    );
    let values = json!({ "workspace": "team" });
    assert_eq!(
        missing_required(&settings, values.as_object().unwrap(), &["token".into()]),
        Vec::<String>::new()
    );
    assert_eq!(
        missing_required(
            &settings,
            json!({ "workspace": "" }).as_object().unwrap(),
            &["token".into()]
        ),
        ["workspace"]
    );
}

#[test]
fn workspace_values_override_personal_ones_for_workspace_settings() {
    let (settings, _) = parse(json!({
        "workspace": { "kind": "text", "scope": "workspace" },
        "region": { "kind": "select", "options": ["eu", "us"], "default": "eu" },
        "token": { "kind": "secret" }
    }));
    let personal = json!({ "workspace": "mine", "region": "us" });
    let in_workspace = json!({ "workspace": "team-a", "region": "eu" });
    let values = effective(
        &settings,
        personal.as_object().unwrap(),
        Some(in_workspace.as_object().unwrap()),
    );
    assert_eq!(values["workspace"], "team-a");
    assert_eq!(
        values["region"], "us",
        "a personal setting ignores workspace values"
    );
    assert!(!values.contains_key("token"));
    assert_eq!(
        effective(&settings, &Map::new(), None)["region"],
        "eu",
        "the default"
    );
}
