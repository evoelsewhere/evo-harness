-- 0019: reactions to channel lines, from the person and from mates
-- (docs/features/evo-mates-messaging/spec.md, FR-3, FR-4).
--
-- One reaction per reactor per line. `message` is the line's id in the
-- channel's entries; `reactor` is 'person' or a mate's id. `told` marks the
-- reactions the line's author has heard of. A reaction never wakes anyone.
--
-- Downgrade (docs/features/evo-mates-messaging/downgrade-0019.py): drop the
-- table, remove the row of version 19 from schema_migrations and set
-- user_version back to 18.
CREATE TABLE channel_reactions (
    channel_id TEXT NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    message    TEXT NOT NULL,
    reactor    TEXT NOT NULL,
    reaction   TEXT NOT NULL CHECK (reaction IN ('heart', 'like', 'dislike', 'laugh', 'emphasis', 'question')),
    at_ms      INTEGER NOT NULL,
    told       INTEGER NOT NULL DEFAULT 0 CHECK (told IN (0, 1)),
    PRIMARY KEY (channel_id, message, reactor)
) STRICT;

CREATE INDEX channel_reactions_untold ON channel_reactions (channel_id, told);
