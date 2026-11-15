-- 0014: plugins use the person's models directly
-- (docs/features/plugin-ai/spec.md, FR-14, FR-15, FR-18, FR-19, FR-24).
--
-- A plugin's model calls are counted as `source = plugin` with the plugin's
-- id; its runs keep their result and last events for a view that opens
-- later; its threads keep conversations per subject. Nothing here holds a
-- credential.

ALTER TABLE usage_events ADD COLUMN plugin_id TEXT;
CREATE INDEX usage_events_plugin ON usage_events (plugin_id, at_ms)
    WHERE plugin_id IS NOT NULL;

CREATE TABLE plugin_ai_runs (
    id            TEXT PRIMARY KEY,
    plugin_id     TEXT NOT NULL,
    thread_id     TEXT,
    status        TEXT NOT NULL CHECK (status IN ('running', 'done', 'error', 'cancelled')),
    provider_id   TEXT,
    model_id      TEXT,
    started_at_ms INTEGER NOT NULL,
    ended_at_ms   INTEGER,
    stopped       TEXT,
    cost_usd      REAL NOT NULL DEFAULT 0,
    result_json   TEXT,
    events_json   TEXT
);
CREATE INDEX plugin_ai_runs_plugin ON plugin_ai_runs (plugin_id, started_at_ms);

CREATE TABLE plugin_ai_threads (
    plugin_id     TEXT NOT NULL,
    thread_id     TEXT NOT NULL,
    title         TEXT,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    PRIMARY KEY (plugin_id, thread_id)
);

CREATE TABLE plugin_ai_messages (
    id           INTEGER PRIMARY KEY,
    plugin_id    TEXT NOT NULL,
    thread_id    TEXT NOT NULL,
    at_ms        INTEGER NOT NULL,
    role         TEXT NOT NULL,
    content_json TEXT NOT NULL,
    provider_id  TEXT,
    model_id     TEXT
);
CREATE INDEX plugin_ai_messages_thread ON plugin_ai_messages (plugin_id, thread_id, id);
