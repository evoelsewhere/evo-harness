//! A stand-in for the Evo app's pipe, for trying the extension and the host
//! without the app (docs/features/browser-bridge/tasks.md T2). Windows only.
//!
//! Copy it next to `evo-bridge-host.exe` as `evo-desktop.exe` (the host only
//! talks to a sibling app), run it, and it prints every frame, answers `hello`
//! with `welcome`, and sends each line appended to `spike-in.txt` (in the
//! current folder) as a message (`{"t":"cdp",...}` and so on). Results print
//! as they come back.

#[cfg(windows)]
#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    use evo_bridge_host::frames::{read_frame, write_frame};
    use evo_bridge_host::peer;
    use std::os::windows::io::AsRawHandle;

    let address = peer::pipe_address()?;
    println!("serving {address}");
    let server = peer::create_server(&address, true)?;
    server.connect().await?;
    let client = peer::pipe_client_pid(server.as_raw_handle()).and_then(peer::process_image);
    println!("client: {client:?}");
    let (mut reader, mut writer) = tokio::io::split(server);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let tx_lines = tx.clone();
    tokio::spawn(async move {
        let _ = std::fs::write("spike-in.txt", "");
        let mut sent = 0usize;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let text = std::fs::read_to_string("spike-in.txt").unwrap_or_default();
            let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
            for line in &lines[sent.min(lines.len())..] {
                println!("-> {}", line.chars().take(200).collect::<String>());
                let _ = tx_lines.send(line.to_string());
            }
            sent = lines.len();
        }
    });
    tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            if write_frame(&mut writer, message.as_bytes()).await.is_err() {
                break;
            }
        }
    });
    while let Some(frame) = read_frame(&mut reader).await? {
        let text = String::from_utf8_lossy(&frame);
        let shown: String = text.chars().take(600).collect();
        println!(
            "<- {shown}{}",
            if text.len() > 600 {
                format!(" … ({} bytes)", text.len())
            } else {
                String::new()
            }
        );
        if text.contains(r#""t":"hello""#) {
            let _ = tx.send(r#"{"t":"welcome","v":1,"app":"spike","label":"Spike"}"#.to_string());
        }
    }
    println!("closed");
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("the spike server runs on Windows");
}
