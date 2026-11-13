-- Approval phase contracts (g5.workbench.gates), DDL only after the 067
-- cutover: every new decision names its phase, the per-workspace stage
-- count goes, and the immutable approval-review job context accepts the
-- phase it settles while historical stage-shaped documents stay valid
-- evidence (jobs.approval_context is immutable and SQL-validated, so the
-- baseline validators are replaced in place rather than edited).

-- Decisions name their phase from now on (the transitional default served
-- the 067 backfill).
ALTER TABLE public.approval_statuses ALTER COLUMN phase DROP DEFAULT;

-- The per-workspace stage count has no reader left; its CHECK goes with it.
ALTER TABLE public.workspaces DROP COLUMN stages;

-- Merge admission evidence (g5.governance.approval.requirements,
-- g5.governance.merge): the Integration Engineering requirement states
-- against the pinned source revision, recorded on the pull request before
-- any provider mutation and copied onto the merge run at local application,
-- which re-reads it rather than evaluating again.
ALTER TABLE public.workspace_pull_requests
    ADD COLUMN merge_admission jsonb
    CONSTRAINT workspace_pull_requests_merge_admission_object
        CHECK (merge_admission IS NULL OR jsonb_typeof(merge_admission) = 'object');
ALTER TABLE public.job_merge_runs
    ADD COLUMN approval_admission jsonb
    CONSTRAINT job_merge_runs_approval_admission_object
        CHECK (approval_admission IS NULL OR jsonb_typeof(approval_admission) = 'object');

-- V3 request context: new submissions carry `phase` (be | se | ie) and are
-- validated by a phase-aware sibling of the baseline validator; the
-- stage-shaped documents admitted before the cutover keep validating with
-- the untouched baseline function (immutable evidence). A document never
-- carries both keys. Rust reads old contexts through a tolerant decoder
-- (planning -> be, absent or finalizing -> ie).
CREATE FUNCTION public.approval_review_context_v3_phase_is_valid(value jsonb) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    AS $_$
    SELECT value ? 'phase'
    AND NOT value ? 'stage'
    AND jsonb_typeof(value -> 'phase') = 'string'
    AND value ->> 'phase' IN ('be', 'se', 'ie')
    AND approval_review_context_v3_is_valid(value - 'phase')
$_$;

ALTER TABLE public.jobs
    DROP CONSTRAINT jobs_approval_context_shape_check,
    ADD CONSTRAINT jobs_approval_context_shape_check CHECK (
        (
            job_type = 'approval_review'
            AND approval_context IS NOT NULL
            AND (
                public.approval_review_context_is_valid(approval_context, workspace_id, id)
                OR public.approval_review_context_v3_is_valid(approval_context)
                OR public.approval_review_context_v3_phase_is_valid(approval_context)
            )
        )
        OR (job_type <> 'approval_review' AND approval_context IS NULL)
    );
