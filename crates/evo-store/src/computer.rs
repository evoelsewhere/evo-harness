//! Windows that computer app control moved off the person's screen
//! (docs/features/computer-app-control/spec.md FR-8). The desktop shell writes
//! a row before it moves a window and removes it once the window is back; rows
//! left by a run that crashed are put back on the next start.

use rusqlite::{params, Connection};
use serde_json::Value;

use crate::chats::now_ms;
use crate::DbError;

/// One window recorded as moved off screen.
#[derive(Debug, Clone, PartialEq)]
pub struct Parked {
    /// The window handle (Windows) or window number (macOS).
    pub window: i64,
    /// The process that owned the window when it moved.
    pub pid: i64,
    /// Where and how the window was before it moved.
    pub placement: Value,
    /// The chat that was driving it.
    pub chat_id: Option<String>,
    pub parked_at: i64,
}

/// Record `window` as moved off screen, replacing an older row for it.
pub fn record(
    conn: &Connection,
    window: i64,
    pid: i64,
    placement: &Value,
    chat_id: Option<&str>,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO computer_parked (window, pid, placement_json, chat_id, parked_at) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (window) DO UPDATE SET pid = excluded.pid, placement_json = excluded.placement_json,
             chat_id = excluded.chat_id, parked_at = excluded.parked_at",
        params![window, pid, placement.to_string(), chat_id, now_ms()],
    )?;
    Ok(())
}

/// `window` is back where it was.
pub fn forget(conn: &Connection, window: i64) -> Result<(), DbError> {
    conn.execute("DELETE FROM computer_parked WHERE window = ?1", [window])?;
    Ok(())
}

/// Every window still recorded, oldest first. A row whose placement is not
/// valid JSON is skipped.
pub fn all(conn: &Connection) -> Result<Vec<Parked>, DbError> {
    let mut statement =
        conn.prepare("SELECT window, pid, placement_json, chat_id, parked_at FROM computer_parked ORDER BY parked_at, window")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, i64>(4)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (window, pid, placement, chat_id, parked_at) = row?;
        if let Ok(placement) = serde_json::from_str(&placement) {
            out.push(Parked {
                window,
                pid,
                placement,
                chat_id,
                parked_at,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn records_replaces_and_forgets_parked_windows() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::open_at(&dir.path().join("evo.db")).unwrap();
        let placement = json!({ "show": 1, "normal": [10, 20, 810, 620] });
        record(&conn, 4242, 7, &placement, Some("chat-1")).unwrap();
        record(&conn, 99, 8, &json!({ "show": 3 }), None).unwrap();
        // The same window parked again replaces its row.
        record(&conn, 4242, 9, &placement, Some("chat-2")).unwrap();

        let rows = all(&conn).unwrap();
        assert_eq!(rows.len(), 2);
        let first = rows.iter().find(|row| row.window == 4242).unwrap();
        assert_eq!((first.pid, first.chat_id.as_deref()), (9, Some("chat-2")));
        assert_eq!(first.placement, placement);

        forget(&conn, 4242).unwrap();
        forget(&conn, 12345).unwrap();
        let rows = all(&conn).unwrap();
        assert_eq!(
            rows.iter().map(|row| row.window).collect::<Vec<_>>(),
            vec![99]
        );
    }

    #[test]
    fn survives_reopening_the_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evo.db");
        {
            let conn = crate::open_at(&path).unwrap();
            record(&conn, 1, 2, &json!({ "show": 1 }), None).unwrap();
        }
        let conn = crate::open_at(&path).unwrap();
        assert_eq!(all(&conn).unwrap().len(), 1);
    }
}
