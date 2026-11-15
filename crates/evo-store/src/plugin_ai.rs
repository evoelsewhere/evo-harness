//! Plugin AI runs, threads and spend (docs/features/plugin-ai/spec.md,
//! FR-14, FR-15, FR-18, FR-19, FR-24; migration 0014).
//!
//! The plugin-AI engine writes runs and thread messages; the shell reads
//! spend for caps and the plugin's AI tab, and deletes a plugin's rows when
//! it is uninstalled. Runs are kept for [`KEEP_RUNS_MS`].

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::DbError;

/// How long finished runs are kept.
pub const KEEP_RUNS_MS: i64 = 7 * 24 * 3600 * 1000;

/// A run with its kept result and events (JSON text).
pub type RunDetail = (Run, Option<String>, Option<String>);

/// A run as the Models tab and `evo.ai.runs()` show it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: String,
    pub plugin_id: String,
    pub thread_id: Option<String>,
    pub status: String,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    pub stopped: Option<String>,
    pub cost_usd: f64,
}

/// What a run ended with.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunEnd {
    pub status: String,
    pub stopped: Option<String>,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub cost_usd: f64,
    pub result_json: Option<String>,
    pub events_json: Option<String>,
}

pub fn start_run(
    conn: &Connection,
    id: &str,
    plugin_id: &str,
    thread_id: Option<&str>,
    at_ms: i64,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT OR REPLACE INTO plugin_ai_runs (id, plugin_id, thread_id, status, started_at_ms) VALUES (?1, ?2, ?3, 'running', ?4)",
        params![id, plugin_id, thread_id, at_ms],
    )?;
    Ok(())
}

pub fn finish_run(conn: &Connection, id: &str, end: &RunEnd, at_ms: i64) -> Result<(), DbError> {
    let status = match end.status.as_str() {
        "done" | "error" | "cancelled" => end.status.as_str(),
        _ => "error",
    };
    conn.execute(
        "UPDATE plugin_ai_runs SET status = ?2, stopped = ?3, provider_id = ?4, model_id = ?5, cost_usd = ?6,
             result_json = ?7, events_json = ?8, ended_at_ms = ?9 WHERE id = ?1",
        params![id, status, end.stopped, end.provider_id, end.model_id, end.cost_usd.max(0.0), end.result_json, end.events_json, at_ms],
    )?;
    Ok(())
}

fn run_of(row: &rusqlite::Row) -> rusqlite::Result<Run> {
    Ok(Run {
        id: row.get(0)?,
        plugin_id: row.get(1)?,
        thread_id: row.get(2)?,
        status: row.get(3)?,
        provider_id: row.get(4)?,
        model_id: row.get(5)?,
        started_at_ms: row.get(6)?,
        ended_at_ms: row.get(7)?,
        stopped: row.get(8)?,
        cost_usd: row.get(9)?,
    })
}

const RUN_COLUMNS: &str = "id, plugin_id, thread_id, status, provider_id, model_id, started_at_ms, ended_at_ms, stopped, cost_usd";

/// A plugin's most recent runs, newest first.
pub fn runs(conn: &Connection, plugin_id: &str, limit: u32) -> Result<Vec<Run>, DbError> {
    let mut statement = conn.prepare(&format!(
        "SELECT {RUN_COLUMNS} FROM plugin_ai_runs WHERE plugin_id = ?1 ORDER BY started_at_ms DESC LIMIT ?2"
    ))?;
    let rows = statement.query_map(params![plugin_id, limit], run_of)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// One run of a plugin, with its kept result and events.
pub fn run_detail(
    conn: &Connection,
    plugin_id: &str,
    id: &str,
) -> Result<Option<RunDetail>, DbError> {
    Ok(conn
        .query_row(
            &format!("SELECT {RUN_COLUMNS}, result_json, events_json FROM plugin_ai_runs WHERE plugin_id = ?1 AND id = ?2"),
            params![plugin_id, id],
            |row| Ok((run_of(row)?, row.get(10)?, row.get(11)?)),
        )
        .optional()?)
}

/// Runs left `running` by an engine that ended (the app quit): ended now.
pub fn end_orphans(conn: &Connection, at_ms: i64) -> Result<usize, DbError> {
    Ok(conn.execute(
        "UPDATE plugin_ai_runs SET status = 'error', stopped = 'error', ended_at_ms = ?1 WHERE status = 'running'",
        params![at_ms],
    )?)
}

/// Remove finished runs older than [`KEEP_RUNS_MS`].
pub fn prune(conn: &Connection, now_ms: i64) -> Result<usize, DbError> {
    Ok(conn.execute(
        "DELETE FROM plugin_ai_runs WHERE status != 'running' AND started_at_ms < ?1",
        params![now_ms - KEEP_RUNS_MS],
    )?)
}

/// What a plugin's model calls cost since `since_ms`, in USD.
pub fn spend(conn: &Connection, plugin_id: &str, since_ms: i64) -> Result<f64, DbError> {
    Ok(conn.query_row(
        "SELECT COALESCE(SUM(cost_usd), 0) FROM usage_events WHERE plugin_id = ?1 AND at_ms >= ?2",
        params![plugin_id, since_ms],
        |row| row.get(0),
    )?)
}

/// Calls and tokens of a plugin since `since_ms`: `(calls, input, output)`.
pub fn calls(
    conn: &Connection,
    plugin_id: &str,
    since_ms: i64,
) -> Result<(i64, i64, i64), DbError> {
    Ok(conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(input), 0), COALESCE(SUM(output), 0) FROM usage_events WHERE plugin_id = ?1 AND at_ms >= ?2",
        params![plugin_id, since_ms],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?)
}

/// A thread's messages as kept: `content_json` is the provider message (JSON).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadMessage {
    pub role: String,
    pub content_json: String,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub at_ms: i64,
}

/// A thread as the Models tab and `evo.ai.threads()` list it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub id: String,
    pub title: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub messages: i64,
}

/// The last `limit` messages of a plugin's thread, oldest first.
pub fn thread_messages(
    conn: &Connection,
    plugin_id: &str,
    thread_id: &str,
    limit: u32,
) -> Result<Vec<ThreadMessage>, DbError> {
    let mut statement = conn.prepare(
        "SELECT role, content_json, provider_id, model_id, at_ms FROM (
             SELECT id, role, content_json, provider_id, model_id, at_ms FROM plugin_ai_messages
             WHERE plugin_id = ?1 AND thread_id = ?2 ORDER BY id DESC LIMIT ?3
         ) ORDER BY id",
    )?;
    let rows = statement.query_map(params![plugin_id, thread_id, limit], |row| {
        Ok(ThreadMessage {
            role: row.get(0)?,
            content_json: row.get(1)?,
            provider_id: row.get(2)?,
            model_id: row.get(3)?,
            at_ms: row.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Add a turn to a plugin's thread, creating it (with `title`) if new.
pub fn append_thread(
    conn: &mut Connection,
    plugin_id: &str,
    thread_id: &str,
    title: Option<&str>,
    messages: &[ThreadMessage],
    at_ms: i64,
) -> Result<(), DbError> {
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO plugin_ai_threads (plugin_id, thread_id, title, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?4)
         ON CONFLICT (plugin_id, thread_id) DO UPDATE SET updated_at_ms = ?4,
             title = COALESCE(plugin_ai_threads.title, excluded.title)",
        params![plugin_id, thread_id, title, at_ms],
    )?;
    for message in messages {
        tx.execute(
            "INSERT INTO plugin_ai_messages (plugin_id, thread_id, at_ms, role, content_json, provider_id, model_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![plugin_id, thread_id, message.at_ms, message.role, message.content_json, message.provider_id, message.model_id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// A plugin's threads, most recently used first.
pub fn threads(conn: &Connection, plugin_id: &str) -> Result<Vec<Thread>, DbError> {
    let mut statement = conn.prepare(
        "SELECT t.thread_id, t.title, t.created_at_ms, t.updated_at_ms,
                (SELECT COUNT(*) FROM plugin_ai_messages m WHERE m.plugin_id = t.plugin_id AND m.thread_id = t.thread_id)
         FROM plugin_ai_threads t WHERE t.plugin_id = ?1 ORDER BY t.updated_at_ms DESC",
    )?;
    let rows = statement.query_map(params![plugin_id], |row| {
        Ok(Thread {
            id: row.get(0)?,
            title: row.get(1)?,
            created_at_ms: row.get(2)?,
            updated_at_ms: row.get(3)?,
            messages: row.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Delete one thread of a plugin; true if it existed.
pub fn delete_thread(conn: &Connection, plugin_id: &str, thread_id: &str) -> Result<bool, DbError> {
    conn.execute(
        "DELETE FROM plugin_ai_messages WHERE plugin_id = ?1 AND thread_id = ?2",
        params![plugin_id, thread_id],
    )?;
    Ok(conn.execute(
        "DELETE FROM plugin_ai_threads WHERE plugin_id = ?1 AND thread_id = ?2",
        params![plugin_id, thread_id],
    )? > 0)
}

/// Everything of a plugin but its usage history (uninstall; FR-19).
pub fn delete_plugin(conn: &Connection, plugin_id: &str) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM plugin_ai_runs WHERE plugin_id = ?1",
        params![plugin_id],
    )?;
    conn.execute(
        "DELETE FROM plugin_ai_messages WHERE plugin_id = ?1",
        params![plugin_id],
    )?;
    conn.execute(
        "DELETE FROM plugin_ai_threads WHERE plugin_id = ?1",
        params![plugin_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::{record, NewUsage};

    fn db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::open_at(&dir.path().join("evo.db")).unwrap();
        (dir, conn)
    }

    fn usage(plugin: Option<&str>, at_ms: i64, cost: f64) -> NewUsage {
        NewUsage {
            at_ms,
            chat_id: None,
            source: "plugin".into(),
            plugin_id: plugin.map(str::to_string),
            provider_id: "p".into(),
            model_id: "m".into(),
            input: 10,
            output: 5,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
            cost_input: 0.0,
            cost_output: cost,
            cost_cache_read: 0.0,
            cost_cache_write: 0.0,
            cost_usd: cost,
            stop_reason: None,
            duration_ms: None,
        }
    }

    #[test]
    fn spend_counts_only_the_plugins_calls_since_a_time() {
        let (_dir, mut conn) = db();
        record(
            &mut conn,
            &[
                usage(Some("a@m"), 100, 0.25),
                usage(Some("a@m"), 200, 0.5),
                usage(Some("b@m"), 200, 9.0),
                usage(None, 200, 1.0),
            ],
        )
        .unwrap();
        assert_eq!(spend(&conn, "a@m", 0).unwrap(), 0.75);
        assert_eq!(spend(&conn, "a@m", 150).unwrap(), 0.5);
        assert_eq!(calls(&conn, "a@m", 0).unwrap(), (2, 20, 10));
        assert_eq!(spend(&conn, "c@m", 0).unwrap(), 0.0);
    }

    #[test]
    fn runs_start_finish_list_prune_and_go_with_the_plugin() {
        let (_dir, conn) = db();
        start_run(&conn, "r1", "a@m", Some("AAPL"), 1_000).unwrap();
        start_run(&conn, "r2", "a@m", None, 2_000).unwrap();
        start_run(&conn, "r3", "b@m", None, 2_000).unwrap();
        let end = RunEnd {
            status: "done".into(),
            stopped: Some("end".into()),
            provider_id: Some("p".into()),
            model_id: Some("m".into()),
            cost_usd: 0.01,
            result_json: Some("{\"text\":\"hi\"}".into()),
            events_json: Some("[]".into()),
        };
        finish_run(&conn, "r1", &end, 1_500).unwrap();
        let list = runs(&conn, "a@m", 10).unwrap();
        assert_eq!(
            list.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["r2", "r1"]
        );
        assert_eq!(list[1].status, "done");
        assert_eq!(list[1].thread_id.as_deref(), Some("AAPL"));
        let (run, result, _) = run_detail(&conn, "a@m", "r1").unwrap().unwrap();
        assert_eq!(
            (run.cost_usd, result.as_deref()),
            (0.01, Some("{\"text\":\"hi\"}"))
        );
        assert!(
            run_detail(&conn, "b@m", "r1").unwrap().is_none(),
            "another plugin's run is not visible"
        );

        assert_eq!(end_orphans(&conn, 3_000).unwrap(), 2);
        assert_eq!(
            prune(&conn, 1_000 + KEEP_RUNS_MS + 1).unwrap(),
            1,
            "only the old finished one"
        );
        delete_plugin(&conn, "a@m").unwrap();
        assert!(runs(&conn, "a@m", 10).unwrap().is_empty());
        assert_eq!(runs(&conn, "b@m", 10).unwrap().len(), 1);
    }
}

#[cfg(test)]
mod thread_tests {
    use super::*;

    fn message(role: &str, text: &str, at_ms: i64) -> ThreadMessage {
        ThreadMessage {
            role: role.into(),
            content_json: format!("{{\"role\":\"{role}\",\"text\":\"{text}\"}}"),
            provider_id: None,
            model_id: None,
            at_ms,
        }
    }

    #[test]
    fn a_thread_grows_by_turns_is_listed_read_and_deleted_per_plugin() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::open_at(&dir.path().join("evo.db")).unwrap();
        append_thread(
            &mut conn,
            "a@m",
            "AAPL",
            Some("How is Apple?"),
            &[message("user", "1", 10), message("assistant", "2", 11)],
            11,
        )
        .unwrap();
        append_thread(
            &mut conn,
            "a@m",
            "AAPL",
            Some("ignored"),
            &[message("user", "3", 20), message("assistant", "4", 21)],
            21,
        )
        .unwrap();
        append_thread(
            &mut conn,
            "a@m",
            "MSFT",
            None,
            &[message("user", "x", 15)],
            15,
        )
        .unwrap();
        append_thread(
            &mut conn,
            "b@m",
            "AAPL",
            None,
            &[message("user", "other", 30)],
            30,
        )
        .unwrap();

        let list = threads(&conn, "a@m").unwrap();
        assert_eq!(
            list.iter()
                .map(|t| (t.id.as_str(), t.messages))
                .collect::<Vec<_>>(),
            [("AAPL", 4), ("MSFT", 1)]
        );
        assert_eq!(
            list[0].title.as_deref(),
            Some("How is Apple?"),
            "the first title stays"
        );
        assert_eq!((list[0].created_at_ms, list[0].updated_at_ms), (11, 21));

        let all = thread_messages(&conn, "a@m", "AAPL", 100).unwrap();
        assert_eq!(
            all.iter().map(|m| m.at_ms).collect::<Vec<_>>(),
            [10, 11, 20, 21]
        );
        let last = thread_messages(&conn, "a@m", "AAPL", 2).unwrap();
        assert_eq!(
            last.iter().map(|m| m.at_ms).collect::<Vec<_>>(),
            [20, 21],
            "the newest, in order"
        );
        assert_eq!(
            thread_messages(&conn, "b@m", "AAPL", 100).unwrap().len(),
            1,
            "another plugin's thread of the same name is its own"
        );

        assert!(delete_thread(&conn, "a@m", "AAPL").unwrap());
        assert!(!delete_thread(&conn, "a@m", "AAPL").unwrap());
        assert!(thread_messages(&conn, "a@m", "AAPL", 100)
            .unwrap()
            .is_empty());
        assert_eq!(threads(&conn, "b@m").unwrap().len(), 1);
        delete_plugin(&conn, "b@m").unwrap();
        assert!(threads(&conn, "b@m").unwrap().is_empty());
    }
}

#[cfg(test)]
mod usage_tests {
    use crate::usage::{events, record, summary, NewUsage, UsageQuery};

    #[test]
    fn usage_lists_plugins_ai_use_by_plugin() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::open_at(&dir.path().join("evo.db")).unwrap();
        let row = |plugin: Option<&str>, cost: f64| NewUsage {
            at_ms: 1_000,
            source: if plugin.is_some() { "plugin" } else { "chat" }.into(),
            plugin_id: plugin.map(str::to_string),
            provider_id: "p".into(),
            model_id: "m".into(),
            input: 10,
            output: 2,
            cost_usd: cost,
            ..Default::default()
        };
        record(
            &mut conn,
            &[
                row(Some("a@m"), 0.3),
                row(Some("a@m"), 0.2),
                row(Some("b@m"), 0.1),
                row(None, 5.0),
            ],
        )
        .unwrap();
        let all = summary(&conn, &UsageQuery::default()).unwrap();
        let plugins: Vec<(&str, f64)> = all
            .by_plugin
            .iter()
            .map(|p| (p.plugin_id.as_str(), p.totals.cost_usd))
            .collect();
        assert_eq!(plugins, [("a@m", 0.5), ("b@m", 0.1)]);
        let list = events(&conn, &UsageQuery::default(), None, 10).unwrap();
        assert_eq!(
            list.iter()
                .filter(|e| e.plugin_id.as_deref() == Some("a@m"))
                .count(),
            2
        );
    }
}
