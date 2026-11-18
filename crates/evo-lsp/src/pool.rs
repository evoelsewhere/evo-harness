//! Running servers: one per (server, project root), started on first use and
//! kept while the engine runs.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::catalog::{self, Server};
use crate::client::Client;
use crate::install;

pub struct Pool {
    pub lsp_dir: PathBuf,
    clients: tokio::sync::Mutex<HashMap<(String, PathBuf), Arc<Client>>>,
    disabled: Mutex<HashSet<String>>,
    /// When [`Pool::warm`] last tried each server, so edits to files of a
    /// language whose server is missing do not look for it every time.
    warmed: Mutex<HashMap<&'static str, Instant>>,
}

/// How long [`Pool::warm`] leaves a server alone after trying to start it.
const WARM_AGAIN: Duration = Duration::from_secs(60);

fn marker_in(dir: &Path, marker: &str) -> bool {
    match marker.strip_prefix('*') {
        Some(suffix) => std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .flatten()
                    .any(|e| e.file_name().to_string_lossy().ends_with(suffix))
            })
            .unwrap_or(false),
        None => dir.join(marker).exists(),
    }
}

/// The project root for `file`: the nearest folder with one of `markers`, not
/// above `cwd` when the file is inside it; else `cwd` (or the file's folder).
pub fn find_root(file: &Path, markers: &[&str], cwd: &Path) -> PathBuf {
    let inside = file.starts_with(cwd);
    // The chat's folder wins when it is a project itself (a Cargo or npm
    // workspace): one server for all of it, not one per member.
    if inside && markers.iter().any(|m| marker_in(cwd, m)) {
        return cwd.to_path_buf();
    }
    let mut dir = file.parent();
    while let Some(d) = dir {
        if markers.iter().any(|m| marker_in(d, m)) {
            return d.to_path_buf();
        }
        if inside && d == cwd {
            break;
        }
        dir = d.parent();
    }
    if inside {
        cwd.to_path_buf()
    } else {
        file.parent().unwrap_or(cwd).to_path_buf()
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Running {
    pub server: &'static str,
    pub root: PathBuf,
    pub indexing: bool,
    pub alive: bool,
}

impl Pool {
    pub fn new(lsp_dir: PathBuf) -> Self {
        Pool {
            lsp_dir,
            clients: tokio::sync::Mutex::new(HashMap::new()),
            disabled: Mutex::new(HashSet::new()),
            warmed: Mutex::new(HashMap::new()),
        }
    }

    pub fn set_disabled(&self, ids: impl IntoIterator<Item = String>) {
        *self.disabled.lock().unwrap() = ids.into_iter().collect();
    }

    pub fn is_disabled(&self, id: &str) -> bool {
        self.disabled.lock().unwrap().contains(id)
    }

    /// The server for `file`, started when needed.
    pub async fn client_for(&self, file: &Path, cwd: &Path) -> Result<Arc<Client>, String> {
        let server: &'static Server = catalog::for_file(file).ok_or_else(|| {
            format!(
                "no language server handles {} files",
                file.extension()
                    .map(|e| format!(".{}", e.to_string_lossy()))
                    .unwrap_or_else(|| "these".into())
            )
        })?;
        if self.is_disabled(server.id) {
            return Err(format!(
                "{} is switched off in Customize → Code intelligence",
                server.name
            ));
        }
        let located = install::locate(server, &self.lsp_dir).ok_or_else(|| {
            format!(
                "{} ({}) is not installed. Install it in Customize → Code intelligence{}",
                server.name,
                server.language,
                match &server.install {
                    catalog::Install::System { hint } => format!(" — {hint}"),
                    _ => String::new(),
                }
            )
        })?;
        let root = find_root(file, server.root_markers, cwd);
        let key = (server.id.to_string(), root.clone());
        let mut clients = self.clients.lock().await;
        if let Some(client) = clients.get(&key) {
            if client.alive() {
                return Ok(client.clone());
            }
            clients.remove(&key);
        }
        let env = install::env_for(server, &self.lsp_dir);
        let init = install::init_options_for(server, &located.path, &self.lsp_dir);
        let client = Client::start(server, &located.path, env, init, &root).await?;
        clients.insert(key, client.clone());
        Ok(client)
    }

    /// The server that handles `file`, unless it is switched off.
    pub fn server_for(&self, file: &Path) -> Option<&'static Server> {
        catalog::for_file(file).filter(|server| !self.is_disabled(server.id))
    }

    /// The running server for `file`, never starting one (and `None` while
    /// another server is being started, rather than waiting for it).
    pub fn running_for(&self, file: &Path, cwd: &Path) -> Option<Arc<Client>> {
        let server = self.server_for(file)?;
        let root = find_root(file, server.root_markers, cwd);
        let clients = self.clients.try_lock().ok()?;
        clients
            .get(&(server.id.to_string(), root))
            .filter(|c| c.alive())
            .cloned()
    }

    /// Start the server for `file` in the background and open the file in it,
    /// so later requests find it ready; with the server already running, just
    /// open the file, so the server keeps it checked (servers that check only
    /// open files report a change's effect on it, FR-23). Does nothing for a
    /// file no server handles, a server switched off, or a start tried in the
    /// last minute.
    pub fn warm(&'static self, file: &Path, cwd: &Path) {
        let Some(server) = self.server_for(file) else {
            return;
        };
        if let Some(client) = self.running_for(file, cwd) {
            let file = file.to_path_buf();
            tokio::spawn(async move {
                if client.open_file(&file).await.is_ok() {
                    // A pull server says nothing until asked: ask, so the
                    // first edit has errors to compare with.
                    let _ = client.pull(&file, Duration::from_secs(20)).await;
                }
            });
            return;
        }
        {
            let mut warmed = self.warmed.lock().unwrap();
            if warmed
                .get(server.id)
                .is_some_and(|at| at.elapsed() < WARM_AGAIN)
            {
                return;
            }
            warmed.insert(server.id, Instant::now());
        }
        let (file, cwd) = (file.to_path_buf(), cwd.to_path_buf());
        tokio::spawn(async move {
            // Opened and saved: rust-analyzer starts its first `cargo check`
            // on the save, so the first edit finds it under way.
            if let Ok(client) = self.client_for(&file, &cwd).await {
                if client.sync_file(&file).await.is_ok() {
                    let _ = client.pull(&file, Duration::from_secs(20)).await;
                }
            }
        });
    }

    /// Every running server.
    pub async fn running(&self) -> Vec<Running> {
        self.clients
            .lock()
            .await
            .values()
            .map(|c| Running {
                server: c.server.id,
                root: c.root.clone(),
                indexing: c.indexing(),
                alive: c.alive(),
            })
            .collect()
    }

    pub async fn clients(&self) -> Vec<Arc<Client>> {
        self.clients
            .lock()
            .await
            .values()
            .filter(|c| c.alive())
            .cloned()
            .collect()
    }

    /// Stop the servers of `id` (after an uninstall), or all.
    pub async fn stop(&self, id: Option<&str>) {
        let mut clients = self.clients.lock().await;
        let keys: Vec<_> = clients
            .keys()
            .filter(|(s, _)| id.is_none_or(|i| i == s))
            .cloned()
            .collect();
        for key in keys {
            if let Some(client) = clients.remove(&key) {
                client.shutdown().await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roots_are_the_nearest_marker_folder() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path();
        std::fs::create_dir_all(cwd.join("crates/a/src")).unwrap();
        std::fs::write(cwd.join("crates/a/Cargo.toml"), "").unwrap();
        let file = cwd.join("crates/a/src/lib.rs");
        assert_eq!(find_root(&file, &["Cargo.toml"], cwd), cwd.join("crates/a"));
        // A workspace at the chat's folder covers its members.
        std::fs::write(cwd.join("Cargo.toml"), "").unwrap();
        assert_eq!(find_root(&file, &["Cargo.toml"], cwd), cwd.to_path_buf());
        assert_eq!(
            find_root(&cwd.join("x.py"), &["pyproject.toml"], cwd),
            cwd.to_path_buf()
        );
        std::fs::create_dir_all(cwd.join("app")).unwrap();
        std::fs::write(cwd.join("app/App.csproj"), "").unwrap();
        assert_eq!(
            find_root(&cwd.join("app/Program.cs"), &["*.csproj"], cwd),
            cwd.join("app")
        );
    }
}
