-- Raise the default workspace VM idle timeout from 30 minutes to 8 hours
-- (g5.lifecycle.idle.settings). The historical 30-minute default (H005) was
-- too short in practice; 8 hours (28800 s) is the new platform default. The
-- org-default-with-workspace-override structure is unchanged, and the existing
-- `idle_timeout_secs BETWEEN 60 AND 86400` CHECK (003) already permits it.
--
-- Metadata-only change: ALTER COLUMN ... SET DEFAULT rewrites no rows and runs
-- no row-level data section under FORCE ROW LEVEL SECURITY
-- (docs/database-migrations.md). It governs only future inserts that omit the
-- column (org_service/workspace_service create settings rows this way).
-- Existing rows keep their stored value, so a workspace an owner explicitly set
-- to a different timeout is left untouched.
ALTER TABLE public.workspace_settings
    ALTER COLUMN idle_timeout_secs SET DEFAULT 28800;
