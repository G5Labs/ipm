-- SEC-H07: queued publication must never acquire a new owner's credentials.
-- Existing queue rows deliberately retain NULL authority and are discarded by
-- the worker. Re-enqueueing records fresh authority; no identity is guessed.
ALTER TABLE public.branch_integration_queue
    ADD COLUMN requested_by_user_id uuid,
    ADD COLUMN lifecycle_generation bigint,
    ADD CONSTRAINT branch_integration_queue_authority_shape CHECK (
        (requested_by_user_id IS NULL AND lifecycle_generation IS NULL)
        OR (requested_by_user_id IS NOT NULL AND lifecycle_generation IS NOT NULL AND lifecycle_generation >= 0)
    );

-- Separate from per-repository reconciliation pauses so a transfer cannot erase
-- a divergence or reset-below-baseline approval requirement.
ALTER TABLE public.workspaces
    ADD COLUMN remote_tracking_requires_resume boolean NOT NULL DEFAULT false;
