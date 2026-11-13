-- Approval phase cutover (g5.workbench.gates): map every stored stage
-- binding and decision onto phases, close in-flight stage-based requests
-- and runs with the existing terminal states, then drop the stage columns.
-- See 066 for the accepted behaviour changes.
--
-- Table-owner migrations run without BYPASSRLS on Cloud SQL: read and write
-- under the temporary owner exemption, which holds an exclusive lock until
-- commit while tenant policies stay enabled (docs/database-migrations.md).
-- This file is straight-line NO FORCE / UPDATE / DROP / FORCE only; the
-- additive DDL lives in 066 and the contract DDL in 068.
ALTER TABLE public.approval_definitions NO FORCE ROW LEVEL SECURITY;
ALTER TABLE public.approval_statuses NO FORCE ROW LEVEL SECURITY;
ALTER TABLE public.approval_requests NO FORCE ROW LEVEL SECURITY;
ALTER TABLE public.approval_review_runs NO FORCE ROW LEVEL SECURITY;

-- Per definition version: {planning, finalizing} -> {be, ie}; planning only
-- -> {be} whatever its fallback (carry_over loses its one-stage ship gating,
-- skip becomes an actionable BE sign-off); finalizing only -> {ie}; neither
-- -> {} (attached but inert, as before).
UPDATE public.approval_definitions
SET phases = CASE
    WHEN stage_planning AND stage_finalizing THEN ARRAY['be', 'ie']::text[]
    WHEN stage_planning THEN ARRAY['be']::text[]
    WHEN stage_finalizing THEN ARRAY['ie']::text[]
    ELSE ARRAY[]::text[]
END;

-- Historical decisions keep their mapped phase: planning -> be, finalizing
-- -> ie (a carry-over finalizing decision stays an IE decision).
UPDATE public.approval_statuses
SET phase = 'be'
WHERE stage = 'planning';

-- Every pending request belongs to the stage-based request identity, which
-- the phase-aware request key replaces; supersede them (existing terminal
-- state, lifecycle CHECK: no fingerprint, closed_at set) so a late
-- settlement is refused and anyone mid-request re-requests. Terminal
-- requests keep phase NULL: the update guard admits no other transition.
UPDATE public.approval_requests
SET state = 'superseded',
    closed_at = now(),
    updated_at = now()
WHERE state = 'pending';

-- Active agent review runs are cancelled (existing terminal state; the
-- transition trigger admits it from every active state). Their jobs and
-- tasks are cancelled through the ordinary task cancel path by the API at
-- startup (maintenance::approval_phase_cutover), so a late terminal
-- callback finds a cancelled run and a superseded request and is harmless.
UPDATE public.approval_review_runs
SET state = 'cancelled'
WHERE state IN ('pending_assignment', 'assigned', 'generated', 'merged_to_draft');

-- Drain deferred checks before altering the tables.
SET CONSTRAINTS ALL IMMEDIATE;

ALTER TABLE public.approval_definitions
    DROP COLUMN stage_planning,
    DROP COLUMN stage_finalizing,
    DROP COLUMN planning_fallback;

ALTER TABLE public.approval_statuses
    DROP COLUMN stage;

ALTER TABLE public.approval_review_runs FORCE ROW LEVEL SECURITY;
ALTER TABLE public.approval_requests FORCE ROW LEVEL SECURITY;
ALTER TABLE public.approval_statuses FORCE ROW LEVEL SECURITY;
ALTER TABLE public.approval_definitions FORCE ROW LEVEL SECURITY;
