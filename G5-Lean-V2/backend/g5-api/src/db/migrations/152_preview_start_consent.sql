-- Start workspace and preview (g5.preview.start-and-preview, G5-Lean
-- workspace kaihuiz-preview-wovm, 2026-11-05).
--
-- * tasks.start_consent_id binds a preview task recorded while the owner's VM
--   was not running to the Start consent it was requested with, as
--   session_inputs.start_consent_id binds a Start-and-send input
--   (migration 110). It is the `id` of workspace_vms.start_intent, a JSONB
--   value, so it carries no foreign key; the same trigger settles bound tasks
--   whenever that consent changes.
-- * The command dispatcher never reserves a bound task
--   (`reserve_queued_command_dispatch`), so a preview waits for its own Start
--   and is never run by a VM another intent brought up.
ALTER TABLE public.tasks
    ADD COLUMN start_consent_id uuid;
CREATE INDEX tasks_start_consent_idx
    ON public.tasks (start_consent_id)
    WHERE start_consent_id IS NOT NULL;

-- Settle Start-bound inputs and preview tasks in the same transition that
-- ends their Start consent. A consent that reaches ready ('consumed')
-- releases them to ordinary queueing and dispatch; one cancelled, expired,
-- failed, superseded by another intent or removed with its row withdraws the
-- inputs still queued and cancels the preview tasks still queued, so a later
-- unrelated Start never runs them. A failed Start keeps the preview's existing
-- `vm_start_failed` ending; every other end is `start_ended`. The preview
-- intent on the VM row settles with the task, as an owner's Close settles it.
CREATE OR REPLACE FUNCTION public.g5_settle_start_bound_inputs() RETURNS trigger
LANGUAGE plpgsql SET search_path TO pg_catalog, public AS $$
DECLARE
    bound uuid;
    successor jsonb;
    preview_reason text;
    cancelled_preview uuid;
BEGIN
    IF OLD.start_intent IS NULL THEN
        RETURN NULL;
    END IF;
    bound := (OLD.start_intent ->> 'id')::uuid;
    successor := CASE WHEN TG_OP = 'DELETE' THEN NULL ELSE NEW.start_intent END;
    IF successor IS NOT NULL AND (successor ->> 'id')::uuid = bound THEN
        IF successor ->> 'status' = 'consumed' THEN
            UPDATE public.session_inputs SET start_consent_id = NULL
            WHERE org_id = OLD.org_id AND start_consent_id = bound;
            UPDATE public.tasks SET start_consent_id = NULL, updated_at = now()
            WHERE org_id = OLD.org_id AND start_consent_id = bound;
            RETURN NULL;
        END IF;
        IF successor ->> 'status' NOT IN ('expired', 'cancelled', 'failed') THEN
            RETURN NULL;
        END IF;
    END IF;
    UPDATE public.session_inputs
    SET delivery = 'withdrawn', withdrawn_at = now(), start_consent_id = NULL,
        delivery_audit = delivery_audit || jsonb_build_array(
            jsonb_build_object('event', 'start_ended', 'start_intent_id', bound))
    WHERE org_id = OLD.org_id AND start_consent_id = bound AND delivery = 'queued';
    UPDATE public.session_inputs SET start_consent_id = NULL
    WHERE org_id = OLD.org_id AND start_consent_id = bound;

    preview_reason := CASE
        WHEN successor IS NOT NULL AND (successor ->> 'id')::uuid = bound
             AND successor ->> 'status' = 'failed' THEN 'vm_start_failed'
        ELSE 'start_ended' END;
    WITH ended AS (
        UPDATE public.tasks
        SET status = 'cancelled',
            outcome = jsonb_build_object(
                'status', 'cancelled', 'reason', preview_reason,
                'start_intent_id', bound),
            hold_reason = NULL, continuation_reason = NULL,
            start_consent_id = NULL, ended_at = now(), updated_at = now()
        WHERE org_id = OLD.org_id AND start_consent_id = bound
          AND kind = 'preview' AND status IN ('queued', 'waiting')
        RETURNING id
    )
    SELECT id INTO cancelled_preview FROM ended LIMIT 1;
    UPDATE public.tasks SET start_consent_id = NULL, updated_at = now()
    WHERE org_id = OLD.org_id AND start_consent_id = bound;
    IF cancelled_preview IS NOT NULL AND TG_OP <> 'DELETE' THEN
        UPDATE public.workspace_vms
        SET preview_intent = jsonb_set(preview_intent, '{status}', '"settled"'),
            updated_at = now()
        WHERE org_id = OLD.org_id AND workspace_id = OLD.workspace_id
          AND preview_intent ->> 'task_id' = cancelled_preview::text;
    END IF;
    RETURN NULL;
END $$;
