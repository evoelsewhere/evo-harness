//! Downloads started by pages (agent browser, FR-16). The shell decides where
//! a download goes — the folder set with `browser_download_config`, or
//! nowhere (cancelled) — and this module holds the pure parts: safe, unique
//! file names and the list of what was started.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Records kept; the oldest go first.
const MAX_RECORDS: usize = 200;

/// started | completed | failed | blocked
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Started,
    Completed,
    Failed,
    Blocked,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Download {
    pub id: u64,
    pub tab: String,
    pub name: String,
    pub url: String,
    /// Where the file is (or will be); empty for a blocked download.
    pub path: String,
    pub bytes: u64,
    pub state: State,
}

/// A file name that is safe to create: no folders, no characters Windows
/// refuses, no reserved device names, not empty, not absurdly long.
pub fn sanitize_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let mut clean: String = base
        .chars()
        .map(|c| if c.is_control() || "<>:\"|?*".contains(c) { '_' } else { c })
        .collect();
    clean = clean.trim_matches(|c: char| c == '.' || c.is_whitespace()).to_string();
    if clean.is_empty() {
        return "download".to_string();
    }
    if clean.chars().count() > 120 {
        // Keep the extension: it says what the file is.
        let (stem, ext) = split_extension(&clean);
        let ext: String = ext.chars().take(16).collect();
        let keep = 120usize.saturating_sub(ext.chars().count() + 1);
        clean = format!("{}.{}", stem.chars().take(keep).collect::<String>(), ext);
    }
    let (stem, _) = split_extension(&clean);
    const RESERVED: [&str; 22] = [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5",
        "lpt6", "lpt7", "lpt8", "lpt9",
    ];
    if RESERVED.contains(&stem.to_ascii_lowercase().as_str()) {
        clean.insert(0, '_');
    }
    clean
}

/// "report.tar.gz" → ("report.tar", "gz"); "notes" → ("notes", "").
fn split_extension(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot) if dot > 0 && dot + 1 < name.len() => (&name[..dot], &name[dot + 1..]),
        _ => (name, ""),
    }
}

/// `name`, or `stem (1).ext`, `stem (2).ext`, … — the first one `taken` does
/// not claim. Never an existing name, so nothing is overwritten.
pub fn unique_name(name: &str, taken: &dyn Fn(&str) -> bool) -> String {
    if !taken(name) {
        return name.to_string();
    }
    let (stem, ext) = split_extension(name);
    for n in 1..10_000u32 {
        let candidate = if ext.is_empty() { format!("{stem} ({n})") } else { format!("{stem} ({n}).{ext}") };
        if !taken(&candidate) {
            return candidate;
        }
    }
    // Absurd, but never overwrite: fall back to a name nothing else has.
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    if ext.is_empty() { format!("{stem} ({stamp})") } else { format!("{stem} ({stamp}).{ext}") }
}

/// What the shell has seen of downloads, newest last.
#[derive(Default)]
pub struct DownloadLog {
    records: Vec<Download>,
    next_id: u64,
}

impl DownloadLog {
    /// A download wants to start into `folder` (`None`: no folder is set).
    /// `exists` says whether a path is already taken on disk. Returns the
    /// record: `started` with its unique path, or `blocked` with none.
    pub fn begin(&mut self, tab: &str, url: &str, suggested: &str, folder: Option<&Path>, exists: &dyn Fn(&Path) -> bool) -> Download {
        self.next_id += 1;
        let name = sanitize_name(suggested);
        let record = match folder {
            None => Download {
                id: self.next_id,
                tab: tab.to_string(),
                name,
                url: url.to_string(),
                path: String::new(),
                bytes: 0,
                state: State::Blocked,
            },
            Some(folder) => {
                // Two downloads may start before either file exists: names
                // already handed out count as taken too.
                let reserved: HashSet<String> =
                    self.records.iter().filter(|r| !r.path.is_empty()).map(|r| r.path.to_lowercase()).collect();
                let taken = |candidate: &str| {
                    let path = folder.join(candidate);
                    exists(&path) || reserved.contains(&path.to_string_lossy().to_lowercase())
                };
                let name = unique_name(&name, &taken);
                Download {
                    id: self.next_id,
                    tab: tab.to_string(),
                    path: folder.join(&name).to_string_lossy().into_owned(),
                    name,
                    url: url.to_string(),
                    bytes: 0,
                    state: State::Started,
                }
            }
        };
        self.records.push(record.clone());
        if self.records.len() > MAX_RECORDS {
            self.records.remove(0);
        }
        record
    }

    /// Change a record; returns the new version when something changed.
    pub fn update(&mut self, id: u64, bytes: Option<u64>, state: Option<State>) -> Option<Download> {
        let record = self.records.iter_mut().find(|r| r.id == id)?;
        let mut changed = false;
        if let Some(bytes) = bytes.filter(|b| *b != record.bytes) {
            record.bytes = bytes;
            changed = true;
        }
        if let Some(state) = state.filter(|s| *s != record.state) {
            record.state = state;
            changed = true;
        }
        changed.then(|| record.clone())
    }

    #[cfg(test)]
    pub fn get(&self, id: u64) -> Option<Download> {
        self.records.iter().find(|r| r.id == id).cloned()
    }

    /// Records of `tab`, or of every tab.
    pub fn list(&self, tab: Option<&str>) -> Vec<Download> {
        self.records.iter().filter(|r| tab.is_none_or(|t| r.tab == t)).cloned().collect()
    }
}

/// The folder for downloads: as given, made if missing.
pub fn prepare_folder(folder: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(folder);
    if !path.is_absolute() {
        return Err(format!("the download folder must be an absolute path: {folder}"));
    }
    std::fs::create_dir_all(&path).map_err(|e| format!("could not create the download folder {folder}: {e}"))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    fn never(_: &Path) -> bool {
        false
    }

    #[test]
    fn a_free_name_is_kept() {
        assert_eq!(unique_name("report.pdf", &|_| false), "report.pdf");
    }

    #[test]
    fn a_taken_name_gets_a_counter_before_the_extension() {
        let taken = ["report.pdf", "report (1).pdf"];
        let is_taken = |n: &str| taken.contains(&n);
        assert_eq!(unique_name("report.pdf", &is_taken), "report (2).pdf");
        assert_eq!(unique_name("archive.tar.gz", &|n| n == "archive.tar.gz"), "archive.tar (1).gz");
        assert_eq!(unique_name("notes", &|n| n == "notes"), "notes (1)");
        assert_eq!(unique_name(".env", &|n| n == ".env"), ".env (1)");
    }

    #[test]
    fn names_are_made_safe() {
        assert_eq!(sanitize_name("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_name(r"C:\Windows\evil.exe"), "evil.exe");
        assert_eq!(sanitize_name("a<b>:c|d?.txt"), "a_b__c_d_.txt");
        assert_eq!(sanitize_name("   "), "download");
        assert_eq!(sanitize_name(""), "download");
        assert_eq!(sanitize_name("trailing. "), "trailing");
        assert_eq!(sanitize_name("con.txt"), "_con.txt");
        assert_eq!(sanitize_name("NUL"), "_NUL");
        assert_eq!(sanitize_name("console.log"), "console.log");
        let long = format!("{}.pdf", "x".repeat(300));
        let name = sanitize_name(&long);
        assert!(name.chars().count() <= 120);
        assert!(name.ends_with(".pdf"));
    }

    #[test]
    fn no_folder_blocks_the_download() {
        let mut log = DownloadLog::default();
        let record = log.begin("browser-1", "https://x.test/a.zip", "a.zip", None, &never);
        assert_eq!(record.state, State::Blocked);
        assert_eq!(record.path, "");
        assert_eq!(record.name, "a.zip");
        assert_eq!(record.id, 1);
        assert_eq!(log.list(None), vec![record]);
    }

    #[test]
    fn a_folder_gives_a_unique_path_on_disk_and_among_downloads_in_flight() {
        let folder = Path::new("downloads");
        let on_disk = RefCell::new(vec![folder.join("a.zip")]);
        let exists = |p: &Path| on_disk.borrow().iter().any(|e| e == p);
        let mut log = DownloadLog::default();

        let first = log.begin("t", "u1", "a.zip", Some(folder), &exists);
        assert_eq!(first.name, "a (1).zip");
        assert_eq!(first.state, State::Started);
        // Not on disk yet, but handed out: the next one must not reuse it.
        let second = log.begin("t", "u2", "a.zip", Some(folder), &exists);
        assert_eq!(second.name, "a (2).zip");
        // Windows paths are case-insensitive: "A (1).ZIP" is the first one's name.
        let third = log.begin("t", "u3", "A (1).ZIP", Some(folder), &exists);
        assert_eq!(third.name, "A (1) (1).ZIP");
        assert!(first.path.ends_with("a (1).zip"));
    }

    #[test]
    fn updates_report_only_real_changes() {
        let mut log = DownloadLog::default();
        let record = log.begin("t", "u", "f.bin", Some(Path::new("d")), &never);
        assert!(log.update(record.id, Some(0), None).is_none());
        let changed = log.update(record.id, Some(10), None).unwrap();
        assert_eq!(changed.bytes, 10);
        let done = log.update(record.id, Some(10), Some(State::Completed)).unwrap();
        assert_eq!(done.state, State::Completed);
        assert!(log.update(record.id, Some(10), Some(State::Completed)).is_none());
        assert!(log.update(999, Some(1), None).is_none());
        assert_eq!(log.get(record.id).unwrap().state, State::Completed);
    }

    #[test]
    fn listing_filters_by_tab() {
        let mut log = DownloadLog::default();
        log.begin("a", "u", "1.bin", Some(Path::new("d")), &never);
        log.begin("b", "u", "2.bin", Some(Path::new("d")), &never);
        log.begin("a", "u", "3.bin", Some(Path::new("d")), &never);
        assert_eq!(log.list(Some("a")).len(), 2);
        assert_eq!(log.list(Some("b")).len(), 1);
        assert_eq!(log.list(None).len(), 3);
        assert!(log.list(Some("c")).is_empty());
    }

    #[test]
    fn the_log_is_bounded() {
        let mut log = DownloadLog::default();
        for i in 0..(MAX_RECORDS + 20) {
            log.begin("t", "u", &format!("{i}.bin"), None, &never);
        }
        let all = log.list(None);
        assert_eq!(all.len(), MAX_RECORDS);
        assert_eq!(all[0].id, 21);
    }

    #[test]
    fn records_serialize_with_lowercase_states() {
        let mut log = DownloadLog::default();
        let record = log.begin("t", "u", "f", None, &never);
        let json = serde_json::to_value(record).unwrap();
        assert_eq!(json["state"], "blocked");
        assert_eq!(json["tab"], "t");
        assert_eq!(json["bytes"], 0);
    }

    #[test]
    fn a_relative_download_folder_is_refused() {
        assert!(prepare_folder("downloads").is_err());
    }
}
