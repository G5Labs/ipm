-- Contract step for 056 / issue 291, adapted from PR 861 (7cc47a0b), whose
-- migration number 117 is now occupied. Base policy lives on
-- remote_integrations.auto_pull_enabled; historical 056 and its test stay.
-- Verify serving and rollback image compatibility before rollout (#903).
-- These drops also remove workspaces_base_auto_pull_actor_required_check
-- and workspaces_base_auto_pull_actor_user_id_fkey. DDL only; no RLS exemption.
SET LOCAL lock_timeout = '5s';
ALTER TABLE public.workspaces
    DROP COLUMN base_auto_pull_enabled,
    DROP COLUMN base_auto_pull_actor_user_id;
