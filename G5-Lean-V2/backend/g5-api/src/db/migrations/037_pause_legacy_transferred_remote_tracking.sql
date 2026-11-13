-- SEC-H07: older transfers retained the sender's automatic publication settings.
-- Require fresh recipient consent even for transfers completed before 034.
-- Table-owner migrations run without BYPASSRLS on Cloud SQL. These ALTERs take
-- exclusive locks until transaction commit, so only the migration can use the
-- temporary owner exemption; tenant policies remain enabled throughout.
ALTER TABLE public.workspaces NO FORCE ROW LEVEL SECURITY;
ALTER TABLE public.workspace_ownership_transfers NO FORCE ROW LEVEL SECURITY;
ALTER TABLE public.workspace_branches NO FORCE ROW LEVEL SECURITY;
ALTER TABLE public.workspace_branch_repos NO FORCE ROW LEVEL SECURITY;
ALTER TABLE public.branch_integration_queue NO FORCE ROW LEVEL SECURITY;

UPDATE public.workspaces w
SET push_mode = 'manual', remote_tracking_requires_resume = true
WHERE w.deleted_at IS NULL
  AND EXISTS (SELECT 1 FROM public.workspace_ownership_transfers t
              WHERE t.workspace_id = w.id AND t.status = 'completed');

UPDATE public.workspace_branch_repos r
SET auto_pull_enabled = false
FROM public.workspace_branches b, public.workspaces w
WHERE r.workspace_branch_id = b.id AND b.workspace_id = w.id
  AND w.remote_tracking_requires_resume;

DELETE FROM public.branch_integration_queue q
USING public.workspace_branch_repos r, public.workspace_branches b, public.workspaces w
WHERE q.workspace_branch_repo_id = r.id AND r.workspace_branch_id = b.id
  AND b.workspace_id = w.id AND w.remote_tracking_requires_resume;

-- Drain deferred tenant checks before restoring table policy flags.
SET CONSTRAINTS ALL IMMEDIATE;

ALTER TABLE public.workspaces FORCE ROW LEVEL SECURITY;
ALTER TABLE public.workspace_ownership_transfers FORCE ROW LEVEL SECURITY;
ALTER TABLE public.workspace_branches FORCE ROW LEVEL SECURITY;
ALTER TABLE public.workspace_branch_repos FORCE ROW LEVEL SECURITY;
ALTER TABLE public.branch_integration_queue FORCE ROW LEVEL SECURITY;
