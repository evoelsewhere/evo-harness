-- 0012: usage of memory indexing as its own source
-- (docs/features/memory-topics-and-chat-recall/spec.md, FR-34).
--
-- An index run is a model call outside any chat's run; its tokens and cost
-- are counted apart from the chat's. SQLite cannot change a CHECK in place,
-- so the table is rebuilt with `memory` allowed and every row carried over.
-- Nothing references `usage_events` and it references nothing.
CREATE TABLE usage_events_rebuilt (
    id               INTEGER PRIMARY KEY,
    at_ms            INTEGER NOT NULL,
    chat_id          TEXT,
    source           TEXT NOT NULL DEFAULT 'chat'
                     CHECK (source IN ('chat', 'subagent', 'plugin', 'side', 'memory')),
    provider_id      TEXT NOT NULL,
    model_id         TEXT NOT NULL,
    input            INTEGER NOT NULL DEFAULT 0,
    output           INTEGER NOT NULL DEFAULT 0,
    cache_read       INTEGER NOT NULL DEFAULT 0,
    cache_write      INTEGER NOT NULL DEFAULT 0,
    reasoning        INTEGER NOT NULL DEFAULT 0,
    cost_input       REAL NOT NULL DEFAULT 0,
    cost_output      REAL NOT NULL DEFAULT 0,
    cost_cache_read  REAL NOT NULL DEFAULT 0,
    cost_cache_write REAL NOT NULL DEFAULT 0,
    cost_usd         REAL NOT NULL DEFAULT 0,
    stop_reason      TEXT,
    duration_ms      INTEGER
) STRICT;

INSERT INTO usage_events_rebuilt (
    id, at_ms, chat_id, source, provider_id, model_id,
    input, output, cache_read, cache_write, reasoning,
    cost_input, cost_output, cost_cache_read, cost_cache_write, cost_usd,
    stop_reason, duration_ms
)
SELECT
    id, at_ms, chat_id, source, provider_id, model_id,
    input, output, cache_read, cache_write, reasoning,
    cost_input, cost_output, cost_cache_read, cost_cache_write, cost_usd,
    stop_reason, duration_ms
FROM usage_events;

DROP TABLE usage_events;
ALTER TABLE usage_events_rebuilt RENAME TO usage_events;

CREATE INDEX usage_events_at ON usage_events (at_ms);
CREATE INDEX usage_events_chat ON usage_events (chat_id, at_ms);
