-- 0010: windows computer app control moved off the person's screen
-- (docs/features/computer-app-control/spec.md FR-8).
--
-- A row is written before a window moves and removed once it is back, so a
-- run that crashed or was killed leaves the rows behind and the next start
-- puts those windows back. `window` is the window handle on Windows (the
-- window number on macOS); `pid` guards against a handle reused by another
-- process; `placement_json` is where and how the window was (position, size,
-- normal / maximized / minimized).
CREATE TABLE computer_parked (
    window         INTEGER PRIMARY KEY,
    pid            INTEGER NOT NULL,
    placement_json TEXT    NOT NULL,
    chat_id        TEXT,
    parked_at      INTEGER NOT NULL
);
