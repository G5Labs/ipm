-- Approvals bind to workbench phases (g5.workbench.gates,
-- g5.governance.approval.requirements): a definition version names the
-- phases it gates — Business ('be'), System ('se') or Integration ('ie')
-- Engineering, none meaning attached but inert — and a decision settles one
-- phase. The planning/finalizing stage model and the per-workspace stage
-- count are removed, not kept alongside. This file adds the phase columns
-- (additive DDL only); 067 backfills and drops the stage columns under the
-- owner RLS exemption; 068 cuts the remaining contracts over.
--
-- Accepted behaviour changes of the cutover (human decision 2026-10-06,
-- stage approvals unsupported):
--   * a planning-only definition with the carry_over fallback loses its
--     one-stage ship gating: it becomes a BE sign-off and no longer gates
--     the merge;
--   * a planning-only definition with the skip fallback, inert on one-stage
--     workspaces until now, becomes an actionable BE sign-off everywhere;
--   * historical decisions keep their mapped phase: a carry-over
--     finalizing decision stays an IE decision and is never turned into a
--     BE sign-off;
--   * every pending approval request is superseded and every active agent
--     review run is cancelled at the cutover (068); anyone mid-request
--     re-requests under the phase-aware request identity.

-- No duplicates and only the three phases; cardinality 0..3. A SQL
-- function keeps the set check out of the CHECK expression (no subqueries
-- there), like uuid_array_is_set for humans_list.
CREATE FUNCTION public.approval_phase_array_is_set(value text[]) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    AS $$
    SELECT value IS NOT NULL
       AND value <@ ARRAY['be', 'se', 'ie']::text[]
       AND array_position(value, NULL) IS NULL
       AND (SELECT COUNT(*) = COUNT(DISTINCT phase) FROM unnest(value) AS t(phase));
$$;

ALTER TABLE public.approval_definitions
    ADD COLUMN phases text[] NOT NULL DEFAULT ARRAY['ie']::text[]
    CONSTRAINT approval_definitions_phases_check
        CHECK (public.approval_phase_array_is_set(phases));

-- A decision settles exactly one phase. The transitional default lets the
-- backfill in 067 run before the stage column goes; 068 drops it so every
-- new row names its phase.
ALTER TABLE public.approval_statuses
    ADD COLUMN phase text NOT NULL DEFAULT 'ie'
    CONSTRAINT approval_statuses_phase_check CHECK (phase IN ('be', 'se', 'ie'));

-- One request chain per (definition version, phase, actor slot): the
-- request key hashes the phase and the row carries it for projection.
-- NULL marks a request admitted under the stage-based identity: every one
-- of them is terminal after 067 (the request update guard permits only the
-- pending-to-terminal transition, so history is not rewritten).
ALTER TABLE public.approval_requests
    ADD COLUMN phase text
    CONSTRAINT approval_requests_phase_check
        CHECK (phase IS NULL OR phase IN ('be', 'se', 'ie'));
