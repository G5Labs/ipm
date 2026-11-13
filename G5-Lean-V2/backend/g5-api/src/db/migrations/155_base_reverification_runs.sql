-- Base re-verification runs (g5.reverification.run). One row per run,
-- inserted in the same transaction as the run's fork admission and never
-- updated: whether a run is in progress, completed or abandoned is read from
-- its fork, its workspace and that workspace's pull requests into Base.
-- `base_heads` are the Base commits the fork froze in each code repository
-- (the run's end point); an incremental run continues from the completed run
-- named by `since_run_id`, whose `base_heads` are its start point.

CREATE TABLE public.base_reverification_runs (
    -- The fork operation's id, which is also the new workspace's id.
    id uuid NOT NULL,
    org_id uuid NOT NULL,
    kind text NOT NULL,
    base_workspace_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    started_by uuid NOT NULL,
    -- [{"repoId", "rootPath", "commit"}] for each code repository of Base.
    base_heads jsonb NOT NULL,
    since_run_id uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT base_reverification_runs_pkey PRIMARY KEY (id),
    CONSTRAINT base_reverification_runs_org_id_key UNIQUE (org_id, id),
    CONSTRAINT base_reverification_runs_kind_check CHECK (kind IN ('incremental', 'full')),
    CONSTRAINT base_reverification_runs_since_check
        CHECK ((kind = 'incremental') = (since_run_id IS NOT NULL)),
    CONSTRAINT base_reverification_runs_heads_check
        CHECK (jsonb_typeof(base_heads) = 'array' AND jsonb_array_length(base_heads) > 0),
    CONSTRAINT base_reverification_runs_fork_fk
        FOREIGN KEY (id) REFERENCES public.workspace_forks(id) ON DELETE RESTRICT,
    CONSTRAINT base_reverification_runs_base_fk
        FOREIGN KEY (org_id, base_workspace_id) REFERENCES public.workspaces(org_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT base_reverification_runs_starter_fk
        FOREIGN KEY (started_by) REFERENCES public.users(id),
    CONSTRAINT base_reverification_runs_since_fk
        FOREIGN KEY (org_id, since_run_id) REFERENCES public.base_reverification_runs(org_id, id)
);
CREATE INDEX base_reverification_runs_org_created
    ON public.base_reverification_runs (org_id, created_at DESC, id);
CREATE INDEX base_reverification_runs_workspace
    ON public.base_reverification_runs (workspace_id);

ALTER TABLE public.base_reverification_runs ENABLE ROW LEVEL SECURITY;
ALTER TABLE ONLY public.base_reverification_runs FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.base_reverification_runs
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
-- A formal review's context reaches a run only through its own workspace, as
-- for every table keyed by a workspace (migration 095's review isolation).
CREATE POLICY formal_review_private_select ON public.base_reverification_runs
    AS RESTRICTIVE FOR SELECT TO g5_tenant
    USING (authz.formal_review_allows(workspace_id, false));
CREATE POLICY formal_review_private_insert ON public.base_reverification_runs
    AS RESTRICTIVE FOR INSERT TO g5_tenant
    WITH CHECK (authz.formal_review_allows(workspace_id, true));
CREATE POLICY formal_review_private_update ON public.base_reverification_runs
    AS RESTRICTIVE FOR UPDATE TO g5_tenant
    USING (authz.formal_review_allows(workspace_id, true))
    WITH CHECK (authz.formal_review_allows(workspace_id, true));
CREATE POLICY formal_review_private_delete ON public.base_reverification_runs
    AS RESTRICTIVE FOR DELETE TO g5_tenant
    USING (authz.formal_review_allows(workspace_id, true));

-- Runs are recorded once and only read afterwards.
GRANT SELECT, INSERT ON TABLE public.base_reverification_runs TO g5_tenant;
