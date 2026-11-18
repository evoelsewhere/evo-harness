//! The per-user pipe the host connects to (docs/features/browser-bridge/spec.md
//! FR-3, NFR-1, NFR-3; plan 3.3 Pipe and process checks).
//!
//! Windows: a named pipe only this user and the system can open; a client
//! counts only when its executable is `evo-bridge-host` beside the app. Unix:
//! a socket in a 0700 folder, mode 0600, with the peer's executable checked
//! where the system tells it. No network port.

use std::sync::atomic::Ordering;

use evo_bridge_host::frames::{read_frame, write_frame};
use evo_bridge_host::{peer, HOST_EXECUTABLE};
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc;

use super::{changed, log, on_frame, Webbridge};

/// Serve the pipe (once; later calls do nothing).
pub fn start(app: &AppHandle) {
    let state = app.state::<Webbridge>();
    if state.running.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = serve(&app).await {
            log(
                &app,
                "error",
                format!("the Webbridge pipe stopped: {error}"),
            );
        }
        app.state::<Webbridge>()
            .running
            .store(false, Ordering::SeqCst);
        changed(&app);
    });
}

fn refuse(app: &AppHandle, reason: String) {
    log(app, "warn", format!("refused a connection: {reason}"));
    *app.state::<Webbridge>()
        .refused
        .lock()
        .expect("webbridge lock") = Some(reason);
    changed(app);
}

#[cfg(windows)]
async fn serve(app: &AppHandle) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle;

    let address = peer::pipe_address()?;
    let own = std::env::current_exe()?;
    let mut server = peer::create_server(&address, true)?;
    log(app, "info", "serving the Webbridge pipe");
    loop {
        server.connect().await?;
        let connected = std::mem::replace(&mut server, peer::create_server(&address, false)?);
        let client = peer::pipe_client_pid(connected.as_raw_handle()).and_then(peer::process_image);
        match client {
            Ok(image) if peer::is_sibling(&own, &image, &[HOST_EXECUTABLE]) => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move { connection(&app, connected).await });
            }
            Ok(image) => refuse(
                app,
                format!("{} is not Evo's Webbridge host", image.display()),
            ),
            Err(error) => refuse(app, format!("could not tell who connected: {error}")),
        }
    }
}

#[cfg(unix)]
async fn serve(app: &AppHandle) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let address = std::path::PathBuf::from(peer::pipe_address()?);
    let folder = address.parent().expect("the socket has a folder");
    std::fs::create_dir_all(folder)?;
    std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700))?;
    let _ = std::fs::remove_file(&address);
    let listener = tokio::net::UnixListener::bind(&address)?;
    std::fs::set_permissions(&address, std::fs::Permissions::from_mode(0o600))?;
    let own = std::env::current_exe()?;
    log(app, "info", "serving the Webbridge socket");
    loop {
        let (stream, _) = listener.accept().await?;
        let pid = stream
            .peer_cred()
            .ok()
            .and_then(|cred| cred.pid())
            .map(|pid| pid as u32);
        let image = pid.map(peer::process_image);
        match image {
            Some(Ok(image)) if !peer::is_sibling(&own, &image, &[HOST_EXECUTABLE]) => {
                refuse(
                    app,
                    format!("{} is not Evo's Webbridge host", image.display()),
                );
                continue;
            }
            // Where the system cannot name the peer, the 0600 socket in a 0700 folder is the check.
            _ => {}
        }
        let app = app.clone();
        tauri::async_runtime::spawn(async move { connection(&app, stream).await });
    }
}

/// One host connection (one browser profile): frames in, frames out, until it ends.
async fn connection<S: AsyncRead + AsyncWrite + Send + 'static>(app: &AppHandle, stream: S) {
    let (mut reader, mut writer) = tokio::io::split(stream);
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let id = app.state::<Webbridge>().add_connection(tx);
    let writing = tauri::async_runtime::spawn(async move {
        while let Some(frame) = rx.recv().await {
            if write_frame(&mut writer, &frame).await.is_err() {
                break;
            }
        }
    });
    loop {
        match read_frame(&mut reader).await {
            Ok(Some(frame)) => {
                if let Err(reason) = on_frame(app, id, &frame) {
                    refuse(app, reason);
                    break;
                }
            }
            Ok(None) | Err(_) => break,
        }
    }
    writing.abort();
    if let Some(profile) = app.state::<Webbridge>().remove_connection(id) {
        log(app, "info", format!("{} disconnected", profile.key));
        super::profile_gone(app, &profile.key);
    }
    changed(app);
}
