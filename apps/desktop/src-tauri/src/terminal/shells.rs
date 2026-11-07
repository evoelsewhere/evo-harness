//! Shell discovery for the side panel terminal.
//!
//! Spec: docs/features/agent-terminal (FR-3: stable shell ids, FR-4: shell
//! list with a default). The fast sources (`discover_shells`) are cheap file
//! checks; the slow ones (WSL distributions, Windows Terminal profiles, Visual
//! Studio developer shells) live in `discover_extra`. Every source is a pure
//! function over explicit roots, so tests run them on temp folders.
// Each platform uses only its own sources; all of them compile everywhere for tests.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use serde::Serialize;

/// How the UI and the integration scripts treat a shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ShellKind {
    Powershell,
    Cmd,
    Posix,
    Fish,
    Nu,
    Wsl,
}

/// A shell the terminal can start.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellInfo {
    pub id: String,
    pub name: String,
    pub program: String,
    pub kind: ShellKind,
    pub is_default: bool,
    #[serde(skip)]
    pub args: Vec<String>,
}

impl ShellInfo {
    pub fn new(id: &str, name: &str, program: &Path, kind: ShellKind, args: &[&str]) -> Self {
        ShellInfo {
            id: id.into(),
            name: name.into(),
            program: program.to_string_lossy().into_owned(),
            kind,
            is_default: false,
            args: args.iter().map(|arg| arg.to_string()).collect(),
        }
    }
}

/// `program` on PATH (with PATHEXT on Windows), as an absolute path.
pub fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".into())
            .split(';')
            .filter(|ext| !ext.is_empty())
            .map(|ext| ext.to_ascii_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };
    std::env::split_paths(&path).find_map(|dir| {
        let has_ext = Path::new(program).extension().is_some();
        if has_ext || !cfg!(windows) {
            let candidate = dir.join(program);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        exts.iter()
            .map(|ext| dir.join(format!("{program}{ext}")))
            .find(|candidate| candidate.is_file())
    })
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn kind_for_stem(stem: &str) -> ShellKind {
    match stem {
        "pwsh" | "powershell" => ShellKind::Powershell,
        "cmd" => ShellKind::Cmd,
        "fish" => ShellKind::Fish,
        "nu" => ShellKind::Nu,
        "wsl" => ShellKind::Wsl,
        _ => ShellKind::Posix,
    }
}

// ---------------------------------------------------------------------------
// Unix
// ---------------------------------------------------------------------------

/// Shell file names looked up in the Homebrew and PATH folders.
const KNOWN_UNIX: [&str; 6] = ["zsh", "bash", "fish", "nu", "pwsh", "sh"];

/// The absolute shell paths in `/etc/shells` text: comments and blank lines
/// are skipped, and so are `false` and `nologin` programs.
pub fn parse_etc_shells(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty()
                && !line.starts_with('#')
                && (line.starts_with('/') || Path::new(line).is_absolute())
        })
        .map(PathBuf::from)
        .filter(|path| {
            let file = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            file != "false" && file != "nologin"
        })
        .collect()
}

pub fn unix_shell_name(id: &str) -> String {
    match id {
        "zsh" => "zsh",
        "bash" => "bash",
        "fish" => "fish",
        "nu" => "Nushell",
        "pwsh" => "PowerShell",
        "sh" => "sh",
        other => other,
    }
    .to_string()
}

/// Where the Unix list comes from.
pub struct UnixSources<'a> {
    /// `$SHELL`: the default.
    pub user_shell: Option<PathBuf>,
    /// The text of `/etc/shells`.
    pub etc_shells: &'a str,
    /// Homebrew-style folders searched for the known shells.
    pub extra_dirs: Vec<PathBuf>,
    /// The PATH folders searched for the known shells.
    pub path_dirs: Vec<PathBuf>,
    /// Start login shells (macOS terminals do; the GUI's PATH is minimal).
    pub login_args: bool,
}

pub fn discover_unix_in(src: &UnixSources) -> Vec<ShellInfo> {
    let mut shells: Vec<ShellInfo> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut add = |path: PathBuf| {
        if !path.is_file() {
            return;
        }
        let real = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if seen.contains(&real) {
            return;
        }
        let id = stem(&path);
        if id.is_empty() || shells.iter().any(|s| s.id == id) {
            return;
        }
        seen.push(real);
        let args: &[&str] = if src.login_args && id != "sh" {
            &["-l"]
        } else {
            &[]
        };
        shells.push(ShellInfo::new(
            &id,
            &unix_shell_name(&id),
            &path,
            kind_for_stem(&id),
            args,
        ));
    };
    if let Some(user) = &src.user_shell {
        add(user.clone());
    }
    for path in parse_etc_shells(src.etc_shells) {
        add(path);
    }
    for dir in src.extra_dirs.iter().chain(src.path_dirs.iter()) {
        for name in KNOWN_UNIX {
            add(dir.join(name));
        }
    }
    if let Some(first) = shells.first_mut() {
        first.is_default = true;
    }
    shells
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

/// Where the Windows list comes from.
pub struct WindowsSources {
    pub system32: PathBuf,
    pub comspec: Option<PathBuf>,
    /// `pwsh.exe` on PATH.
    pub pwsh: Option<PathBuf>,
    /// `git.exe` on PATH.
    pub git_exe: Option<PathBuf>,
    /// `%ProgramFiles%`, `%ProgramW6432%`, `%LOCALAPPDATA%`.
    pub program_dirs: Vec<PathBuf>,
    /// Drive roots (`C:\`) searched for MSYS2 and Cygwin.
    pub drive_roots: Vec<PathBuf>,
}

fn join_all(base: &Path, parts: &[&str]) -> PathBuf {
    parts
        .iter()
        .fold(base.to_path_buf(), |acc, part| acc.join(part))
}

pub fn discover_windows_in(src: &WindowsSources) -> Vec<ShellInfo> {
    let mut shells = Vec::new();
    if let Some(pwsh) = src.pwsh.as_ref().filter(|p| p.is_file()) {
        shells.push(ShellInfo::new(
            "pwsh",
            "PowerShell",
            pwsh,
            ShellKind::Powershell,
            &["-NoLogo"],
        ));
    }
    let powershell = join_all(
        &src.system32,
        &["WindowsPowerShell", "v1.0", "powershell.exe"],
    );
    if powershell.is_file() {
        shells.push(ShellInfo::new(
            "powershell",
            "Windows PowerShell",
            &powershell,
            ShellKind::Powershell,
            &["-NoLogo"],
        ));
    }
    let cmd = src
        .comspec
        .clone()
        .filter(|path| path.is_file())
        .unwrap_or_else(|| src.system32.join("cmd.exe"));
    if cmd.is_file() {
        shells.push(ShellInfo::new(
            "cmd",
            "Command Prompt",
            &cmd,
            ShellKind::Cmd,
            &[],
        ));
    }
    // Git Bash: next to git.exe (…\Git\cmd\git.exe → …\Git\bin\bash.exe), or the usual install folders.
    let git_bash = src
        .git_exe
        .as_ref()
        .and_then(|git| {
            git.parent()?
                .parent()
                .map(|root| join_all(root, &["bin", "bash.exe"]))
        })
        .filter(|path| path.is_file())
        .or_else(|| {
            src.program_dirs.iter().find_map(|base| {
                [
                    vec!["Git", "bin", "bash.exe"],
                    vec!["Programs", "Git", "bin", "bash.exe"],
                ]
                .iter()
                .map(|rel| join_all(base, rel))
                .find(|path| path.is_file())
            })
        });
    if let Some(bash) = git_bash {
        shells.push(ShellInfo::new(
            "git-bash",
            "Git Bash",
            &bash,
            ShellKind::Posix,
            &["--login", "-i"],
        ));
    }
    let find_in_roots = |dirs: &[&[&str]]| {
        src.drive_roots.iter().find_map(|root| {
            dirs.iter()
                .map(|rel| join_all(root, rel))
                .find(|path| path.is_file())
        })
    };
    if let Some(bash) = find_in_roots(&[&["msys64", "usr", "bin", "bash.exe"]]) {
        shells.push(ShellInfo::new(
            "msys2",
            "MSYS2 Bash",
            &bash,
            ShellKind::Posix,
            &["--login", "-i"],
        ));
    }
    if let Some(bash) = find_in_roots(&[
        &["cygwin64", "bin", "bash.exe"],
        &["cygwin", "bin", "bash.exe"],
    ]) {
        shells.push(ShellInfo::new(
            "cygwin",
            "Cygwin Bash",
            &bash,
            ShellKind::Posix,
            &["--login", "-i"],
        ));
    }
    if let Some(first) = shells.first_mut() {
        first.is_default = true;
    }
    shells
}

/// The fast shell list for this computer; exactly one entry is the default.
pub fn discover_shells() -> Vec<ShellInfo> {
    #[cfg(windows)]
    {
        let system_root = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let program_dirs = ["ProgramFiles", "ProgramW6432", "LOCALAPPDATA"]
            .iter()
            .filter_map(|var| std::env::var_os(var).map(PathBuf::from))
            .collect();
        let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        discover_windows_in(&WindowsSources {
            system32: system_root.join("System32"),
            comspec: std::env::var_os("ComSpec").map(PathBuf::from),
            pwsh: which("pwsh.exe"),
            git_exe: which("git.exe"),
            program_dirs,
            drive_roots: vec![PathBuf::from(format!("{drive}\\"))],
        })
    }
    #[cfg(not(windows))]
    {
        let etc = std::fs::read_to_string("/etc/shells").unwrap_or_default();
        let mut path_dirs: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        path_dirs.extend(["/usr/bin", "/bin"].map(PathBuf::from));
        discover_unix_in(&UnixSources {
            user_shell: std::env::var_os("SHELL")
                .map(PathBuf::from)
                .filter(|p| !p.as_os_str().is_empty()),
            etc_shells: &etc,
            extra_dirs: ["/opt/homebrew/bin", "/usr/local/bin"]
                .map(PathBuf::from)
                .to_vec(),
            path_dirs,
            // macOS terminals start login shells (the GUI app's PATH is minimal otherwise).
            login_args: cfg!(target_os = "macos"),
        })
    }
}

// ---------------------------------------------------------------------------
// Slow sources: WSL, Windows Terminal, Visual Studio
// ---------------------------------------------------------------------------

/// Decode `wsl.exe -l -q` output (UTF-16LE, maybe with a BOM) into distribution names.
pub fn parse_wsl_list(bytes: &[u8]) -> Vec<String> {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    let text = String::from_utf16_lossy(&units);
    text.lines()
        .map(|line| line.trim_matches(|c: char| c == '\u{feff}' || c == '\0' || c.is_whitespace()))
        .filter(|name| !name.is_empty())
        // Internal distributions of Docker Desktop are not shells.
        .filter(|name| !name.starts_with("docker-desktop"))
        .map(str::to_string)
        .collect()
}

pub fn wsl_shells(distributions: &[String], wsl_exe: &Path) -> Vec<ShellInfo> {
    distributions
        .iter()
        .map(|distro| wsl_shell(distro, wsl_exe))
        .collect()
}

pub fn wsl_shell(distro: &str, wsl_exe: &Path) -> ShellInfo {
    ShellInfo::new(
        &format!("wsl:{distro}"),
        &format!("WSL: {distro}"),
        wsl_exe,
        ShellKind::Wsl,
        &["-d", distro],
    )
}

pub fn slugify(text: &str) -> String {
    let mut slug = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').to_string()
}

/// Remove `//` and `/* */` comments and trailing commas (Windows Terminal's settings are JSONC).
pub fn strip_jsonc(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    let (mut i, mut in_string) = (0, false);
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if c == '\\' && i + 1 < chars.len() {
                out.push(chars[i + 1]);
                i += 1;
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
            out.push(c);
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            continue;
        } else {
            out.push(c);
        }
        i += 1;
    }
    // Trailing commas before } or ].
    let mut cleaned = String::with_capacity(out.len());
    let (mut in_string, mut escaped) = (false, false);
    for (idx, &c) in out.iter().enumerate() {
        if in_string {
            cleaned.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        if c == '"' {
            in_string = true;
        } else if c == ',' {
            let next = out[idx + 1..].iter().find(|n| !n.is_whitespace());
            if matches!(next, Some('}') | Some(']')) {
                continue;
            }
        }
        cleaned.push(c);
    }
    cleaned
}

/// Replace `%NAME%` using `lookup`.
pub fn expand_env(text: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after
            .find('%')
            .and_then(|end| lookup(&after[..end]).map(|value| (end, value)))
        {
            Some((end, value)) => {
                out.push_str(&value);
                rest = &after[end + 1..];
            }
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Split a Windows command line into the program and its arguments (double quotes group).
pub fn split_commandline(line: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let (mut in_quotes, mut has_token) = (false, false);
    for c in line.trim().chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                has_token = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if has_token {
                    parts.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            c => {
                current.push(c);
                has_token = true;
            }
        }
    }
    if has_token {
        parts.push(current);
    }
    parts
}

/// Profiles of a Windows Terminal `settings.json` that name a command line.
/// `resolve` turns a program (path or bare name) into an existing file.
pub fn parse_wt_profiles(
    text: &str,
    lookup: &dyn Fn(&str) -> Option<String>,
    resolve: &dyn Fn(&str) -> Option<PathBuf>,
) -> Vec<ShellInfo> {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&strip_jsonc(text)) else {
        return Vec::new();
    };
    let list = json
        .pointer("/profiles/list")
        .or_else(|| json.get("profiles"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut shells = Vec::new();
    for profile in list {
        if profile
            .get("hidden")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            continue;
        }
        let Some(name) = profile.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(commandline) = profile.get("commandline").and_then(|v| v.as_str()) else {
            continue;
        };
        let mut parts = split_commandline(&expand_env(commandline, lookup));
        if parts.is_empty() {
            continue;
        }
        let program = parts.remove(0);
        let Some(path) = resolve(&program) else {
            continue;
        };
        let key = match profile.get("guid").and_then(|v| v.as_str()) {
            Some(guid) => guid
                .trim_matches(|c| c == '{' || c == '}')
                .to_ascii_lowercase(),
            None => slugify(name),
        };
        let mut shell = ShellInfo::new(
            &format!("wt:{key}"),
            &format!("{name} (Windows Terminal)"),
            &path,
            kind_for_stem(&stem(&path)),
            &[],
        );
        shell.args = parts;
        shells.push(shell);
    }
    shells
}

/// Developer shells for Visual Studio installs: `(display name, install path)`.
pub fn vs_shells(instances: &[(String, PathBuf)], powershell: &Path, cmd: &Path) -> Vec<ShellInfo> {
    let mut shells = Vec::new();
    for (name, root) in instances {
        let tools = join_all(root, &["Common7", "Tools"]);
        let bat = tools.join("VsDevCmd.bat");
        if !bat.is_file() {
            continue;
        }
        let slug = slugify(name);
        let mut prompt = ShellInfo::new(
            &format!("vs:{slug}-cmd"),
            &format!("Developer Command Prompt: {name}"),
            cmd,
            ShellKind::Cmd,
            &[],
        );
        prompt.args = vec!["/k".into(), bat.to_string_lossy().into_owned()];
        shells.push(prompt);
        let dll = tools.join("Microsoft.VisualStudio.DevShell.dll");
        if dll.is_file() {
            let script = format!(
                "& {{ Import-Module '{}'; Enter-VsDevShell -VsInstallPath '{}' -SkipAutomaticLocation }}",
                dll.to_string_lossy(),
                root.to_string_lossy()
            );
            let mut ps = ShellInfo::new(
                &format!("vs:{slug}-powershell"),
                &format!("Developer PowerShell: {name}"),
                powershell,
                ShellKind::Powershell,
                &["-NoLogo", "-NoExit", "-Command"],
            );
            ps.args.push(script);
            shells.push(ps);
        }
    }
    shells
}

/// Run `command`, returning its stdout, or `None` after `timeout` (the child is killed).
#[cfg(windows)]
fn run_with_timeout(
    mut command: std::process::Command,
    timeout: std::time::Duration,
) -> Option<Vec<u8>> {
    use std::io::Read;
    use std::os::windows::process::CommandExt;
    use std::process::Stdio;
    command
        .creation_flags(0x0800_0000)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stdout.read_to_end(&mut buffer);
        let _ = tx.send(buffer);
    });
    match rx.recv_timeout(timeout) {
        Ok(bytes) => {
            let _ = child.wait();
            Some(bytes)
        }
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            None
        }
    }
}

/// WSL distributions, Windows Terminal profiles and Visual Studio developer
/// shells. Slow (spawns `wsl.exe`, `vswhere.exe`); empty off Windows.
pub fn discover_extra() -> Vec<ShellInfo> {
    #[cfg(windows)]
    {
        windows_extra()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

#[cfg(windows)]
fn system32() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("System32")
}

/// `wsl.exe` if present.
#[cfg(windows)]
pub fn wsl_exe() -> Option<PathBuf> {
    Some(system32().join("wsl.exe")).filter(|p| p.is_file())
}

#[cfg(windows)]
fn windows_extra() -> Vec<ShellInfo> {
    use std::time::Duration;
    let mut shells = Vec::new();
    if let Some(wsl) = wsl_exe() {
        let mut command = std::process::Command::new(&wsl);
        command.args(["-l", "-q"]);
        if let Some(bytes) = run_with_timeout(command, Duration::from_secs(3)) {
            shells.extend(wsl_shells(&parse_wsl_list(&bytes), &wsl));
        }
    }
    // Windows Terminal profiles.
    let lookup = |name: &str| std::env::var(name).ok();
    let resolve = |program: &str| {
        let direct = PathBuf::from(program);
        if direct.is_absolute() {
            direct.is_file().then_some(direct)
        } else {
            which(program)
        }
    };
    if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        let mut files = vec![join_all(
            &local,
            &["Microsoft", "Windows Terminal", "settings.json"],
        )];
        if let Ok(entries) = std::fs::read_dir(local.join("Packages")) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with("Microsoft.WindowsTerminal") && name.ends_with("_8wekyb3d8bbwe")
                {
                    files.push(join_all(&entry.path(), &["LocalState", "settings.json"]));
                }
            }
        }
        for file in files {
            if let Ok(text) = std::fs::read_to_string(&file) {
                for shell in parse_wt_profiles(&text, &lookup, &resolve) {
                    if !shells.iter().any(|s: &ShellInfo| s.id == shell.id) {
                        shells.push(shell);
                    }
                }
            }
        }
    }
    // Visual Studio developer shells.
    let powershell = join_all(
        &system32(),
        &["WindowsPowerShell", "v1.0", "powershell.exe"],
    );
    let cmd = std::env::var_os("ComSpec")
        .map(PathBuf::from)
        .unwrap_or_else(|| system32().join("cmd.exe"));
    let mut instances = vs_instances();
    instances.sort();
    instances.dedup();
    shells.extend(vs_shells(&instances, &powershell, &cmd));
    shells
}

#[cfg(windows)]
fn vs_instances() -> Vec<(String, PathBuf)> {
    let x86 = std::env::var_os("ProgramFiles(x86)").map(PathBuf::from);
    let mut found = Vec::new();
    if let Some(vswhere) = x86
        .as_ref()
        .map(|dir| {
            join_all(
                dir,
                &["Microsoft Visual Studio", "Installer", "vswhere.exe"],
            )
        })
        .filter(|p| p.is_file())
    {
        let mut command = std::process::Command::new(vswhere);
        command.args(["-all", "-products", "*", "-format", "json"]);
        if let Some(bytes) = run_with_timeout(command, std::time::Duration::from_secs(3)) {
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                for item in json.as_array().cloned().unwrap_or_default() {
                    let path = item.get("installationPath").and_then(|v| v.as_str());
                    let name = item.get("displayName").and_then(|v| v.as_str());
                    if let (Some(path), Some(name)) = (path, name) {
                        found.push((name.to_string(), PathBuf::from(path)));
                    }
                }
            }
        }
    }
    if found.is_empty() {
        // The usual install folders: …\Microsoft Visual Studio\<year>\<edition>.
        for var in ["ProgramFiles", "ProgramFiles(x86)"] {
            let Some(base) =
                std::env::var_os(var).map(|p| PathBuf::from(p).join("Microsoft Visual Studio"))
            else {
                continue;
            };
            for year in std::fs::read_dir(&base).into_iter().flatten().flatten() {
                for edition in std::fs::read_dir(year.path())
                    .into_iter()
                    .flatten()
                    .flatten()
                {
                    let name = format!(
                        "Visual Studio {} {}",
                        year.file_name().to_string_lossy(),
                        edition.file_name().to_string_lossy()
                    );
                    found.push((name, edition.path()));
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"").unwrap();
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("evo-shells-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn win_sources(root: &Path) -> WindowsSources {
        WindowsSources {
            system32: root.join("Windows").join("System32"),
            comspec: None,
            pwsh: None,
            git_exe: None,
            program_dirs: vec![root.join("Program Files")],
            drive_roots: vec![root.to_path_buf()],
        }
    }

    #[test]
    fn windows_finds_git_msys2_and_cygwin_in_temp_roots() {
        let root = temp("win");
        let sys = root.join("Windows").join("System32");
        touch(&sys.join("cmd.exe"));
        touch(
            &sys.join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe"),
        );
        touch(
            &root
                .join("Program Files")
                .join("Git")
                .join("bin")
                .join("bash.exe"),
        );
        touch(&root.join("msys64").join("usr").join("bin").join("bash.exe"));
        touch(&root.join("cygwin64").join("bin").join("bash.exe"));
        let shells = discover_windows_in(&win_sources(&root));
        let ids: Vec<&str> = shells.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["powershell", "cmd", "git-bash", "msys2", "cygwin"]);
        assert_eq!(shells.iter().filter(|s| s.is_default).count(), 1);
        assert!(shells[0].is_default);
        let names: Vec<&str> = shells.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names[2..], ["Git Bash", "MSYS2 Bash", "Cygwin Bash"]);
        assert!(shells.iter().all(|s| s.kind != ShellKind::Wsl));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn windows_git_bash_is_found_next_to_git_exe() {
        let root = temp("gitexe");
        touch(&root.join("Git").join("bin").join("bash.exe"));
        touch(&root.join("Git").join("cmd").join("git.exe"));
        touch(&root.join("Windows").join("System32").join("cmd.exe"));
        let mut src = win_sources(&root);
        src.program_dirs.clear();
        src.git_exe = Some(root.join("Git").join("cmd").join("git.exe"));
        let shells = discover_windows_in(&src);
        assert!(shells.iter().any(|s| s.id == "git-bash"));
        assert_eq!(shells[0].id, "cmd");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn etc_shells_skips_comments_blanks_and_nologin() {
        let text = "# valid login shells\n\n/bin/sh\n  /bin/zsh  \n/usr/bin/false\n/usr/sbin/nologin\nrelative/bash\n/bin/bash\n";
        let paths = parse_etc_shells(text);
        assert_eq!(
            paths,
            [
                PathBuf::from("/bin/sh"),
                PathBuf::from("/bin/zsh"),
                PathBuf::from("/bin/bash")
            ]
        );
    }

    #[test]
    fn unix_list_has_one_default_ids_from_file_names_and_homebrew() {
        let root = temp("unix");
        let bin = root.join("bin");
        let brew = root.join("opt").join("homebrew").join("bin");
        let local = root.join("usr").join("local").join("bin");
        for (dir, name) in [
            (&bin, "zsh"),
            (&bin, "bash"),
            (&brew, "fish"),
            (&local, "nu"),
            (&bin, "sh"),
        ] {
            touch(&dir.join(name));
        }
        // Distinct file contents are not needed: the files are distinct paths.
        let etc = format!(
            "# shells\n{}\n{}\n{}\n",
            bin.join("bash").display(),
            bin.join("false").display(),
            bin.join("zsh").display()
        );
        let shells = discover_unix_in(&UnixSources {
            user_shell: Some(bin.join("zsh")),
            etc_shells: &etc,
            extra_dirs: vec![brew, local],
            path_dirs: vec![bin.clone(), bin.clone()],
            login_args: false,
        });
        let ids: Vec<&str> = shells.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["zsh", "bash", "fish", "nu", "sh"]);
        assert_eq!(shells.iter().filter(|s| s.is_default).count(), 1);
        assert!(shells[0].is_default);
        assert_eq!(shells[3].name, "Nushell");
        assert_eq!(shells[2].kind, ShellKind::Fish);
        assert_eq!(shells[3].kind, ShellKind::Nu);
        assert_eq!(shells[0].kind, ShellKind::Posix);
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn unix_list_dedupes_by_canonical_path() {
        let root = temp("dedupe");
        touch(&root.join("a").join("bash"));
        fs::create_dir_all(root.join("b")).unwrap();
        std::os::unix::fs::symlink(root.join("a").join("bash"), root.join("b").join("sh")).unwrap();
        let shells = discover_unix_in(&UnixSources {
            user_shell: Some(root.join("a").join("bash")),
            etc_shells: "",
            extra_dirs: vec![],
            path_dirs: vec![root.join("b")],
            login_args: false,
        });
        assert_eq!(shells.len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn serializes_camel_case_without_args() {
        let mut shell = ShellInfo::new(
            "pwsh",
            "PowerShell",
            Path::new("pwsh"),
            ShellKind::Powershell,
            &["-NoLogo"],
        );
        shell.is_default = true;
        let json = serde_json::to_value(&shell).unwrap();
        assert_eq!(json["isDefault"], true);
        assert_eq!(json["kind"], "powershell");
        assert!(json.get("args").is_none());
        assert_eq!(serde_json::to_value(ShellKind::Posix).unwrap(), "posix");
    }

    #[test]
    fn wsl_list_decodes_utf16_and_skips_docker() {
        let text = "\u{feff}Ubuntu\r\ndocker-desktop\r\nDebian\r\n\r\n";
        let bytes: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        let names = parse_wsl_list(&bytes);
        assert_eq!(names, ["Ubuntu", "Debian"]);
        let shells = wsl_shells(&names, Path::new("wsl.exe"));
        assert_eq!(shells[0].id, "wsl:Ubuntu");
        assert_eq!(shells[0].name, "WSL: Ubuntu");
        assert_eq!(shells[0].kind, ShellKind::Wsl);
        assert_eq!(shells[0].args, ["-d", "Ubuntu"]);
    }

    #[test]
    fn windows_terminal_profiles_parse_jsonc() {
        let text = r#"{
            // comment
            "profiles": { "list": [
                { "guid": "{ABC-1}", "name": "Dev", "commandline": "\"%PF%\\My Shell\\pwsh.exe\" -NoLogo", },
                { "guid": "{ABC-2}", "name": "Hidden", "hidden": true, "commandline": "cmd.exe" },
                { "guid": "{ABC-3}", "name": "Generated", "source": "Windows.Terminal.Wsl" },
                { "name": "Plain Cmd", "commandline": "cmd.exe /k echo hi" }
            ] }
        }"#;
        let lookup = |name: &str| (name == "PF").then(|| "C:\\Prog".to_string());
        let resolve = |program: &str| Some(PathBuf::from(program));
        let shells = parse_wt_profiles(text, &lookup, &resolve);
        assert_eq!(shells.len(), 2);
        assert_eq!(shells[0].id, "wt:abc-1");
        assert_eq!(shells[0].kind, ShellKind::Powershell);
        assert_eq!(shells[0].args, ["-NoLogo"]);
        assert_eq!(shells[1].id, "wt:plain-cmd");
        assert_eq!(shells[1].kind, ShellKind::Cmd);
        assert_eq!(shells[1].args, ["/k", "echo", "hi"]);
    }

    #[test]
    fn vs_shells_need_vsdevcmd() {
        let root = temp("vs");
        let tools = root
            .join("2022")
            .join("Community")
            .join("Common7")
            .join("Tools");
        touch(&tools.join("VsDevCmd.bat"));
        touch(&tools.join("Microsoft.VisualStudio.DevShell.dll"));
        let instances = vec![
            (
                "Visual Studio Community 2022".to_string(),
                root.join("2022").join("Community"),
            ),
            ("Missing".to_string(), root.join("nope")),
        ];
        let shells = vs_shells(
            &instances,
            Path::new("powershell.exe"),
            Path::new("cmd.exe"),
        );
        let ids: Vec<&str> = shells.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "vs:visual-studio-community-2022-cmd",
                "vs:visual-studio-community-2022-powershell"
            ]
        );
        assert_eq!(shells[0].kind, ShellKind::Cmd);
        assert_eq!(shells[1].kind, ShellKind::Powershell);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn slug_and_commandline_helpers() {
        assert_eq!(slugify("Dev  Shell (x64)!"), "dev-shell-x64");
        assert_eq!(
            split_commandline(r#""C:\a b\x.exe" -c "d e" f"#),
            [r"C:\a b\x.exe", "-c", "d e", "f"]
        );
    }
}
