-- 0021: linked conversations and followed threads
-- (docs/features/evo-mates-threads-board/spec.md, FR-16, FR-18 – FR-21).
--
-- `mate_shares` records each share of lines from one conversation into
-- another (by the person or a mate's `share_to`): both conversations list it
-- under Linked. `from_lines` is a JSON array of the shared lines' ids.
--
-- `thread_follows` keeps one row per thread of a channel (its parent line's
-- id, `root`): whether the person follows it, whether it is resolved, how
-- many replies it has, the last reply's time and author, and when the person
-- last read it (unread = followed and a reply by someone else after that).
--
-- Downgrade (docs/features/evo-mates-threads-board/downgrade-0021.py): drop
-- both tables, remove version 21 from schema_migrations and set
-- user_version back to 20.
CREATE TABLE mate_shares (
    id         TEXT PRIMARY KEY NOT NULL,
    from_chat  TEXT NOT NULL REFERENCES chats (id) ON DELETE CASCADE,
    from_lines TEXT NOT NULL DEFAULT '[]',
    excerpt    TEXT NOT NULL DEFAULT '',
    to_chat    TEXT NOT NULL REFERENCES chats (id) ON DELETE CASCADE,
    note       TEXT NOT NULL DEFAULT '',
    by         TEXT NOT NULL,
    at_ms      INTEGER NOT NULL
) STRICT;

CREATE INDEX mate_shares_from ON mate_shares (from_chat, at_ms);
CREATE INDEX mate_shares_to ON mate_shares (to_chat, at_ms);

CREATE TABLE thread_follows (
    chat_id      TEXT NOT NULL REFERENCES chats (id) ON DELETE CASCADE,
    root         TEXT NOT NULL,
    root_excerpt TEXT NOT NULL DEFAULT '',
    following    INTEGER NOT NULL DEFAULT 0 CHECK (following IN (0, 1)),
    resolved     INTEGER NOT NULL DEFAULT 0 CHECK (resolved IN (0, 1)),
    replies      INTEGER NOT NULL DEFAULT 0,
    last_ms      INTEGER NOT NULL DEFAULT 0,
    last_author  TEXT NOT NULL DEFAULT '',
    last_read_ms INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (chat_id, root)
) STRICT;

CREATE INDEX thread_follows_following ON thread_follows (following, last_ms);
