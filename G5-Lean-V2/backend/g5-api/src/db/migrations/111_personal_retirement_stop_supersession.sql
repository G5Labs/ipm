-- A retired personal workspace's stop supersedes a pending deadline stop
-- (g5.personal.retirement, G5-Lean#624).
--
-- Retirement stops the VM with owner_removed and no save window. When a drain,
-- rollout, credential, capacity or delete nudge is already pending, that stop
-- replaces the nudge's unreleased operation. 099's identity fence admits that
-- replacement for user, transfer, idle and member_deactivated stops only, so
-- the retirement stop was refused and the VM kept running until the nudge
-- deadline. The function body is 099's with owner_removed added to that list.
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
                   NEW.stop_reason IN ('user', 'transfer', 'idle', 'member_deactivated', 'owner_removed')
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
