//! Learned skills: the skills an agent writes itself, kept apart from the
//! skills the person installed (docs/features/learning-layers/spec.md,
//! FR-15 – FR-18; plan 3.4).
//!
//! * Evo Harness: `<agent dir>/learned-skills/<name>/SKILL.md`, per person.
//! * A mate: `<agent dir>/mates/<id>/skills/<name>/SKILL.md` (its role's
//!   `learnedSkillsDir`).
//!
//! They come last in the skill search order, so a skill of the same name the
//! person installed always wins (plan 3.7). A learned skill switched off
//! carries an [`OFF_MARKER`] file in its folder.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use evo_harness::frontmatter::parse_frontmatter;
use serde_json::Value;

/// A learned skill switched off: this file in its folder.
pub const OFF_MARKER: &str = ".off";
/// A learned skill is kept under this size.
pub const MAX_SKILL: usize = 30_000;

/// This engine's learned-skills folder: its mate's, or the person's.
pub fn learned_dir() -> Option<PathBuf> {
    if let Some(dir) = crate::session_roles::current()
        .and_then(|role| role.learned_skills_dir.as_deref())
        .filter(|dir| !dir.trim().is_empty())
    {
        return Some(PathBuf::from(dir));
    }
    // A mate without the field (an older shell): no learned skills.
    if crate::session_roles::current().is_some_and(|role| role.mate.is_some()) {
        return None;
    }
    crate::config::agent_dir()
        .ok()
        .map(|agent| agent.join("learned-skills"))
}

fn normalized(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

/// Whether `path` is inside a learned-skills folder.
pub fn is_learned(path: &Path) -> bool {
    let path = normalized(path);
    if path.contains("/learned-skills/") {
        return true;
    }
    learned_dir().is_some_and(|dir| {
        path.starts_with(&format!("{}/", normalized(&dir).trim_end_matches('/')))
    })
}

/// A learned skill whose `SKILL.md` is at `skill_md` is switched off.
pub fn switched_off(skill_md: &Path) -> bool {
    skill_md
        .parent()
        .is_some_and(|dir| dir.join(OFF_MARKER).exists())
}

/// A name that is free and fits a learned skill: kebab-case, short, not a
/// name an installed skill has.
pub fn valid_name(name: &str, taken: &HashSet<String>) -> Result<(), String> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-');
    if !ok {
        return Err(format!(
            "`{name}` is not a skill name (lowercase words and dashes)"
        ));
    }
    if taken.contains(name) {
        return Err(format!("`{name}` is the name of an installed skill"));
    }
    Ok(())
}

/// The `name` and `description` of a `SKILL.md`.
pub fn front(skill_md: &str) -> Option<(String, String)> {
    let (front, _) = parse_frontmatter(skill_md.trim_start_matches('\u{feff}')).ok()?;
    let text = |key: &str| {
        front
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string)
    };
    Some((text("name")?, text("description")?))
}

/// A learned skill as it is on disk.
#[derive(Debug, Clone, PartialEq)]
pub struct Learned {
    pub name: String,
    pub description: String,
    pub text: String,
    pub on: bool,
}

/// The learned skills in `dir`.
pub fn list(dir: &Path) -> Vec<Learned> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<Learned> = entries
        .flatten()
        .filter_map(|entry| {
            let skill_md = entry.path().join("SKILL.md");
            let text = std::fs::read_to_string(&skill_md).ok()?;
            let (name, description) = front(&text)?;
            Some(Learned {
                name,
                description,
                on: !switched_off(&skill_md),
                text,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Write a learned skill through a temporary file and a rename; the text
/// before (`None` for a new one).
pub fn write(dir: &Path, name: &str, text: &str) -> Result<Option<String>, String> {
    let folder = dir.join(name);
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let path = folder.join("SKILL.md");
    let before = std::fs::read_to_string(&path).ok();
    let partial = folder.join(format!("SKILL.md.{}.partial", std::process::id()));
    std::fs::write(&partial, text).map_err(|e| e.to_string())?;
    std::fs::rename(&partial, &path).map_err(|e| {
        let _ = std::fs::remove_file(&partial);
        e.to_string()
    })?;
    Ok(before)
}

/// The names of the skills the person installed, as this engine finds them:
/// every skill folder in the search order but the learned one.
pub fn installed_names(cwd: &Path) -> HashSet<String> {
    let mut names = HashSet::new();
    for dir in crate::resource_dirs::skill_dirs(cwd) {
        if is_learned(&dir.join("x")) || learned_dir().is_some_and(|learned| learned == dir) {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if entry.path().join("SKILL.md").exists() {
                    names.insert(entry.file_name().to_string_lossy().into_owned());
                }
            }
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learned_skill_names_and_files() {
        let taken: HashSet<String> = ["deploy".to_string()].into();
        assert!(valid_name("summary-first-reports", &taken).is_ok());
        assert!(valid_name("deploy", &taken)
            .unwrap_err()
            .contains("installed"));
        for bad in ["", "Has Caps", "-x", "x-", "a/b", &"x".repeat(70)] {
            assert!(valid_name(bad, &taken).is_err(), "{bad}");
        }
        let folder = tempfile::tempdir().unwrap();
        let md = "---\nname: summary-first\ndescription: Reports open with a summary\n---\n\n# Summary first\n";
        assert_eq!(write(folder.path(), "summary-first", md).unwrap(), None);
        assert_eq!(
            write(folder.path(), "summary-first", md)
                .unwrap()
                .as_deref(),
            Some(md)
        );
        let listed = list(folder.path());
        assert_eq!(listed.len(), 1);
        assert!(listed[0].on);
        std::fs::write(folder.path().join("summary-first").join(OFF_MARKER), "").unwrap();
        assert!(!list(folder.path())[0].on);
        assert!(is_learned(Path::new(
            "/home/me/.evo/agent/learned-skills/x/SKILL.md"
        )));
        assert!(!is_learned(Path::new(
            "/home/me/.evo/agent/skills/x/SKILL.md"
        )));
    }
}
