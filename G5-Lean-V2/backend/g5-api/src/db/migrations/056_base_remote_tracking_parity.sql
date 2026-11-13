-- Base remote tracking uses the ordinary per-repository pull policy and never
-- publishes (g5.workspace.remote.base-parity). Switch every existing Base binding's
-- auto-pull on (g5.workspace.remote.base-auto-pull-default) and clear the
-- retired workspace-level Base auto-pull setting, so API pods still running
-- the previous image stop their coordinated Base pull during the rollout and
-- no row keeps a user reference there. The retired columns and the
-- repo_mirror_refs ledger stay until a later contract migration, after no
-- running image reads them.
-- Table-owner migrations run without BYPASSRLS on Cloud SQL: read and write
-- under the temporary owner exemption, which holds an exclusive lock until
-- commit while tenant policies stay enabled.
ALTER TABLE public.workspaces NO FORCE ROW LEVEL SECURITY;
ALTER TABLE public.remote_integrations NO FORCE ROW LEVEL SECURITY;

UPDATE public.remote_integrations AS i
SET auto_pull_enabled = true, updated_at = now()
FROM public.workspaces AS w
WHERE w.id = i.workspace_id AND w.org_id = i.org_id
  AND w.is_base AND NOT i.auto_pull_enabled;

UPDATE public.workspaces
SET base_auto_pull_enabled = false, base_auto_pull_actor_user_id = NULL
WHERE base_auto_pull_enabled OR base_auto_pull_actor_user_id IS NOT NULL;

-- Base never publishes to its provider, so its push mode is always manual.
UPDATE public.workspaces
SET push_mode = 'manual', updated_at = now()
WHERE is_base AND push_mode <> 'manual';

-- Drain deferred tenant checks before restoring the table policy flags.
SET CONSTRAINTS ALL IMMEDIATE;

ALTER TABLE public.remote_integrations FORCE ROW LEVEL SECURITY;
ALTER TABLE public.workspaces FORCE ROW LEVEL SECURITY;
