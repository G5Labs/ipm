-- External issue trackers, organization issue domains and workspace issue
-- links (docs/external-issue-integration-design.md). The tracker owns issue
-- content; G5 stores only identity, routing metadata and associations.

-- A provider namespace within an organization. Issue identity is
-- (tracker_id, external_issue_id), so reconnecting credentials or renaming a
-- repository or project never changes which issue a link names.
CREATE TABLE public.issue_trackers (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    provider text NOT NULL,
    -- github.com for GitHub; the stable cloud id for Jira Cloud.
    instance_key text NOT NULL,
    display_name text NOT NULL,
    web_base_url text NOT NULL,
    api_base_url text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT issue_trackers_pkey PRIMARY KEY (id),
    CONSTRAINT issue_trackers_org_id_id_key UNIQUE (org_id, id),
    CONSTRAINT issue_trackers_org_id_id_provider_key UNIQUE (org_id, id, provider),
    CONSTRAINT issue_trackers_instance_key UNIQUE (org_id, provider, instance_key),
    CONSTRAINT issue_trackers_provider_check CHECK (provider IN ('github', 'jira_cloud')),
    CONSTRAINT issue_trackers_text_check
        CHECK (btrim(instance_key) <> '' AND btrim(display_name) <> ''),
    CONSTRAINT issue_trackers_org_fk
        FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE
);

-- Where an organization's issues live: a GitHub repository or a Jira Cloud
-- project, named by the provider's stable id. Configuration only; it does
-- not reference G5 code repositories.
CREATE TABLE public.issue_domains (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    tracker_id uuid NOT NULL,
    provider text NOT NULL,
    container_kind text NOT NULL,
    -- GitHub numeric repository id or Jira project id, as a decimal string.
    external_container_id text NOT NULL,
    -- owner/name or project key at configuration time; display only.
    display_key text NOT NULL,
    -- Routing metadata refreshed after renames, e.g. {"fullName": "acme/web"}.
    locator jsonb DEFAULT '{}'::jsonb NOT NULL,
    is_default_create_target boolean DEFAULT false NOT NULL,
    -- Whether issue content from this domain is trusted enough for G5's full
    -- Markdown rendering (diagrams, widgets, images). Untrusted content is
    -- rendered in a restricted mode.
    content_trusted boolean DEFAULT false NOT NULL,
    configured_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT issue_domains_pkey PRIMARY KEY (id),
    CONSTRAINT issue_domains_org_id_id_key UNIQUE (org_id, id),
    CONSTRAINT issue_domains_container_key UNIQUE (tracker_id, external_container_id),
    CONSTRAINT issue_domains_kind_check CHECK (
        (provider = 'github' AND container_kind = 'github_repository')
        OR (provider = 'jira_cloud' AND container_kind = 'jira_project')
    ),
    CONSTRAINT issue_domains_container_id_check
        CHECK (external_container_id ~ '^[1-9][0-9]*$'),
    CONSTRAINT issue_domains_text_check CHECK (btrim(display_key) <> ''),
    CONSTRAINT issue_domains_locator_check CHECK (jsonb_typeof(locator) = 'object'),
    CONSTRAINT issue_domains_tracker_same_org_fk
        FOREIGN KEY (org_id, tracker_id, provider)
        REFERENCES public.issue_trackers(org_id, id, provider) ON DELETE CASCADE,
    CONSTRAINT issue_domains_configured_by_fk
        FOREIGN KEY (configured_by) REFERENCES public.users(id)
);

CREATE UNIQUE INDEX issue_domains_one_default_per_org
    ON public.issue_domains (org_id) WHERE is_default_create_target;

-- Associations between workspaces and external issues. At most one primary
-- issue per workspace and one primary workspace per issue among active
-- links. Unlinking (including workspace deletion) sets unlinked_at, which
-- releases the claim; rows remain as history.
CREATE TABLE public.workspace_issue_links (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    tracker_id uuid NOT NULL,
    -- GitHub REST issue id or Jira issue id, as a decimal string. Never the
    -- repository-local number or the Jira key.
    external_issue_id text NOT NULL,
    display_key text NOT NULL,
    canonical_url text NOT NULL,
    -- Routing metadata, e.g. {"repositoryId": "1001", "number": 12}.
    locator jsonb DEFAULT '{}'::jsonb NOT NULL,
    is_primary boolean DEFAULT false NOT NULL,
    linked_by uuid NOT NULL,
    linked_at timestamp with time zone DEFAULT now() NOT NULL,
    unlinked_by uuid,
    unlinked_at timestamp with time zone,
    last_resolved_at timestamp with time zone,
    CONSTRAINT workspace_issue_links_pkey PRIMARY KEY (id),
    CONSTRAINT workspace_issue_links_issue_id_check
        CHECK (external_issue_id ~ '^[1-9][0-9]*$'),
    CONSTRAINT workspace_issue_links_text_check
        CHECK (btrim(display_key) <> '' AND btrim(canonical_url) <> ''),
    CONSTRAINT workspace_issue_links_locator_check CHECK (jsonb_typeof(locator) = 'object'),
    CONSTRAINT workspace_issue_links_unlink_check
        CHECK ((unlinked_at IS NULL) = (unlinked_by IS NULL)),
    CONSTRAINT workspace_issue_links_tracker_same_org_fk
        FOREIGN KEY (org_id, tracker_id)
        REFERENCES public.issue_trackers(org_id, id) ON DELETE CASCADE,
    CONSTRAINT workspace_issue_links_workspace_same_org_fk
        FOREIGN KEY (org_id, workspace_id)
        REFERENCES public.workspaces(org_id, id) ON DELETE CASCADE,
    CONSTRAINT workspace_issue_links_linked_by_fk
        FOREIGN KEY (linked_by) REFERENCES public.users(id),
    CONSTRAINT workspace_issue_links_unlinked_by_fk
        FOREIGN KEY (unlinked_by) REFERENCES public.users(id)
);

CREATE UNIQUE INDEX workspace_issue_links_active_pair
    ON public.workspace_issue_links (workspace_id, tracker_id, external_issue_id)
    WHERE unlinked_at IS NULL;
CREATE UNIQUE INDEX workspace_issue_links_one_primary_per_workspace
    ON public.workspace_issue_links (workspace_id)
    WHERE is_primary AND unlinked_at IS NULL;
-- tracker_id identifies exactly one organization (composite FK above), so
-- this enforces one primary workspace per issue within the organization.
CREATE UNIQUE INDEX workspace_issue_links_one_primary_per_issue
    ON public.workspace_issue_links (tracker_id, external_issue_id)
    WHERE is_primary AND unlinked_at IS NULL;
CREATE INDEX workspace_issue_links_external_lookup
    ON public.workspace_issue_links (tracker_id, external_issue_id)
    WHERE unlinked_at IS NULL;

CREATE TRIGGER trg_issue_trackers_set_org
    BEFORE INSERT ON public.issue_trackers
    FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();
CREATE TRIGGER trg_issue_domains_set_org
    BEFORE INSERT ON public.issue_domains
    FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();
CREATE TRIGGER trg_workspace_issue_links_set_org
    BEFORE INSERT ON public.workspace_issue_links
    FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

ALTER TABLE public.issue_trackers ENABLE ROW LEVEL SECURITY;
ALTER TABLE ONLY public.issue_trackers FORCE ROW LEVEL SECURITY;
ALTER TABLE public.issue_domains ENABLE ROW LEVEL SECURITY;
ALTER TABLE ONLY public.issue_domains FORCE ROW LEVEL SECURITY;
ALTER TABLE public.workspace_issue_links ENABLE ROW LEVEL SECURITY;
ALTER TABLE ONLY public.workspace_issue_links FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON public.issue_trackers
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
CREATE POLICY tenant_isolation ON public.issue_domains
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
CREATE POLICY tenant_isolation ON public.workspace_issue_links
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));

GRANT SELECT, INSERT, UPDATE ON TABLE public.issue_trackers TO g5_tenant;
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE public.issue_domains TO g5_tenant;
-- Links are unlinked, never deleted, so history survives.
GRANT SELECT, INSERT, UPDATE ON TABLE public.workspace_issue_links TO g5_tenant;
