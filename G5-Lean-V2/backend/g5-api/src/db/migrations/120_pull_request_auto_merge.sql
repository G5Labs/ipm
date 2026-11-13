-- Merge when ready (g5.git.pr-auto-merge): a person's per-pull-request
-- decision that G5 merges the pull request as them once it is ready, and the
-- background pass's progress toward that merge
-- (g5.git.pr-auto-merge.readiness). Columns on the pull request so its RLS,
-- grants and cascade apply. Both decision columns are null while it is off.
ALTER TABLE public.workspace_pull_requests
    ADD COLUMN auto_merge_enabled_by uuid REFERENCES public.users(id),
    ADD COLUMN auto_merge_enabled_at timestamp with time zone,
    -- Latest reason it is still waiting, or why G5 turned it off.
    ADD COLUMN auto_merge_reason text,
    ADD COLUMN auto_merge_checked_at timestamp with time zone,
    -- Provider head per repository as last read with passing checks, and
    -- since when: the settle window.
    ADD COLUMN auto_merge_observed_heads jsonb,
    ADD COLUMN auto_merge_observed_since timestamp with time zone,
    -- The cut a merge attempt that did not land ran against, for retries.
    ADD COLUMN auto_merge_attempt_key text,
    ADD COLUMN auto_merge_attempted_at timestamp with time zone,
    ADD CONSTRAINT workspace_pull_requests_auto_merge_decision_check CHECK (
        (auto_merge_enabled_by IS NULL) = (auto_merge_enabled_at IS NULL)
    );

CREATE INDEX idx_workspace_pr_auto_merge_open
    ON public.workspace_pull_requests (org_id, id)
    WHERE status = 'open' AND auto_merge_enabled_by IS NOT NULL;

ALTER TABLE public.workspace_pull_request_events
    DROP CONSTRAINT workspace_pull_request_events_event_type_check,
    ADD CONSTRAINT workspace_pull_request_events_event_type_check CHECK (event_type = ANY (ARRAY[
        'created'::text, 'refreshed'::text, 'source_rebased'::text,
        'merge_blocked'::text, 'merged'::text, 'closed'::text,
        'provider_created'::text, 'provider_updated'::text,
        'provider_action_required'::text, 'provider_failed'::text,
        'provider_merged'::text, 'auto_merge_enabled'::text,
        'auto_merge_disabled'::text
    ]));
