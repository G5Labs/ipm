-- Org-wide approval activity feed (G5-Lean#1099,
-- g5.governance.approval.activity-feed). The feed and its summary scan approval
-- requests and decisions by organization over a bounded time window (capped at
-- 31 days per call). The existing indexes are keyed by workspace and definition
-- (idx_approval_requests_source_history, idx_approval_statuses_eval) or the
-- pending human inbox; none supports an org-wide time window. Add one per event
-- source so a windowed scan stays bounded.

-- Requests made in a window (`requested` events) and the pending-now rollup.
CREATE INDEX idx_approval_requests_org_created
    ON public.approval_requests USING btree (org_id, created_at, id);

-- Requests closed in a window without a decision (`closed` events). Partial, so
-- it indexes only closed requests.
CREATE INDEX idx_approval_requests_org_closed
    ON public.approval_requests USING btree (org_id, closed_at, id)
    WHERE closed_at IS NOT NULL;

-- Decisions recorded in a window (`decided` events).
CREATE INDEX idx_approval_statuses_org_created
    ON public.approval_statuses USING btree (org_id, created_at, id);
