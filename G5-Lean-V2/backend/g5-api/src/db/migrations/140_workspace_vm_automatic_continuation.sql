-- Automatic continuation after a platform-caused interruption
-- (g5.lifecycle.start.automatic-continuation). The workspace VM lifecycle
-- owner records, per ended incarnation (epoch), whether it started the
-- workspace again for its interrupted chat turns, and counts the loss-caused
-- continuations since the owner's last Start so a workspace that keeps losing
-- its VM cannot loop. An owner's Start clears the column.
ALTER TABLE public.workspace_vms
    ADD COLUMN automatic_continuation jsonb;

-- Rollout boundary: incarnations that ended before this release were decided
-- under the old rule (the owner starts them again), and a Stop before it left
-- no trace on the VM row. Record them as decided, and likewise any running VM
-- already under a stop the platform does not continue, so no historical
-- interruption is started again after deployment.
UPDATE public.workspace_vms
SET automatic_continuation = jsonb_build_object(
        'epoch', epoch, 'outcome', 'none', 'cause', 'rollout_boundary',
        'count', 0, 'at', now())
WHERE status IN ('stopped', 'failed')
   OR (pending_stop_reason IS NOT NULL AND pending_stop_reason NOT IN ('drain', 'rollout'))
   OR (stop_reason IS NOT NULL AND stop_reason NOT IN ('drain', 'rollout'));

-- The automatic continuation is its own start consent kind. The retired
-- `rollout_continuation` spelling stays accepted for rows that may carry it.
CREATE OR REPLACE FUNCTION public.g5_start_intent_is_valid(value jsonb) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    SET search_path TO 'pg_catalog'
    AS $$
SELECT value IS NULL OR (
    jsonb_typeof(value) = 'object'
    AND (value ->> 'id') ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'
    AND value ->> 'kind' IN (
        'resume', 'compound_start', 'automatic_continuation', 'rollout_continuation'
    )
    AND value ->> 'status' IN (
        'pending', 'queued_for_capacity', 'starting', 'consumed',
        'expired', 'cancelled', 'failed'
    )
    AND (value ->> 'owner_user_id') ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'
    AND (value ->> 'lifecycle_generation') ~ '^[0-9]+$'
    AND (value ->> 'attempts') ~ '^[0-9]+$'
    AND value ? 'created_at'
    AND value ? 'expires_at'
)
$$;
