-- P4.13: observations and drain correlation owned by the host registry.
--
-- Slot occupancy remains derived from workspace_vms. Retained roots are
-- host observations used only as a placement preference; they never grant
-- authority or reserve capacity. drain_id lets reconnects and host.drained
-- notifications correlate with the durable operator request.

ALTER TABLE public.worker_hosts
    ADD COLUMN retained_roots jsonb NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN drain_id uuid,
    ADD CONSTRAINT worker_hosts_retained_roots_array
        CHECK (jsonb_typeof(retained_roots) = 'array'),
    ADD CONSTRAINT worker_hosts_drain_shape
        CHECK (
            (drain_id IS NULL AND drain_requested_at IS NULL AND drain_reason IS NULL)
            OR (drain_id IS NOT NULL AND drain_requested_at IS NOT NULL AND drain_reason IS NOT NULL)
        );

CREATE INDEX worker_hosts_placement_idx
    ON public.worker_hosts(org_id, region, created_at, host_id)
    WHERE status = 'ready';

COMMENT ON COLUMN public.worker_hosts.retained_roots IS
    'Last host-reported retained workspace roots. Advisory placement preference only; workspace_vms remains the capacity and incarnation authority.';
COMMENT ON COLUMN public.worker_hosts.drain_id IS
    'Current durable host drain operation correlated with host.drain and host.drained.';

-- P4.3a: a dispatcher retry after a lost completion response must resolve the
-- same receipt instead of starting a second capture for one lifecycle call.
ALTER TABLE public.workspace_flushes
    ADD COLUMN request_key text,
    ADD CONSTRAINT workspace_flushes_request_key_shape CHECK (
        (origin = 'flush' AND request_key IS NOT NULL
            AND request_key = btrim(request_key) AND request_key <> '')
        OR (origin = 'fork_copy' AND request_key IS NULL)
    );

CREATE UNIQUE INDEX workspace_flushes_incarnation_request_idx
    ON public.workspace_flushes(workspace_id, vm_id, request_key)
    WHERE origin = 'flush';

COMMENT ON COLUMN public.workspace_flushes.request_key IS
    'Dispatcher lifecycle idempotency key. Immutable for one receipt-first capture.';

-- P4.5 owns the validated, immutable VmSpec. The source-manifest artifact and
-- its digest moved to P8.12, so the dormant P4.1 column must not become a
-- second boot authority merely to satisfy the transitional pair constraint.
ALTER TABLE public.workspace_vms
    DROP CONSTRAINT workspace_vms_boot_source_pair,
    ADD COLUMN stop_command_sent_at timestamptz,
    ADD CONSTRAINT workspace_vms_stop_command_shape CHECK (
        stop_command_sent_at IS NULL OR (
            status = 'stopping' AND stop_operation_id IS NOT NULL
        )
    );

COMMENT ON COLUMN public.workspace_vms.stop_command_sent_at IS
    'First durable delivery attempt for the current idempotent vm.stop command. Used to distinguish a confirmed stop exit from an unexpected pre-stop exit.';

-- A failed ordinary flush reopens admission and returns the same incarnation
-- to running. That is the one valid way to clear an unreleased stop operation
-- without clearing vm_id; every other replacement remains fenced.
CREATE OR REPLACE FUNCTION public.g5_workspace_vm_identity_is_fenced() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog'
    AS $$
BEGIN
    IF OLD.vm_id IS NOT NULL AND NEW.vm_id = OLD.vm_id AND NEW.epoch = OLD.epoch THEN
        IF OLD.boot_spec IS NOT NULL AND NEW.boot_spec IS DISTINCT FROM OLD.boot_spec THEN
            RAISE EXCEPTION 'boot source is immutable within an incarnation' USING ERRCODE = '23514';
        END IF;
        IF OLD.stop_operation_id IS NOT NULL
           AND NEW.stop_operation_id IS DISTINCT FROM OLD.stop_operation_id
           AND NOT (
               OLD.status = 'stopping' AND NEW.status = 'running'
               AND NEW.stop_operation_id IS NULL
               AND NEW.stop_generation IS NULL
               AND NEW.stop_reason IS NULL
               AND NEW.stop_retry_at IS NULL
               AND NEW.stop_deadline_at IS NULL
               AND NEW.stop_command_sent_at IS NULL
           )
        THEN
            RAISE EXCEPTION 'unreleased stop operation cannot be overwritten' USING ERRCODE = '23514';
        END IF;
    END IF;

    IF OLD.start_intent IS NOT NULL
       AND OLD.start_intent ->> 'status' IN ('pending', 'queued_for_capacity', 'starting')
       AND NEW.start_intent IS NOT NULL
       AND NEW.start_intent ->> 'id' IS DISTINCT FROM OLD.start_intent ->> 'id'
       AND NEW.start_intent ->> 'supersedes_intent_id' IS DISTINCT FROM OLD.start_intent ->> 'id'
    THEN
        RAISE EXCEPTION 'active start intent requires explicit fenced supersession'
            USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;
