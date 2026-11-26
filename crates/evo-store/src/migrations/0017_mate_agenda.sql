-- Mates that speak first (docs/features/evo-mates/spec.md, FR-63 - FR-71;
-- plan 3.9): each mate's initiative, its agenda of open threads and
-- check-ins, and the count of first messages for its daily budget.

ALTER TABLE mates ADD COLUMN initiative TEXT NOT NULL DEFAULT 'medium'
    CHECK (initiative IN ('off', 'low', 'medium'));

CREATE TABLE mate_agenda (
    id          TEXT PRIMARY KEY NOT NULL,
    mate_id     TEXT NOT NULL REFERENCES mates (id) ON DELETE CASCADE,
    kind        TEXT NOT NULL CHECK (kind IN ('promise', 'event', 'question', 'checkin')),
    -- What to come back to, in the mate's words.
    text        TEXT NOT NULL,
    -- When it is due (ms); a check-in's next time.
    due_at      INTEGER NOT NULL,
    -- A check-in's repeat: `weekdays 09:00`, `daily 18:00`, `mon 09:00`; NULL: once.
    repeat      TEXT,
    status      TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'done', 'never', 'paused')),
    -- The chat it came from.
    chat_id     TEXT,
    created_at  INTEGER NOT NULL,
    -- When a heartbeat last took it up.
    fired_at    INTEGER
) STRICT;

CREATE INDEX mate_agenda_due ON mate_agenda (mate_id, status, due_at);

CREATE TABLE mate_initiative (
    mate_id     TEXT NOT NULL REFERENCES mates (id) ON DELETE CASCADE,
    -- The local date, `YYYY-MM-DD`.
    day         TEXT NOT NULL,
    -- First messages sent that day.
    sent        INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (mate_id, day)
) STRICT;

-- First messages in a row the person has not answered (reset when they write).
ALTER TABLE mates ADD COLUMN unanswered INTEGER NOT NULL DEFAULT 0;
