-- Merge with override (G5-Lean#873, #550): G5-override and GitHub-override
-- facts that the merge admission evidence (workspace_pull_requests
-- .merge_admission) does not already hold.
--
-- 1. A pending review request that an override merge leaves undecided closes
--    as `bypassed` when the merge lands (g5.governance.approval.request-status).
--    It is never a decision, so like cancellation it carries no settlement
--    fingerprint.
ALTER TABLE public.approval_requests
    DROP CONSTRAINT chk_approval_requests_state,
    ADD CONSTRAINT chk_approval_requests_state CHECK (state = ANY (ARRAY[
        'pending'::text, 'settled'::text, 'cancelled'::text, 'expired'::text,
        'superseded'::text, 'bypassed'::text
    ])),
    DROP CONSTRAINT chk_approval_request_lifecycle,
    ADD CONSTRAINT chk_approval_request_lifecycle CHECK (
        (state = 'pending'::text AND settlement_fingerprint IS NULL AND closed_at IS NULL)
        OR (state = 'settled'::text AND settlement_fingerprint IS NOT NULL AND closed_at IS NOT NULL)
        OR (state = ANY (ARRAY['cancelled'::text, 'expired'::text, 'superseded'::text, 'bypassed'::text])
            AND settlement_fingerprint IS NULL AND closed_at IS NOT NULL)
    );

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

-- 2. Pull-request events: an admitted override (its actor and reason; kept
--    even when GitHub then refuses the merge) and a repository GitHub merged
--    before G5 merged it (g5.git.provider-merged-outside).
ALTER TABLE public.workspace_pull_request_events
    DROP CONSTRAINT workspace_pull_request_events_event_type_check,
    ADD CONSTRAINT workspace_pull_request_events_event_type_check CHECK (event_type = ANY (ARRAY[
        'created'::text, 'refreshed'::text, 'source_rebased'::text,
        'merge_blocked'::text, 'merged'::text, 'closed'::text,
        'provider_created'::text, 'provider_updated'::text,
        'provider_action_required'::text, 'provider_failed'::text,
        'provider_merged'::text, 'auto_merge_enabled'::text,
        'auto_merge_disabled'::text, 'merge_overridden'::text,
        'provider_merged_outside'::text
    ]));

-- 3. A provider repository merged on GitHub outside G5 (someone used GitHub's
--    own merge, bypass included) rather than by G5's provider merge.
ALTER TABLE public.workspace_pull_request_provider_repos
    ADD COLUMN merged_outside_g5 boolean NOT NULL DEFAULT false;

COMMENT ON COLUMN public.workspace_pull_request_provider_repos.merged_outside_g5 IS
    'GitHub merged this repository before G5 merged it (G5-Lean#550, g5.git.provider-merged-outside).';
