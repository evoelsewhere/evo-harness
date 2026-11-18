//! Try the check after an edit (docs/features/code-intelligence/spec.md,
//! FR-15–FR-18) from a terminal: `<file>` is written with each version's text
//! in turn, as the agent's `write` would, and the text the check adds is
//! printed with how long it took.
//!
//! ```text
//! cargo run -p evo-lsp --example edit-check -- <lsp dir> <project> <file> <version>…
//! ```
//!
//! The first write finds no server running, so it starts one and waits for
//! it within the budget (usually adding nothing); the example then waits for
//! the server to be ready.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use evo_lsp::edit_check::{after_edit, before_edit, EDIT_WAIT};

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() < 4 {
        println!("usage: edit-check <lsp dir> <project> <file> <version>…");
        return;
    }
    let pool: &'static evo_lsp::Pool =
        Box::leak(Box::new(evo_lsp::Pool::new(PathBuf::from(&a[0]))));
    let cwd = std::fs::canonicalize(&a[1]).expect("project");
    let file = cwd.join(&a[2]);
    // `READ_FIRST_MS=n`: start the server as reading the file does, then
    // write the first version n ms later, as an agent that reads then edits.
    let read_first = std::env::var("READ_FIRST_MS")
        .ok()
        .and_then(|ms| ms.parse::<u64>().ok());
    if let Some(ms) = read_first {
        pool.warm(&file, &cwd);
        tokio::time::sleep(Duration::from_millis(ms)).await;
        // `READ_ALSO=a,b`: other files the agent read, once the server runs.
        for other in std::env::var("READ_ALSO").unwrap_or_default().split(',') {
            if !other.is_empty() {
                pool.warm(&cwd.join(other), &cwd);
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    for (i, version) in a[3..].iter().enumerate() {
        let text = std::fs::read_to_string(version).expect("version file");
        let started = Instant::now();
        let running = pool.running_for(&file, &cwd).is_some();
        let before = before_edit(pool, &file, &cwd).await;
        std::fs::write(&file, text).expect("write");
        let note = after_edit(pool, &cwd, &file, before, EDIT_WAIT).await;
        println!(
            "── write {} ({version}) · server running before: {running} · {:.1} s",
            i + 1,
            started.elapsed().as_secs_f32()
        );
        println!("{}", note.as_deref().unwrap_or("(nothing added)"));
        if !running {
            // Let the background start finish, as the next edits would.
            let ready = Instant::now();
            while pool.running_for(&file, &cwd).is_none()
                && ready.elapsed() < Duration::from_secs(60)
            {
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            if let Some(client) = pool.running_for(&file, &cwd) {
                client.wait_idle(Duration::from_secs(60)).await;
            }
            println!(
                "   (server ready after {:.1} s)",
                ready.elapsed().as_secs_f32()
            );
        }
    }
}
