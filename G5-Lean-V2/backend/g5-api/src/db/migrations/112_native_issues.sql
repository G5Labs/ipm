-- G5's native issue tracker (g5.issue.native, issue #681): one more issue
-- provider beside GitHub. The organization's native domain lives in
-- issue_domains like any other; its issues, assignees, comments and state
-- events live here, limited to the common envelope (g5.issue.native.scope).
-- Workspace links keep using workspace_issue_links: a native issue's stable
-- identity is its numeric id, so the existing link and fork checks hold.

ALTER TABLE public.issue_trackers DROP CONSTRAINT issue_trackers_provider_check;
ALTER TABLE public.issue_trackers ADD CONSTRAINT issue_trackers_provider_check
    CHECK (provider IN ('github', 'jira_cloud', 'g5'));

-- The native domain's display key is its issue key (`ACME` in `ACME-12`),
-- fixed when the domain is created; its container id is always 1.
ALTER TABLE public.issue_domains DROP CONSTRAINT issue_domains_kind_check;
ALTER TABLE public.issue_domains ADD CONSTRAINT issue_domains_kind_check CHECK (
    (provider = 'github' AND container_kind = 'github_repository')
    OR (provider = 'jira_cloud' AND container_kind = 'jira_project')
    OR (provider = 'g5' AND container_kind = 'g5_project'
        AND external_container_id = '1'
        AND display_key ~ '^[A-Z][A-Z0-9]{1,9}$')
);
-- One native domain per organization.
CREATE UNIQUE INDEX issue_domains_one_native_per_org
    ON public.issue_domains (org_id) WHERE provider = 'g5';

CREATE TABLE public.native_issues (
    id bigint GENERATED ALWAYS AS IDENTITY,
    org_id uuid NOT NULL,
    domain_id uuid NOT NULL,
    -- Per-domain display number, allocated under the domain row's lock.
    number bigint NOT NULL,
    title text NOT NULL,
    body text DEFAULT '' NOT NULL,
    state text DEFAULT 'open' NOT NULL,
    -- `completed` when closed from G5; null while open.
    state_reason text,
    parent_id bigint,
    author_user_id uuid NOT NULL,
    -- `person`, or `agent` when a workspace's agent created it as its owner.
    origin text NOT NULL,
    idempotency_key text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    -- The edit version: conditional edits compare it.
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    closed_at timestamp with time zone,
    closed_by uuid,
    search tsvector GENERATED ALWAYS AS (
        to_tsvector('english'::regconfig, title || ' ' || body)
    ) STORED,
    CONSTRAINT native_issues_pkey PRIMARY KEY (id),
    CONSTRAINT native_issues_org_id_id_key UNIQUE (org_id, id),
    CONSTRAINT native_issues_number_key UNIQUE (domain_id, number),
    CONSTRAINT native_issues_idempotency_key UNIQUE (org_id, author_user_id, idempotency_key),
    CONSTRAINT native_issues_number_check CHECK (number > 0),
    CONSTRAINT native_issues_title_check
        CHECK (btrim(title) <> '' AND char_length(title) <= 256),
    CONSTRAINT native_issues_state_check CHECK (state IN ('open', 'closed')),
    CONSTRAINT native_issues_closed_check CHECK (
        (state = 'closed') = (closed_at IS NOT NULL)
        AND (closed_at IS NULL) = (closed_by IS NULL)
    ),
    CONSTRAINT native_issues_origin_check CHECK (origin IN ('person', 'agent')),
    CONSTRAINT native_issues_key_check CHECK (btrim(idempotency_key) <> ''),
    CONSTRAINT native_issues_org_fk
        FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE,
    -- No cascade: a native domain that still has issues cannot be removed.
    CONSTRAINT native_issues_domain_fk
        FOREIGN KEY (org_id, domain_id) REFERENCES public.issue_domains(org_id, id),
    CONSTRAINT native_issues_parent_fk
        FOREIGN KEY (org_id, parent_id) REFERENCES public.native_issues(org_id, id),
    CONSTRAINT native_issues_author_fk
        FOREIGN KEY (author_user_id) REFERENCES public.users(id),
    CONSTRAINT native_issues_closed_by_fk
        FOREIGN KEY (closed_by) REFERENCES public.users(id)
);

CREATE INDEX native_issues_search ON public.native_issues USING gin (search);
CREATE INDEX native_issues_domain_updated
    ON public.native_issues (domain_id, updated_at DESC, id DESC);
CREATE INDEX native_issues_parent ON public.native_issues (parent_id)
    WHERE parent_id IS NOT NULL;

CREATE TABLE public.native_issue_assignees (
    org_id uuid NOT NULL,
    issue_id bigint NOT NULL,
    user_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT native_issue_assignees_pkey PRIMARY KEY (issue_id, user_id),
    CONSTRAINT native_issue_assignees_issue_fk
        FOREIGN KEY (org_id, issue_id) REFERENCES public.native_issues(org_id, id)
        ON DELETE CASCADE,
    CONSTRAINT native_issue_assignees_user_fk
        FOREIGN KEY (user_id) REFERENCES public.users(id)
);
CREATE INDEX native_issue_assignees_user ON public.native_issue_assignees (user_id);

CREATE TABLE public.native_issue_comments (
    id bigint GENERATED ALWAYS AS IDENTITY,
    org_id uuid NOT NULL,
    issue_id bigint NOT NULL,
    author_user_id uuid NOT NULL,
    origin text NOT NULL,
    body text NOT NULL,
    idempotency_key text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT native_issue_comments_pkey PRIMARY KEY (id),
    CONSTRAINT native_issue_comments_idempotency_key
        UNIQUE (org_id, author_user_id, idempotency_key),
    CONSTRAINT native_issue_comments_body_check CHECK (btrim(body) <> ''),
    CONSTRAINT native_issue_comments_origin_check CHECK (origin IN ('person', 'agent')),
    CONSTRAINT native_issue_comments_key_check CHECK (btrim(idempotency_key) <> ''),
    CONSTRAINT native_issue_comments_issue_fk
        FOREIGN KEY (org_id, issue_id) REFERENCES public.native_issues(org_id, id)
        ON DELETE CASCADE,
    CONSTRAINT native_issue_comments_author_fk
        FOREIGN KEY (author_user_id) REFERENCES public.users(id)
);
CREATE INDEX native_issue_comments_issue
    ON public.native_issue_comments (issue_id, created_at, id);

-- History of actual state and assignment changes, shown in Activity.
CREATE TABLE public.native_issue_events (
    id bigint GENERATED ALWAYS AS IDENTITY,
    org_id uuid NOT NULL,
    issue_id bigint NOT NULL,
    actor_user_id uuid NOT NULL,
    kind text NOT NULL,
    -- The assignee an `assigned`/`unassigned` event names.
    subject_user_id uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT native_issue_events_pkey PRIMARY KEY (id),
    CONSTRAINT native_issue_events_kind_check
        CHECK (kind IN ('closed', 'reopened', 'assigned', 'unassigned')),
    CONSTRAINT native_issue_events_subject_check CHECK (
        (kind IN ('assigned', 'unassigned')) = (subject_user_id IS NOT NULL)
    ),
    CONSTRAINT native_issue_events_issue_fk
        FOREIGN KEY (org_id, issue_id) REFERENCES public.native_issues(org_id, id)
        ON DELETE CASCADE,
    CONSTRAINT native_issue_events_actor_fk
        FOREIGN KEY (actor_user_id) REFERENCES public.users(id),
    CONSTRAINT native_issue_events_subject_fk
        FOREIGN KEY (subject_user_id) REFERENCES public.users(id)
);
CREATE INDEX native_issue_events_issue
    ON public.native_issue_events (issue_id, created_at, id);

CREATE TRIGGER trg_native_issues_set_org
    BEFORE INSERT ON public.native_issues
    FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();
CREATE TRIGGER trg_native_issue_assignees_set_org
    BEFORE INSERT ON public.native_issue_assignees
    FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();
CREATE TRIGGER trg_native_issue_comments_set_org
    BEFORE INSERT ON public.native_issue_comments
    FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();
CREATE TRIGGER trg_native_issue_events_set_org
    BEFORE INSERT ON public.native_issue_events
    FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

ALTER TABLE public.native_issues ENABLE ROW LEVEL SECURITY;
ALTER TABLE ONLY public.native_issues FORCE ROW LEVEL SECURITY;
ALTER TABLE public.native_issue_assignees ENABLE ROW LEVEL SECURITY;
ALTER TABLE ONLY public.native_issue_assignees FORCE ROW LEVEL SECURITY;
ALTER TABLE public.native_issue_comments ENABLE ROW LEVEL SECURITY;
ALTER TABLE ONLY public.native_issue_comments FORCE ROW LEVEL SECURITY;
ALTER TABLE public.native_issue_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE ONLY public.native_issue_events FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON public.native_issues
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
CREATE POLICY tenant_isolation ON public.native_issue_assignees
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
CREATE POLICY tenant_isolation ON public.native_issue_comments
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
CREATE POLICY tenant_isolation ON public.native_issue_events
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));

-- Issues and comments are never deleted, only edited or closed; events are
-- append-only.
GRANT SELECT, INSERT, UPDATE ON TABLE public.native_issues TO g5_tenant;
GRANT SELECT, INSERT, DELETE ON TABLE public.native_issue_assignees TO g5_tenant;
GRANT SELECT, INSERT, UPDATE ON TABLE public.native_issue_comments TO g5_tenant;
GRANT SELECT, INSERT ON TABLE public.native_issue_events TO g5_tenant;
