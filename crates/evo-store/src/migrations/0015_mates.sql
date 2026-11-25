-- 0015: Evo Mates (docs/features/evo-mates/spec.md; plan 3.4).
--
-- A mate is a lasting AI teammate: a profile here and one direct chat of
-- kind `mate`. A channel is a chat of kind `channel` whose engine runs the
-- channel's coordinator; each member works in a `seat`, a nested session of
-- the channel's chat. Every chat made before this migration is a plain
-- `chat`, which is all that Evo Harness lists (FR-24, NFR-8).
ALTER TABLE chats ADD COLUMN kind TEXT NOT NULL DEFAULT 'chat'
    CHECK (kind IN ('chat', 'mate', 'channel', 'seat'));
-- The mate a `mate` or `seat` chat belongs to.
ALTER TABLE chats ADD COLUMN mate_id TEXT;
CREATE INDEX chats_mate ON chats (mate_id) WHERE mate_id IS NOT NULL;

-- A mate's profile. List columns (tools, connectors, skills, apps) are JSON
-- arrays; NULL means "everything the app has" (apps) or "the defaults".
CREATE TABLE mates (
    id             TEXT PRIMARY KEY NOT NULL,
    name           TEXT NOT NULL,
    -- JSON: the face (open-bot-avatar definition id, hue, material) or a picture.
    avatar         TEXT NOT NULL,
    -- One line: what the mate does.
    role           TEXT NOT NULL DEFAULT '',
    instructions   TEXT NOT NULL DEFAULT '',
    provider       TEXT,
    model          TEXT,
    thinking       TEXT,
    tools          TEXT,
    connectors     TEXT,
    skills         TEXT,
    apps           TEXT,
    approval_mode  TEXT NOT NULL DEFAULT 'ask' CHECK (approval_mode IN ('ask', 'auto-edits', 'auto')),
    sending        TEXT NOT NULL DEFAULT 'ask' CHECK (sending IN ('ask', 'send')),
    notifications  INTEGER NOT NULL DEFAULT 1 CHECK (notifications IN (0, 1)),
    paused         INTEGER NOT NULL DEFAULT 0 CHECK (paused IN (0, 1)),
    -- The mate's own folder (FR-25) and, instead, a project folder.
    folder         TEXT NOT NULL,
    project_folder TEXT,
    -- The direct conversation.
    chat_id        TEXT NOT NULL REFERENCES chats (id),
    -- The gallery role it was made from, if any.
    template       TEXT,
    pinned         INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0, 1)),
    hidden         INTEGER NOT NULL DEFAULT 0 CHECK (hidden IN (0, 1)),
    position       INTEGER NOT NULL DEFAULT 0,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL
) STRICT;

CREATE TABLE channels (
    id            TEXT PRIMARY KEY NOT NULL,
    name          TEXT NOT NULL,
    instructions  TEXT NOT NULL DEFAULT '',
    reply_rule    TEXT NOT NULL DEFAULT 'auto' CHECK (reply_rule IN ('auto', 'lead', 'everyone', 'mentioned')),
    lead_mate_id  TEXT REFERENCES mates (id) ON DELETE SET NULL,
    folder        TEXT,
    chat_id       TEXT NOT NULL REFERENCES chats (id),
    pinned        INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0, 1)),
    hidden        INTEGER NOT NULL DEFAULT 0 CHECK (hidden IN (0, 1)),
    position      INTEGER NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
) STRICT;

CREATE TABLE channel_members (
    channel_id   TEXT NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    mate_id      TEXT NOT NULL REFERENCES mates (id) ON DELETE CASCADE,
    -- The member's seat, once it first worked in the channel.
    seat_chat_id TEXT,
    position     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (channel_id, mate_id)
) STRICT;

-- A mate's "Always allow" rules: read-only tools only (FR-41).
CREATE TABLE mate_grants (
    mate_id    TEXT NOT NULL REFERENCES mates (id) ON DELETE CASCADE,
    tool       TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (mate_id, tool)
) STRICT;

-- Usage gains the source `mates`: the router's picks in channels (FR-28).
-- SQLite cannot change a CHECK in place, so the table is rebuilt as in 0012,
-- every row and the plugin column of 0014 carried over.
CREATE TABLE usage_events_rebuilt (
    id               INTEGER PRIMARY KEY,
    at_ms            INTEGER NOT NULL,
    chat_id          TEXT,
    source           TEXT NOT NULL DEFAULT 'chat'
                     CHECK (source IN ('chat', 'subagent', 'plugin', 'side', 'memory', 'mates')),
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
