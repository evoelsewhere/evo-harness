-- 0016: reactions to messages in Evo Mates (docs/features/evo-mates/spec.md, FR-60).
--
-- One reaction per message of a mate's conversation; `message` is the
-- message's key in the transcript (its turn's entry and side). `excerpt` is
-- the start of the message, for the note the mate gets; `told` marks the
-- reactions the mate has heard of.
CREATE TABLE mate_reactions (
    chat_id  TEXT NOT NULL REFERENCES chats (id) ON DELETE CASCADE,
    message  TEXT NOT NULL,
    reaction TEXT NOT NULL CHECK (reaction IN ('heart', 'like', 'dislike', 'laugh', 'emphasis', 'question')),
    excerpt  TEXT NOT NULL DEFAULT '',
    at_ms    INTEGER NOT NULL,
    told     INTEGER NOT NULL DEFAULT 0 CHECK (told IN (0, 1)),
    PRIMARY KEY (chat_id, message)
) STRICT;
