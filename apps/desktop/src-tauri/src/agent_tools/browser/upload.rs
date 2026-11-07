//! `browser_upload` (docs/features/agent-browser FR-5, T17): put files on a
//! file input by ref.
//!
//! A change: it hands the page files from the machine, so it asks in the
//! approval modes that ask for changes, and it only reads from the chat's
//! working folder (and below) or the operating system's temp folder. The files
//! are set with the DevTools protocol (`DOM.setFileInputFiles` on the input's
//! object, found by the ref the page helper put on it), which also reaches
//! inputs inside same-origin frames; the input's `files` are read back to
//! verify. Windows only: elsewhere the tool says so.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{json, Value};

use super::driver::{finish, gate_page, is_dialog_error, resolve_page, Page, ToolResult};
use super::wait::with_element;
use crate::agent_tools::{BoxFuture, Class, Tool, ToolCtx, ToolOutput};

const UPLOAD: &str = "Attach files to a file input (<input type=file>, a ref from browser_read_page or browser_find; its role shows as \"file\"). paths are 1 to 10 file paths, relative to the working folder or absolute; they must be files inside the working folder or the system temp folder. Several paths need an input with multiple. Reads the files back from the input and reports their names. Many pages hide the input behind a button: find it with browser_find \"file\" and pass its ref; do not click the button (that opens the system file dialog).";

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "ref": { "type": "string", "description": "The file input (a ref from browser_read_page / browser_find)." },
            "paths": { "type": "array", "items": { "type": "string" }, "minItems": 1, "maxItems": 10, "description": "File paths, relative to the working folder or absolute." },
            "tabId": { "type": "string", "description": "A tab id from browser_tabs; default: the active tab." },
        },
        "required": ["ref", "paths"],
        "additionalProperties": false,
    })
}

fn run(ctx: ToolCtx, args: Value) -> BoxFuture<ToolOutput> {
    Box::pin(async move { finish(upload(&ctx, &args).await) })
}

/// The tool of this module.
pub fn tools() -> Vec<Tool> {
    vec![Tool { name: "browser_upload", description: UPLOAD, schema, class: Class::Command, run }]
}

// ── Arguments and path rules ────────────────────────────────────────────────

pub const MAX_FILES: usize = 10;

#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub reference: String,
    pub paths: Vec<String>,
}

pub fn parse_args(args: &Value) -> Result<Request, String> {
    let reference = match args.get("ref") {
        Some(Value::String(r)) if !r.trim().is_empty() => r.trim().to_string(),
        _ => return Err("browser_upload needs the ref of a file input.".into()),
    };
    let Some(Value::Array(items)) = args.get("paths") else {
        return Err("browser_upload needs paths: a list of 1 to 10 file paths.".into());
    };
    if items.is_empty() || items.len() > MAX_FILES {
        return Err(format!("paths must hold 1 to {MAX_FILES} file paths, not {}.", items.len()));
    }
    let mut paths = Vec::with_capacity(items.len());
    for item in items {
        match item {
            Value::String(path) if !path.trim().is_empty() => paths.push(path.trim().to_string()),
            _ => return Err("every path must be a non-empty string.".into()),
        }
    }
    Ok(Request { reference, paths })
}

/// `raw` as a path: absolute stays, relative is under `base`.
pub fn resolve(base: &Path, raw: &str) -> PathBuf {
    let path = Path::new(raw);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

/// The folders an upload may read from.
pub fn allowed_roots(folder: &Path, temp: &Path) -> Vec<PathBuf> {
    vec![folder.to_path_buf(), temp.to_path_buf()]
}

/// The paths as the operating system should be told them (no `\\?\` prefix).
pub fn os_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => rest.to_string(),
        _ => text.to_string(),
    }
}

/// Resolve and check every path: it exists, is a file, and lies inside `folder` or `temp` (after
/// `..` and links are resolved). On success the canonical paths; on failure the message to return.
pub fn check_paths(raw: &[String], folder: &Path, temp: &Path) -> Result<Vec<PathBuf>, String> {
    let roots: Vec<PathBuf> = allowed_roots(folder, temp).iter().filter_map(|root| std::fs::canonicalize(root).ok()).collect();
    let mut found = Vec::with_capacity(raw.len());
    for item in raw {
        let wanted = resolve(folder, item);
        let real = std::fs::canonicalize(&wanted).map_err(|_| format!("{item} does not exist (looked for {}).", os_path(&wanted)))?;
        if !real.is_file() {
            return Err(format!("{item} is not a file."));
        }
        if !roots.iter().any(|root| real.starts_with(root)) {
            return Err(format!(
                "{item} is outside the folders uploads may read from: {}, {}. Copy the file into the working folder first.",
                os_path(folder),
                os_path(temp)
            ));
        }
        found.push(real);
    }
    Ok(found)
}

/// The names of `files`, in order.
pub fn file_names(files: &[PathBuf]) -> Vec<String> {
    files.iter().map(|f| f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()).collect()
}

/// The line the gate shows: `Attach report.pdf to ref_3`.
pub fn attach_summary(names: &[String], reference: &str) -> String {
    match names {
        [one] => format!("Attach {one} to {reference}"),
        many => format!("Attach {} files to {reference} ({})", many.len(), many.join(", ")),
    }
}

// ── The input ───────────────────────────────────────────────────────────────

/// What the page says about the element a ref names.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum Input {
    Gone,
    NotFile { what: String },
    Ok { multiple: bool, disabled: bool },
}

const INPUT_PROBE: &str = r#"if (!el || !el.isConnected) return { status: "gone" };
if (!(el.tagName === "INPUT" && el.type === "file")) return { status: "notfile", what: el.tagName.toLowerCase() + (el.type ? "[type=" + el.type + "]" : "") };
return { status: "ok", multiple: !!el.multiple, disabled: !!el.disabled };"#;

/// Whether `count` files may go into this input; the message when not.
pub fn check_input(input: &Input, reference: &str, count: usize) -> Result<(), String> {
    match input {
        Input::Gone => Err(format!("{reference} is no longer on the page; read the page again.")),
        Input::NotFile { what } => Err(format!(
            "{reference} is not a file input (it is {what}). Pages often hide the input behind a button: use browser_find with the query \"file\" for the input's own ref."
        )),
        Input::Ok { disabled: true, .. } => Err(format!("{reference} is disabled.")),
        Input::Ok { multiple: false, .. } if count > 1 => {
            Err(format!("{reference} accepts one file (it has no multiple attribute), but {count} paths were given."))
        }
        Input::Ok { .. } => Ok(()),
    }
}

fn element_expression(reference: &str) -> String {
    with_element(reference, "return el;")
}

/// Whether the files read back from the input are the ones sent (by name and count).
pub fn verify(sent: &[String], read: &[String]) -> Result<(), String> {
    if sent == read {
        return Ok(());
    }
    Err(format!(
        "The input holds {} after the upload ({}), not the {} sent ({}).",
        read.len(),
        if read.is_empty() { "nothing".to_string() } else { read.join(", ") },
        sent.len(),
        sent.join(", ")
    ))
}

async fn set_files(page: &Page, reference: &str, files: &[PathBuf]) -> Result<(), String> {
    let found = page.cdp("Runtime.evaluate", json!({ "expression": element_expression(reference), "returnByValue": false })).await?;
    let object = found
        .get("result")
        .and_then(|r| r.get("objectId"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("{reference} is no longer on the page; read the page again."))?;
    let paths: Vec<String> = files.iter().map(|f| os_path(f)).collect();
    let params = json!({ "objectId": object, "files": paths });
    let mut result = page.cdp("DOM.setFileInputFiles", params.clone()).await;
    if let Err(error) = &result {
        if !is_dialog_error(error) {
            // The DOM domain may need switching on first.
            let _ = page.cdp("DOM.enable", json!({})).await;
            result = page.cdp("DOM.setFileInputFiles", params).await;
        }
    }
    let _ = page.cdp("Runtime.releaseObject", json!({ "objectId": object })).await;
    result.map(|_| ())
}

async fn upload(ctx: &ToolCtx, args: &Value) -> ToolResult {
    let request = parse_args(args)?;
    let page = resolve_page(ctx, args)?;
    if !cfg!(windows) {
        return Err("browser_upload is not available on this platform (it needs the DevTools protocol, Windows): ask the person to attach the file.".into());
    }
    let files = check_paths(&request.paths, &ctx.folder(), &std::env::temp_dir())?;
    let names = file_names(&files);
    gate_page(ctx, &page, &attach_summary(&names, &request.reference)).await?;

    let reference = &request.reference;
    let probe = page.eval(&with_element(reference, INPUT_PROBE), false).await?;
    let input: Input = serde_json::from_value(probe).map_err(|error| format!("the page answered something unexpected: {error}"))?;
    check_input(&input, reference, files.len())?;

    set_files(&page, reference, &files).await?;

    let read = page.eval(&with_element(reference, "return el ? Array.from(el.files || []).map((f) => f.name) : null;"), false).await?;
    let read: Vec<String> = read.as_array().map(|items| items.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
    verify(&names, &read)?;

    let multiple = matches!(input, Input::Ok { multiple: true, .. });
    let mut lines = vec![format!(
        "Attached {} to {reference}{}: {}.",
        if names.len() == 1 { "1 file".to_string() } else { format!("{} files", names.len()) },
        if multiple { " (multiple)" } else { "" },
        names.join(", ")
    )];
    if let Some(state) = page.state_of(reference).await {
        lines.push(format!("Now: {state}"));
    }
    Ok(ToolOutput::text(lines.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A scratch tree: `work/` (the folder), `tmp/` (the temp folder), `other/` (neither).
    struct Tree(PathBuf);

    impl Tree {
        fn new(name: &str) -> Self {
            let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
            let root = std::env::temp_dir().join(format!("evo-upload-test-{name}-{}-{nanos}", std::process::id()));
            for dir in ["work/sub", "tmp", "other"] {
                fs::create_dir_all(root.join(dir)).unwrap();
            }
            Tree(root)
        }

        fn file(&self, relative: &str) -> PathBuf {
            let path = self.0.join(relative);
            fs::write(&path, b"data").unwrap();
            path
        }

        fn work(&self) -> PathBuf {
            self.0.join("work")
        }

        fn temp(&self) -> PathBuf {
            self.0.join("tmp")
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn arguments_need_a_ref_and_one_to_ten_paths() {
        let ok = parse_args(&json!({ "ref": " ref_4 ", "paths": ["a.txt", " b.txt "] })).unwrap();
        assert_eq!(ok, Request { reference: "ref_4".into(), paths: strings(&["a.txt", "b.txt"]) });
        assert!(parse_args(&json!({ "paths": ["a"] })).unwrap_err().contains("ref"));
        assert!(parse_args(&json!({ "ref": "ref_1" })).unwrap_err().contains("paths"));
        assert!(parse_args(&json!({ "ref": "ref_1", "paths": [] })).unwrap_err().contains("1 to 10"));
        let eleven: Vec<String> = (0..11).map(|i| format!("f{i}")).collect();
        assert!(parse_args(&json!({ "ref": "ref_1", "paths": eleven })).unwrap_err().contains("not 11"));
        assert!(parse_args(&json!({ "ref": "ref_1", "paths": ["a", 3] })).is_err());
        assert!(parse_args(&json!({ "ref": "ref_1", "paths": [" "] })).is_err());
        assert!(parse_args(&json!({ "ref": "ref_1", "paths": "a.txt" })).is_err());
        let ten: Vec<String> = (0..10).map(|i| format!("f{i}")).collect();
        assert!(parse_args(&json!({ "ref": "ref_1", "paths": ten })).is_ok());
    }

    #[test]
    fn relative_paths_are_under_the_working_folder() {
        let base = Path::new("work");
        assert_eq!(resolve(base, "a.txt"), Path::new("work").join("a.txt"));
        let absolute = std::env::temp_dir().join("x.txt");
        assert_eq!(resolve(base, &absolute.to_string_lossy()), absolute);
    }

    #[test]
    fn files_in_the_working_folder_and_the_temp_folder_are_allowed() {
        let tree = Tree::new("allowed");
        let inside = tree.file("work/a.txt");
        tree.file("work/sub/b.txt");
        let in_temp = tree.file("tmp/c.txt");
        let found = check_paths(&strings(&["a.txt", "sub/b.txt", &in_temp.to_string_lossy()]), &tree.work(), &tree.temp()).unwrap();
        assert_eq!(found.len(), 3);
        assert_eq!(file_names(&found), ["a.txt", "b.txt", "c.txt"]);
        assert_eq!(found[0], fs::canonicalize(inside).unwrap());
    }

    #[test]
    fn a_file_outside_both_folders_is_refused_with_the_allowed_ones() {
        let tree = Tree::new("outside");
        let outside = tree.file("other/secret.txt");
        let error = check_paths(&strings(&[&outside.to_string_lossy()]), &tree.work(), &tree.temp()).unwrap_err();
        assert!(error.contains("outside the folders uploads may read from"), "{error}");
        assert!(error.contains(&os_path(&tree.work())) && error.contains(&os_path(&tree.temp())), "{error}");
    }

    #[test]
    fn dot_dot_does_not_leave_the_folder() {
        let tree = Tree::new("dotdot");
        tree.file("other/secret.txt");
        let error = check_paths(&strings(&["../other/secret.txt"]), &tree.work(), &tree.temp()).unwrap_err();
        assert!(error.contains("outside"), "{error}");
    }

    #[test]
    fn missing_files_and_folders_are_refused() {
        let tree = Tree::new("missing");
        let error = check_paths(&strings(&["nope.txt"]), &tree.work(), &tree.temp()).unwrap_err();
        assert!(error.starts_with("nope.txt does not exist"), "{error}");
        let error = check_paths(&strings(&["sub"]), &tree.work(), &tree.temp()).unwrap_err();
        assert_eq!(error, "sub is not a file.");
    }

    #[test]
    fn one_bad_path_refuses_the_whole_call() {
        let tree = Tree::new("mixed");
        tree.file("work/a.txt");
        assert!(check_paths(&strings(&["a.txt", "gone.txt"]), &tree.work(), &tree.temp()).is_err());
    }

    #[test]
    fn verbatim_prefixes_are_dropped_for_the_system() {
        assert_eq!(os_path(Path::new(r"\\?\C:\Users\me\a.txt")), r"C:\Users\me\a.txt");
        assert_eq!(os_path(Path::new(r"\\?\UNC\server\share\a.txt")), r"\\server\share\a.txt");
        assert_eq!(os_path(Path::new("/home/me/a.txt")), "/home/me/a.txt");
    }

    #[test]
    fn the_gate_line_names_the_files() {
        assert_eq!(attach_summary(&strings(&["report.pdf"]), "ref_3"), "Attach report.pdf to ref_3");
        assert_eq!(attach_summary(&strings(&["a", "b"]), "ref_3"), "Attach 2 files to ref_3 (a, b)");
    }

    #[test]
    fn the_input_decides_how_many_files_it_takes() {
        let single = Input::Ok { multiple: false, disabled: false };
        let multi = Input::Ok { multiple: true, disabled: false };
        assert!(check_input(&single, "ref_1", 1).is_ok());
        assert!(check_input(&single, "ref_1", 2).unwrap_err().contains("accepts one file"));
        assert!(check_input(&multi, "ref_1", 10).is_ok());
        assert!(check_input(&Input::Ok { multiple: true, disabled: true }, "ref_1", 1).unwrap_err().contains("disabled"));
        assert!(check_input(&Input::Gone, "ref_1", 1).unwrap_err().contains("no longer on the page"));
        let error = check_input(&Input::NotFile { what: "button".into() }, "ref_1", 1).unwrap_err();
        assert!(error.contains("not a file input") && error.contains("browser_find"));
    }

    #[test]
    fn the_page_answers_are_read_by_status() {
        assert_eq!(serde_json::from_value::<Input>(json!({ "status": "gone" })).unwrap(), Input::Gone);
        assert_eq!(serde_json::from_value::<Input>(json!({ "status": "notfile", "what": "div" })).unwrap(), Input::NotFile { what: "div".into() });
        assert_eq!(
            serde_json::from_value::<Input>(json!({ "status": "ok", "multiple": true, "disabled": false })).unwrap(),
            Input::Ok { multiple: true, disabled: false }
        );
    }

    #[test]
    fn what_the_input_holds_is_checked_against_what_was_sent() {
        assert!(verify(&strings(&["a", "b"]), &strings(&["a", "b"])).is_ok());
        let error = verify(&strings(&["a", "b"]), &strings(&["a"])).unwrap_err();
        assert!(error.contains("holds 1 after the upload (a), not the 2 sent (a, b)"), "{error}");
        assert!(verify(&strings(&["a"]), &[]).unwrap_err().contains("nothing"));
    }

    #[test]
    fn the_tool_is_a_change_with_a_strict_schema() {
        let tool = &tools()[0];
        assert_eq!((tool.name, tool.class), ("browser_upload", Class::Command));
        let schema = (tool.schema)();
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["required"], json!(["ref", "paths"]));
        assert_eq!(schema["properties"]["paths"]["maxItems"], 10);
        assert!(element_expression("f1:ref_9").contains("evoElement(\"f1:ref_9\")"));
    }
}
