-- PR 227 reverted the workspace-level gate introduced by 036 while keeping
-- the applied migration chain. Pause/resume and actor authorization now use
-- remote_integrations. Keep the historical 037 regression pinned to 037.
-- Rollout/rollback prerequisites: docs/db-audit-903.md (#903).
SET LOCAL lock_timeout = '5s';
ALTER TABLE public.workspaces DROP COLUMN remote_tracking_requires_resume;
