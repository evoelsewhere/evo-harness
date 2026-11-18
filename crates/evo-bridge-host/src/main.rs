//! `evo-bridge-host`: the browser starts it for the Evo extension
//! (docs/features/browser-bridge/plan.md, 3.5 Connect). It reaches the Evo
//! app's pipe, starting Evo hidden when nothing answers, checks the app is the
//! one installed beside it, introduces the extension's origin, then relays
//! frames until either side closes.

use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use evo_bridge_host::frames::{read_frame, relay, write_frame};
use evo_bridge_host::{
    intro_frame, origin_arg, peer, unreachable_frame, wants_launch, APP_EXECUTABLES,
};

/// How long the host waits for Evo after starting it.
const START_WAIT: Duration = Duration::from_secs(10);

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(origin) = origin_arg(&args).map(str::to_string) else {
        eprintln!("evo-bridge-host is started by Chrome or Edge for the Evo extension.");
        std::process::exit(2);
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("evo-bridge-host: {error}");
            std::process::exit(1);
        }
    };
    let code = runtime.block_on(run(&origin));
    std::process::exit(code);
}

async fn run(origin: &str) -> i32 {
    let mut stdout = tokio::io::stdout();
    let mut stdin = tokio::io::stdin();
    let own = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => return fail(&mut stdout, &format!("own-path: {error}")).await,
    };
    // The extension speaks first (`hello`). It asks the host to start Evo only
    // when the browser itself just started, so reconnecting never reopens an
    // Evo the person quit.
    let hello = match read_frame(&mut stdin).await {
        Ok(Some(frame)) => frame,
        _ => return 0,
    };
    let pipe = match connect(&own, wants_launch(&hello)).await {
        Ok(pipe) => pipe,
        Err(reason) => return fail(&mut stdout, &reason).await,
    };
    let (mut from_app, mut to_app) = tokio::io::split(pipe);
    if write_frame(&mut to_app, &intro_frame(origin))
        .await
        .is_err()
        || write_frame(&mut to_app, &hello).await.is_err()
    {
        return fail(&mut stdout, "evo-closed").await;
    }
    // Either direction ending ends the host: the browser closed the port, or the app went away.
    tokio::select! {
        _ = relay(&mut stdin, &mut to_app) => 0,
        _ = relay(&mut from_app, &mut stdout) => 0,
    }
}

async fn fail(stdout: &mut tokio::io::Stdout, reason: &str) -> i32 {
    let _ = write_frame(stdout, &unreachable_frame(reason)).await;
    1
}

#[cfg(windows)]
async fn connect(
    own: &std::path::Path,
    launch: bool,
) -> Result<tokio::net::windows::named_pipe::NamedPipeClient, String> {
    use std::os::windows::io::AsRawHandle;
    use tokio::net::windows::named_pipe::ClientOptions;

    const ERROR_PIPE_BUSY: i32 = 231;
    let address = peer::pipe_address().map_err(|e| format!("address: {e}"))?;
    let mut started: Option<Instant> = None;
    let mut busy_since: Option<Instant> = None;
    let client = loop {
        match ClientOptions::new().open(&address) {
            Ok(client) => break client,
            Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
                let since = *busy_since.get_or_insert_with(Instant::now);
                if since.elapsed() > START_WAIT {
                    return Err("evo-busy".into());
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => match started {
                None if !launch => return Err("evo-not-running".into()),
                None => {
                    start_app(own).map_err(|e| format!("evo-not-running: {e}"))?;
                    started = Some(Instant::now());
                }
                Some(at) if at.elapsed() > START_WAIT => return Err("evo-not-running".into()),
                Some(_) => {}
            },
            Err(error) => return Err(format!("connect: {error}")),
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let server = peer::pipe_server_pid(client.as_raw_handle())
        .and_then(peer::process_image)
        .map_err(|e| format!("not-evo: {e}"))?;
    if !peer::is_sibling(own, &server, APP_EXECUTABLES) {
        return Err(format!("not-evo: {}", server.display()));
    }
    Ok(client)
}

#[cfg(unix)]
async fn connect(own: &std::path::Path, launch: bool) -> Result<tokio::net::UnixStream, String> {
    let address = peer::pipe_address().map_err(|e| format!("address: {e}"))?;
    let mut started: Option<Instant> = None;
    loop {
        match tokio::net::UnixStream::connect(&address).await {
            Ok(stream) => return Ok(stream),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                ) =>
            {
                match started {
                    None if !launch => return Err("evo-not-running".into()),
                    None => {
                        start_app(own).map_err(|e| format!("evo-not-running: {e}"))?;
                        started = Some(Instant::now());
                    }
                    Some(at) if at.elapsed() > START_WAIT => return Err("evo-not-running".into()),
                    Some(_) => {}
                }
            }
            Err(error) => return Err(format!("connect: {error}")),
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// The Evo app beside the host.
fn app_executable(own: &std::path::Path) -> Option<PathBuf> {
    let dir = own.parent()?;
    APP_EXECUTABLES
        .iter()
        .map(|name| dir.join(name))
        .find(|path| path.is_file())
}

/// Start Evo in the tray, outside the browser's process group, so it outlives the host.
fn start_app(own: &std::path::Path) -> io::Result<()> {
    let app = app_executable(own).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Evo is not installed beside the host",
        )
    })?;
    let mut command = std::process::Command::new(&app);
    command
        .arg("--hidden")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        command.creation_flags(
            DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB,
        );
        if command.spawn().is_ok() {
            return Ok(());
        }
        // The browser's job may not allow breaking away.
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    command.spawn().map(|_| ())
}
