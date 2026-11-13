-- Raise Base workspaces still on the previous 30-minute idle timeout to the
-- 8-hour default (g5.lifecycle.idle.settings). 072 only changed the column
-- default, which a fork never consults: workspace_fork_service copies
-- idle_timeout_secs from the source row, and every working workspace is a
-- fork rooted at its org's Base, whose row predates 072 and still holds
-- 1800. So no new workspace reached the default (observed on QA 2026-10-13).
-- The owner decided on 2026-10-13 to raise Base rows only: existing working
-- workspaces keep their value, and forks made from now on inherit 8 hours.
-- A Base on any other value is an explicit choice and is kept.
--
-- Table-owner migrations run without BYPASSRLS on Cloud SQL: write under the
-- temporary owner exemption on both tables the statement touches, which
-- holds an exclusive lock until commit while tenant policies stay enabled
-- (docs/database-migrations.md). Straight-line NO FORCE / UPDATE / FORCE only.
ALTER TABLE public.workspaces NO FORCE ROW LEVEL SECURITY;
ALTER TABLE public.workspace_settings NO FORCE ROW LEVEL SECURITY;

UPDATE public.workspace_settings AS settings
SET idle_timeout_secs = 28800
FROM public.workspaces AS workspace
WHERE workspace.id = settings.workspace_id
  AND workspace.is_base
  AND settings.idle_timeout_secs = 1800;

-- Drain deferred tenant checks before restoring the table policy flags.
SET CONSTRAINTS ALL IMMEDIATE;

ALTER TABLE public.workspace_settings FORCE ROW LEVEL SECURITY;
ALTER TABLE public.workspaces FORCE ROW LEVEL SECURITY;
