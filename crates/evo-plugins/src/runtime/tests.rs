// docs/features/plugin-settings/spec.md (FR-10, FR-11, FR-15, FR-16; T5).
use super::*;
use serde_json::json;

fn plugin(files: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for file in files {
        let path = dir.path().join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "x").unwrap();
    }
    dir
}

#[test]
fn a_backend_is_read_with_defaults() {
    let dir = plugin(&["server/main.ts"]);
    let (runtime, problems) = parse_runtime(
        dir.path(),
        Some(
            &json!({ "server": { "language": "typescript", "entry": "./server/main.ts", "env": { "REGION": "${user_config.region}", "API_TOKEN": "Bearer ${secret.apiToken}" } } }),
        ),
    );
    assert!(problems.is_empty(), "{problems:?}");
    let runtime = runtime.unwrap();
    assert_eq!(
        (runtime.language, runtime.entry.as_str()),
        (Language::Typescript, "server/main.ts")
    );
    assert_eq!(
        (runtime.startup, runtime.idle_stop_minutes),
        (StartMode::OnDemand, DEFAULT_IDLE_MINUTES)
    );
    assert_eq!(runtime.env.len(), 2);
    assert_eq!(parse_runtime(dir.path(), None), (None, vec![]));
}

#[test]
fn a_wrong_backend_says_why() {
    let dir = plugin(&["main.py", "pyproject.toml"]);
    let read = |value: Value| parse_runtime(dir.path(), Some(&json!({ "server": value })));
    assert!(read(json!({ "language": "ruby", "entry": "main.py" })).1[0].contains("language"));
    assert!(
        read(json!({ "language": "python", "entry": "../x.py" })).1[0]
            .contains("not a file inside")
    );
    assert!(
        read(json!({ "language": "python", "entry": "missing.py" })).1[0]
            .contains("not a file inside")
    );
    let (runtime, problems) = read(
        json!({ "language": "python", "entry": "main.py", "startup": "always", "idleStopMinutes": 0, "env": { "lower": "x", "EVO_PLUGIN_KEY": "x", "OK": 1 } }),
    );
    assert!(runtime.is_some(), "minor problems keep the backend");
    let text = problems.join("\n");
    for expected in [
        "`always`",
        "idleStopMinutes",
        "`lower`",
        "`EVO_PLUGIN_KEY`",
        "`OK`",
        "uv.lock",
    ] {
        assert!(
            text.contains(expected),
            "no problem mentions {expected:?}:\n{text}"
        );
    }
}

#[test]
fn deno_gets_only_what_was_approved() {
    let dir = plugin(&["server/main.ts"]);
    let runtime = parse_runtime(dir.path(), Some(&json!({ "server": { "language": "typescript", "entry": "server/main.ts", "env": { "REGION": "${user_config.region}", "API_TOKEN": "${secret.apiToken}" } } }))).0.unwrap();
    let data = dir.path().join("data");
    let workspace = dir.path().join("ws");
    let approved: Vec<String> = [
        "runtime",
        "network:https://api.github.com",
        "network:https://api.example.com:8443",
        "files:read:workspace",
        "files:write:C:/out",
    ]
    .map(String::from)
    .to_vec();
    let values = json!({ "region": "eu" });
    let launch = launch(&LaunchInput {
        runtime: &runtime,
        root: dir.path(),
        data: &data,
        approved: &approved,
        deno: Path::new("deno.exe"),
        host_port: Some(40123),
        workspace: Some(&workspace),
        values: values.as_object().unwrap(),
    });
    let args = launch.args.join(" ");
    assert!(
        args.contains("--allow-net=api.github.com:443,api.example.com:8443,127.0.0.1:40123"),
        "{args}"
    );
    assert!(
        args.contains(&format!(
            "--allow-read={},{},{},C:/out",
            dir.path().display(),
            data.display(),
            workspace.display()
        )),
        "{args}"
    );
    assert!(
        args.contains(&format!("--allow-write={},C:/out", data.display())),
        "{args}"
    );
    assert!(
        args.contains("--allow-env=EVO_PLUGIN_HOST,EVO_PLUGIN_KEY,API_TOKEN,REGION"),
        "{args}"
    );
    for flag in [
        "--cached-only",
        "--no-prompt",
        "--node-modules-dir=none",
        "--deny-run",
        "--deny-ffi",
    ] {
        assert!(args.contains(flag), "{flag}: {args}");
    }
    assert!(
        !args.contains("--allow-all") && !args.contains("--allow-run"),
        "{args}"
    );
    assert_eq!(launch.env["REGION"], "eu");
    assert_eq!(
        launch.env["API_TOKEN"], "${secret.apiToken}",
        "secrets are filled by the shell"
    );
    assert_eq!(launch.env["EVO_PLUGIN_HOST"], "http://127.0.0.1:40123");
    assert_eq!(launch.cwd, data);
}

#[test]
fn with_no_network_approved_deno_gets_no_network() {
    let dir = plugin(&["main.js"]);
    let runtime = parse_runtime(
        dir.path(),
        Some(&json!({ "server": { "language": "javascript", "entry": "main.js" } })),
    )
    .0
    .unwrap();
    let launch = launch(&LaunchInput {
        runtime: &runtime,
        root: dir.path(),
        data: dir.path(),
        approved: &["runtime".into(), "files:read:workspace".into()],
        deno: Path::new("deno"),
        host_port: None,
        workspace: None,
        values: &Map::new(),
    });
    assert!(
        !launch.args.iter().any(|a| a.starts_with("--allow-net")),
        "{:?}",
        launch.args
    );
}

#[test]
fn python_runs_from_its_environment_and_is_not_enforced() {
    let dir = plugin(&["main.py", "uv.lock"]);
    let runtime = parse_runtime(
        dir.path(),
        Some(&json!({ "server": { "language": "python", "entry": "main.py" } })),
    )
    .0
    .unwrap();
    assert!(!runtime.language.enforced());
    let data = dir.path().join("data");
    let launch = launch(&LaunchInput {
        runtime: &runtime,
        root: dir.path(),
        data: &data,
        approved: &[],
        deno: Path::new("deno"),
        host_port: Some(1),
        workspace: None,
        values: &Map::new(),
    });
    assert!(
        launch.program.starts_with(data.join("venv")),
        "{:?}",
        launch.program
    );
    assert_eq!(
        launch.args,
        [dir.path().join("main.py").display().to_string()]
    );
}

#[test]
fn secrets_fill_in_or_the_variable_is_left_out() {
    let mut env = BTreeMap::from([
        ("A".to_string(), "Bearer ${secret.token}".to_string()),
        ("B".to_string(), "${secret.missing}".to_string()),
        ("C".to_string(), "plain".to_string()),
    ]);
    assert_eq!(secret_keys(&env), ["token", "missing"]);
    fill_secrets(&mut env, |key| (key == "token").then(|| "t-1".to_string()));
    assert_eq!(env.get("A").map(String::as_str), Some("Bearer t-1"));
    assert!(!env.contains_key("B"));
    assert_eq!(env["C"], "plain");
    assert_eq!(net_host("network:https://x.com"), Some("x.com:443".into()));
    assert_eq!(net_host("chat:read"), None);
}
