//! Past-chat search stays fast with a long history
//! (docs/features/memory-topics-and-chat-recall/spec.md, NFR-2): a search
//! over 1,000 chats returns in under 2 seconds.
//!
//! Ignored by default (it writes 1,000 transcript files); run with
//! `cargo test -p evo-store --release search_speed -- --ignored`.

use std::path::Path;
use std::time::{Duration, Instant};

use evo_store::chats::{self, ChatQuery, NewChat};
use evo_store::rusqlite::{params, Connection};
use evo_store::transcripts;

const CHATS: usize = 1_000;
/// About 20 kB of text per chat.
const WORDS_PER_CHAT: usize = 3_000;

fn write_transcript(dir: &Path, id: &str, state: &str) {
    let path = transcripts::path_in(dir, id).unwrap();
    transcripts::prepare(&path).unwrap();
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(transcripts::SESSION_SCHEMA).unwrap();
    conn.execute(
        "INSERT INTO sessions (id, created_at, parent_session_id, state_json) VALUES (?1, 0, NULL, ?2)",
        params![id, state],
    )
    .unwrap();
}

#[test]
#[ignore = "writes 1,000 transcript files; run in release with --ignored"]
fn search_speed_over_a_thousand_chats() {
    let dir = tempfile::tempdir().unwrap();
    let mut conn = evo_store::open_at(&dir.path().join("evo.db")).unwrap();
    let files = dir.path().join(transcripts::DIR_NAME);
    for index in 0..CHATS {
        let id = format!("chat-{index}");
        chats::create_chat(
            &conn,
            &NewChat {
                id: id.clone(),
                title: format!("Chat number {index}"),
                ..Default::default()
            },
        )
        .unwrap();
        let mut text = String::with_capacity(WORDS_PER_CHAT * 7);
        for word in 0..WORDS_PER_CHAT {
            text.push_str(&format!("w{} ", (index * 31 + word * 17) % 5_000));
        }
        // One chat in a hundred holds both words, apart from each other.
        if index % 100 == 7 {
            text.insert_str(0, "the release date moved ");
            text.push_str(" and the freeze starts later");
        }
        let state = serde_json::json!({ "records": [{ "text": text }] }).to_string();
        write_transcript(&files, &id, &state);
    }

    // The first search reads every transcript once; that is not what NFR-2 measures.
    let query = ChatQuery {
        query: "release freeze",
        limit: 10,
        ..Default::default()
    };
    chats::find_chats(&mut conn, &files, &query).unwrap();

    let started = Instant::now();
    let found = chats::find_chats(&mut conn, &files, &query).unwrap();
    let took = started.elapsed();
    assert_eq!(found.len(), 10);
    assert!(
        took < Duration::from_secs(2),
        "a search over {CHATS} chats took {took:?}"
    );
    println!("search over {CHATS} chats: {took:?}");
}
