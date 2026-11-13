-- Guided journey marks (workbench g5.workbench.journey.marks): an owner's
-- durable "Mark done" / "Not done" on a guided-journey step. One event per
-- mark; a step's current mark is its latest event, so there is no second copy
-- of the state. The timeline (g5.workbench.landing) projects every row as a
-- "Guided step" entry. Additive DDL only (docs/database-migrations.md).
CREATE TABLE public.workspace_journey_events (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    -- Allocated when the serialized write inserts, so the latest event is the
    -- highest seq even when an earlier-started transaction lands later
    -- (`created_at` is the transaction start, kept for the timeline).
    seq bigint GENERATED ALWAYS AS IDENTITY NOT NULL,
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    step_id text NOT NULL,
    done boolean NOT NULL,
    actor_user_id uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT workspace_journey_events_pkey PRIMARY KEY (id),
    -- The client catalog's stable ids (lowercase words and dashes); the
    -- server does not know the catalog, only its shape.
    CONSTRAINT workspace_journey_events_step_id_check
        CHECK (step_id ~ '^[a-z0-9][a-z0-9-]{0,63}$')
);
-- Like fk_workspace_phase_events_workspace_same_org: an event belongs to the
-- organization of the workspace it records.
ALTER TABLE ONLY public.workspace_journey_events
    ADD CONSTRAINT fk_workspace_journey_events_workspace_same_org
    FOREIGN KEY (org_id, workspace_id) REFERENCES public.workspaces(org_id, id) ON DELETE CASCADE;
CREATE UNIQUE INDEX idx_workspace_journey_events_seq
    ON public.workspace_journey_events USING btree (seq);
CREATE INDEX idx_workspace_journey_events_workspace
    ON public.workspace_journey_events USING btree (workspace_id, step_id, seq DESC);
CREATE INDEX idx_workspace_journey_events_workspace_created
    ON public.workspace_journey_events USING btree (workspace_id, created_at DESC);
ALTER TABLE public.workspace_journey_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE ONLY public.workspace_journey_events FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.workspace_journey_events
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE public.workspace_journey_events TO g5_tenant;
GRANT SELECT ON TABLE public.workspace_journey_events TO g5_app;
