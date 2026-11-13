-- 075: question votes (g5.documents.question.votes).
--
-- A reader's vote on a referenced question: one row per workspace, set path,
-- question id and voter, holding the option ids the entry offered and
-- optional text. A vote is platform state beside the Git-owned question and
-- answer, like a thread reply: it changes no repository content and is not a
-- decision. Additive DDL only; no data section runs under row-level security.
CREATE TABLE public.question_votes (
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    set_path text NOT NULL,
    question_id text NOT NULL,
    user_id uuid NOT NULL,
    selected_option_ids text[] NOT NULL DEFAULT '{}'::text[],
    text text NOT NULL DEFAULT '',
    question_revision bigint NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT question_votes_pkey PRIMARY KEY (workspace_id, set_path, question_id, user_id)
);
-- A vote can only belong to the organization of the workspace it is cast in
-- (target: the baseline's uq_workspaces_org_id_id UNIQUE (org_id, id)).
ALTER TABLE ONLY public.question_votes
    ADD CONSTRAINT fk_question_votes_workspace_same_org
    FOREIGN KEY (org_id, workspace_id) REFERENCES public.workspaces(org_id, id) ON DELETE CASCADE;
ALTER TABLE ONLY public.question_votes
    ADD CONSTRAINT question_votes_user_id_fkey
    FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;
ALTER TABLE public.question_votes ENABLE ROW LEVEL SECURITY;
ALTER TABLE ONLY public.question_votes FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.question_votes
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE public.question_votes TO g5_tenant;
GRANT SELECT ON TABLE public.question_votes TO g5_app;
