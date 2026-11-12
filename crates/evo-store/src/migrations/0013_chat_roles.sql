-- 0013: a nested session can run as an agent type, on a model and thinking
-- level of its own (docs/features/session-roles-and-mail/spec.md, FR-1,
-- NFR-4). The resolved role is kept with the chat as JSON, so the session
-- keeps it across turns and restarts; NULL is a chat without a role, which is
-- every chat made before this migration.

ALTER TABLE chats ADD COLUMN role TEXT;
