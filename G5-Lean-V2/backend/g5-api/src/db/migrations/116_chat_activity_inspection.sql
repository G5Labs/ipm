-- Chat activity and personal result inspection (chat g5.chat.activity.turns,
-- g5.chat.inspection.acknowledge). Activity is derived from tasks and attempts;
-- only each person's inspection watermark is stored.

-- Results that ended before this instant count as inspected by everyone. The
-- default is evaluated once for existing rows, so their historical results do
-- not all turn unread at rollout; a new chat's floor is its creation.
ALTER TABLE public.chat_sessions
    ADD COLUMN inspection_floor_at timestamptz NOT NULL DEFAULT now();

-- The workspace list orders every chat before its page limit. The index on
-- (workspace_id, ...) left with the dropped status column in 008.
CREATE INDEX chat_sessions_workspace_idx ON public.chat_sessions (workspace_id);

-- A chat's newest turns, for its active turn and its latest finished result.
CREATE INDEX tasks_agent_session_recent_idx
    ON public.tasks (chat_session_id, created_at DESC, id DESC)
    WHERE kind = 'agent';

CREATE TABLE public.chat_session_inspections (
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    session_id uuid NOT NULL,
    user_id uuid NOT NULL,
    inspected_through timestamptz NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT chat_session_inspections_pkey PRIMARY KEY (session_id, user_id),
    CONSTRAINT chat_session_inspections_session_fk FOREIGN KEY (org_id, workspace_id, session_id)
        REFERENCES public.chat_sessions(org_id, workspace_id, id) ON DELETE CASCADE,
    CONSTRAINT chat_session_inspections_user_fk FOREIGN KEY (user_id)
        REFERENCES public.users(id) ON DELETE CASCADE
);
ALTER TABLE public.chat_session_inspections ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.chat_session_inspections FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.chat_session_inspections
    USING (org_id = ANY(authz.current_org_ids()))
    WITH CHECK (org_id = ANY(authz.current_org_ids()));
CREATE POLICY formal_review_private_select ON public.chat_session_inspections
    AS RESTRICTIVE FOR SELECT TO g5_tenant
    USING (authz.formal_review_allows(workspace_id, false)
        AND authz.formal_review_allows(authz.formal_review_session_workspace(session_id), false));
CREATE POLICY formal_review_private_insert ON public.chat_session_inspections
    AS RESTRICTIVE FOR INSERT TO g5_tenant
    WITH CHECK (authz.formal_review_allows(workspace_id, true)
        AND authz.formal_review_allows(authz.formal_review_session_workspace(session_id), true));
CREATE POLICY formal_review_private_update ON public.chat_session_inspections
    AS RESTRICTIVE FOR UPDATE TO g5_tenant
    USING (authz.formal_review_allows(workspace_id, true)
        AND authz.formal_review_allows(authz.formal_review_session_workspace(session_id), true))
    WITH CHECK (authz.formal_review_allows(workspace_id, true)
        AND authz.formal_review_allows(authz.formal_review_session_workspace(session_id), true));
CREATE POLICY formal_review_private_delete ON public.chat_session_inspections
    AS RESTRICTIVE FOR DELETE TO g5_tenant
    USING (authz.formal_review_allows(workspace_id, true)
        AND authz.formal_review_allows(authz.formal_review_session_workspace(session_id), true));
GRANT SELECT, INSERT, UPDATE, DELETE ON public.chat_session_inspections TO g5_tenant;
GRANT SELECT ON public.chat_session_inspections TO g5_app;
