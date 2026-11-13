-- Ontology changes cards (chat g5.chat.ontology-card): a workspace chat turn's
-- start snapshot, recorded once by the in-VM runner before the provider starts,
-- and the unchanged statements its agent names as governing
-- (g5.chat.ontology-card.governing). The card itself is stored on the turn's
-- reply (chat_messages.artifacts.ontology_card); this row is dropped when the
-- turn ends.
CREATE TABLE public.chat_turn_ontology (
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    task_id uuid NOT NULL,
    baseline jsonb,
    baseline_attempt integer CHECK (baseline_attempt IS NULL OR baseline_attempt > 0),
    governing jsonb NOT NULL DEFAULT '[]'::jsonb
        CHECK (jsonb_typeof(governing) = 'array' AND jsonb_array_length(governing) <= 8),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT chat_turn_ontology_pkey PRIMARY KEY (task_id),
    CONSTRAINT chat_turn_ontology_baseline_attempt CHECK ((baseline IS NULL) = (baseline_attempt IS NULL)),
    CONSTRAINT chat_turn_ontology_task_fk FOREIGN KEY (org_id, workspace_id, task_id)
        REFERENCES public.tasks(org_id, workspace_id, id) ON DELETE CASCADE
);
ALTER TABLE public.chat_turn_ontology ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.chat_turn_ontology FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.chat_turn_ontology
    USING (org_id = ANY(authz.current_org_ids()))
    WITH CHECK (org_id = ANY(authz.current_org_ids()));
CREATE POLICY formal_review_private_select ON public.chat_turn_ontology
    AS RESTRICTIVE FOR SELECT TO g5_tenant
    USING (authz.formal_review_allows(workspace_id, false)
        AND authz.formal_review_allows(authz.formal_review_task_workspace(task_id), false));
CREATE POLICY formal_review_private_insert ON public.chat_turn_ontology
    AS RESTRICTIVE FOR INSERT TO g5_tenant
    WITH CHECK (authz.formal_review_allows(workspace_id, true)
        AND authz.formal_review_allows(authz.formal_review_task_workspace(task_id), true));
CREATE POLICY formal_review_private_update ON public.chat_turn_ontology
    AS RESTRICTIVE FOR UPDATE TO g5_tenant
    USING (authz.formal_review_allows(workspace_id, true)
        AND authz.formal_review_allows(authz.formal_review_task_workspace(task_id), true))
    WITH CHECK (authz.formal_review_allows(workspace_id, true)
        AND authz.formal_review_allows(authz.formal_review_task_workspace(task_id), true));
CREATE POLICY formal_review_private_delete ON public.chat_turn_ontology
    AS RESTRICTIVE FOR DELETE TO g5_tenant
    USING (authz.formal_review_allows(workspace_id, true)
        AND authz.formal_review_allows(authz.formal_review_task_workspace(task_id), true));
GRANT SELECT, INSERT, UPDATE, DELETE ON public.chat_turn_ontology TO g5_tenant;
GRANT SELECT ON public.chat_turn_ontology TO g5_app;
