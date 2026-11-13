-- An open divergence run completed automatically after its proven resolution
-- landed (g5.workspace.remote.divergence-auto-complete). The integration audit
-- records that event so the owner sees why tracking resumed. The widened check
-- governs new rows now; 050 validates existing rows after this lock is released.
ALTER TABLE public.remote_sync_runs
    DROP CONSTRAINT remote_sync_runs_status_check,
    ADD CONSTRAINT remote_sync_runs_status_check CHECK (status IN (
        'noop', 'fetched', 'imported', 'pushed', 'diverged',
        'dirty_skipped', 'advisory_updated', 'auth_required',
        'push_failed', 'fetch_failed', 'resolved'
    )) NOT VALID;
