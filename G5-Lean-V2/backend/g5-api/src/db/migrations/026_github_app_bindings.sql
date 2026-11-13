-- Organization binding to the G5-owned GitHub App installation.
--
-- An org administrator installs the platform GitHub App; G5 verifies the
-- administrator's access to that installation and records append-only
-- binding snapshots. Each snapshot fixes the installation, the permissions
-- GitHub granted it, and which GitHub repositories it covers. GitHub owns
-- the installation's permissions, so an installation refresh records them
-- again when an owner accepts changes on GitHub. The latest version per org
-- is authoritative; disconnect is a new snapshot, not a delete.

-- The GitHub App an org connects through. Only the G5-owned platform app is
-- supported now; customer-owned apps will add a kind and credential custody.
CREATE TABLE public.github_app_identities (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    kind text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT github_app_identities_pkey PRIMARY KEY (id),
    CONSTRAINT github_app_identities_org_id_id_key UNIQUE (org_id, id),
    CONSTRAINT github_app_identities_org_kind_key UNIQUE (org_id, kind),
    CONSTRAINT github_app_identities_kind_check CHECK (kind IN ('platform_managed')),
    CONSTRAINT github_app_identities_org_fk
        FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE
);

CREATE TABLE public.github_app_connection_attempts (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    app_identity_id uuid NOT NULL,
    -- SHA-256 of the random callback state; the raw state is never stored.
    state_digest bytea NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    CONSTRAINT github_app_connection_attempts_pkey PRIMARY KEY (id),
    CONSTRAINT github_app_connection_attempts_org_id_id_key UNIQUE (org_id, id),
    CONSTRAINT github_app_connection_attempts_state_digest_key UNIQUE (state_digest),
    CONSTRAINT github_app_connection_attempts_digest_check
        CHECK (octet_length(state_digest) = 32),
    CONSTRAINT github_app_connection_attempts_expiry_check
        CHECK (expires_at > created_at),
    CONSTRAINT github_app_connection_attempts_org_fk
        FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE,
    CONSTRAINT github_app_connection_attempts_identity_same_org_fk
        FOREIGN KEY (org_id, app_identity_id)
        REFERENCES public.github_app_identities(org_id, id) ON DELETE RESTRICT,
    CONSTRAINT github_app_connection_attempts_created_by_fk
        FOREIGN KEY (created_by) REFERENCES public.users(id)
);

CREATE TABLE public.github_app_installation_bindings (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    app_identity_id uuid NOT NULL,
    version bigint NOT NULL,
    disposition text NOT NULL,
    installation_id bigint NOT NULL,
    account_id bigint NOT NULL,
    account_login text NOT NULL,
    -- GitHub's granted installation permissions at snapshot time,
    -- e.g. {"issues": "write", "metadata": "read"}.
    permissions jsonb NOT NULL,
    provenance text NOT NULL,
    connection_attempt_id uuid,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT github_app_installation_bindings_pkey PRIMARY KEY (id),
    CONSTRAINT github_app_installation_bindings_org_id_id_key UNIQUE (org_id, id),
    CONSTRAINT github_app_installation_bindings_org_version_key UNIQUE (org_id, version),
    -- A connection attempt is consumed by at most one binding.
    CONSTRAINT github_app_installation_bindings_attempt_key UNIQUE (connection_attempt_id),
    CONSTRAINT github_app_installation_bindings_version_check CHECK (version > 0),
    CONSTRAINT github_app_installation_bindings_disposition_check
        CHECK (disposition IN ('active', 'disconnected')),
    CONSTRAINT github_app_installation_bindings_provenance_check
        CHECK (provenance IN ('installation_callback', 'coverage_change', 'disconnect', 'reinstall',
                              'installation_refresh')),
    CONSTRAINT github_app_installation_bindings_ids_check
        CHECK (installation_id > 0 AND account_id > 0 AND btrim(account_login) <> ''),
    CONSTRAINT github_app_installation_bindings_permissions_check
        CHECK (jsonb_typeof(permissions) = 'object'),
    CONSTRAINT github_app_installation_bindings_callback_attempt_check
        CHECK (provenance NOT IN ('installation_callback', 'reinstall')
               OR connection_attempt_id IS NOT NULL),
    CONSTRAINT github_app_installation_bindings_org_fk
        FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE,
    CONSTRAINT github_app_installation_bindings_identity_same_org_fk
        FOREIGN KEY (org_id, app_identity_id)
        REFERENCES public.github_app_identities(org_id, id) ON DELETE RESTRICT,
    CONSTRAINT github_app_installation_bindings_attempt_same_org_fk
        FOREIGN KEY (org_id, connection_attempt_id)
        REFERENCES public.github_app_connection_attempts(org_id, id) ON DELETE RESTRICT,
    CONSTRAINT github_app_installation_bindings_created_by_fk
        FOREIGN KEY (created_by) REFERENCES public.users(id)
);

-- GitHub repositories covered by one binding snapshot, by GitHub's stable
-- numeric repository ID. Covered repositories need not be G5 code
-- repositories; an org's issue repository is configuration, not a G5 repo.
CREATE TABLE public.github_app_installation_binding_repositories (
    org_id uuid NOT NULL,
    binding_id uuid NOT NULL,
    github_repo_id bigint NOT NULL,
    -- owner/name at snapshot time; display metadata only.
    full_name text NOT NULL,
    CONSTRAINT github_app_installation_binding_repositories_pkey
        PRIMARY KEY (binding_id, github_repo_id),
    CONSTRAINT github_app_installation_binding_repositories_ids_check
        CHECK (github_repo_id > 0 AND btrim(full_name) <> ''),
    CONSTRAINT github_app_installation_binding_repositories_binding_same_org_fk
        FOREIGN KEY (org_id, binding_id)
        REFERENCES public.github_app_installation_bindings(org_id, id) ON DELETE CASCADE
);

CREATE INDEX idx_github_app_connection_attempts_org_expiry
    ON public.github_app_connection_attempts (org_id, expires_at);
CREATE INDEX idx_github_app_installation_bindings_latest
    ON public.github_app_installation_bindings (org_id, version DESC);
CREATE INDEX idx_github_app_installation_binding_repositories_repo
    ON public.github_app_installation_binding_repositories (org_id, github_repo_id);

CREATE TRIGGER trg_github_app_identities_set_org
    BEFORE INSERT ON public.github_app_identities
    FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();
CREATE TRIGGER trg_github_app_connection_attempts_set_org
    BEFORE INSERT ON public.github_app_connection_attempts
    FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();
CREATE TRIGGER trg_github_app_installation_bindings_set_org
    BEFORE INSERT ON public.github_app_installation_bindings
    FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();
CREATE TRIGGER trg_github_app_installation_binding_repositories_set_org
    BEFORE INSERT ON public.github_app_installation_binding_repositories
    FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

ALTER TABLE public.github_app_identities ENABLE ROW LEVEL SECURITY;
ALTER TABLE ONLY public.github_app_identities FORCE ROW LEVEL SECURITY;
ALTER TABLE public.github_app_connection_attempts ENABLE ROW LEVEL SECURITY;
ALTER TABLE ONLY public.github_app_connection_attempts FORCE ROW LEVEL SECURITY;
ALTER TABLE public.github_app_installation_bindings ENABLE ROW LEVEL SECURITY;
ALTER TABLE ONLY public.github_app_installation_bindings FORCE ROW LEVEL SECURITY;
ALTER TABLE public.github_app_installation_binding_repositories ENABLE ROW LEVEL SECURITY;
ALTER TABLE ONLY public.github_app_installation_binding_repositories FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON public.github_app_identities
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
CREATE POLICY tenant_isolation ON public.github_app_connection_attempts
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
CREATE POLICY tenant_isolation ON public.github_app_installation_bindings
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
CREATE POLICY tenant_isolation ON public.github_app_installation_binding_repositories
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));

-- Snapshots are append-only: the tenant role may read and insert, never
-- update or delete. Expired attempts are left for a later retention sweep.
GRANT SELECT, INSERT ON TABLE public.github_app_identities TO g5_tenant;
GRANT SELECT, INSERT ON TABLE public.github_app_connection_attempts TO g5_tenant;
GRANT SELECT, INSERT ON TABLE public.github_app_installation_bindings TO g5_tenant;
GRANT SELECT, INSERT ON TABLE public.github_app_installation_binding_repositories TO g5_tenant;
