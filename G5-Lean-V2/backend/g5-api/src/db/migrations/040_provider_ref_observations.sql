-- Provider refresh is independent of workspace reconciliation and publication.
ALTER TABLE public.remote_sync_runs DROP CONSTRAINT branch_sync_runs_status_check;
ALTER TABLE public.remote_sync_runs ADD CONSTRAINT remote_sync_runs_status_check
    CHECK (status IN ('noop', 'fetched', 'imported', 'pushed', 'diverged',
                      'dirty_skipped', 'advisory_updated', 'auth_required',
                      'push_failed', 'fetch_failed'));

-- A configured remote binding synchronizes publications by default. Owners can
-- still select manual publication through the existing workspace policy.
ALTER TABLE public.workspaces ALTER COLUMN push_mode SET DEFAULT 'auto';
