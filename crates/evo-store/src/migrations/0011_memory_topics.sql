-- 0011: memory topics and chat recall
-- (docs/features/memory-topics-and-chat-recall/spec.md).
--
-- Three facts about a chat, chosen before its first message or set by what
-- started it:
--   memory_off: the chat uses no memory, saves none and cannot search past
--               chats;
--   incognito:  as memory_off, and the chat leaves no history: its row and
--               transcript are removed when it ends, and none of its text is
--               kept in this file (fixed title, no preview, no search text);
--   scheduled:  the Scheduler started it, so indexing writes its summary but
--               changes no memory topic.
ALTER TABLE chats ADD COLUMN memory_off INTEGER NOT NULL DEFAULT 0;
ALTER TABLE chats ADD COLUMN incognito INTEGER NOT NULL DEFAULT 0;
ALTER TABLE chats ADD COLUMN scheduled INTEGER NOT NULL DEFAULT 0;

-- What automatic indexing keeps per chat: a summary (what was asked, done,
-- decided, left open) that past-chat search also matches, and how far the
-- transcript was read (`indexed_through` is a transcript entry id). A failed
-- run leaves `failed_at` and `error`; the next good run clears them.
CREATE TABLE chat_index (
    chat_id         TEXT PRIMARY KEY NOT NULL REFERENCES chats (id) ON DELETE CASCADE,
    summary         TEXT,
    indexed_through TEXT,
    indexed_at      INTEGER,
    failed_at       INTEGER,
    error           TEXT
) STRICT;

-- The changes indexing made to memory topics, so each can be shown with the
-- chat it came from and undone. `before_text` and `after_text` are the whole
-- topic file before and after (NULL when the topic was added or deleted).
-- `chat_id` is not a foreign key: the list outlives its chats. `project` is
-- the folder slug of a project topic, NULL for a personal one.
CREATE TABLE memory_changes (
    id          INTEGER PRIMARY KEY,
    at_ms       INTEGER NOT NULL,
    scope       TEXT NOT NULL CHECK (scope IN ('personal', 'project')),
    project     TEXT,
    name        TEXT NOT NULL,
    action      TEXT NOT NULL CHECK (action IN ('added', 'updated', 'deleted')),
    chat_id     TEXT,
    before_text TEXT,
    after_text  TEXT,
    undone_at   INTEGER
) STRICT;
