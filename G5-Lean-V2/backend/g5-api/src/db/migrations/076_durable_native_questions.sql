-- A question may outlive its provider process. Native deferral stores the
-- original tool-use ID as provider_request_id and the native session to resume.
ALTER TABLE public.provider_requests
    ALTER COLUMN response_deadline_at DROP NOT NULL,
    ADD COLUMN resume_session_id text,
    ADD CONSTRAINT provider_requests_resume_shape CHECK (
        resume_session_id IS NULL OR (kind = 'question'
            AND length(resume_session_id) BETWEEN 1 AND 512
            AND response_deadline_at IS NULL)),
    ADD CONSTRAINT provider_requests_permission_deadline CHECK (
        kind = 'question' OR response_deadline_at IS NOT NULL);

ALTER TABLE public.tasks DROP CONSTRAINT tasks_hold_reason_check;
ALTER TABLE public.tasks ADD CONSTRAINT tasks_hold_reason_check CHECK (
    hold_reason IS NULL OR hold_reason IN (
        'owner_stop', 'idle_stop', 'transfer', 'start_required',
        'response_timeout', 'vm_lost', 'delivery_unknown', 'awaiting_answer'));

CREATE OR REPLACE FUNCTION public.g5_provider_request_is_immutable() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog'
    AS $$
BEGIN
    IF NEW.org_id IS DISTINCT FROM OLD.org_id
       OR NEW.task_id IS DISTINCT FROM OLD.task_id
       OR NEW.attempt IS DISTINCT FROM OLD.attempt
       OR NEW.provider_request_id IS DISTINCT FROM OLD.provider_request_id
       OR NEW.resume_session_id IS DISTINCT FROM OLD.resume_session_id
       OR NEW.kind IS DISTINCT FROM OLD.kind
       OR NEW.tool_name IS DISTINCT FROM OLD.tool_name
       OR NEW.payload IS DISTINCT FROM OLD.payload
       OR NEW.display IS DISTINCT FROM OLD.display
       OR NEW.response_deadline_at IS DISTINCT FROM OLD.response_deadline_at
       OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
        RAISE EXCEPTION 'provider request execution identity and bodies are immutable'
            USING ERRCODE = '23514';
    END IF;

    IF OLD.response IS NOT NULL AND (
        (NEW.response - 'delivery_evidence') IS DISTINCT FROM
            (OLD.response - 'delivery_evidence')
        OR NEW.response_author_id IS DISTINCT FROM OLD.response_author_id
    ) THEN
        RAISE EXCEPTION 'accepted provider response body, digest, and author are immutable'
            USING ERRCODE = '23514';
    END IF;

    IF (OLD.status IN ('responded', 'invalidated') AND NEW.status <> OLD.status)
       OR (OLD.status = 'reply_pending' AND NEW.status NOT IN ('reply_pending', 'responded', 'invalidated'))
       OR (OLD.status = 'pending' AND NEW.status NOT IN ('pending', 'reply_pending', 'invalidated')) THEN
        RAISE EXCEPTION 'invalid provider request state transition' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;
