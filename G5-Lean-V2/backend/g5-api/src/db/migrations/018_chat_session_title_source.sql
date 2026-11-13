-- Session title ownership. Sessions are now named automatically: first from
-- the first user message ('derived'), then by the agent's one-line summary
-- ('summary'). A title the user typed or cleared is 'user' and is never
-- replaced by either automatic step.
--
-- Legacy rows keep NULL: a NULL source with a non-NULL title is treated as
-- user-owned, a NULL source with a NULL title as platform-owned. No DML here
-- on purpose: the migrator runs under FORCE ROW LEVEL SECURITY without
-- BYPASSRLS on Cloud SQL, so a backfill UPDATE would silently touch 0 rows
-- (docs/database-migrations.md).
ALTER TABLE chat_sessions
    ADD COLUMN title_source text
        CONSTRAINT chat_sessions_title_source_check
        CHECK (title_source IN ('derived', 'summary', 'user'));
