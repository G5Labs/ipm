-- SQLx commits each migration separately. Validate after 049 releases its
-- ACCESS EXCLUSIVE lock; the constraint already governs new and changed rows.
ALTER TABLE public.remote_sync_runs
    VALIDATE CONSTRAINT remote_sync_runs_status_check;
