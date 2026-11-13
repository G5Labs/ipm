-- Exact workspace names are organization-wide claims even when two fork
-- requests touch disjoint repository vectors. Repository row locks cannot
-- serialize that case, so PostgreSQL owns the final concurrent admission.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM public.workspace_forks
        WHERE status IN ('pending', 'aborting')
        GROUP BY org_id, lower(proposed_name)
        HAVING count(*) > 1
    ) THEN
        RAISE EXCEPTION 'duplicate active workspace fork name claims must be reconciled before migration 013';
    END IF;
END
$$;

CREATE UNIQUE INDEX workspace_forks_active_org_name_unique
    ON public.workspace_forks (org_id, lower(proposed_name))
    WHERE status IN ('pending', 'aborting');
