-- Workbench phase (g5.workbench.phases.state): the engineering phase a
-- workspace is in — Business ('be'), System ('se') or Integration ('ie')
-- Engineering — is explicit, owner-moved workspace state. A fork of an
-- ordinary workspace copies its source's phase; a fork of Base, and Base
-- itself, start at 'be'. Never derived from gates, Git or task state.
-- Additive DDL only: existing rows take the default, so no row-level data
-- section runs under FORCE ROW LEVEL SECURITY (docs/database-migrations.md).
ALTER TABLE public.workspaces
    ADD COLUMN phase text NOT NULL DEFAULT 'be'
    CONSTRAINT workspaces_phase_check CHECK (phase IN ('be', 'se', 'ie'));

-- Durable record of each phase move (g5.workbench.phases.state), written in
-- the same transaction as the workspaces.phase update. The timeline
-- (g5.workbench.landing) is a projection over durable tables, so phase
-- changes need their own rows; the feed entry itself is a later phase.
CREATE TABLE public.workspace_phase_events (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    from_phase text NOT NULL,
    to_phase text NOT NULL,
    actor_user_id uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT workspace_phase_events_pkey PRIMARY KEY (id),
    CONSTRAINT workspace_phase_events_from_phase_check CHECK (from_phase IN ('be', 'se', 'ie')),
    CONSTRAINT workspace_phase_events_to_phase_check CHECK (to_phase IN ('be', 'se', 'ie'))
);
-- Like fk_wpre_pull_request_same_org on workspace_pull_request_events: an
-- event can only belong to the organization of the workspace it records
-- (target: the baseline's uq_workspaces_org_id_id UNIQUE (org_id, id)).
ALTER TABLE ONLY public.workspace_phase_events
    ADD CONSTRAINT fk_workspace_phase_events_workspace_same_org
    FOREIGN KEY (org_id, workspace_id) REFERENCES public.workspaces(org_id, id) ON DELETE CASCADE;
CREATE INDEX idx_workspace_phase_events_workspace
    ON public.workspace_phase_events USING btree (workspace_id, created_at DESC);
ALTER TABLE public.workspace_phase_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE ONLY public.workspace_phase_events FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.workspace_phase_events
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE public.workspace_phase_events TO g5_tenant;
GRANT SELECT ON TABLE public.workspace_phase_events TO g5_app;
