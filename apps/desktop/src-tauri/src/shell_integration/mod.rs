//! Shell integration for terminal tabs: scripts that make a shell report
//! prompt and command boundaries as OSC 133 markers, and the arguments and
//! environment that load them.
//!
//! Spec: docs/features/agent-terminal (FR-16; plan 3.5 "Shell integration").
//! The scripts are embedded in the binary and written to
//! `<app data>/shell-integration/` on every start ([`install`]), so they always
//! match the app version. Markers: `ESC ] 133 ; A BEL` prompt start, `B` prompt
//! end, `C` command output start, `D ; <exit>` command done. Command Prompt can
//! only report A, B and D, and D carries no exit code.

use std::io;
use std::path::{Path, PathBuf};

use crate::terminal::shells::{ShellInfo, ShellKind};

const EVO_PS1: &str = include_str!("scripts/evo.ps1");
const EVO_BASH: &str = include_str!("scripts/evo.bash");
const EVO_ZSH: &str = include_str!("scripts/evo.zsh");
const EVO_FISH: &str = include_str!("scripts/evo.fish");
const EVO_NU: &str = include_str!("scripts/evo.nu");
const ZDOTDIR_ZSHENV: &str = include_str!("scripts/zdotdir/zshenv");
const ZDOTDIR_ZPROFILE: &str = include_str!("scripts/zdotdir/zprofile");
const ZDOTDIR_ZSHRC: &str = include_str!("scripts/zdotdir/zshrc");

/// Command Prompt's `PROMPT`: D (no exit code), A, the usual `$P$G`, B; each
/// marker ends with ST (`ESC \`). `$E` is ESC.
const CMD_PROMPT: &str = "$E]133;D$E\\$E]133;A$E\\$P$G$E]133;B$E\\";

/// How to start a shell with its integration script.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Integration {
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// Write the scripts to `<app_data_dir>/shell-integration/`; returns that folder.
pub fn install(app_data_dir: &Path) -> io::Result<PathBuf> {
    let dir = app_data_dir.join("shell-integration");
    write_scripts(&dir)?;
    Ok(dir)
}

fn write_scripts(dir: &Path) -> io::Result<()> {
    let zdotdir = dir.join("zdotdir");
    std::fs::create_dir_all(&zdotdir)?;
    let files: [(PathBuf, &str); 8] = [
        (dir.join("evo.ps1"), EVO_PS1),
        (dir.join("evo.bash"), EVO_BASH),
        (dir.join("evo.zsh"), EVO_ZSH),
        (dir.join("evo.fish"), EVO_FISH),
        (dir.join("evo.nu"), EVO_NU),
        (zdotdir.join(".zshenv"), ZDOTDIR_ZSHENV),
        (zdotdir.join(".zprofile"), ZDOTDIR_ZPROFILE),
        (zdotdir.join(".zshrc"), ZDOTDIR_ZSHRC),
    ];
    for (path, text) in files {
        // Unix shells choke on CRLF, which a Windows checkout may have added.
        std::fs::write(path, text.replace("\r\n", "\n"))?;
    }
    Ok(())
}

/// `C:\a\b` as `<prefix>/c/a/b` (`/mnt`, `/cygdrive`); other paths just get forward slashes.
pub fn drive_path(path: &str, prefix: &str) -> String {
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        let rest = path[2..].replace('\\', "/");
        let rest = rest.trim_start_matches('/');
        let drive = (bytes[0] as char).to_ascii_lowercase();
        return if rest.is_empty() {
            format!("{prefix}/{drive}")
        } else {
            format!("{prefix}/{drive}/{rest}")
        };
    }
    path.replace('\\', "/")
}

/// `C:\a\b` as `C:/a/b`, which Git Bash and MSYS2 accept.
pub fn forward_slashes(path: &str) -> String {
    path.replace('\\', "/")
}

fn quote_single(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

fn quote_fish(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn program_stem(shell: &ShellInfo) -> String {
    Path::new(&shell.program)
        .file_stem()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

/// The integration for `shell`, or `None` when it cannot be integrated (then
/// the shell starts with its plain arguments). `dir` is [`install`]'s folder;
/// `orig_zdotdir` is the person's `ZDOTDIR` (else their home folder).
pub fn plan(shell: &ShellInfo, dir: &Path, orig_zdotdir: &str) -> Option<Integration> {
    // Windows Terminal profiles and Visual Studio developer shells run their own setup.
    if shell.id.starts_with("wt:") || shell.id.starts_with("vs:") {
        return None;
    }
    let script = |name: &str| dir.join(name).to_string_lossy().into_owned();
    let mut env = vec![("EVO_TERMINAL".to_string(), "1".to_string())];
    let args = match shell.kind {
        ShellKind::Powershell => {
            let mut args: Vec<String> = vec!["-NoLogo".into(), "-NoExit".into()];
            if shell.id == "powershell" {
                // Dot-sourcing a file fails under a Restricted policy in 5.1; this is process scope only.
                args.extend(["-ExecutionPolicy".into(), "Bypass".into()]);
            }
            args.extend([
                "-Command".into(),
                format!(". {}", quote_single(&script("evo.ps1"))),
            ]);
            args
        }
        ShellKind::Cmd => {
            if shell.id != "cmd" {
                return None;
            }
            env.push(("PROMPT".into(), CMD_PROMPT.into()));
            shell.args.clone()
        }
        ShellKind::Fish => {
            let mut args = shell.args.clone();
            args.extend([
                "--init-command".into(),
                format!("source {}", quote_fish(&script("evo.fish"))),
            ]);
            args
        }
        ShellKind::Nu => {
            let mut args = shell.args.clone();
            args.extend([
                "--execute".into(),
                format!("source {}", quote_single(&script("evo.nu"))),
            ]);
            args
        }
        ShellKind::Wsl => {
            if !shell.id.starts_with("wsl:") {
                return None;
            }
            let mut args = shell.args.clone();
            args.extend([
                "--".into(),
                "bash".into(),
                "--init-file".into(),
                drive_path(&script("evo.bash"), "/mnt"),
                "-i".into(),
            ]);
            args
        }
        ShellKind::Posix => match program_stem(shell).as_str() {
            "bash" => {
                let path = script("evo.bash");
                let path = match shell.id.as_str() {
                    "git-bash" | "msys2" => {
                        // Git for Windows' /etc/profile changes to $HOME unless told not to.
                        env.push(("CHERE_INVOKING".into(), "1".into()));
                        forward_slashes(&path)
                    }
                    "cygwin" => {
                        env.push(("CHERE_INVOKING".into(), "1".into()));
                        drive_path(&path, "/cygdrive")
                    }
                    _ => path,
                };
                vec!["--init-file".into(), path, "-i".into()]
            }
            "zsh" => {
                env.push(("ZDOTDIR".into(), script("zdotdir")));
                env.push(("EVO_ORIG_ZDOTDIR".into(), orig_zdotdir.to_string()));
                env.push((
                    "EVO_INTEGRATION_DIR".into(),
                    dir.to_string_lossy().into_owned(),
                ));
                shell.args.clone()
            }
            _ => return None,
        },
    };
    Some(Integration { args, env })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell(id: &str, program: &str, kind: ShellKind, args: &[&str]) -> ShellInfo {
        ShellInfo::new(id, id, Path::new(program), kind, args)
    }

    fn env_of<'a>(integration: &'a Integration, key: &str) -> Option<&'a str> {
        integration
            .env
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    #[test]
    fn translates_drive_paths() {
        assert_eq!(
            drive_path(r"C:\Users\Me\evo", "/mnt"),
            "/mnt/c/Users/Me/evo"
        );
        assert_eq!(drive_path(r"D:\x", "/cygdrive"), "/cygdrive/d/x");
        assert_eq!(drive_path("C:\\", "/mnt"), "/mnt/c");
        assert_eq!(drive_path("/already/unix", "/mnt"), "/already/unix");
        assert_eq!(forward_slashes(r"C:\a\b"), "C:/a/b");
    }

    #[test]
    fn install_writes_every_script_into_a_temp_dir() {
        let root = std::env::temp_dir().join(format!("evo-integration-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = install(&root).unwrap();
        assert_eq!(dir, root.join("shell-integration"));
        for file in ["evo.ps1", "evo.bash", "evo.zsh", "evo.fish", "evo.nu"] {
            assert!(dir.join(file).is_file(), "{file}");
        }
        for file in [".zshenv", ".zprofile", ".zshrc"] {
            assert!(dir.join("zdotdir").join(file).is_file(), "{file}");
        }
        // Installing again (every start) overwrites without error and without CRs.
        install(&root).unwrap();
        assert!(!std::fs::read_to_string(dir.join("evo.bash"))
            .unwrap()
            .contains('\r'));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn scripts_emit_the_osc_133_markers() {
        // A (prompt start), B (prompt end), C (output start) and D (done, with the exit code).
        for (name, text) in [("bash", EVO_BASH), ("zsh", EVO_ZSH), ("fish", EVO_FISH)] {
            for marker in ["133;A", "133;B", "133;C", "133;D;"] {
                assert!(text.contains(marker), "{name} lacks {marker}");
            }
        }
        for (name, text) in [("ps1", EVO_PS1), ("nu", EVO_NU)] {
            for marker in ["133;A", "133;B", "133;C", "133;D;"] {
                assert!(text.contains(marker), "{name} lacks {marker}");
            }
        }
        assert!(EVO_PS1.contains("Set-PSReadLineKeyHandler -Key Enter"));
        assert!(
            CMD_PROMPT.contains("133;A")
                && CMD_PROMPT.contains("133;B")
                && CMD_PROMPT.contains("133;D")
        );
        assert!(ZDOTDIR_ZSHRC.contains("EVO_ORIG_ZDOTDIR"));
    }

    #[test]
    fn powershell_args_load_the_script_and_windows_powershell_bypasses_policy() {
        let dir = Path::new("/evo/shell-integration");
        let pwsh = plan(
            &shell("pwsh", "pwsh", ShellKind::Powershell, &["-NoLogo"]),
            dir,
            "/h",
        )
        .unwrap();
        assert_eq!(pwsh.args[..3], ["-NoLogo", "-NoExit", "-Command"]);
        assert!(pwsh.args[3].starts_with(". '") && pwsh.args[3].contains("evo.ps1"));
        assert_eq!(env_of(&pwsh, "EVO_TERMINAL"), Some("1"));
        let ps5 = plan(
            &shell("powershell", "powershell.exe", ShellKind::Powershell, &[]),
            dir,
            "/h",
        )
        .unwrap();
        assert!(ps5
            .args
            .windows(2)
            .any(|w| w == ["-ExecutionPolicy", "Bypass"]));
        assert!(!pwsh.args.contains(&"Bypass".to_string()));
    }

    #[test]
    fn bash_flavors_get_init_file_in_the_right_path_style() {
        let dir = Path::new(r"C:\Users\Me\AppData\evo\shell-integration");
        let git = plan(
            &shell(
                "git-bash",
                r"C:\Git\bin\bash.exe",
                ShellKind::Posix,
                &["--login", "-i"],
            ),
            dir,
            "",
        )
        .unwrap();
        assert_eq!(git.args[0], "--init-file");
        assert!(
            git.args[1].starts_with("C:/Users/Me/AppData/evo/shell-integration/"),
            "{:?}",
            git.args
        );
        assert!(git.args[1].ends_with("evo.bash"));
        assert_eq!(git.args[2], "-i");
        assert_eq!(env_of(&git, "CHERE_INVOKING"), Some("1"));
        let cyg = plan(
            &shell("cygwin", r"C:\cygwin64\bin\bash.exe", ShellKind::Posix, &[]),
            dir,
            "",
        )
        .unwrap();
        assert!(
            cyg.args[1].starts_with("/cygdrive/c/Users/Me/"),
            "{:?}",
            cyg.args
        );
        let plain = plan(
            &shell("bash", "/bin/bash", ShellKind::Posix, &["-l"]),
            Path::new("/d"),
            "",
        )
        .unwrap();
        assert_eq!(plain.args[0], "--init-file");
        assert!(plain.args[1].ends_with("evo.bash"));
        assert_eq!(plain.args[2], "-i");
    }

    #[test]
    fn zsh_uses_a_zdotdir_shim_and_keeps_the_original() {
        let dir = Path::new("/d");
        let zsh = plan(
            &shell("zsh", "/bin/zsh", ShellKind::Posix, &["-l"]),
            dir,
            "/home/me",
        )
        .unwrap();
        assert_eq!(zsh.args, ["-l"]);
        assert_eq!(
            env_of(&zsh, "ZDOTDIR"),
            Some(dir.join("zdotdir").to_string_lossy().as_ref())
        );
        assert_eq!(env_of(&zsh, "EVO_ORIG_ZDOTDIR"), Some("/home/me"));
        assert!(env_of(&zsh, "EVO_INTEGRATION_DIR").is_some());
    }

    #[test]
    fn fish_nu_wsl_and_cmd() {
        let dir = Path::new("/d");
        let fish = plan(
            &shell("fish", "/usr/bin/fish", ShellKind::Fish, &[]),
            dir,
            "",
        )
        .unwrap();
        assert_eq!(fish.args[0], "--init-command");
        assert!(fish.args[1].starts_with("source '") && fish.args[1].contains("evo.fish"));
        let nu = plan(&shell("nu", "/usr/bin/nu", ShellKind::Nu, &[]), dir, "").unwrap();
        assert_eq!(nu.args[0], "--execute");
        assert!(nu.args[1].starts_with("source '") && nu.args[1].contains("evo.nu"));
        let wsl_dir = Path::new(r"C:\Data\shell-integration");
        let wsl = plan(
            &shell("wsl:Ubuntu", "wsl.exe", ShellKind::Wsl, &["-d", "Ubuntu"]),
            wsl_dir,
            "",
        )
        .unwrap();
        assert_eq!(wsl.args[..4], ["-d", "Ubuntu", "--", "bash"]);
        assert_eq!(wsl.args[4], "--init-file");
        assert!(
            wsl.args[5].starts_with("/mnt/c/Data/shell-integration/"),
            "{:?}",
            wsl.args
        );
        assert_eq!(wsl.args[6], "-i");
        let cmd = plan(&shell("cmd", "cmd.exe", ShellKind::Cmd, &[]), dir, "").unwrap();
        assert!(cmd.args.is_empty());
        assert_eq!(env_of(&cmd, "PROMPT"), Some(CMD_PROMPT));
    }

    #[test]
    fn unsupported_shells_start_plain() {
        let dir = Path::new("/d");
        assert!(plan(&shell("sh", "/bin/sh", ShellKind::Posix, &[]), dir, "").is_none());
        assert!(plan(
            &shell("vs:x-cmd", "cmd.exe", ShellKind::Cmd, &["/k", "x"]),
            dir,
            ""
        )
        .is_none());
        assert!(plan(
            &shell("wt:abc", "pwsh.exe", ShellKind::Powershell, &[]),
            dir,
            ""
        )
        .is_none());
        assert!(plan(&shell("wsl", "wsl.exe", ShellKind::Wsl, &[]), dir, "").is_none());
    }
}
