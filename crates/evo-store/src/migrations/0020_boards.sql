-- 0020: a board per channel, opt-in (docs/features/evo-mates-threads-board/spec.md,
-- FR-8 – FR-15).
--
-- `boards` holds a channel's board: its key (`LAUNCH`, numbering cards
-- `LAUNCH-12`), whether it is on (off keeps the cards) and the next card
-- number (never reused). Columns are ordered by `position`. A card sits in a
-- column at a `position` (REAL, so a drop between two cards needs no
-- renumbering); `labels`, `checklist` and `links` are JSON arrays; `source`
-- is the assignment a card came from (JSON, the coordinator's); `thread_root`
-- links the card to a thread of the channel. `assignee` and `created_by` are
-- 'person' or a mate's id. Every change to a card writes a row in
-- `board_events` (its activity log).
--
-- Downgrade (docs/features/evo-mates-threads-board/downgrade-0020.py): drop
-- the four tables, remove version 20 from schema_migrations and set
-- user_version back to 19.
CREATE TABLE boards (
    channel_id  TEXT PRIMARY KEY NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    key         TEXT NOT NULL UNIQUE CHECK (length(key) BETWEEN 2 AND 6),
    enabled     INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    next_number INTEGER NOT NULL DEFAULT 1,
    created_ms  INTEGER NOT NULL
) STRICT;

CREATE TABLE board_columns (
    id         TEXT PRIMARY KEY NOT NULL,
    channel_id TEXT NOT NULL REFERENCES boards (channel_id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    position   INTEGER NOT NULL
) STRICT;

CREATE INDEX board_columns_channel ON board_columns (channel_id, position);

CREATE TABLE board_cards (
    id          TEXT PRIMARY KEY NOT NULL,
    channel_id  TEXT NOT NULL REFERENCES boards (channel_id) ON DELETE CASCADE,
    number      INTEGER NOT NULL,
    column_id   TEXT NOT NULL REFERENCES board_columns (id),
    position    REAL NOT NULL,
    title       TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    assignee    TEXT,
    priority    TEXT NOT NULL DEFAULT 'none' CHECK (priority IN ('none', 'low', 'medium', 'high', 'urgent')),
    due_ms      INTEGER,
    labels      TEXT NOT NULL DEFAULT '[]',
    checklist   TEXT NOT NULL DEFAULT '[]',
    links       TEXT NOT NULL DEFAULT '[]',
    thread_root TEXT,
    source      TEXT,
    created_by  TEXT NOT NULL,
    created_ms  INTEGER NOT NULL,
    updated_ms  INTEGER NOT NULL,
    UNIQUE (channel_id, number)
) STRICT;

CREATE INDEX board_cards_column ON board_cards (channel_id, column_id, position);
CREATE INDEX board_cards_assignee ON board_cards (assignee);

CREATE TABLE board_events (
    id      INTEGER PRIMARY KEY,
    card_id TEXT NOT NULL REFERENCES board_cards (id) ON DELETE CASCADE,
    actor   TEXT NOT NULL,
    kind    TEXT NOT NULL,
    text    TEXT NOT NULL,
    at_ms   INTEGER NOT NULL
) STRICT;

CREATE INDEX board_events_card ON board_events (card_id, at_ms);
