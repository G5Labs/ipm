-- Stop workspaces by user (G5-Lean#710, `g5.lifecycle.stop.by-user`).
--
-- The membership's cleanup identifier is its execution hold: deactivation and
-- stop by user both set it, and it clears once every owned workspace has
-- settled. Stop by user leaves `deactivated_at` alone, so access is unchanged.
COMMENT ON COLUMN public.org_memberships.deactivation_cleanup_id IS
    'Execution hold: set by member deactivation or stop by user; refuses execution for the member''s workspaces until cleanup settles them.';

-- The organization owner can be held; only deactivation stays refused.
ALTER TABLE public.org_memberships
    DROP CONSTRAINT org_owner_always_active,
    ADD CONSTRAINT org_owner_always_active CHECK (role <> 'owner' OR deactivated_at IS NULL);

ALTER TABLE public.workspace_vms
    DROP CONSTRAINT workspace_vms_stop_reason_check,
    DROP CONSTRAINT workspace_vms_pending_stop_reason_check,
    ADD CONSTRAINT workspace_vms_stop_reason_check CHECK (stop_reason IS NULL OR stop_reason IN
      ('user','transfer','idle','drain','rollout','credential','capacity','delete','member_deactivated','owner_removed','execution_hold')),
    ADD CONSTRAINT workspace_vms_pending_stop_reason_check CHECK (pending_stop_reason IS NULL OR pending_stop_reason IN
      ('user','transfer','idle','drain','rollout','credential','capacity','delete','member_deactivated','owner_removed','execution_hold'));

-- A hold's stop supersedes a nudged platform stop, as deactivation's and
-- personal retirement's do. The function body is 111's with execution_hold added.
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
           AND NOT (
               OLD.status IN ('starting', 'running')
               AND OLD.pending_stop_reason IS NOT NULL
               AND OLD.pending_stop_deadline_at IS NOT NULL
               AND OLD.nudged_at IS NOT NULL
               AND NEW.status = 'stopping'
               AND NEW.stop_operation_id IS NOT NULL
               AND (
                   NEW.stop_reason IN ('user', 'transfer', 'idle', 'member_deactivated', 'owner_removed', 'execution_hold')
                   OR (
                       NEW.stop_reason = 'delete'
                       AND NEW.stop_retry_at <= statement_timestamp()
                   )
               )
               AND NEW.stop_generation IS NOT NULL
               AND NEW.stop_deadline_at IS NOT NULL
               AND NEW.pending_stop_reason IS NULL
               AND NEW.pending_stop_deadline_at IS NULL
               AND NEW.nudged_at IS NULL
               AND NEW.stop_command_sent_at IS NULL
           )
           AND NOT (
               OLD.status = 'stopping'
               AND NEW.status = 'stopping'
               AND NEW.stop_reason = 'delete'
               AND NEW.stop_operation_id IS NOT NULL
               AND NEW.stop_generation > OLD.stop_generation
               AND NEW.stop_retry_at <= statement_timestamp()
               AND NEW.pending_stop_reason IS NULL
               AND NEW.pending_stop_deadline_at IS NULL
               AND NEW.nudged_at IS NULL
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
