-- Approval requests made while no PR is open, under the request rule
-- (g5.governance.approval.request-pr). They are active and decidable like
-- requests bound to the open PR. Rows recorded before PR association keep
-- the default and remain historical, never resurrected as active requests.
ALTER TABLE public.approval_requests
    ADD COLUMN without_pull_request boolean NOT NULL DEFAULT false,
    ADD CONSTRAINT approval_request_without_pull_request
        CHECK (NOT without_pull_request OR pull_request_id IS NULL);

CREATE OR REPLACE FUNCTION public.guard_approval_request_update() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF NEW.id IS DISTINCT FROM OLD.id
       OR NEW.seq IS DISTINCT FROM OLD.seq
       OR NEW.org_id IS DISTINCT FROM OLD.org_id
       OR NEW.source_workspace_id IS DISTINCT FROM OLD.source_workspace_id
       OR NEW.approval_definition_id IS DISTINCT FROM OLD.approval_definition_id
       OR NEW.approval_definition_version IS DISTINCT FROM OLD.approval_definition_version
       OR NEW.request_key IS DISTINCT FROM OLD.request_key
       OR NEW.actor_type IS DISTINCT FROM OLD.actor_type
       OR NEW.actor_user_id IS DISTINCT FROM OLD.actor_user_id
       OR NEW.requested_by IS DISTINCT FROM OLD.requested_by
       OR NEW.expires_at IS DISTINCT FROM OLD.expires_at
       OR NEW.note IS DISTINCT FROM OLD.note
       OR NEW.without_pull_request IS DISTINCT FROM OLD.without_pull_request
       OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
        RAISE EXCEPTION 'approval_requests identity fields are immutable'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'approval_requests_immutable';
    END IF;

    IF OLD.state <> 'pending'
       OR NEW.state NOT IN ('settled', 'cancelled', 'expired', 'superseded', 'bypassed') THEN
        RAISE EXCEPTION 'approval_requests permits one pending-to-terminal transition'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'approval_requests_transition';
    END IF;

    IF NEW.closed_at IS DISTINCT FROM transaction_timestamp()
       OR NEW.updated_at IS DISTINCT FROM transaction_timestamp() THEN
        RAISE EXCEPTION 'approval_requests close timestamps must use transaction time'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'approval_requests_server_timestamps';
    END IF;

    NEW.closed_at := transaction_timestamp();
    NEW.updated_at := transaction_timestamp();

    RETURN NEW;
END;
$$;
