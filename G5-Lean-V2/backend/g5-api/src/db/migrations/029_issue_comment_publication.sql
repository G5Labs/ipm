-- Human comments on a workspace's primary external issue, posted from G5
-- under the person's own GitHub identity (docs/external-issue-integration-design.md,
-- "Comments from a workspace reviewer" and "Outbound publication and
-- audience policy").
--
-- A person connects GitHub once through the G5 GitHub App's user
-- authorization; the resulting user access token (and refresh token, when
-- GitHub issues expiring tokens) is stored encrypted. Organization admins
-- enable comments per issue domain; GitHub must also report the repository
-- as non-public at the time of each write. Every submission is recorded with
-- its exact payload before it is sent, so an ambiguous outcome is reconciled
-- rather than posted twice.

-- One GitHub App user authorization per G5 user. User-owned, not tenant data:
-- read and written only by the application role.
CREATE TABLE public.github_app_user_grants (
    user_id uuid NOT NULL,
    github_user_id bigint NOT NULL,
    github_login text NOT NULL,
    access_token_enc bytea NOT NULL,
    -- NULL when GitHub issued a non-expiring token.
    access_token_expires_at timestamp with time zone,
    refresh_token_enc bytea,
    refresh_token_expires_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT github_app_user_grants_pkey PRIMARY KEY (user_id),
    CONSTRAINT github_app_user_grants_github_user_check
        CHECK (github_user_id > 0 AND btrim(github_login) <> ''),
    CONSTRAINT github_app_user_grants_refresh_check
        CHECK (refresh_token_enc IS NOT NULL OR refresh_token_expires_at IS NULL),
    CONSTRAINT github_app_user_grants_user_fk
        FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE
);

-- A pending "Connect GitHub" authorization. The raw callback state is never
-- stored; an attempt is consumed by exactly one grant.
CREATE TABLE public.github_app_user_connection_attempts (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    state_digest bytea NOT NULL,
    -- Relative G5 path to return to after authorization.
    return_path text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    consumed_at timestamp with time zone,
    CONSTRAINT github_app_user_connection_attempts_pkey PRIMARY KEY (id),
    CONSTRAINT github_app_user_connection_attempts_state_digest_key UNIQUE (state_digest),
    CONSTRAINT github_app_user_connection_attempts_digest_check
        CHECK (octet_length(state_digest) = 32),
    CONSTRAINT github_app_user_connection_attempts_expiry_check
        CHECK (expires_at > created_at),
    CONSTRAINT github_app_user_connection_attempts_return_path_check
        CHECK (return_path ~ '^/[^/\\]' AND length(return_path) <= 2048),
    CONSTRAINT github_app_user_connection_attempts_user_fk
        FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE
);

GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE public.github_app_user_grants TO g5_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE public.github_app_user_connection_attempts TO g5_app;

-- Organization approval for G5 comments on a domain. Provider write access
-- alone is insufficient; GitHub must also report the repository as
-- non-public when each comment is sent.
ALTER TABLE public.issue_domains
    ADD COLUMN comments_enabled boolean DEFAULT false NOT NULL,
    ADD COLUMN comments_enabled_by uuid,
    ADD COLUMN comments_enabled_at timestamp with time zone,
    ADD CONSTRAINT issue_domains_comments_enabled_check CHECK (
        (comments_enabled AND comments_enabled_by IS NOT NULL AND comments_enabled_at IS NOT NULL)
        OR NOT comments_enabled
    ),
    ADD CONSTRAINT issue_domains_comments_enabled_by_fk
        FOREIGN KEY (comments_enabled_by) REFERENCES public.users(id);

-- One submitted comment: its frozen destination and payload, and what
-- GitHub did with it. `outcome_unknown` means the send may or may not have
-- happened; it is resolved by finding the payload's marker on the issue,
-- never by sending again.
CREATE TABLE public.issue_comment_publications (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    domain_id uuid NOT NULL,
    tracker_id uuid NOT NULL,
    external_issue_id text NOT NULL,
    display_key text NOT NULL,
    -- Routing metadata at submission, e.g. {"repositoryId": "1001", "number": 12}.
    locator jsonb NOT NULL,
    author_user_id uuid NOT NULL,
    github_login text NOT NULL,
    idempotency_key text NOT NULL,
    -- The exact text sent to GitHub, including the G5 marker.
    payload text NOT NULL,
    state text DEFAULT 'pending' NOT NULL,
    external_comment_id text,
    external_comment_url text,
    error jsonb,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    published_at timestamp with time zone,
    CONSTRAINT issue_comment_publications_pkey PRIMARY KEY (id),
    CONSTRAINT issue_comment_publications_idempotency_key
        UNIQUE (org_id, author_user_id, idempotency_key),
    CONSTRAINT issue_comment_publications_state_check
        CHECK (state IN ('pending', 'published', 'failed', 'outcome_unknown', 'discarded')),
    CONSTRAINT issue_comment_publications_published_check CHECK (
        (state = 'published') = (external_comment_id IS NOT NULL AND published_at IS NOT NULL)
    ),
    CONSTRAINT issue_comment_publications_issue_id_check
        CHECK (external_issue_id ~ '^[1-9][0-9]*$'),
    CONSTRAINT issue_comment_publications_comment_id_check
        CHECK (external_comment_id IS NULL OR external_comment_id ~ '^[1-9][0-9]*$'),
    CONSTRAINT issue_comment_publications_key_check CHECK (
        idempotency_key = btrim(idempotency_key)
        AND length(idempotency_key) BETWEEN 1 AND 200
    ),
    CONSTRAINT issue_comment_publications_payload_check
        CHECK (btrim(payload) <> '' AND length(payload) <= 65536),
    CONSTRAINT issue_comment_publications_locator_check
        CHECK (jsonb_typeof(locator) = 'object'),
    CONSTRAINT issue_comment_publications_workspace_same_org_fk
        FOREIGN KEY (org_id, workspace_id)
        REFERENCES public.workspaces(org_id, id) ON DELETE CASCADE,
    CONSTRAINT issue_comment_publications_domain_same_org_fk
        FOREIGN KEY (org_id, domain_id)
        REFERENCES public.issue_domains(org_id, id) ON DELETE CASCADE,
    CONSTRAINT issue_comment_publications_tracker_same_org_fk
        FOREIGN KEY (org_id, tracker_id)
        REFERENCES public.issue_trackers(org_id, id) ON DELETE CASCADE,
    CONSTRAINT issue_comment_publications_author_fk
        FOREIGN KEY (author_user_id) REFERENCES public.users(id)
);

CREATE INDEX issue_comment_publications_workspace_open
    ON public.issue_comment_publications (workspace_id, created_at)
    WHERE state IN ('pending', 'failed', 'outcome_unknown');

-- Destination, author and payload are frozen at submission.
CREATE FUNCTION public.g5_issue_comment_publication_is_frozen() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog'
    AS $$
BEGIN
    IF NEW.org_id IS DISTINCT FROM OLD.org_id
       OR NEW.workspace_id IS DISTINCT FROM OLD.workspace_id
       OR NEW.domain_id IS DISTINCT FROM OLD.domain_id
       OR NEW.tracker_id IS DISTINCT FROM OLD.tracker_id
       OR NEW.external_issue_id IS DISTINCT FROM OLD.external_issue_id
       OR NEW.locator IS DISTINCT FROM OLD.locator
       OR NEW.author_user_id IS DISTINCT FROM OLD.author_user_id
       OR NEW.github_login IS DISTINCT FROM OLD.github_login
       OR NEW.idempotency_key IS DISTINCT FROM OLD.idempotency_key
       OR NEW.payload IS DISTINCT FROM OLD.payload
       OR (OLD.state = 'published' AND NEW.state IS DISTINCT FROM OLD.state)
    THEN
        RAISE EXCEPTION 'issue comment publication is frozen' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER issue_comment_publications_frozen
    BEFORE UPDATE ON public.issue_comment_publications
    FOR EACH ROW EXECUTE FUNCTION public.g5_issue_comment_publication_is_frozen();

CREATE TRIGGER trg_issue_comment_publications_set_org
    BEFORE INSERT ON public.issue_comment_publications
    FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

ALTER TABLE public.issue_comment_publications ENABLE ROW LEVEL SECURITY;
ALTER TABLE ONLY public.issue_comment_publications FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON public.issue_comment_publications
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));

-- Publications are resolved or discarded, never deleted, so history survives.
GRANT SELECT, INSERT, UPDATE ON TABLE public.issue_comment_publications TO g5_tenant;
