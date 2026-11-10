//! App updates (docs/features/app-updates/spec.md; design in plan.md, ADR 0017).
//!
//! The shell owns the whole update: it checks the latest release's
//! `latest.json` shortly after start and every 6 hours (FR-1), downloads a
//! newer version in the background and keeps it on disk once its signature
//! matches the key built into the app (FR-5, FR-6), and installs it only when
//! the person chooses **Restart to update** (FR-8), asking first when chats
//! are working (FR-9). Linux packages, and macOS builds a release has no entry
//! for, are only told about the new version (FR-12). The webview mirrors the
//! state through `updates_state` and the `updates://state` event; only the
//! main window may call these commands, and the agent has no way to.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use evo_store::logs::NewLog;
use semver::Version;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::db::Db;

/// App setting: check automatically (FR-2). Missing means on.
pub const AUTO_CHECK_KEY: &str = "updates.autoCheck";
const LAST_CHECKED_KEY: &str = "updates.lastCheckedAt";
const STATE_EVENT: &str = "updates://state";

/// The first check waits until the window is up (NFR-5).
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(30);
/// How often the timer looks at the clock; the wall clock decides, so a
/// computer that slept checks soon after it wakes.
const WAKE_EVERY: Duration = Duration::from_secs(10 * 60);
/// Time between automatic checks (FR-1).
const CHECK_EVERY_MS: u64 = 6 * 60 * 60 * 1000;
/// Download progress events, at most this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(250);

/// Development builds only: test against a fork's release (plan 3.7).
const ENDPOINT_ENV: &str = "EVO_UPDATES_ENDPOINT";
const PUBKEY_ENV: &str = "EVO_UPDATES_PUBKEY";

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Check,
    Download,
    Verify,
    Install,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Status {
    /// A development build, or no release key in this build (FR-14).
    Unavailable,
    Idle,
    UpToDate,
    Checking {
        manual: bool,
    },
    Downloading {
        version: String,
        received: u64,
        total: Option<u64>,
    },
    Ready {
        version: String,
    },
    /// Newer, but this app cannot install it itself (FR-12).
    Notice {
        version: String,
    },
    Installing {
        version: String,
    },
    Failed {
        stage: Stage,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        version: Option<String>,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateState {
    current: String,
    last_checked_at: Option<u64>,
    status: Status,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InstallReply {
    /// The app is about to close (Windows) or restart (macOS).
    Started,
    /// Chats are working and `force` was false (FR-9).
    Confirm {
        working: usize,
    },
    NotReady,
}

/// A verified update on disk, waiting for the person.
struct Ready {
    update: Update,
    path: PathBuf,
}

struct Inner {
    status: Status,
    last_checked_at: Option<u64>,
    ready: Option<Ready>,
}

pub struct UpdatesState {
    inner: Mutex<Inner>,
    /// One check, download or install at a time.
    busy: tokio::sync::Mutex<()>,
    /// The release key's public half (base64 of the minisign `.pub` file).
    pubkey: Mutex<String>,
}

impl Default for UpdatesState {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner {
                status: Status::Idle,
                last_checked_at: None,
                ready: None,
            }),
            busy: tokio::sync::Mutex::new(()),
            pubkey: Mutex::new(String::new()),
        }
    }
}

impl UpdatesState {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn pubkey(&self) -> String {
        self.pubkey
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

// ── Pure decisions (tested below) ───────────────────────────────────────────

/// What `check()` came back with, without the plugin's types.
#[derive(Debug)]
enum Found {
    /// A newer release with an entry for this platform.
    Update(String),
    /// Not newer.
    Nothing,
    /// The release has no entry for this platform (`TargetsNotFound`).
    NoEntry,
    Error(String),
}

#[derive(Debug, PartialEq)]
enum Decision {
    UpToDate,
    Notice(String),
    Download(String),
    Error(String),
}

/// `seen` is what the version comparator recorded: the remote version and
/// whether it is newer than this one. The plugin calls the comparator before
/// it looks up this platform's entry, so a release without an entry for us
/// still tells us its version (plan 3.5; re-checked on every plugin upgrade).
fn decide(found: Found, seen: Option<(String, bool)>, notice_only: bool) -> Decision {
    match found {
        Found::Update(version) if notice_only => Decision::Notice(version),
        Found::Update(version) => Decision::Download(version),
        Found::Nothing => Decision::UpToDate,
        Found::NoEntry => match seen {
            Some((version, true)) => Decision::Notice(version),
            Some((_, false)) => Decision::UpToDate,
            None => Decision::Error("the release has no update for this platform".into()),
        },
        Found::Error(message) => Decision::Error(message),
    }
}

/// An automatic check is due after `CHECK_EVERY_MS` (or when the clock went back).
fn due(now: u64, last_checked_at: Option<u64>) -> bool {
    match last_checked_at {
        None => true,
        Some(last) => now < last || now - last >= CHECK_EVERY_MS,
    }
}

/// Cache folders to delete at start: every version not newer than the
/// running one, and anything that is not a version.
fn stale_folders<'a>(names: impl IntoIterator<Item = &'a str>, current: &Version) -> Vec<&'a str> {
    names
        .into_iter()
        .filter(|name| Version::parse(name).map_or(true, |version| &version <= current))
        .collect()
}

/// The file name to keep a download under: the URL's last segment, made safe.
fn file_name(url: &str) -> String {
    let last = url.rsplit('/').next().unwrap_or_default();
    let last = last.split(['?', '#']).next().unwrap_or_default();
    let safe: String = last
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if safe.trim_matches(['.', '_']).is_empty() {
        "update.bin".into()
    } else {
        safe
    }
}

/// Check `data` against a release signature, both as `latest.json` carries
/// them: base64 of the minisign `.sig` file and of the `.pub` file (FR-6).
fn verify(data: &[u8], signature: &str, pubkey: &str) -> Result<(), String> {
    let text = |encoded: &str, what: &str| -> Result<String, String> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded.trim())
            .map_err(|error| format!("the {what} is not valid base64: {error}"))?;
        String::from_utf8(bytes).map_err(|_| format!("the {what} is not text"))
    };
    let key = minisign_verify::PublicKey::decode(&text(pubkey, "release key")?)
        .map_err(|error| format!("the release key is not valid: {error}"))?;
    let signature = minisign_verify::Signature::decode(&text(signature, "signature")?)
        .map_err(|error| format!("the signature is not valid: {error}"))?;
    key.verify(data, &signature, true)
        .map_err(|error| format!("the signature does not match: {error}"))
}

// ── Shell side ──────────────────────────────────────────────────────────────

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

fn current_version(app: &AppHandle) -> Version {
    app.package_info().version.clone()
}

fn snapshot(app: &AppHandle) -> UpdateState {
    let state = app.state::<UpdatesState>();
    let inner = state.lock();
    UpdateState {
        current: current_version(app).to_string(),
        last_checked_at: inner.last_checked_at,
        status: inner.status.clone(),
    }
}

fn set_status(app: &AppHandle, status: Status) {
    app.state::<UpdatesState>().lock().status = status;
    let _ = app.emit(STATE_EVENT, snapshot(app));
}

fn log(app: &AppHandle, level: &str, message: impl Into<String>) {
    app.state::<Db>()
        .log(NewLog::new(level, "updates", message.into()));
}

fn setting(app: &AppHandle, key: &str) -> Option<Value> {
    app.state::<Db>()
        .with(|conn| evo_store::settings::get(conn, key))
        .ok()
        .flatten()
}

fn auto_check(app: &AppHandle) -> bool {
    setting(app, AUTO_CHECK_KEY)
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

fn cache_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_cache_dir()
        .ok()
        .map(|dir| dir.join("updates"))
}

/// Development builds update only when pointed at a test release.
fn dev_endpoint() -> Option<String> {
    if cfg!(debug_assertions) {
        std::env::var(ENDPOINT_ENV)
            .ok()
            .filter(|v| !v.trim().is_empty())
    } else {
        None
    }
}

fn available(app: &AppHandle) -> bool {
    let state = app.state::<UpdatesState>();
    (!cfg!(debug_assertions) || dev_endpoint().is_some()) && !state.pubkey().trim().is_empty()
}

/// Where this app cannot replace itself: a system package (FR-12).
fn notice_only() -> bool {
    cfg!(target_os = "linux")
}

/// Register the state; clean the download cache; start the timer.
pub fn setup(app: &AppHandle) {
    app.manage(UpdatesState::default());
    let state = app.state::<UpdatesState>();
    let configured = app
        .config()
        .plugins
        .0
        .get("updater")
        .and_then(|updater| updater.get("pubkey"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let pubkey = if cfg!(debug_assertions) {
        std::env::var(PUBKEY_ENV).unwrap_or(configured)
    } else {
        configured
    };
    *state.pubkey.lock().unwrap_or_else(PoisonError::into_inner) = pubkey;
    state.lock().last_checked_at = setting(app, LAST_CHECKED_KEY).and_then(|v| v.as_u64());

    if !available(app) {
        state.lock().status = Status::Unavailable;
        return;
    }
    clean_cache(app);

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK_DELAY).await;
        let mut first = true;
        loop {
            let last = app.state::<UpdatesState>().lock().last_checked_at;
            if auto_check(&app) && (first || due(now_ms(), last)) {
                check(&app, false).await;
            }
            first = false;
            tokio::time::sleep(WAKE_EVERY).await;
        }
    });
}

fn clean_cache(app: &AppHandle) {
    let Some(dir) = cache_dir(app) else { return };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    let current = current_version(app);
    for name in stale_folders(names.iter().map(String::as_str), &current) {
        let _ = std::fs::remove_dir_all(dir.join(name));
    }
}

/// The setting was turned on: check now rather than at the next tick (FR-2).
pub fn settings_changed(app: &AppHandle, key: &str, value: &Value) {
    if key == AUTO_CHECK_KEY && value.as_bool() != Some(false) && available(app) {
        let app = app.clone();
        tauri::async_runtime::spawn(async move { check(&app, false).await });
    }
}

/// One check, then the download if one is due. `manual` checks report
/// their errors; automatic ones only log them (FR-13).
async fn check(app: &AppHandle, manual: bool) {
    let state = app.state::<UpdatesState>();
    let Ok(_busy) = state.busy.try_lock() else {
        return;
    };
    let before = state.lock().status.clone();
    set_status(app, Status::Checking { manual });

    let seen: Arc<Mutex<Option<(String, bool)>>> = Arc::default();
    let found = match updater(app, seen.clone()) {
        Err(error) => Found::Error(error),
        Ok(updater) => match updater.check().await {
            Ok(Some(update)) => {
                let version = update.version.clone();
                // Keep the plugin's object for the download and the install.
                return after_check(
                    app,
                    Found::Update(version),
                    seen,
                    before,
                    manual,
                    Some(update),
                )
                .await;
            }
            Ok(None) => Found::Nothing,
            Err(tauri_plugin_updater::Error::TargetsNotFound(_)) => Found::NoEntry,
            Err(error) => Found::Error(error.to_string()),
        },
    };
    after_check(app, found, seen, before, manual, None).await;
}

fn updater(
    app: &AppHandle,
    seen: Arc<Mutex<Option<(String, bool)>>>,
) -> Result<tauri_plugin_updater::Updater, String> {
    let handle = app.clone();
    let mut builder = app
        .updater_builder()
        .pubkey(app.state::<UpdatesState>().pubkey())
        // Record what the release offers before the plugin looks for our
        // platform; offer only a higher version (FR-4).
        .version_comparator(move |current, release| {
            let newer = release.version > current;
            *seen.lock().unwrap_or_else(PoisonError::into_inner) =
                Some((release.version.to_string(), newer));
            newer
        })
        // Windows: the plugin ends the app right after this, without the
        // exit handlers (FR-10).
        .on_before_exit(move || {
            crate::system::shutdown(&handle);
            handle.cleanup_before_exit();
        });
    if let Some(endpoint) = dev_endpoint() {
        let url = endpoint
            .parse()
            .map_err(|error| format!("{ENDPOINT_ENV}: {error}"))?;
        builder = builder
            .endpoints(vec![url])
            .map_err(|error| error.to_string())?;
    }
    builder.build().map_err(|error| error.to_string())
}

async fn after_check(
    app: &AppHandle,
    found: Found,
    seen: Arc<Mutex<Option<(String, bool)>>>,
    before: Status,
    manual: bool,
    update: Option<Update>,
) {
    let seen = seen.lock().unwrap_or_else(PoisonError::into_inner).clone();
    let decision = decide(found, seen, notice_only());
    if !matches!(decision, Decision::Error(_)) {
        let now = now_ms();
        app.state::<UpdatesState>().lock().last_checked_at = Some(now);
        let _ = app
            .state::<Db>()
            .with(|conn| evo_store::settings::set(conn, LAST_CHECKED_KEY, &json!(now)));
    }
    match decision {
        Decision::UpToDate => {
            forget_ready(app);
            set_status(app, Status::UpToDate);
        }
        Decision::Notice(version) => {
            forget_ready(app);
            set_status(app, Status::Notice { version });
        }
        Decision::Error(message) => {
            log(app, "warn", format!("update check failed: {message}"));
            // An automatic check that failed changes nothing the person sees.
            let status = if manual {
                Status::Failed {
                    stage: Stage::Check,
                    message,
                    version: None,
                }
            } else if matches!(before, Status::Checking { .. }) {
                Status::Idle
            } else {
                before
            };
            set_status(app, status);
        }
        Decision::Download(version) => match update {
            Some(update) => download(app, update, version).await,
            None => set_status(app, Status::Idle),
        },
    }
}

/// Drop the update that was waiting, and its file.
fn forget_ready(app: &AppHandle) {
    let ready = app.state::<UpdatesState>().lock().ready.take();
    if let Some(ready) = ready {
        if let Some(dir) = ready.path.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

async fn download(app: &AppHandle, update: Update, version: String) {
    let state = app.state::<UpdatesState>();
    // Already waiting for this version: nothing to do.
    if matches!(&state.lock().ready, Some(ready) if ready.update.version == version) {
        set_status(app, Status::Ready { version });
        return;
    }
    // A newer version replaces the one that was waiting (spec, edge cases).
    forget_ready(app);
    let Some(dir) = cache_dir(app).map(|dir| dir.join(&version)) else {
        set_status(
            app,
            Status::Failed {
                stage: Stage::Download,
                message: "no cache folder".into(),
                version: Some(version),
            },
        );
        return;
    };
    let path = dir.join(file_name(update.download_url.as_str()));

    // Downloaded in an earlier run, and still intact: offer it again.
    if let Ok(bytes) = tokio::fs::read(&path).await {
        if verify(&bytes, &update.signature, &state.pubkey()).is_ok() {
            state.lock().ready = Some(Ready { update, path });
            set_status(app, Status::Ready { version });
            return;
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    set_status(
        app,
        Status::Downloading {
            version: version.clone(),
            received: 0,
            total: None,
        },
    );
    let mut received = 0u64;
    let mut last = Instant::now();
    let progress_app = app.clone();
    let progress_version = version.clone();
    let result = update
        .download(
            |chunk, total| {
                received += chunk as u64;
                if last.elapsed() >= PROGRESS_EVERY {
                    last = Instant::now();
                    set_status(
                        &progress_app,
                        Status::Downloading {
                            version: progress_version.clone(),
                            received,
                            total,
                        },
                    );
                }
            },
            || {},
        )
        .await;
    let bytes = match result {
        Ok(bytes) => bytes,
        Err(error) => {
            let stage = if matches!(error, tauri_plugin_updater::Error::Minisign(_)) {
                Stage::Verify
            } else {
                Stage::Download
            };
            fail_download(app, &dir, stage, error.to_string(), version);
            return;
        }
    };
    // Write under a temporary name, then rename: a cut-off write is never
    // taken for a finished download (NFR-6).
    let partial = path.with_extension("partial");
    let saved = async {
        tokio::fs::create_dir_all(&dir).await?;
        tokio::fs::write(&partial, &bytes).await?;
        tokio::fs::rename(&partial, &path).await
    }
    .await;
    if let Err(error) = saved {
        fail_download(
            app,
            &dir,
            Stage::Download,
            format!("could not save the update: {error}"),
            version,
        );
        return;
    }
    log(
        app,
        "info",
        format!("update {version} downloaded and verified"),
    );
    state.lock().ready = Some(Ready { update, path });
    set_status(app, Status::Ready { version });
}

fn fail_download(app: &AppHandle, dir: &Path, stage: Stage, message: String, version: String) {
    let _ = std::fs::remove_dir_all(dir);
    log(
        app,
        if stage == Stage::Verify {
            "error"
        } else {
            "warn"
        },
        format!("update {version}: {message}"),
    );
    set_status(
        app,
        Status::Failed {
            stage,
            message,
            version: Some(version),
        },
    );
}

/// Restart to update (FR-8 to FR-10).
async fn install(app: &AppHandle, force: bool) -> InstallReply {
    let state = app.state::<UpdatesState>();
    let working = app.state::<crate::engine::EngineState>().working();
    if !force && working > 0 {
        return InstallReply::Confirm { working };
    }
    let _busy = state.busy.lock().await;
    let Some(ready) = state.lock().ready.take() else {
        return InstallReply::NotReady;
    };
    let version = ready.update.version.clone();
    set_status(
        app,
        Status::Installing {
            version: version.clone(),
        },
    );

    // Everything that can fail runs before the engines stop: read the file
    // back and check its signature again (plan 3.7).
    let bytes = match tokio::fs::read(&ready.path).await {
        Ok(bytes) => bytes,
        Err(error) => {
            return install_failed(
                app,
                None,
                format!("could not read the update: {error}"),
                version,
            )
        }
    };
    if let Err(message) = verify(&bytes, &ready.update.signature, &state.pubkey()) {
        if let Some(dir) = ready.path.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
        log(
            app,
            "error",
            format!("update {version} failed its second check: {message}"),
        );
        set_status(
            app,
            Status::Failed {
                stage: Stage::Verify,
                message,
                version: Some(version),
            },
        );
        return InstallReply::NotReady;
    }
    log(app, "info", format!("installing update {version}"));

    let task_app = app.clone();
    let update = ready.update.clone();
    // Blocking work: the installer, and on Windows the shutdown inside it.
    let result = tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        update.install(&bytes).map_err(|error| error.to_string())?;
        // Windows never gets here: the plugin ended the app. macOS replaced
        // the bundle: stop everything and start the new version.
        crate::system::shutdown(&task_app);
        task_app.restart();
    })
    .await
    .map_err(|error| error.to_string())
    .and_then(|result| result);
    match result {
        Ok(()) => InstallReply::Started,
        Err(message) => install_failed(app, Some(ready), message, version),
    }
}

fn install_failed(
    app: &AppHandle,
    ready: Option<Ready>,
    message: String,
    version: String,
) -> InstallReply {
    log(
        app,
        "error",
        format!("update {version} could not be installed: {message}"),
    );
    // Keep the update so the person can try again (FR-13).
    app.state::<UpdatesState>().lock().ready = ready;
    set_status(
        app,
        Status::Failed {
            stage: Stage::Install,
            message,
            version: Some(version),
        },
    );
    InstallReply::NotReady
}

fn main_only(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("updates are handled by the main window".into())
    }
}

#[tauri::command]
pub fn updates_state(window: WebviewWindow) -> Result<UpdateState, String> {
    main_only(&window)?;
    Ok(snapshot(window.app_handle()))
}

/// Check now, by hand (FR-3): errors are shown.
#[tauri::command]
pub async fn updates_check(window: WebviewWindow) -> Result<UpdateState, String> {
    main_only(&window)?;
    let app = window.app_handle().clone();
    if available(&app) {
        check(&app, true).await;
    }
    Ok(snapshot(&app))
}

#[tauri::command]
pub async fn updates_install(window: WebviewWindow, force: bool) -> Result<InstallReply, String> {
    main_only(&window)?;
    if notice_only() {
        return Ok(InstallReply::NotReady);
    }
    Ok(install(window.app_handle(), force).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway key pair made for these tests with `tauri signer generate`;
    /// only the public half and one signature are kept.
    const TEST_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEIwRUIzRjU0NDA5MURCRTMKUldUajI1RkFWRC9yc0ttVWFmQmRUYnIyZ3d4S0hQdDF6RUF6Wlo0cis0VzgrcFc0N2pxSUV1dFcK";
    const OTHER_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDRCQTMxMUI3QkIwRDk5NjcKUldSbm1RMjd0eEdqUzlqSmVqdkRnN2lOWm5walM1U3p5dTg5RXo5VHE4dkRnUlZEaTZIR1U0VzkK";
    const PAYLOAD: &[u8] = b"evo update test payload\n";
    const SIGNATURE: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVUajI1RkFWRC9yc0lzZmRtYWREQmFsT0IvMU43M29hR2RkMjFaSjRkbmo5NGRUd1BRSFZwWGovUVJDeXgxbUcrTVc1RkkvbXZKZFNaRkZhTURIM1BOTkRZRUZJU1h0b0FvPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzk0MzIzNjc4CWZpbGU6cGF5bG9hZC5iaW4KZFRIaDE1OXE1ZnJDZzdGNFN0T3FXeVFJRFh6S3pQZUZCd0Q3dUZFU2Q3cFlBQlBuV2J1eGtabmU0aENhSXAvR0k0MlNqN1RJN0ZTdm9KN0NyM2VRQkE9PQo=";

    #[test]
    fn only_a_file_signed_with_the_release_key_is_accepted() {
        assert_eq!(verify(PAYLOAD, SIGNATURE, TEST_PUBKEY), Ok(()));
        let mut changed = PAYLOAD.to_vec();
        changed[0] ^= 1;
        assert!(verify(&changed, SIGNATURE, TEST_PUBKEY).is_err());
        assert!(verify(PAYLOAD, SIGNATURE, OTHER_PUBKEY).is_err());
        assert!(verify(PAYLOAD, "not base64!", TEST_PUBKEY).is_err());
        assert!(verify(PAYLOAD, SIGNATURE, "").is_err());
    }

    #[test]
    fn a_newer_release_is_downloaded_or_announced() {
        let seen = |v: &str, newer| Some((v.to_string(), newer));
        assert_eq!(
            decide(Found::Update("4.3.0".into()), seen("4.3.0", true), false),
            Decision::Download("4.3.0".into())
        );
        // Linux packages are only told (FR-12).
        assert_eq!(
            decide(Found::Update("4.3.0".into()), seen("4.3.0", true), true),
            Decision::Notice("4.3.0".into())
        );
        assert_eq!(
            decide(Found::Nothing, seen("4.2.1", false), false),
            Decision::UpToDate
        );
    }

    #[test]
    fn a_release_without_our_platform_is_announced_only_when_newer() {
        let seen = |v: &str, newer| Some((v.to_string(), newer));
        assert_eq!(
            decide(Found::NoEntry, seen("4.3.0", true), false),
            Decision::Notice("4.3.0".into())
        );
        // Equal or lower is never offered (FR-4).
        assert_eq!(
            decide(Found::NoEntry, seen("4.2.1", false), false),
            Decision::UpToDate
        );
        assert_eq!(
            decide(Found::NoEntry, seen("4.0.0", false), true),
            Decision::UpToDate
        );
        // The comparator did not run: the plugin changed; say so.
        assert!(matches!(
            decide(Found::NoEntry, None, false),
            Decision::Error(_)
        ));
        assert_eq!(
            decide(Found::Error("offline".into()), None, false),
            Decision::Error("offline".into())
        );
    }

    #[test]
    fn an_automatic_check_is_due_every_six_hours() {
        let hour = 60 * 60 * 1000;
        assert!(due(10 * hour, None));
        assert!(!due(10 * hour, Some(5 * hour)));
        assert!(due(11 * hour, Some(5 * hour)));
        // The clock went back: check rather than wait for it.
        assert!(due(hour, Some(5 * hour)));
    }

    #[test]
    fn old_and_unknown_cache_folders_are_removed() {
        let current = Version::parse("4.3.0").unwrap();
        let stale = stale_folders(
            ["4.2.1", "4.3.0", "4.4.0", "junk", "5.0.0-beta.1"],
            &current,
        );
        assert_eq!(stale, vec!["4.2.1", "4.3.0", "junk"]);
    }

    #[test]
    fn downloads_keep_a_safe_file_name() {
        assert_eq!(
            file_name(
                "https://github.com/o/r/releases/download/v4.3.0/Evo.Harness_4.3.0_x64-setup.exe"
            ),
            "Evo.Harness_4.3.0_x64-setup.exe"
        );
        assert_eq!(
            file_name("https://x/a%20b.app.tar.gz?token=1"),
            "a_20b.app.tar.gz"
        );
        assert_eq!(file_name("https://x/.."), "update.bin");
        assert_eq!(file_name("https://x/"), "update.bin");
    }

    #[test]
    fn the_state_is_sent_as_the_plan_writes_it() {
        let state = UpdateState {
            current: "4.2.1".into(),
            last_checked_at: Some(1),
            status: Status::Downloading {
                version: "4.3.0".into(),
                received: 5,
                total: None,
            },
        };
        assert_eq!(
            serde_json::to_value(state).unwrap(),
            json!({ "current": "4.2.1", "lastCheckedAt": 1, "status": { "kind": "downloading", "version": "4.3.0", "received": 5, "total": null } })
        );
        let failed = Status::Failed {
            stage: Stage::Verify,
            message: "m".into(),
            version: None,
        };
        assert_eq!(
            serde_json::to_value(failed).unwrap(),
            json!({ "kind": "failed", "stage": "verify", "message": "m" })
        );
        assert_eq!(
            serde_json::to_value(Status::UpToDate).unwrap(),
            json!({ "kind": "upToDate" })
        );
        assert_eq!(
            serde_json::to_value(InstallReply::Confirm { working: 2 }).unwrap(),
            json!({ "kind": "confirm", "working": 2 })
        );
    }

    /// The release key and endpoint built into the app (plan 3.8): a key that
    /// does not decode would refuse every update, an endpoint that is not
    /// the release's HTTPS manifest would never find one.
    #[test]
    fn the_app_is_built_with_a_valid_release_key_and_endpoint() {
        let config: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let updater = &config["plugins"]["updater"];
        let pubkey = updater["pubkey"].as_str().unwrap();
        let text = String::from_utf8(base64::engine::general_purpose::STANDARD.decode(pubkey).unwrap()).unwrap();
        assert!(minisign_verify::PublicKey::decode(&text).is_ok(), "the release key does not decode");
        // A file signed with another key is refused with this one.
        assert!(verify(PAYLOAD, SIGNATURE, pubkey).is_err());
        assert_eq!(
            updater["endpoints"],
            json!(["https://github.com/evoelsewhere/evo-harness/releases/latest/download/latest.json"])
        );
        assert_eq!(updater["windows"]["installMode"], "passive");
        // Update files are made only by the release pipeline (NFR-7).
        assert!(config["bundle"].get("createUpdaterArtifacts").is_none());
    }
}
