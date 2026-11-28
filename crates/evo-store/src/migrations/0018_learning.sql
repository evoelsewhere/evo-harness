-- 0018: the learning layers (docs/features/learning-layers/spec.md).
--
-- `journal` replaces `memory_changes`: one row per change to a memory topic
-- or a learned skill, by whoever made it (FR-1, FR-2, FR-18). `owner` is
-- `personal`, `project:<slug>` or `mate:<id>`; `before_text` and
-- `after_text` are the whole file before and after (NULL when it was added
-- or deleted); `undoable` is 0 when the earlier text held a secret or was
-- too long to keep. `chat_id` is not a foreign key: the journal outlives its
-- chats. Rows of indexing are carried over with `by = 'index'`.
CREATE TABLE journal (
    id          INTEGER PRIMARY KEY,
    at_ms       INTEGER NOT NULL,
    owner       TEXT NOT NULL,
    subject     TEXT NOT NULL CHECK (subject IN ('memory', 'skill')),
    name        TEXT NOT NULL,
    action      TEXT NOT NULL
                CHECK (action IN ('added', 'updated', 'deleted', 'archived', 'undone')),
    by          TEXT NOT NULL
                CHECK (by IN ('agent', 'person', 'index', 'tidy', 'dream', 'review', 'retro', 'undo')),
    chat_id     TEXT,
    reply_id    TEXT,
    diff        TEXT,
    before_text TEXT,
    after_text  TEXT,
    undoable    INTEGER NOT NULL DEFAULT 1,
    undone_at   INTEGER
) STRICT;

INSERT INTO journal (id, at_ms, owner, subject, name, action, by, chat_id,
                     before_text, after_text, undone_at)
SELECT id, at_ms,
       CASE WHEN scope = 'project' THEN 'project:' || COALESCE(project, '') ELSE 'personal' END,
       'memory', name, action, 'index', chat_id, before_text, after_text, undone_at
FROM memory_changes;

DROP TABLE memory_changes;

CREATE INDEX journal_owner ON journal (owner, subject, id);
CREATE INDEX journal_chat ON journal (chat_id) WHERE chat_id IS NOT NULL;

-- Per memory owner: the last nightly tidy-up (a local day), the last dream,
-- failures in a row, and per chat the tool-step count at its last skill
-- review (FR-6, FR-10, FR-16, NFR-5).
CREATE TABLE learning_state (
    owner         TEXT PRIMARY KEY NOT NULL,
    tidied_on     TEXT,
    dreamed_at_ms INTEGER,
    failures      INTEGER NOT NULL DEFAULT 0,
    review_steps  TEXT NOT NULL DEFAULT '{}'
) STRICT;

-- Ideas the dream offers the person (FR-14): `schedule` or `skill`;
-- `pending`, `accepted` or `declined`. A declined one is never offered again.
CREATE TABLE learning_suggestions (
    id         INTEGER PRIMARY KEY,
    owner      TEXT NOT NULL,
    kind       TEXT NOT NULL CHECK (kind IN ('schedule', 'skill')),
    text       TEXT NOT NULL,
    from_chats TEXT NOT NULL DEFAULT '[]',
    status     TEXT NOT NULL DEFAULT 'pending'
               CHECK (status IN ('pending', 'accepted', 'declined')),
    at_ms      INTEGER NOT NULL
) STRICT;

-- Usage gains a source per background step (NFR-4). SQLite cannot change a
-- CHECK in place, so the table is rebuilt as in 0015, every row carried over.
CREATE TABLE usage_events_rebuilt (
    id               INTEGER PRIMARY KEY,
    at_ms            INTEGER NOT NULL,
    chat_id          TEXT,
    source           TEXT NOT NULL DEFAULT 'chat'
                     CHECK (source IN ('chat', 'subagent', 'plugin', 'side', 'memory', 'mates',
                                       'upkeep', 'dream', 'review', 'retro')),
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
    duration_ms      INTEGER,
    plugin_id        TEXT
) STRICT;

INSERT INTO usage_events_rebuilt (
    id, at_ms, chat_id, source, provider_id, model_id,
    input, output, cache_read, cache_write, reasoning,
    cost_input, cost_output, cost_cache_read, cost_cache_write, cost_usd,
    stop_reason, duration_ms, plugin_id
)
SELECT
    id, at_ms, chat_id, source, provider_id, model_id,
    input, output, cache_read, cache_write, reasoning,
    cost_input, cost_output, cost_cache_read, cost_cache_write, cost_usd,
    stop_reason, duration_ms, plugin_id
FROM usage_events;

DROP TABLE usage_events;
ALTER TABLE usage_events_rebuilt RENAME TO usage_events;

CREATE INDEX usage_events_at ON usage_events (at_ms);
CREATE INDEX usage_events_chat ON usage_events (chat_id, at_ms);
CREATE INDEX usage_events_plugin ON usage_events (plugin_id, at_ms)
    WHERE plugin_id IS NOT NULL;
