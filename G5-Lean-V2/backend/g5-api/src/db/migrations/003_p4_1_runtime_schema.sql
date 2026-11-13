-- 003_p4_1_runtime_schema.sql — P4.1 additive ownership, shared-agent-session,
-- and workspace-VM schema.
--
-- This migration deliberately does not activate the P4.5 ownership/branch
-- cutover.  Legacy membership roles, workspace branch tables, task kinds,
-- dependency arrays, and question records remain readable/writable while the
-- new projections are populated and audited.  New runtime paths must fail
-- closed when an owner or mapping is unresolved.

-- -------------------------------------------------------------------------
-- Shared validation helpers
-- -------------------------------------------------------------------------

CREATE FUNCTION public.g5_git_branch_name_is_valid(value text) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    SET search_path TO 'pg_catalog'
    AS $$
SELECT value IS NOT NULL
   AND octet_length(value) BETWEEN 1 AND 255
   AND value <> '@'
   AND value !~ '[[:cntrl:] ~^:?*]'
   AND strpos(value, '[') = 0
   AND strpos(value, chr(92)) = 0
   AND strpos(value, '..') = 0
   AND strpos(value, '@{') = 0
   AND value !~ '(^|/)\.'
   AND value !~ '(^|/)($|/)'
   AND value !~ '[./]$'
   AND value !~ '(^|/)[^/]*\.lock($|/)'
$$;

CREATE FUNCTION public.g5_workspace_root_path_is_valid(value text) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    SET search_path TO 'pg_catalog'
    AS $$
SELECT value IS NOT NULL
   AND value !~ '(^|/)\.\.?(/|$)'
   AND value !~ '^/'
   AND value !~ '/$'
$$;

CREATE FUNCTION public.g5_jsonb_uuid_array_is_unique_nonempty(value jsonb)
RETURNS boolean
    LANGUAGE plpgsql IMMUTABLE PARALLEL SAFE
    SET search_path TO 'pg_catalog'
    AS $$
DECLARE
    element jsonb;
    seen uuid[] := ARRAY[]::uuid[];
    parsed uuid;
BEGIN
    IF jsonb_typeof(value) <> 'array' OR jsonb_array_length(value) = 0 THEN
        RETURN false;
    END IF;
    FOR element IN SELECT * FROM jsonb_array_elements(value)
    LOOP
        IF jsonb_typeof(element) <> 'object'
           OR jsonb_typeof(element -> 'repo_id') <> 'string' THEN
            RETURN false;
        END IF;
        BEGIN
            parsed := (element ->> 'repo_id')::uuid;
        EXCEPTION WHEN invalid_text_representation THEN
            RETURN false;
        END;
        IF parsed = ANY(seen) THEN
            RETURN false;
        END IF;
        seen := array_append(seen, parsed);
    END LOOP;
    RETURN true;
END;
$$;

CREATE FUNCTION public.g5_uuid_array_is_unique_nonempty(value uuid[])
RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    SET search_path TO 'pg_catalog'
    AS $$
SELECT value IS NOT NULL
   AND cardinality(value) > 0
   AND cardinality(value) = (SELECT count(DISTINCT item) FROM unnest(value) AS item)
$$;

-- -------------------------------------------------------------------------
-- Ownership, configuration, and access generations
-- -------------------------------------------------------------------------

ALTER TABLE public.orgs
    ADD COLUMN access_generation bigint NOT NULL DEFAULT 0
        CHECK (access_generation >= 0);

CREATE FUNCTION public.g5_access_generation_is_monotonic() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog'
    AS $$
BEGIN
    IF NEW.access_generation IS DISTINCT FROM OLD.access_generation
       AND NEW.access_generation <> OLD.access_generation + 1 THEN
        RAISE EXCEPTION 'access_generation must advance by exactly one'
            USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER orgs_access_generation_monotonic
    BEFORE UPDATE OF access_generation ON public.orgs
    FOR EACH ROW EXECUTE FUNCTION public.g5_access_generation_is_monotonic();

ALTER TABLE public.org_memberships
    ADD COLUMN owner_eligible_user_id uuid GENERATED ALWAYS AS (
        CASE WHEN role IN ('owner', 'admin', 'member') THEN user_id ELSE NULL END
    ) STORED,
    ADD CONSTRAINT org_memberships_owner_eligible_unique
        UNIQUE (org_id, owner_eligible_user_id);

ALTER TABLE public.workspaces
    ADD COLUMN owner_user_id uuid,
    ADD COLUMN active_owner_user_id uuid GENERATED ALWAYS AS (
        CASE WHEN deleted_at IS NULL THEN owner_user_id ELSE NULL END
    ) STORED,
    ADD COLUMN branch_name text,
    ADD COLUMN lifecycle_generation bigint NOT NULL DEFAULT 0
        CHECK (lifecycle_generation >= 0);

UPDATE public.workspaces AS workspace
SET owner_user_id = workspace.created_by
WHERE workspace.created_by IS NOT NULL
  AND EXISTS (
      SELECT 1
      FROM public.org_memberships AS membership
      WHERE membership.org_id = workspace.org_id
        AND membership.user_id = workspace.created_by
        AND membership.role IN ('owner', 'admin', 'member')
  );

ALTER TABLE public.workspaces
    ADD CONSTRAINT workspaces_owner_same_org_fk
    FOREIGN KEY (org_id, active_owner_user_id)
    REFERENCES public.org_memberships(org_id, owner_eligible_user_id)
    DEFERRABLE INITIALLY DEFERRED;

ALTER TABLE public.workspace_settings
    ADD COLUMN idle_timeout_secs integer NOT NULL DEFAULT 1800
        CHECK (idle_timeout_secs BETWEEN 60 AND 86400),
    ADD COLUMN response_deadline_secs integer NOT NULL DEFAULT 900
        CHECK (response_deadline_secs BETWEEN 30 AND 86400),
    ADD COLUMN retain_hours integer NOT NULL DEFAULT 24
        CHECK (retain_hours BETWEEN 0 AND 8760),
    ADD COLUMN bounded_execution_slots integer NOT NULL DEFAULT 2
        CHECK (bounded_execution_slots BETWEEN 1 AND 64),
    ADD COLUMN permission_policy jsonb
        CHECK (permission_policy IS NULL OR jsonb_typeof(permission_policy) = 'object');

CREATE TABLE public.configs (
    org_id uuid NOT NULL,
    scope text NOT NULL CHECK (scope IN ('org', 'user')),
    scope_id uuid NOT NULL,
    member_user_id uuid GENERATED ALWAYS AS (
        CASE WHEN scope = 'user' THEN scope_id ELSE NULL END
    ) STORED,
    key text NOT NULL CHECK (key = btrim(key) AND key <> ''),
    value jsonb NOT NULL,
    updated_by uuid,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (org_id, scope, scope_id, key),
    CONSTRAINT configs_org_fk FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE,
    CONSTRAINT configs_member_fk FOREIGN KEY (org_id, member_user_id)
        REFERENCES public.org_memberships(org_id, user_id) ON DELETE CASCADE,
    CONSTRAINT configs_updated_by_fk FOREIGN KEY (updated_by) REFERENCES public.users(id) ON DELETE SET NULL,
    CONSTRAINT configs_scope_identity CHECK (
        (scope = 'org' AND scope_id = org_id)
        OR scope = 'user'
    )
);

CREATE FUNCTION public.g5_workspace_egress_policy_is_valid(value jsonb)
RETURNS boolean
    LANGUAGE plpgsql IMMUTABLE PARALLEL SAFE
    SET search_path TO 'pg_catalog'
    AS $$
DECLARE
    rule jsonb;
    port jsonb;
    parsed_cidr inet;
    port_values integer[];
BEGIN
    IF jsonb_typeof(value) <> 'object'
       OR NOT value ?& ARRAY['version', 'digest', 'allow']
       OR (SELECT count(*) FROM jsonb_object_keys(value)) <> 3
       OR jsonb_typeof(value -> 'version') <> 'number'
       OR (value ->> 'version') !~ '^[1-9][0-9]*$'
       OR jsonb_typeof(value -> 'digest') <> 'string'
       OR (value ->> 'digest') !~ '^[0-9a-f]{64}$'
       OR jsonb_typeof(value -> 'allow') <> 'array' THEN
        RETURN false;
    END IF;

    FOR rule IN SELECT * FROM jsonb_array_elements(value -> 'allow')
    LOOP
        IF jsonb_typeof(rule) <> 'object'
           OR NOT rule ?& ARRAY['ipv4_cidr', 'protocol', 'ports']
           OR (SELECT count(*) FROM jsonb_object_keys(rule)) <> 3
           OR jsonb_typeof(rule -> 'ipv4_cidr') <> 'string'
           OR jsonb_typeof(rule -> 'protocol') <> 'string'
           OR rule ->> 'protocol' NOT IN ('tcp', 'udp')
           OR jsonb_typeof(rule -> 'ports') <> 'array'
           OR jsonb_array_length(rule -> 'ports') = 0 THEN
            RETURN false;
        END IF;

        BEGIN
            parsed_cidr := (rule ->> 'ipv4_cidr')::inet;
        EXCEPTION WHEN invalid_text_representation THEN
            RETURN false;
        END;
        IF family(parsed_cidr) <> 4
           OR (rule ->> 'ipv4_cidr') !~ '/[0-9]{1,2}$'
           OR masklen(parsed_cidr) NOT BETWEEN 0 AND 32
           OR parsed_cidr <> network(parsed_cidr)::inet THEN
            RETURN false;
        END IF;

        port_values := ARRAY[]::integer[];
        FOR port IN SELECT * FROM jsonb_array_elements(rule -> 'ports')
        LOOP
            IF jsonb_typeof(port) <> 'number'
               OR (port #>> '{}') !~ '^[0-9]+$'
               OR (port #>> '{}')::numeric NOT BETWEEN 1 AND 65535
               OR (port #>> '{}')::integer = ANY(port_values) THEN
                RETURN false;
            END IF;
            port_values := array_append(port_values, (port #>> '{}')::integer);
        END LOOP;
    END LOOP;
    RETURN true;
END;
$$;

CREATE FUNCTION public.g5_config_value_is_registered(
    config_scope text,
    config_key text,
    config_value jsonb
) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    SET search_path TO 'pg_catalog'
    AS $$
SELECT CASE
    WHEN config_scope = 'org' AND config_key = 'worker_host_region'
        THEN jsonb_typeof(config_value) = 'string'
             AND length(btrim(config_value #>> '{}')) BETWEEN 1 AND 128
    WHEN config_scope = 'org' AND config_key = 'max_running_vms'
        THEN jsonb_typeof(config_value) = 'number'
             AND (config_value #>> '{}') ~ '^[0-9]+$'
             AND (config_value #>> '{}')::numeric BETWEEN 0 AND 100000
    WHEN config_scope = 'org' AND config_key = 'allowed_images'
        THEN jsonb_typeof(config_value) = 'array'
             AND NOT EXISTS (
                 SELECT 1 FROM jsonb_array_elements(config_value) AS image
                 WHERE jsonb_typeof(image) <> 'string'
                    OR (image #>> '{}') !~ '^[a-z0-9._-]+(:[0-9]+)?(/[a-z0-9._-]+)+@sha256:[0-9a-f]{64}$'
             )
    WHEN config_scope = 'org' AND config_key = 'workspace_egress_policy'
        THEN public.g5_workspace_egress_policy_is_valid(config_value)
    WHEN config_scope IN ('org', 'user') AND config_key LIKE 'feature.%'
        THEN jsonb_typeof(config_value) = 'boolean'
    WHEN config_scope = 'user' AND config_key = 'agent.default_provider'
        THEN config_value #>> '{}' IN ('claude_code', 'codex')
    WHEN config_scope = 'user' AND config_key = 'agent.default_model'
        THEN jsonb_typeof(config_value) = 'string'
             AND length(btrim(config_value #>> '{}')) BETWEEN 1 AND 256
    WHEN config_scope = 'user' AND config_key = 'agent.permission_policy'
        THEN jsonb_typeof(config_value) = 'object'
    ELSE false
END
$$;

ALTER TABLE public.configs
    ADD CONSTRAINT configs_registered_key_value
    CHECK (public.g5_config_value_is_registered(scope, key, value));

CREATE FUNCTION public.g5_validate_config_scope() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog', 'public'
    AS $$
BEGIN
    IF NEW.scope = 'org' THEN
        IF NEW.scope_id <> NEW.org_id THEN
            RAISE EXCEPTION 'org config scope_id must equal org_id' USING ERRCODE = '23514';
        END IF;
    END IF;
    RETURN NULL;
END;
$$;

CREATE CONSTRAINT TRIGGER configs_same_org_scope
    AFTER INSERT OR UPDATE OF org_id, scope, scope_id ON public.configs
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION public.g5_validate_config_scope();

-- Seed only values with a defined, non-secret mapping.  Existing encrypted
-- config rows remain authoritative until their owning producers are cut over.
INSERT INTO public.configs(org_id, scope, scope_id, key, value)
SELECT id, 'org', id, 'max_running_vms', '10'::jsonb FROM public.orgs
ON CONFLICT DO NOTHING;

INSERT INTO public.configs(org_id, scope, scope_id, key, value)
SELECT id, 'org', id, 'allowed_images',
       CASE WHEN default_base_image_ref IS NULL THEN '[]'::jsonb
            ELSE jsonb_build_array(default_base_image_ref) END
FROM public.orgs
ON CONFLICT DO NOTHING;

INSERT INTO public.configs(org_id, scope, scope_id, key, value)
SELECT id, 'org', id, 'workspace_egress_policy',
       jsonb_build_object(
           'version', 1,
           'digest', encode(digest('{"allow":[],"version":1}', 'sha256'), 'hex'),
           'allow', '[]'::jsonb
       )
FROM public.orgs
ON CONFLICT DO NOTHING;

-- The same-org validator is deferred so callers can compose membership and
-- config writes in one transaction. Flush this migration's seed events before
-- later ALTER TABLE statements; PostgreSQL refuses to alter a relation with
-- pending deferred trigger events on upgrade databases that already have orgs.
SET CONSTRAINTS configs_same_org_scope IMMEDIATE;
SET CONSTRAINTS configs_same_org_scope DEFERRED;

-- Access-generation bumps are deliberately installed after the migration's
-- backfill so rollout starts at generation zero.
CREATE FUNCTION public.g5_bump_access_generation() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog'
    AS $$
DECLARE
    target_org uuid;
    affected_rows integer;
BEGIN
    target_org := COALESCE(NEW.org_id, OLD.org_id);
    UPDATE public.orgs SET access_generation = access_generation + 1 WHERE id = target_org;
    GET DIAGNOSTICS affected_rows = ROW_COUNT;
    IF affected_rows <> 1 THEN
        -- A parent org DELETE removes the org before its membership cascade
        -- reaches this trigger, so there is no surviving generation to bump.
        -- Direct membership deletes still see and must update exactly one org;
        -- RLS-filtered deletes affect no membership row and never fire here.
        IF TG_OP = 'DELETE'
           AND TG_TABLE_NAME = 'org_memberships'
           AND affected_rows = 0 THEN
            RETURN NULL;
        END IF;
        RAISE EXCEPTION 'access_generation update is outside the active organization context'
            USING ERRCODE = '42501';
    END IF;
    IF TG_OP = 'UPDATE' AND OLD.org_id IS DISTINCT FROM NEW.org_id THEN
        UPDATE public.orgs SET access_generation = access_generation + 1 WHERE id = OLD.org_id;
        GET DIAGNOSTICS affected_rows = ROW_COUNT;
        IF affected_rows <> 1 THEN
            RAISE EXCEPTION 'prior access_generation update is outside the active organization context'
                USING ERRCODE = '42501';
        END IF;
    END IF;
    RETURN NULL;
END;
$$;

CREATE TRIGGER org_memberships_bump_access_generation
    AFTER INSERT OR DELETE OR UPDATE OF role ON public.org_memberships
    FOR EACH ROW EXECUTE FUNCTION public.g5_bump_access_generation();

CREATE TRIGGER workspace_members_bump_access_generation
    AFTER INSERT OR DELETE OR UPDATE OF org_id, workspace_id, role, user_id ON public.workspace_members
    FOR EACH ROW EXECUTE FUNCTION public.g5_bump_access_generation();

CREATE TRIGGER workspaces_bump_access_generation
    AFTER UPDATE OF visibility, owner_user_id, deleted_at ON public.workspaces
    FOR EACH ROW
    WHEN (OLD.visibility IS DISTINCT FROM NEW.visibility
       OR OLD.owner_user_id IS DISTINCT FROM NEW.owner_user_id
       OR OLD.deleted_at IS DISTINCT FROM NEW.deleted_at)
    EXECUTE FUNCTION public.g5_bump_access_generation();

-- -------------------------------------------------------------------------
-- Workspace repository mapping and durable operations
-- -------------------------------------------------------------------------

CREATE TABLE public.workspace_repos (
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    repo_id uuid NOT NULL,
    root_path text NOT NULL DEFAULT '',
    role text NOT NULL DEFAULT 'code' CHECK (role IN ('code', 'docs')),
    base_branch text NOT NULL,
    head_sha_cache text,
    backfilled_from_legacy_at timestamptz,
    PRIMARY KEY (workspace_id, repo_id),
    UNIQUE (org_id, workspace_id, repo_id),
    CONSTRAINT workspace_repos_workspace_fk FOREIGN KEY (org_id, workspace_id)
        REFERENCES public.workspaces(org_id, id) ON DELETE CASCADE,
    CONSTRAINT workspace_repos_repo_fk FOREIGN KEY (org_id, repo_id)
        REFERENCES public.repos(org_id, id) ON DELETE RESTRICT
);

INSERT INTO public.workspace_repos(
    org_id, workspace_id, repo_id, root_path, role, base_branch,
    head_sha_cache, backfilled_from_legacy_at
)
SELECT DISTINCT ON (branch.workspace_id, branch.repo_id)
       branch.org_id,
       branch.workspace_id,
       branch.repo_id,
       COALESCE(binding.root_path, ''),
       CASE WHEN workspace.docs_repo_id = branch.repo_id THEN 'docs' ELSE 'code' END,
       COALESCE(binding.base_branch, branch.base_branch),
       COALESCE(NULLIF(binding.head_commit, ''), NULLIF(branch.head_commit, '')),
       now()
FROM public.workspace_branches AS branch
JOIN public.workspaces AS workspace ON workspace.id = branch.workspace_id
LEFT JOIN public.workspace_branch_repos AS binding
  ON binding.workspace_branch_id = branch.id AND binding.repo_id = branch.repo_id
WHERE branch.authority_scope_kind IS NULL OR branch.authority_scope_kind = 'workspace'
ORDER BY branch.workspace_id, branch.repo_id,
         (branch.authority_scope_kind = 'workspace') DESC, branch.id
ON CONFLICT (workspace_id, repo_id) DO UPDATE
SET root_path = EXCLUDED.root_path,
    role = EXCLUDED.role,
    base_branch = EXCLUDED.base_branch,
    head_sha_cache = EXCLUDED.head_sha_cache,
    backfilled_from_legacy_at = EXCLUDED.backfilled_from_legacy_at;

ALTER TABLE public.workspace_repos
    ADD CONSTRAINT workspace_repos_root_path CHECK (
        public.g5_workspace_root_path_is_valid(root_path)
    ) NOT VALID;

WITH common_names AS (
    SELECT branch.workspace_id,
           min(branch.branch_name) AS branch_name,
           count(DISTINCT branch.branch_name) AS distinct_names
    FROM public.workspace_branches AS branch
    WHERE branch.authority_scope_kind IS NULL OR branch.authority_scope_kind = 'workspace'
    GROUP BY branch.workspace_id
)
UPDATE public.workspaces AS workspace
SET branch_name = names.branch_name
FROM common_names AS names
WHERE names.workspace_id = workspace.id
  AND names.distinct_names = 1
  AND NOT workspace.is_base;

CREATE TABLE public.workspace_ownership_transfers (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    from_user_id uuid NOT NULL,
    to_user_id uuid NOT NULL,
    requested_by uuid NOT NULL,
    idempotency_key text NOT NULL CHECK (idempotency_key = btrim(idempotency_key) AND idempotency_key <> ''),
    request_digest text NOT NULL CHECK (request_digest ~ '^[0-9a-f]{64}$'),
    requested_at timestamptz NOT NULL DEFAULT now(),
    status text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'completed', 'aborted')),
    completed_at timestamptz,
    error jsonb,
    UNIQUE (workspace_id, requested_by, idempotency_key),
    CONSTRAINT workspace_ownership_transfers_workspace_fk FOREIGN KEY (org_id, workspace_id)
        REFERENCES public.workspaces(org_id, id) ON DELETE RESTRICT,
    CONSTRAINT workspace_ownership_transfers_from_fk FOREIGN KEY (org_id, from_user_id)
        REFERENCES public.org_memberships(org_id, owner_eligible_user_id) ON DELETE RESTRICT,
    CONSTRAINT workspace_ownership_transfers_to_fk FOREIGN KEY (org_id, to_user_id)
        REFERENCES public.org_memberships(org_id, owner_eligible_user_id) ON DELETE RESTRICT,
    CONSTRAINT workspace_ownership_transfers_requested_by_fk FOREIGN KEY (org_id, requested_by)
        REFERENCES public.org_memberships(org_id, owner_eligible_user_id) ON DELETE RESTRICT,
    CONSTRAINT workspace_ownership_transfers_distinct_owner CHECK (from_user_id <> to_user_id),
    CONSTRAINT workspace_ownership_transfers_terminal_shape CHECK (
        (status = 'completed' AND completed_at IS NOT NULL)
        OR (status <> 'completed' AND completed_at IS NULL)
    )
);

CREATE TABLE public.workspace_forks (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id uuid NOT NULL,
    source_workspace_id uuid NOT NULL,
    target_workspace_id uuid,
    requested_by uuid NOT NULL,
    owner_user_id uuid NOT NULL,
    idempotency_key text NOT NULL CHECK (idempotency_key = btrim(idempotency_key) AND idempotency_key <> ''),
    request_digest text NOT NULL CHECK (request_digest ~ '^[0-9a-f]{64}$'),
    proposed_name text NOT NULL CHECK (public.g5_git_branch_name_is_valid(proposed_name)),
    status text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'completed', 'aborting', 'aborted')),
    repo_manifest jsonb NOT NULL CHECK (public.g5_jsonb_uuid_array_is_unique_nonempty(repo_manifest)),
    source_flush_id uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    completed_at timestamptz,
    error jsonb,
    UNIQUE (org_id, requested_by, idempotency_key),
    CONSTRAINT workspace_forks_source_fk FOREIGN KEY (org_id, source_workspace_id)
        REFERENCES public.workspaces(org_id, id) ON DELETE RESTRICT,
    CONSTRAINT workspace_forks_target_fk FOREIGN KEY (org_id, target_workspace_id)
        REFERENCES public.workspaces(org_id, id) ON DELETE RESTRICT,
    CONSTRAINT workspace_forks_requester_fk FOREIGN KEY (org_id, requested_by)
        REFERENCES public.org_memberships(org_id, owner_eligible_user_id) ON DELETE RESTRICT,
    CONSTRAINT workspace_forks_owner_fk FOREIGN KEY (org_id, owner_user_id)
        REFERENCES public.org_memberships(org_id, owner_eligible_user_id) ON DELETE RESTRICT,
    CONSTRAINT workspace_forks_distinct_workspace CHECK (
        target_workspace_id IS NULL OR target_workspace_id <> source_workspace_id
    ),
    CONSTRAINT workspace_forks_terminal_shape CHECK (
        (status = 'completed' AND target_workspace_id IS NOT NULL AND completed_at IS NOT NULL)
        OR (status <> 'completed' AND completed_at IS NULL)
    )
);

CREATE INDEX workspace_forks_active_org_idx ON public.workspace_forks(org_id)
    WHERE status IN ('pending', 'aborting');

CREATE FUNCTION public.g5_workspace_fork_claim_is_immutable() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog'
    AS $$
BEGIN
    IF NEW.org_id IS DISTINCT FROM OLD.org_id
       OR NEW.source_workspace_id IS DISTINCT FROM OLD.source_workspace_id
       OR NEW.requested_by IS DISTINCT FROM OLD.requested_by
       OR NEW.owner_user_id IS DISTINCT FROM OLD.owner_user_id
       OR NEW.idempotency_key IS DISTINCT FROM OLD.idempotency_key
       OR NEW.request_digest IS DISTINCT FROM OLD.request_digest
       OR NEW.proposed_name IS DISTINCT FROM OLD.proposed_name
       OR (SELECT array_agg(member ->> 'repo_id' ORDER BY member ->> 'repo_id')
           FROM jsonb_array_elements(NEW.repo_manifest) AS member)
          IS DISTINCT FROM
          (SELECT array_agg(member ->> 'repo_id' ORDER BY member ->> 'repo_id')
           FROM jsonb_array_elements(OLD.repo_manifest) AS member)
    THEN
        RAISE EXCEPTION 'workspace fork claim identity is immutable' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER workspace_forks_immutable_claim
    BEFORE UPDATE ON public.workspace_forks
    FOR EACH ROW EXECUTE FUNCTION public.g5_workspace_fork_claim_is_immutable();

CREATE FUNCTION public.g5_validate_workspace_fork_members() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog', 'public'
    AS $$
DECLARE
    member jsonb;
BEGIN
    FOR member IN SELECT * FROM jsonb_array_elements(NEW.repo_manifest)
    LOOP
        IF NOT EXISTS (
            SELECT 1 FROM public.repos
            WHERE org_id = NEW.org_id AND id = (member ->> 'repo_id')::uuid
        ) THEN
            RAISE EXCEPTION 'workspace fork member is not an existing same-org repository'
                USING ERRCODE = '23503';
        END IF;
    END LOOP;
    RETURN NULL;
END;
$$;

CREATE CONSTRAINT TRIGGER workspace_forks_same_org_members
    AFTER INSERT OR UPDATE OF org_id, repo_manifest ON public.workspace_forks
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION public.g5_validate_workspace_fork_members();

-- -------------------------------------------------------------------------
-- Workspace VM custody, durability receipts, and host event journal
-- -------------------------------------------------------------------------

ALTER TABLE public.worker_hosts
    DROP CONSTRAINT worker_hosts_status_check;

ALTER TABLE public.worker_hosts
    ADD CONSTRAINT worker_hosts_status_check
        CHECK (status IN ('provisioning', 'ready', 'unreachable', 'draining', 'terminated')),
    ADD COLUMN kind text NOT NULL DEFAULT 'gce'
        CHECK (kind IN ('gce', 'self_hosted', 'macos_dev')),
    ADD COLUMN slots_total integer NOT NULL DEFAULT 1 CHECK (slots_total BETWEEN 1 AND 1024),
    ADD COLUMN slot_cpus integer NOT NULL DEFAULT 2 CHECK (slot_cpus BETWEEN 1 AND 1024),
    ADD COLUMN slot_mem_mib integer NOT NULL DEFAULT 4096 CHECK (slot_mem_mib BETWEEN 256 AND 1048576),
    ADD COLUMN scratch_disk_mib integer NOT NULL DEFAULT 10240 CHECK (scratch_disk_mib BETWEEN 1024 AND 1073741824),
    ADD COLUMN runtime_identity jsonb NOT NULL DEFAULT '{}'::jsonb
        CHECK (jsonb_typeof(runtime_identity) = 'object'),
    ADD COLUMN capabilities text[] NOT NULL DEFAULT '{}'::text[],
    ADD COLUMN credential_expires_at timestamptz,
    ADD COLUMN drain_requested_at timestamptz,
    ADD COLUMN drain_reason text,
    ADD COLUMN last_error jsonb,
    ADD COLUMN registration_token_hash text
        CHECK (registration_token_hash IS NULL OR registration_token_hash ~ '^[0-9a-f]{64}$'),
    ADD COLUMN registration_expires_at timestamptz,
    ADD COLUMN registration_redeemed_at timestamptz,
    ADD COLUMN registration_revoked_at timestamptz,
    ADD COLUMN credential_generation bigint NOT NULL DEFAULT 0 CHECK (credential_generation >= 0),
    ADD COLUMN identity_hash text CHECK (identity_hash IS NULL OR identity_hash ~ '^[0-9a-f]{64}$'),
    ADD COLUMN identity_changed_at timestamptz,
    ADD COLUMN image_source jsonb CHECK (image_source IS NULL OR jsonb_typeof(image_source) = 'object'),
    ADD COLUMN image_oid_vector jsonb CHECK (image_oid_vector IS NULL OR jsonb_typeof(image_oid_vector) = 'array'),
    ADD CONSTRAINT worker_hosts_registration_shape CHECK (
        (registration_token_hash IS NULL AND registration_expires_at IS NULL
            AND registration_redeemed_at IS NULL AND registration_revoked_at IS NULL)
        OR (registration_token_hash IS NOT NULL AND registration_expires_at IS NOT NULL
            AND registration_expires_at > created_at)
    );

ALTER TABLE public.worker_hosts
    ADD CONSTRAINT worker_hosts_org_host_unique UNIQUE (org_id, host_id);

CREATE FUNCTION public.g5_start_intent_is_valid(value jsonb) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    SET search_path TO 'pg_catalog'
    AS $$
SELECT value IS NULL OR (
    jsonb_typeof(value) = 'object'
    AND (value ->> 'id') ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'
    AND value ->> 'kind' IN ('resume', 'restart', 'compound_start', 'rollout_continuation')
    AND value ->> 'status' IN (
        'pending', 'queued_for_capacity', 'starting', 'consumed',
        'expired', 'cancelled', 'failed'
    )
    AND (value ->> 'owner_user_id') ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'
    AND (value ->> 'lifecycle_generation') ~ '^[0-9]+$'
    AND (value ->> 'attempts') ~ '^[0-9]+$'
    AND value ? 'created_at'
    AND value ? 'expires_at'
)
$$;

CREATE FUNCTION public.g5_preview_intent_is_valid(value jsonb) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    SET search_path TO 'pg_catalog'
    AS $$
SELECT value IS NULL OR (
    jsonb_typeof(value) = 'object'
    AND (value ->> 'generation') ~ '^[0-9]+$'
    AND value ->> 'action' IN ('start', 'replace', 'cancel')
    AND value ->> 'status' IN ('queued', 'starting', 'running', 'cancelling', 'settled', 'failed')
    AND (value ->> 'requested_by') ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'
)
$$;

CREATE TABLE public.workspace_vms (
    org_id uuid NOT NULL,
    workspace_id uuid PRIMARY KEY,
    host_id uuid,
    vm_id text,
    epoch bigint NOT NULL DEFAULT 0 CHECK (epoch >= 0),
    status text NOT NULL DEFAULT 'stopped' CHECK (status IN ('stopped', 'starting', 'running', 'stopping', 'failed')),
    image_ref text,
    image_digest text CHECK (image_digest IS NULL OR image_digest ~ '^sha256:[0-9a-f]{64}$'),
    image_source jsonb CHECK (image_source IS NULL OR jsonb_typeof(image_source) = 'object'),
    image_oid_vector jsonb CHECK (image_oid_vector IS NULL OR jsonb_typeof(image_oid_vector) = 'array'),
    module_catalog_version text,
    boot_spec jsonb CHECK (boot_spec IS NULL OR jsonb_typeof(boot_spec) = 'object'),
    source_manifest_digest text CHECK (source_manifest_digest IS NULL OR source_manifest_digest ~ '^[0-9a-f]{64}$'),
    lease_sequence bigint NOT NULL DEFAULT 0 CHECK (lease_sequence >= 0),
    lease_expires_at timestamptz,
    start_deadline_at timestamptz,
    started_at timestamptz,
    last_activity_at timestamptz,
    stop_reason text CHECK (stop_reason IS NULL OR stop_reason IN (
        'user', 'restart', 'transfer', 'idle', 'drain', 'rollout', 'credential', 'capacity'
    )),
    stop_operation_id uuid,
    stop_generation bigint CHECK (stop_generation IS NULL OR stop_generation >= 0),
    stop_retry_at timestamptz,
    stop_deadline_at timestamptz,
    pending_stop_reason text CHECK (pending_stop_reason IS NULL OR pending_stop_reason IN (
        'user', 'restart', 'transfer', 'idle', 'drain', 'rollout', 'credential', 'capacity'
    )),
    pending_stop_deadline_at timestamptz,
    nudged_at timestamptz,
    start_intent jsonb CHECK (public.g5_start_intent_is_valid(start_intent)),
    preview_generation bigint NOT NULL DEFAULT 0 CHECK (preview_generation >= 0),
    preview_intent jsonb CHECK (public.g5_preview_intent_is_valid(preview_intent)),
    idle_retry_after timestamptz,
    last_flush_id uuid,
    unflushed_loss_at timestamptz,
    session_harvest_loss_at timestamptz,
    last_error jsonb CHECK (last_error IS NULL OR jsonb_typeof(last_error) = 'object'),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (org_id, workspace_id),
    UNIQUE (vm_id),
    CONSTRAINT workspace_vms_workspace_fk FOREIGN KEY (org_id, workspace_id)
        REFERENCES public.workspaces(org_id, id) ON DELETE CASCADE,
    CONSTRAINT workspace_vms_host_fk FOREIGN KEY (org_id, host_id)
        REFERENCES public.worker_hosts(org_id, host_id) ON DELETE RESTRICT,
    CONSTRAINT workspace_vms_incarnation_shape CHECK (
        (status IN ('stopped', 'failed') AND host_id IS NULL AND vm_id IS NULL)
        OR (status IN ('starting', 'running', 'stopping')
            AND host_id IS NOT NULL AND vm_id IS NOT NULL)
    ),
    CONSTRAINT workspace_vms_stop_operation_shape CHECK (
        (stop_operation_id IS NULL AND stop_generation IS NULL AND stop_reason IS NULL
            AND stop_retry_at IS NULL AND stop_deadline_at IS NULL)
        OR (stop_operation_id IS NOT NULL AND stop_generation IS NOT NULL)
    ),
    CONSTRAINT workspace_vms_pending_stop_shape CHECK (
        (pending_stop_reason IS NULL AND pending_stop_deadline_at IS NULL AND nudged_at IS NULL)
        OR (pending_stop_reason IS NOT NULL AND pending_stop_deadline_at IS NOT NULL
            AND nudged_at IS NOT NULL AND stop_operation_id IS NOT NULL)
    ),
    CONSTRAINT workspace_vms_boot_source_pair CHECK (
        (boot_spec IS NULL) = (source_manifest_digest IS NULL)
    )
);

CREATE INDEX workspace_vms_capacity_queue_idx
    ON public.workspace_vms(org_id, ((start_intent ->> 'created_at')))
    WHERE start_intent ->> 'status' IN ('pending', 'queued_for_capacity');
CREATE INDEX workspace_vms_host_occupancy_idx ON public.workspace_vms(host_id, status)
    WHERE status IN ('starting', 'running', 'stopping');

CREATE TABLE public.workspace_flushes (
    flush_id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    origin text NOT NULL CHECK (origin IN ('flush', 'fork_copy')),
    vm_id text,
    epoch bigint,
    source_flush_id uuid,
    reason text NOT NULL CHECK (reason IN ('shutdown', 'fork')),
    required_repo_ids uuid[] NOT NULL CHECK (public.g5_uuid_array_is_unique_nonempty(required_repo_ids)),
    repos jsonb NOT NULL DEFAULT '[]'::jsonb CHECK (jsonb_typeof(repos) = 'array'),
    concurrent_writers jsonb NOT NULL DEFAULT '[]'::jsonb CHECK (jsonb_typeof(concurrent_writers) = 'array'),
    started_at timestamptz NOT NULL DEFAULT now(),
    complete_at timestamptz,
    pruned_at timestamptz,
    error jsonb,
    UNIQUE (org_id, flush_id),
    CONSTRAINT workspace_flushes_workspace_fk FOREIGN KEY (org_id, workspace_id)
        REFERENCES public.workspaces(org_id, id) ON DELETE RESTRICT,
    CONSTRAINT workspace_flushes_source_fk FOREIGN KEY (org_id, source_flush_id)
        REFERENCES public.workspace_flushes(org_id, flush_id)
        DEFERRABLE INITIALLY DEFERRED,
    CONSTRAINT workspace_flushes_origin_shape CHECK (
        (origin = 'flush' AND vm_id IS NOT NULL AND epoch IS NOT NULL AND source_flush_id IS NULL)
        OR (origin = 'fork_copy' AND vm_id IS NULL AND epoch IS NULL AND source_flush_id IS NOT NULL)
    ),
    CONSTRAINT workspace_flushes_completion_order CHECK (
        (complete_at IS NULL OR complete_at >= started_at)
        AND (pruned_at IS NULL OR complete_at IS NOT NULL AND pruned_at >= complete_at)
    )
);

ALTER TABLE public.workspace_vms
    ADD CONSTRAINT workspace_vms_last_flush_fk FOREIGN KEY (org_id, last_flush_id)
        REFERENCES public.workspace_flushes(org_id, flush_id) ON DELETE RESTRICT
        DEFERRABLE INITIALLY DEFERRED;

ALTER TABLE public.workspace_forks
    ADD CONSTRAINT workspace_forks_source_flush_fk FOREIGN KEY (org_id, source_flush_id)
        REFERENCES public.workspace_flushes(org_id, flush_id) ON DELETE RESTRICT;

CREATE UNIQUE INDEX workspace_flushes_one_active_origin_idx
    ON public.workspace_flushes(workspace_id)
    WHERE origin = 'flush' AND complete_at IS NULL AND error IS NULL;
CREATE INDEX workspace_flushes_complete_idx
    ON public.workspace_flushes(workspace_id, complete_at DESC)
    WHERE complete_at IS NOT NULL AND pruned_at IS NULL;

CREATE TABLE public.repo_mirror_refs (
    org_id uuid NOT NULL,
    repo_id uuid NOT NULL,
    ref_name text NOT NULL CHECK (ref_name = btrim(ref_name) AND ref_name <> ''),
    oid text NOT NULL CHECK (oid ~ '^([0-9a-f]{40}|[0-9a-f]{64})$'),
    provider_oid text CHECK (provider_oid IS NULL OR provider_oid ~ '^([0-9a-f]{40}|[0-9a-f]{64})$'),
    state text NOT NULL CHECK (state IN ('fresh', 'rewrite', 'provider_missing', 'fetch_failed')),
    previous_oid text CHECK (previous_oid IS NULL OR previous_oid ~ '^([0-9a-f]{40}|[0-9a-f]{64})$'),
    last_success_at timestamptz,
    last_attempt_at timestamptz,
    last_error jsonb,
    PRIMARY KEY (repo_id, ref_name),
    UNIQUE (org_id, repo_id, ref_name),
    CONSTRAINT repo_mirror_refs_repo_fk FOREIGN KEY (org_id, repo_id)
        REFERENCES public.repos(org_id, id) ON DELETE CASCADE
);

CREATE TABLE public.worker_vm_events (
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    host_id uuid NOT NULL,
    vm_id text NOT NULL,
    seq bigint NOT NULL CHECK (seq >= 0),
    kind text NOT NULL CHECK (kind = btrim(kind) AND kind <> ''),
    payload jsonb NOT NULL CHECK (jsonb_typeof(payload) = 'object'),
    at timestamptz NOT NULL,
    received_at timestamptz NOT NULL DEFAULT now(),
    acknowledged_at timestamptz,
    PRIMARY KEY (vm_id, seq),
    UNIQUE (org_id, vm_id, seq),
    CONSTRAINT worker_vm_events_workspace_fk FOREIGN KEY (org_id, workspace_id)
        REFERENCES public.workspaces(org_id, id) ON DELETE RESTRICT,
    CONSTRAINT worker_vm_events_host_fk FOREIGN KEY (org_id, host_id)
        REFERENCES public.worker_hosts(org_id, host_id) ON DELETE RESTRICT
);

CREATE INDEX worker_vm_events_workspace_received_idx
    ON public.worker_vm_events(workspace_id, received_at DESC);
CREATE INDEX worker_vm_events_unacked_idx
    ON public.worker_vm_events(host_id, received_at)
    WHERE acknowledged_at IS NULL;

-- -------------------------------------------------------------------------
-- Logical tasks, attempts, dependencies, events, and native requests
-- -------------------------------------------------------------------------

ALTER TABLE public.tasks
    ALTER COLUMN job_id DROP NOT NULL,
    ADD COLUMN workspace_id uuid,
    ADD COLUMN kind text CHECK (kind IS NULL OR kind IN ('agent', 'command', 'preview')),
    ADD COLUMN spec jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(spec) = 'object'),
    ADD COLUMN outcome jsonb CHECK (outcome IS NULL OR jsonb_typeof(outcome) = 'object'),
    ADD COLUMN attempt integer NOT NULL DEFAULT 1 CHECK (attempt > 0),
    ADD COLUMN vm_id text,
    ADD COLUMN epoch bigint CHECK (epoch IS NULL OR epoch >= 0),
    ADD COLUMN start_confirmed boolean NOT NULL DEFAULT false,
    ADD COLUMN chat_session_id uuid,
    ADD COLUMN continuation_reason text CHECK (
        continuation_reason IS NULL OR continuation_reason IN ('interrupted', 'vm_lost', 'suspended')
    ),
    ADD COLUMN hold_reason text CHECK (
        hold_reason IS NULL OR hold_reason IN (
            'owner_stop', 'idle_stop', 'transfer', 'start_required',
            'response_timeout', 'vm_lost', 'delivery_unknown'
        )
    ),
    ADD COLUMN created_by uuid,
    ADD COLUMN started_at timestamptz,
    ADD COLUMN ended_at timestamptz,
    ADD CONSTRAINT tasks_current_incarnation_shape CHECK (
        (vm_id IS NULL AND epoch IS NULL AND NOT start_confirmed)
        OR (vm_id IS NOT NULL AND epoch IS NOT NULL)
    ),
    ADD CONSTRAINT tasks_time_order CHECK (
        (started_at IS NULL OR started_at >= created_at)
        AND (ended_at IS NULL OR started_at IS NULL OR ended_at >= started_at)
    );

ALTER TABLE public.tasks DROP CONSTRAINT tasks_status_check;
ALTER TABLE public.tasks ADD CONSTRAINT tasks_status_check CHECK (status IN (
    'pending', 'claimed', 'running', 'paused', 'completed', 'failed', 'blocked',
    'cancelled', 'rolled_back', 'waiting', 'recovering', 'recovery_blocked',
    'queued', 'succeeded'
));

UPDATE public.tasks AS task
SET workspace_id = job.workspace_id,
    created_by = job.created_by,
    kind = CASE task.task_type
        WHEN 'chat' THEN 'agent'
        WHEN 'regular' THEN 'agent'
        WHEN 'sync' THEN 'agent'
        WHEN 'explainer_generate' THEN 'agent'
        WHEN 'bootstrap' THEN 'command'
        WHEN 'golden_builder' THEN 'command'
        WHEN 'preview' THEN 'preview'
        WHEN 'merge' THEN NULL
        ELSE NULL
    END,
    spec = jsonb_strip_nulls(jsonb_build_object(
        'legacy', jsonb_build_object(
            'task_type', task.task_type,
            'async_decision_policy', task.async_decision_policy
        ),
        'limits', CASE WHEN task.max_duration_secs IS NULL THEN NULL
            ELSE jsonb_build_object('wall_clock_ms', task.max_duration_secs::bigint * 1000) END
    )),
    started_at = task.assigned_at,
    ended_at = CASE WHEN task.status IN ('completed', 'failed', 'cancelled', 'rolled_back')
                    THEN task.updated_at ELSE NULL END
FROM public.jobs AS job
WHERE job.id = task.job_id;

ALTER TABLE public.tasks
    ADD CONSTRAINT tasks_org_workspace_id_unique UNIQUE (org_id, workspace_id, id),
    ADD CONSTRAINT tasks_workspace_fk FOREIGN KEY (org_id, workspace_id)
        REFERENCES public.workspaces(org_id, id) ON DELETE RESTRICT,
    ADD CONSTRAINT tasks_job_same_workspace_fk FOREIGN KEY (job_id, org_id, workspace_id)
        REFERENCES public.jobs(id, org_id, workspace_id) ON DELETE CASCADE;

ALTER TABLE public.chat_sessions
    ADD CONSTRAINT chat_sessions_org_id_unique UNIQUE (org_id, id),
    ADD CONSTRAINT chat_sessions_org_workspace_id_unique UNIQUE (org_id, workspace_id, id);

UPDATE public.tasks AS task
SET chat_session_id = candidate.session_id
FROM (
    SELECT task_id, (array_agg(id ORDER BY id))[1] AS session_id
    FROM public.chat_sessions
    WHERE task_id IS NOT NULL
    GROUP BY task_id
) AS candidate
WHERE candidate.task_id = task.id;

ALTER TABLE public.tasks
    ADD CONSTRAINT tasks_chat_session_fk FOREIGN KEY (org_id, workspace_id, chat_session_id)
        REFERENCES public.chat_sessions(org_id, workspace_id, id)
        ON DELETE SET NULL (chat_session_id);

CREATE TABLE public.task_attempts (
    org_id uuid NOT NULL,
    task_id uuid NOT NULL,
    attempt integer NOT NULL CHECK (attempt > 0),
    vm_id text,
    epoch bigint CHECK (epoch IS NULL OR epoch >= 0),
    start_confirmed boolean NOT NULL DEFAULT false,
    policy_snapshot_ref jsonb CHECK (
        policy_snapshot_ref IS NULL OR jsonb_typeof(policy_snapshot_ref) = 'object'
    ),
    session_files_state text NOT NULL DEFAULT 'absent'
        CHECK (session_files_state IN ('present', 'absent', 'persist_failed')),
    session_manifest_generation integer NOT NULL DEFAULT 0
        CHECK (session_manifest_generation >= 0),
    started_at timestamptz NOT NULL DEFAULT now(),
    ended_at timestamptz,
    outcome jsonb CHECK (outcome IS NULL OR jsonb_typeof(outcome) = 'object'),
    reason text CHECK (reason IS NULL OR reason IN (
        'exit_code', 'provider_error', 'vm_lost', 'timed_out',
        'cancelled', 'interrupted', 'suspended'
    )),
    acked_event_seq bigint NOT NULL DEFAULT 0 CHECK (acked_event_seq >= 0),
    PRIMARY KEY (task_id, attempt),
    UNIQUE (org_id, task_id, attempt),
    CONSTRAINT task_attempts_task_fk FOREIGN KEY (org_id, task_id)
        REFERENCES public.tasks(org_id, id) ON DELETE RESTRICT,
    CONSTRAINT task_attempts_incarnation_shape CHECK (
        (vm_id IS NULL AND epoch IS NULL AND NOT start_confirmed)
        OR (vm_id IS NOT NULL AND epoch IS NOT NULL)
    ),
    CONSTRAINT task_attempts_time_order CHECK (ended_at IS NULL OR ended_at >= started_at),
    CONSTRAINT task_attempts_settlement_shape CHECK (
        (ended_at IS NULL AND outcome IS NULL AND reason IS NULL)
        OR (ended_at IS NOT NULL AND reason IS NOT NULL)
    )
);

INSERT INTO public.task_attempts(
    org_id, task_id, attempt, vm_id, epoch, start_confirmed,
    policy_snapshot_ref, started_at, ended_at, outcome, reason
)
SELECT legacy.org_id,
       legacy.task_id,
       legacy.attempt_number,
       NULL,
       NULL,
       false,
       NULL,
       legacy.started_at,
       legacy.ended_at,
       CASE WHEN legacy.ended_at IS NULL THEN NULL
            ELSE jsonb_strip_nulls(jsonb_build_object(
                'legacy_attempt_id', legacy.id,
                'disposition', legacy.disposition,
                'reason', legacy.reason,
                'close_evidence', legacy.close_evidence
            )) END,
       CASE
           WHEN legacy.ended_at IS NULL THEN NULL
           WHEN legacy.disposition = 'completed' THEN 'exit_code'
           WHEN legacy.disposition = 'cancelled' THEN 'cancelled'
           WHEN legacy.disposition = 'interrupted' THEN 'interrupted'
           ELSE 'provider_error'
       END
FROM public.task_execution_attempts AS legacy
JOIN public.tasks AS task ON task.org_id = legacy.org_id AND task.id = legacy.task_id
ON CONFLICT (task_id, attempt) DO NOTHING;

UPDATE public.tasks AS task
SET attempt = latest.attempt
FROM (
    SELECT task_id, max(attempt) AS attempt
    FROM public.task_attempts
    GROUP BY task_id
) AS latest
WHERE latest.task_id = task.id;

CREATE FUNCTION public.g5_task_attempt_identity_is_immutable() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog'
    AS $$
BEGIN
    IF NEW.org_id IS DISTINCT FROM OLD.org_id
       OR NEW.task_id IS DISTINCT FROM OLD.task_id
       OR NEW.attempt IS DISTINCT FROM OLD.attempt
       OR NEW.vm_id IS DISTINCT FROM OLD.vm_id
       OR NEW.epoch IS DISTINCT FROM OLD.epoch
       OR NEW.policy_snapshot_ref IS DISTINCT FROM OLD.policy_snapshot_ref
       OR NEW.started_at IS DISTINCT FROM OLD.started_at
    THEN
        RAISE EXCEPTION 'task attempt execution identity is immutable' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER task_attempts_immutable_identity
    BEFORE UPDATE ON public.task_attempts
    FOR EACH ROW EXECUTE FUNCTION public.g5_task_attempt_identity_is_immutable();

CREATE TABLE public.task_dependencies (
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    task_id uuid NOT NULL,
    depends_on_task_id uuid NOT NULL,
    PRIMARY KEY (task_id, depends_on_task_id),
    UNIQUE (org_id, workspace_id, task_id, depends_on_task_id),
    CHECK (task_id <> depends_on_task_id),
    CONSTRAINT task_dependencies_task_fk FOREIGN KEY (org_id, workspace_id, task_id)
        REFERENCES public.tasks(org_id, workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT task_dependencies_prerequisite_fk FOREIGN KEY (org_id, workspace_id, depends_on_task_id)
        REFERENCES public.tasks(org_id, workspace_id, id) ON DELETE RESTRICT
);

CREATE INDEX task_dependencies_reverse_idx
    ON public.task_dependencies(depends_on_task_id, task_id);

CREATE FUNCTION public.g5_task_dependency_is_acyclic() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog', 'public'
    AS $$
BEGIN
    IF EXISTS (
        WITH RECURSIVE reachable(task_id) AS (
            SELECT NEW.depends_on_task_id
            UNION
            SELECT edge.depends_on_task_id
            FROM public.task_dependencies AS edge
            JOIN reachable ON edge.task_id = reachable.task_id
            WHERE edge.org_id = NEW.org_id AND edge.workspace_id = NEW.workspace_id
        )
        SELECT 1 FROM reachable WHERE task_id = NEW.task_id
    ) THEN
        RAISE EXCEPTION 'task dependency would create a cycle' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER task_dependencies_reject_cycles
    BEFORE INSERT OR UPDATE ON public.task_dependencies
    FOR EACH ROW EXECUTE FUNCTION public.g5_task_dependency_is_acyclic();

WITH RECURSIVE legacy_edges AS (
    SELECT task.org_id,
           task.workspace_id,
           task.id AS task_id,
           dependency_id AS depends_on_task_id
    FROM public.tasks AS task
    CROSS JOIN LATERAL unnest(task.depends_on) AS dependency_id
    JOIN public.tasks AS dependency
      ON dependency.id = dependency_id
     AND dependency.org_id = task.org_id
     AND dependency.workspace_id = task.workspace_id
    WHERE task.workspace_id IS NOT NULL AND dependency_id <> task.id
), reach(source_id, target_id) AS (
    SELECT task_id, depends_on_task_id FROM legacy_edges
    UNION
    SELECT reach.source_id, edge.depends_on_task_id
    FROM reach
    JOIN legacy_edges AS edge ON edge.task_id = reach.target_id
)
INSERT INTO public.task_dependencies(org_id, workspace_id, task_id, depends_on_task_id)
SELECT DISTINCT edge.org_id, edge.workspace_id, edge.task_id, edge.depends_on_task_id
FROM legacy_edges AS edge
WHERE NOT EXISTS (
    SELECT 1 FROM reach
    WHERE source_id = edge.depends_on_task_id AND target_id = edge.task_id
)
ON CONFLICT DO NOTHING;

CREATE FUNCTION public.g5_sync_legacy_task_dependencies() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog', 'public'
    AS $$
DECLARE
    dependency_id uuid;
BEGIN
    DELETE FROM public.task_dependencies WHERE task_id = NEW.id;
    IF NEW.workspace_id IS NULL THEN
        RETURN NULL;
    END IF;
    FOR dependency_id IN SELECT DISTINCT unnest(NEW.depends_on)
    LOOP
        IF dependency_id = NEW.id OR NOT EXISTS (
            SELECT 1 FROM public.tasks
            WHERE id = dependency_id AND org_id = NEW.org_id AND workspace_id = NEW.workspace_id
        ) THEN
            CONTINUE;
        END IF;
        IF EXISTS (
            WITH RECURSIVE reachable(task_id) AS (
                SELECT dependency_id
                UNION
                SELECT edge.depends_on_task_id
                FROM public.task_dependencies AS edge
                JOIN reachable ON edge.task_id = reachable.task_id
                WHERE edge.org_id = NEW.org_id AND edge.workspace_id = NEW.workspace_id
            )
            SELECT 1 FROM reachable WHERE task_id = NEW.id
        ) THEN
            CONTINUE;
        END IF;
        INSERT INTO public.task_dependencies(org_id, workspace_id, task_id, depends_on_task_id)
        VALUES (NEW.org_id, NEW.workspace_id, NEW.id, dependency_id)
        ON CONFLICT DO NOTHING;
    END LOOP;
    RETURN NULL;
END;
$$;

CREATE TRIGGER tasks_sync_legacy_dependencies
    AFTER INSERT OR UPDATE OF depends_on, workspace_id ON public.tasks
    FOR EACH ROW EXECUTE FUNCTION public.g5_sync_legacy_task_dependencies();

ALTER TABLE public.timeline_entries RENAME TO task_events;
ALTER TABLE public.task_events
    ADD COLUMN attempt integer CHECK (attempt IS NULL OR attempt > 0),
    ADD COLUMN vm_id text,
    ADD COLUMN seq bigint CHECK (seq IS NULL OR seq >= 0),
    ADD CONSTRAINT task_events_execution_sequence_unique UNIQUE (task_id, attempt, vm_id, seq),
    ADD CONSTRAINT task_events_task_same_org_fk FOREIGN KEY (org_id, task_id)
        REFERENCES public.tasks(org_id, id) ON DELETE CASCADE;

CREATE VIEW public.timeline_entries WITH (security_invoker = true) AS
SELECT id, task_id, entry_type, content, metadata, created_at, org_id
FROM public.task_events;

GRANT SELECT, INSERT, UPDATE, DELETE ON public.timeline_entries TO g5_tenant;

CREATE TABLE public.provider_requests (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id uuid NOT NULL,
    task_id uuid NOT NULL,
    attempt integer NOT NULL,
    provider_request_id text NOT NULL CHECK (
        provider_request_id = btrim(provider_request_id)
        AND length(provider_request_id) BETWEEN 1 AND 512
    ),
    kind text NOT NULL CHECK (kind IN ('question', 'permission')),
    tool_name text,
    payload jsonb NOT NULL CHECK (
        octet_length(convert_to(payload::text, 'UTF8')) <= 65536
    ),
    display jsonb NOT NULL CHECK (
        jsonb_typeof(display) = 'object'
        AND octet_length(convert_to(display::text, 'UTF8')) <= 65536
    ),
    status text NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'reply_pending', 'responded', 'invalidated')),
    response jsonb CHECK (
        response IS NULL OR (
            jsonb_typeof(response) = 'object'
            AND response ?& ARRAY['body', 'digest', 'accepted_at']
            AND (response ->> 'digest') ~ '^[0-9a-f]{64}$'
            AND octet_length(convert_to(response::text, 'UTF8')) <= 65536
        )
    ),
    response_author_id uuid,
    response_deadline_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    ended_at timestamptz,
    end_reason text,
    UNIQUE (task_id, attempt, provider_request_id),
    CONSTRAINT provider_requests_attempt_fk FOREIGN KEY (org_id, task_id, attempt)
        REFERENCES public.task_attempts(org_id, task_id, attempt) ON DELETE RESTRICT,
    CONSTRAINT provider_requests_response_author_fk FOREIGN KEY (response_author_id)
        REFERENCES public.users(id) ON DELETE RESTRICT,
    CONSTRAINT provider_requests_response_author_same_org_fk
        FOREIGN KEY (org_id, response_author_id)
        REFERENCES public.org_memberships(org_id, user_id) ON DELETE RESTRICT,
    CONSTRAINT provider_requests_state_shape CHECK (
        (status = 'pending' AND response IS NULL AND response_author_id IS NULL
            AND ended_at IS NULL AND end_reason IS NULL)
        OR (status = 'reply_pending' AND response IS NOT NULL AND response_author_id IS NOT NULL
            AND ended_at IS NULL AND end_reason IS NULL)
        OR (status = 'responded' AND response IS NOT NULL AND response_author_id IS NOT NULL
            AND ended_at IS NOT NULL AND end_reason IS NOT NULL)
        OR (status = 'invalidated' AND ended_at IS NOT NULL AND end_reason IS NOT NULL
            AND ((response IS NULL AND response_author_id IS NULL)
                 OR (response IS NOT NULL AND response_author_id IS NOT NULL)))
    ),
    CONSTRAINT provider_requests_deadline_order CHECK (response_deadline_at > created_at)
);

CREATE FUNCTION public.g5_provider_request_is_immutable() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog'
    AS $$
BEGIN
    IF NEW.org_id IS DISTINCT FROM OLD.org_id
       OR NEW.task_id IS DISTINCT FROM OLD.task_id
       OR NEW.attempt IS DISTINCT FROM OLD.attempt
       OR NEW.provider_request_id IS DISTINCT FROM OLD.provider_request_id
       OR NEW.kind IS DISTINCT FROM OLD.kind
       OR NEW.tool_name IS DISTINCT FROM OLD.tool_name
       OR NEW.payload IS DISTINCT FROM OLD.payload
       OR NEW.display IS DISTINCT FROM OLD.display
       OR NEW.response_deadline_at IS DISTINCT FROM OLD.response_deadline_at
       OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
        RAISE EXCEPTION 'provider request execution identity and bodies are immutable'
            USING ERRCODE = '23514';
    END IF;

    IF OLD.response IS NOT NULL AND (
        (NEW.response - 'delivery_evidence') IS DISTINCT FROM
            (OLD.response - 'delivery_evidence')
        OR NEW.response_author_id IS DISTINCT FROM OLD.response_author_id
    ) THEN
        RAISE EXCEPTION 'accepted provider response body, digest, and author are immutable'
            USING ERRCODE = '23514';
    END IF;

    IF (OLD.status IN ('responded', 'invalidated') AND NEW.status <> OLD.status)
       OR (OLD.status = 'reply_pending' AND NEW.status NOT IN ('reply_pending', 'responded', 'invalidated'))
       OR (OLD.status = 'pending' AND NEW.status NOT IN ('pending', 'reply_pending', 'invalidated')) THEN
        RAISE EXCEPTION 'invalid provider request state transition' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER provider_requests_immutable_identity_response
    BEFORE UPDATE ON public.provider_requests
    FOR EACH ROW EXECUTE FUNCTION public.g5_provider_request_is_immutable();

-- -------------------------------------------------------------------------
-- Shared agent sessions and the mutable FIFO inbox
-- -------------------------------------------------------------------------

ALTER TABLE public.chat_sessions
    ADD COLUMN permission_policy jsonb
        CHECK (permission_policy IS NULL OR jsonb_typeof(permission_policy) = 'object'),
    ADD COLUMN provider_session_id text CHECK (
        provider_session_id IS NULL
        OR (provider_session_id = btrim(provider_session_id)
            AND length(provider_session_id) BETWEEN 1 AND 512)
    ),
    ADD COLUMN session_manifest_generation integer NOT NULL DEFAULT 0
        CHECK (session_manifest_generation >= 0);

CREATE TABLE public.session_inputs (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id uuid NOT NULL,
    session_id uuid NOT NULL,
    input_index bigint NOT NULL CHECK (input_index >= 0),
    content text NOT NULL,
    artifacts jsonb NOT NULL DEFAULT '[]'::jsonb,
    revision integer NOT NULL DEFAULT 1 CHECK (revision > 0),
    input_origin text NOT NULL CHECK (input_origin IN ('human', 'workflow', 'continuation')),
    template_ref text,
    template_revision text,
    delivery text NOT NULL DEFAULT 'queued'
        CHECK (delivery IN ('queued', 'reserved', 'delivered', 'delivery_unknown', 'withdrawn')),
    reservation_id uuid,
    reservation_revision integer CHECK (reservation_revision IS NULL OR reservation_revision > 0),
    delivery_generation integer NOT NULL DEFAULT 0 CHECK (delivery_generation >= 0),
    task_id uuid,
    attempt integer,
    created_at timestamptz NOT NULL DEFAULT now(),
    delivered_at timestamptz,
    withdrawn_at timestamptz,
    delivery_audit jsonb NOT NULL DEFAULT '[]'::jsonb CHECK (jsonb_typeof(delivery_audit) = 'array'),
    UNIQUE (session_id, input_index),
    UNIQUE (org_id, session_id, id),
    CONSTRAINT session_inputs_session_fk FOREIGN KEY (org_id, session_id)
        REFERENCES public.chat_sessions(org_id, id) ON DELETE CASCADE,
    CONSTRAINT session_inputs_attempt_fk FOREIGN KEY (org_id, task_id, attempt)
        REFERENCES public.task_attempts(org_id, task_id, attempt) ON DELETE RESTRICT,
    CONSTRAINT session_inputs_attempt_pair CHECK ((task_id IS NULL) = (attempt IS NULL)),
    CONSTRAINT session_inputs_continuation_binding CHECK (
        input_origin <> 'continuation' OR task_id IS NOT NULL
    ),
    CONSTRAINT session_inputs_template_shape CHECK (
        (input_origin = 'workflow' AND (template_ref IS NULL) = (template_revision IS NULL))
        OR (input_origin <> 'workflow' AND template_ref IS NULL AND template_revision IS NULL)
    ),
    CONSTRAINT session_inputs_reservation_shape CHECK (
        (delivery = 'queued' AND reservation_id IS NULL AND reservation_revision IS NULL
            AND delivered_at IS NULL AND withdrawn_at IS NULL)
        OR (delivery IN ('reserved', 'delivery_unknown') AND reservation_id IS NOT NULL
            AND reservation_revision IS NOT NULL AND task_id IS NOT NULL
            AND delivered_at IS NULL AND withdrawn_at IS NULL)
        OR (delivery = 'delivered' AND reservation_id IS NOT NULL
            AND reservation_revision IS NOT NULL
            AND delivered_at IS NOT NULL AND withdrawn_at IS NULL)
        OR (delivery = 'withdrawn' AND withdrawn_at IS NOT NULL AND delivered_at IS NULL)
    )
);

CREATE UNIQUE INDEX session_inputs_one_input_per_attempt_idx
    ON public.session_inputs(task_id, attempt)
    WHERE task_id IS NOT NULL;
CREATE INDEX session_inputs_fifo_idx
    ON public.session_inputs(session_id, input_index)
    WHERE delivery IN ('queued', 'reserved', 'delivery_unknown');

CREATE FUNCTION public.g5_validate_session_input_binding() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog', 'public'
    AS $$
BEGIN
    IF NEW.task_id IS NOT NULL AND NOT EXISTS (
        SELECT 1
        FROM public.tasks AS task
        JOIN public.chat_sessions AS session
          ON session.org_id = NEW.org_id AND session.id = NEW.session_id
        WHERE task.org_id = NEW.org_id
          AND task.id = NEW.task_id
          AND task.workspace_id = session.workspace_id
          AND (task.chat_session_id IS NULL OR task.chat_session_id = NEW.session_id)
    ) THEN
        RAISE EXCEPTION 'session input task must belong to the same workspace/session'
            USING ERRCODE = '23514';
    END IF;
    RETURN NULL;
END;
$$;

CREATE CONSTRAINT TRIGGER session_inputs_same_session_task
    AFTER INSERT OR UPDATE OF org_id, session_id, task_id, attempt ON public.session_inputs
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION public.g5_validate_session_input_binding();

ALTER TABLE public.chat_messages
    ADD COLUMN input_id uuid,
    ADD COLUMN task_id uuid,
    ADD COLUMN attempt integer CHECK (attempt IS NULL OR attempt > 0),
    ADD CONSTRAINT chat_messages_input_unique UNIQUE (input_id),
    ADD CONSTRAINT chat_messages_input_identity CHECK (input_id IS NULL OR id = input_id),
    ADD CONSTRAINT chat_messages_attempt_pair CHECK ((task_id IS NULL) = (attempt IS NULL)),
    ADD CONSTRAINT chat_messages_input_fk FOREIGN KEY (org_id, session_id, input_id)
        REFERENCES public.session_inputs(org_id, session_id, id)
        DEFERRABLE INITIALLY DEFERRED,
    ADD CONSTRAINT chat_messages_attempt_fk FOREIGN KEY (org_id, task_id, attempt)
        REFERENCES public.task_attempts(org_id, task_id, attempt) ON DELETE RESTRICT;

-- A pre-P4.1 user row already present in the transcript is treated as
-- delivered unless it carries the legacy pending marker.  Pending rows reserve
-- their existing stable message UUID but remain queued until P4.7's acceptance
-- transaction links them; this never duplicates visible history.
INSERT INTO public.session_inputs(
    id, org_id, session_id, input_index, content, artifacts, revision,
    input_origin, delivery, reservation_id, reservation_revision,
    delivery_generation, task_id, attempt, created_at, delivered_at, delivery_audit
)
SELECT message.id,
       message.org_id,
       message.session_id,
       message.message_index,
       message.content,
       COALESCE(message.artifacts, '[]'::jsonb),
       1,
       CASE WHEN session.session_type = 'chat' THEN 'human' ELSE 'workflow' END,
       CASE WHEN message.status = 'pending' THEN 'queued' ELSE 'delivered' END,
       CASE WHEN message.status = 'pending' THEN NULL ELSE gen_random_uuid() END,
       CASE WHEN message.status = 'pending' THEN NULL ELSE 1 END,
       CASE WHEN message.status = 'pending' THEN 0 ELSE 1 END,
       NULL,
       NULL,
       message.created_at,
       CASE WHEN message.status = 'pending' THEN NULL ELSE message.created_at END,
       CASE WHEN message.status = 'pending' THEN '[]'::jsonb ELSE
           jsonb_build_array(jsonb_build_object('event', 'legacy_content_rewrite_pending'))
       END
FROM public.chat_messages AS message
JOIN public.chat_sessions AS session ON session.id = message.session_id
WHERE message.role = 'user';

UPDATE public.chat_messages
SET input_id = id
WHERE role = 'user' AND status <> 'pending';

ALTER TABLE public.chat_attachments
    ADD COLUMN input_id uuid,
    ADD CONSTRAINT chat_attachments_input_fk FOREIGN KEY (org_id, session_id, input_id)
        REFERENCES public.session_inputs(org_id, session_id, id)
        DEFERRABLE INITIALLY DEFERRED;

UPDATE public.chat_attachments AS attachment
SET input_id = attachment.message_id
WHERE attachment.message_id IS NOT NULL
  AND EXISTS (
      SELECT 1 FROM public.session_inputs
      WHERE id = attachment.message_id
        AND org_id = attachment.org_id
        AND session_id = attachment.session_id
  );

-- Existing transcript rows can enqueue deferred same-session checks. Resolve
-- them before the later RLS ALTER TABLE statements for the new inbox.
SET CONSTRAINTS session_inputs_same_session_task IMMEDIATE;
SET CONSTRAINTS session_inputs_same_session_task DEFERRED;

ALTER TABLE public.chat_session_files
    ADD COLUMN harvested_epoch bigint CHECK (harvested_epoch IS NULL OR harvested_epoch >= 0),
    ADD COLUMN manifest_generation integer NOT NULL DEFAULT 0 CHECK (manifest_generation >= 0),
    ADD CONSTRAINT chat_session_files_incarnation_shape CHECK (
        (harvested_from_vm_id IS NULL AND harvested_epoch IS NULL)
        OR (harvested_from_vm_id IS NOT NULL
            AND (harvested_epoch IS NOT NULL OR manifest_generation = 0))
    );

-- Accepted input rows and their delivered transcript content are immutable.
CREATE FUNCTION public.g5_session_input_delivery_is_immutable() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog'
    AS $$
DECLARE
    legacy_content_rewrite boolean;
BEGIN
    legacy_content_rewrite :=
        OLD.delivery = 'delivered'
        AND NEW.content IS DISTINCT FROM OLD.content
        AND NEW.org_id IS NOT DISTINCT FROM OLD.org_id
        AND NEW.session_id IS NOT DISTINCT FROM OLD.session_id
        AND NEW.input_index IS NOT DISTINCT FROM OLD.input_index
        AND NEW.artifacts IS NOT DISTINCT FROM OLD.artifacts
        AND NEW.revision IS NOT DISTINCT FROM OLD.revision
        AND NEW.input_origin IS NOT DISTINCT FROM OLD.input_origin
        AND NEW.template_ref IS NOT DISTINCT FROM OLD.template_ref
        AND NEW.template_revision IS NOT DISTINCT FROM OLD.template_revision
        AND NEW.reservation_id IS NOT DISTINCT FROM OLD.reservation_id
        AND NEW.reservation_revision IS NOT DISTINCT FROM OLD.reservation_revision
        AND NEW.delivery_generation IS NOT DISTINCT FROM OLD.delivery_generation
        AND NEW.task_id IS NOT DISTINCT FROM OLD.task_id
        AND NEW.attempt IS NOT DISTINCT FROM OLD.attempt
        AND NEW.delivered_at IS NOT DISTINCT FROM OLD.delivered_at
        AND OLD.delivery_audit @> '[{"event":"legacy_content_rewrite_pending"}]'::jsonb
        AND NOT OLD.delivery_audit @> '[{"event":"legacy_content_rewritten"}]'::jsonb;

    IF OLD.delivery = 'delivered' AND (
        NEW.org_id IS DISTINCT FROM OLD.org_id
        OR NEW.session_id IS DISTINCT FROM OLD.session_id
        OR NEW.input_index IS DISTINCT FROM OLD.input_index
        OR NEW.content IS DISTINCT FROM OLD.content
        OR NEW.artifacts IS DISTINCT FROM OLD.artifacts
        OR NEW.revision IS DISTINCT FROM OLD.revision
        OR NEW.input_origin IS DISTINCT FROM OLD.input_origin
        OR NEW.template_ref IS DISTINCT FROM OLD.template_ref
        OR NEW.template_revision IS DISTINCT FROM OLD.template_revision
        OR NEW.reservation_id IS DISTINCT FROM OLD.reservation_id
        OR NEW.reservation_revision IS DISTINCT FROM OLD.reservation_revision
        OR NEW.delivery_generation IS DISTINCT FROM OLD.delivery_generation
        OR NEW.task_id IS DISTINCT FROM OLD.task_id
        OR NEW.attempt IS DISTINCT FROM OLD.attempt
        OR NEW.delivered_at IS DISTINCT FROM OLD.delivered_at
        OR NOT NEW.delivery_audit @> OLD.delivery_audit
    ) THEN
        IF legacy_content_rewrite THEN
            -- P4.1 retains one old worker rewrite of its already-rendered
            -- prompt. Consume the marker so accepted content is then frozen.
            NEW.delivery_audit := OLD.delivery_audit ||
                jsonb_build_array(jsonb_build_object('event', 'legacy_content_rewritten'));
        ELSE
            RAISE EXCEPTION 'delivered session input is immutable' USING ERRCODE = '23514';
        END IF;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER session_inputs_immutable_after_delivery
    BEFORE UPDATE ON public.session_inputs
    FOR EACH ROW EXECUTE FUNCTION public.g5_session_input_delivery_is_immutable();

CREATE FUNCTION public.g5_accepted_chat_message_is_immutable() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog', 'public'
    AS $$
DECLARE
    rewritten bigint;
BEGIN
    IF OLD.input_id IS NOT NULL
       AND NEW.content IS DISTINCT FROM OLD.content
       AND NEW.id IS NOT DISTINCT FROM OLD.id
       AND NEW.org_id IS NOT DISTINCT FROM OLD.org_id
       AND NEW.session_id IS NOT DISTINCT FROM OLD.session_id
       AND NEW.input_id IS NOT DISTINCT FROM OLD.input_id
       AND NEW.role IS NOT DISTINCT FROM OLD.role
       AND NEW.artifacts IS NOT DISTINCT FROM OLD.artifacts
       AND NEW.task_id IS NOT DISTINCT FROM OLD.task_id
       AND NEW.attempt IS NOT DISTINCT FROM OLD.attempt THEN
        UPDATE public.session_inputs
        SET content = NEW.content
        WHERE id = OLD.input_id
          AND org_id = OLD.org_id
          AND session_id = OLD.session_id
          AND delivery_audit @> '[{"event":"legacy_content_rewrite_pending"}]'::jsonb
          AND NOT delivery_audit @> '[{"event":"legacy_content_rewritten"}]'::jsonb;
        GET DIAGNOSTICS rewritten = ROW_COUNT;
        IF rewritten = 1 THEN
            RETURN NEW;
        END IF;
    END IF;

    IF OLD.input_id IS NOT NULL AND (
        NEW.id IS DISTINCT FROM OLD.id
        OR NEW.org_id IS DISTINCT FROM OLD.org_id
        OR NEW.session_id IS DISTINCT FROM OLD.session_id
        OR NEW.input_id IS DISTINCT FROM OLD.input_id
        OR NEW.role IS DISTINCT FROM OLD.role
        OR NEW.content IS DISTINCT FROM OLD.content
        OR NEW.artifacts IS DISTINCT FROM OLD.artifacts
        OR NEW.task_id IS DISTINCT FROM OLD.task_id
        OR NEW.attempt IS DISTINCT FROM OLD.attempt
    ) THEN
        RAISE EXCEPTION 'accepted input transcript message is immutable' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER chat_messages_immutable_accepted_input
    BEFORE UPDATE ON public.chat_messages
    FOR EACH ROW EXECUTE FUNCTION public.g5_accepted_chat_message_is_immutable();

CREATE FUNCTION public.g5_project_legacy_chat_message_input() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog', 'public'
    AS $$
DECLARE
    origin text;
    reservation uuid;
BEGIN
    IF NEW.role <> 'user' OR NEW.input_id IS NOT NULL THEN
        RETURN NULL;
    END IF;
    SELECT CASE WHEN session_type = 'chat' THEN 'human' ELSE 'workflow' END
    INTO origin FROM public.chat_sessions WHERE id = NEW.session_id;

    IF NEW.status = 'pending' THEN
        INSERT INTO public.session_inputs(
            id, org_id, session_id, input_index, content, artifacts,
            input_origin, delivery, created_at
        ) VALUES (
            NEW.id, NEW.org_id, NEW.session_id, NEW.message_index, NEW.content,
            COALESCE(NEW.artifacts, '[]'::jsonb), origin, 'queued', NEW.created_at
        ) ON CONFLICT (id) DO NOTHING;
    ELSE
        reservation := gen_random_uuid();
        INSERT INTO public.session_inputs(
            id, org_id, session_id, input_index, content, artifacts,
            input_origin, delivery, reservation_id, reservation_revision,
            delivery_generation, created_at, delivered_at, delivery_audit
        ) VALUES (
            NEW.id, NEW.org_id, NEW.session_id, NEW.message_index, NEW.content,
            COALESCE(NEW.artifacts, '[]'::jsonb), origin, 'delivered', reservation,
            1, 1, NEW.created_at, NEW.created_at,
            jsonb_build_array(jsonb_build_object('event', 'legacy_content_rewrite_pending'))
        ) ON CONFLICT (id) DO NOTHING;
        UPDATE public.chat_messages SET input_id = id WHERE id = NEW.id AND input_id IS NULL;
    END IF;
    RETURN NULL;
END;
$$;

CREATE TRIGGER chat_messages_project_legacy_input_insert
    AFTER INSERT ON public.chat_messages
    FOR EACH ROW EXECUTE FUNCTION public.g5_project_legacy_chat_message_input();

CREATE FUNCTION public.g5_settle_legacy_pending_chat_input() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog', 'public'
    AS $$
BEGIN
    IF OLD.role = 'user' AND OLD.status = 'pending' AND NEW.status <> 'pending'
       AND NEW.input_id IS NULL THEN
        UPDATE public.session_inputs
        SET delivery = 'delivered',
            reservation_id = gen_random_uuid(),
            reservation_revision = revision,
            delivery_generation = GREATEST(delivery_generation, 1),
            delivered_at = now(),
            delivery_audit = delivery_audit ||
                jsonb_build_array(jsonb_build_object('event', 'legacy_content_rewrite_pending'))
        WHERE id = NEW.id AND delivery = 'queued';
        UPDATE public.chat_messages SET input_id = id WHERE id = NEW.id AND input_id IS NULL;
    END IF;
    RETURN NULL;
END;
$$;

CREATE TRIGGER chat_messages_settle_legacy_pending_input
    AFTER UPDATE OF status ON public.chat_messages
    FOR EACH ROW EXECUTE FUNCTION public.g5_settle_legacy_pending_chat_input();

CREATE FUNCTION public.g5_link_legacy_attachment_input() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog', 'public'
    AS $$
BEGIN
    IF NEW.input_id IS NULL AND NEW.message_id IS NOT NULL AND EXISTS (
        SELECT 1 FROM public.session_inputs
        WHERE id = NEW.message_id AND org_id = NEW.org_id AND session_id = NEW.session_id
    ) THEN
        NEW.input_id := NEW.message_id;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER chat_attachments_link_legacy_input
    BEFORE INSERT OR UPDATE OF message_id ON public.chat_attachments
    FOR EACH ROW EXECUTE FUNCTION public.g5_link_legacy_attachment_input();

-- -------------------------------------------------------------------------
-- Compatibility projections and migration-time invariant checks
-- -------------------------------------------------------------------------

CREATE FUNCTION public.g5_project_legacy_workspace_branch() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog', 'public'
    AS $$
DECLARE
    branch_row public.workspace_branches%ROWTYPE;
    affected_workspace uuid;
    common_name text;
    distinct_names bigint;
BEGIN
    IF TG_OP = 'DELETE' THEN
        branch_row := OLD;
    ELSE
        branch_row := NEW;
    END IF;
    affected_workspace := branch_row.workspace_id;

    IF branch_row.authority_scope_kind IS NULL OR branch_row.authority_scope_kind = 'workspace' THEN
        IF TG_OP = 'DELETE' THEN
            DELETE FROM public.workspace_repos
            WHERE workspace_id = branch_row.workspace_id
              AND repo_id = branch_row.repo_id
              AND NOT EXISTS (
                  SELECT 1 FROM public.workspace_branches AS survivor
                  WHERE survivor.workspace_id = branch_row.workspace_id
                    AND survivor.repo_id = branch_row.repo_id
                    AND (survivor.authority_scope_kind IS NULL
                         OR survivor.authority_scope_kind = 'workspace')
              );
        ELSIF EXISTS (
            SELECT 1
            FROM public.workspace_branch_repos AS binding
            WHERE binding.workspace_branch_id = branch_row.id
              AND binding.repo_id = branch_row.repo_id
              AND NOT public.g5_workspace_root_path_is_valid(binding.root_path)
        ) THEN
            -- Keep the legacy producer writable, but remove its invalid
            -- projection so P4.1 consumers fail closed on a missing mapping.
            DELETE FROM public.workspace_repos
            WHERE workspace_id = branch_row.workspace_id
              AND repo_id = branch_row.repo_id;
        ELSE
            INSERT INTO public.workspace_repos(
                org_id, workspace_id, repo_id, root_path, role, base_branch,
                head_sha_cache, backfilled_from_legacy_at
            )
            SELECT branch_row.org_id,
                   branch_row.workspace_id,
                   branch_row.repo_id,
                   COALESCE(binding.root_path, ''),
                   CASE WHEN workspace.docs_repo_id = branch_row.repo_id THEN 'docs' ELSE 'code' END,
                   COALESCE(binding.base_branch, branch_row.base_branch),
                   COALESCE(NULLIF(binding.head_commit, ''), NULLIF(branch_row.head_commit, '')),
                   now()
            FROM public.workspaces AS workspace
            LEFT JOIN public.workspace_branch_repos AS binding
              ON binding.workspace_branch_id = branch_row.id
             AND binding.repo_id = branch_row.repo_id
            WHERE workspace.id = branch_row.workspace_id
            ON CONFLICT (workspace_id, repo_id) DO UPDATE
            SET root_path = EXCLUDED.root_path,
                role = EXCLUDED.role,
                base_branch = EXCLUDED.base_branch,
                head_sha_cache = EXCLUDED.head_sha_cache,
                backfilled_from_legacy_at = EXCLUDED.backfilled_from_legacy_at;
        END IF;
    END IF;

    SELECT min(branch_name), count(DISTINCT branch_name)
    INTO common_name, distinct_names
    FROM public.workspace_branches
    WHERE workspace_id = affected_workspace
      AND (authority_scope_kind IS NULL OR authority_scope_kind = 'workspace');

    -- Base views intentionally retain per-repository protected/default names
    -- and have no common working destination. Avoid a redundant parent update
    -- so system-role org provisioning needs no new workspace UPDATE grant.
    IF EXISTS (
        SELECT 1 FROM public.workspaces
        WHERE id = affected_workspace AND NOT is_base
    ) THEN
        UPDATE public.workspaces
        SET branch_name = CASE WHEN distinct_names = 1 THEN common_name ELSE NULL END
        WHERE id = affected_workspace;
    END IF;
    RETURN NULL;
END;
$$;

CREATE TRIGGER workspace_branches_project_v9_mapping
    AFTER INSERT OR DELETE OR UPDATE OF org_id, workspace_id, repo_id, branch_name,
        base_branch, head_commit, authority_scope_kind
    ON public.workspace_branches
    FOR EACH ROW EXECUTE FUNCTION public.g5_project_legacy_workspace_branch();

CREATE FUNCTION public.g5_project_legacy_workspace_binding() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog', 'public'
    AS $$
DECLARE
    binding_row public.workspace_branch_repos%ROWTYPE;
BEGIN
    IF TG_OP = 'DELETE' THEN
        binding_row := OLD;
    ELSE
        binding_row := NEW;
    END IF;

    IF TG_OP <> 'DELETE'
       AND NOT public.g5_workspace_root_path_is_valid(binding_row.root_path) THEN
        -- Legacy rows historically accepted paths that P4.1 cannot mount.
        -- Do not reject the old write, and never retain a stale valid mapping.
        DELETE FROM public.workspace_repos AS mapping
        USING public.workspace_branches AS branch
        WHERE branch.id = binding_row.workspace_branch_id
          AND branch.repo_id = binding_row.repo_id
          AND mapping.workspace_id = branch.workspace_id
          AND mapping.repo_id = branch.repo_id;
        RETURN NULL;
    END IF;

    INSERT INTO public.workspace_repos(
        org_id, workspace_id, repo_id, root_path, role, base_branch,
        head_sha_cache, backfilled_from_legacy_at
    )
    SELECT branch.org_id,
           branch.workspace_id,
           branch.repo_id,
           CASE WHEN TG_OP = 'DELETE' THEN '' ELSE binding_row.root_path END,
           CASE WHEN workspace.docs_repo_id = branch.repo_id THEN 'docs' ELSE 'code' END,
           CASE WHEN TG_OP = 'DELETE' THEN branch.base_branch ELSE binding_row.base_branch END,
           CASE WHEN TG_OP = 'DELETE' THEN NULLIF(branch.head_commit, '')
                ELSE COALESCE(NULLIF(binding_row.head_commit, ''), NULLIF(branch.head_commit, '')) END,
           now()
    FROM public.workspace_branches AS branch
    JOIN public.workspaces AS workspace ON workspace.id = branch.workspace_id
    WHERE branch.id = binding_row.workspace_branch_id
      AND branch.repo_id = binding_row.repo_id
      AND (branch.authority_scope_kind IS NULL OR branch.authority_scope_kind = 'workspace')
    ON CONFLICT (workspace_id, repo_id) DO UPDATE
    SET root_path = EXCLUDED.root_path,
        role = EXCLUDED.role,
        base_branch = EXCLUDED.base_branch,
        head_sha_cache = EXCLUDED.head_sha_cache,
        backfilled_from_legacy_at = EXCLUDED.backfilled_from_legacy_at;
    RETURN NULL;
END;
$$;

CREATE TRIGGER workspace_branch_repos_project_v9_mapping
    AFTER INSERT OR DELETE OR UPDATE OF org_id, workspace_branch_id, repo_id,
        root_path, base_branch, head_commit
    ON public.workspace_branch_repos
    FOR EACH ROW EXECUTE FUNCTION public.g5_project_legacy_workspace_binding();

CREATE FUNCTION public.g5_resolve_eligible_workspace_creator() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog', 'public'
    AS $$
BEGIN
    IF NEW.owner_user_id IS NULL AND NEW.created_by IS NOT NULL THEN
        IF NEW.org_id = ANY (authz.current_org_ids()) THEN
            -- Runtime producers already hold an explicit org context; the
            -- deferred same-org owner FK remains the eligibility authority.
            NEW.owner_user_id := NEW.created_by;
        ELSIF has_table_privilege(
            current_user, 'public.org_memberships', 'SELECT'
        ) THEN
            -- Migration/admin fixtures may be intentionally unscoped, but
            -- only an independently verified eligible creator is projected.
            IF EXISTS (
                SELECT 1
                FROM public.org_memberships AS membership
                WHERE membership.org_id = NEW.org_id
                  AND membership.user_id = NEW.created_by
                  AND membership.role IN ('owner', 'admin', 'member')
            ) THEN
                NEW.owner_user_id := NEW.created_by;
            END IF;
        END IF;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER workspaces_resolve_eligible_creator
    BEFORE INSERT OR UPDATE OF created_by, org_id ON public.workspaces
    FOR EACH ROW EXECUTE FUNCTION public.g5_resolve_eligible_workspace_creator();

CREATE TRIGGER workspaces_insert_bump_access_generation
    AFTER INSERT ON public.workspaces
    FOR EACH ROW EXECUTE FUNCTION public.g5_bump_access_generation();

CREATE FUNCTION public.g5_project_legacy_task_fields() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog', 'public'
    AS $$
DECLARE
    parent_workspace uuid;
    parent_creator uuid;
    mapped_kind text;
    legacy_spec jsonb;
BEGIN
    IF NEW.job_id IS NOT NULL THEN
        SELECT workspace_id, created_by INTO parent_workspace, parent_creator
        FROM public.jobs WHERE id = NEW.job_id AND org_id = NEW.org_id;
        NEW.workspace_id := COALESCE(NEW.workspace_id, parent_workspace);
        NEW.created_by := COALESCE(NEW.created_by, parent_creator);
    END IF;

    mapped_kind := CASE NEW.task_type
        WHEN 'chat' THEN 'agent'
        WHEN 'regular' THEN 'agent'
        WHEN 'sync' THEN 'agent'
        WHEN 'explainer_generate' THEN 'agent'
        WHEN 'bootstrap' THEN 'command'
        WHEN 'golden_builder' THEN 'command'
        WHEN 'preview' THEN 'preview'
        ELSE NULL
    END;
    NEW.kind := COALESCE(NEW.kind, mapped_kind);

    IF NEW.spec = '{}'::jsonb OR NOT (NEW.spec ? 'legacy') THEN
        legacy_spec := jsonb_build_object(
            'task_type', NEW.task_type,
            'async_decision_policy', NEW.async_decision_policy
        );
        NEW.spec := jsonb_set(NEW.spec, '{legacy}', legacy_spec, true);
    END IF;
    IF NEW.max_duration_secs IS NOT NULL
       AND NOT (COALESCE(NEW.spec -> 'limits', '{}'::jsonb) ? 'wall_clock_ms') THEN
        NEW.spec := jsonb_set(
            NEW.spec,
            '{limits}',
            COALESCE(NEW.spec -> 'limits', '{}'::jsonb)
                || jsonb_build_object('wall_clock_ms', NEW.max_duration_secs::bigint * 1000),
            true
        );
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER tasks_project_legacy_target_fields
    BEFORE INSERT OR UPDATE OF job_id, task_type, max_duration_secs, async_decision_policy
    ON public.tasks
    FOR EACH ROW EXECUTE FUNCTION public.g5_project_legacy_task_fields();

CREATE FUNCTION public.g5_link_legacy_task_session() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog', 'public'
    AS $$
BEGIN
    IF NEW.task_id IS NOT NULL THEN
        UPDATE public.tasks
        SET chat_session_id = NEW.id
        WHERE id = NEW.task_id
          AND org_id = NEW.org_id
          AND workspace_id = NEW.workspace_id
          AND chat_session_id IS NULL;
    END IF;
    RETURN NULL;
END;
$$;

CREATE TRIGGER chat_sessions_project_legacy_task_link
    AFTER INSERT OR UPDATE OF task_id ON public.chat_sessions
    FOR EACH ROW EXECUTE FUNCTION public.g5_link_legacy_task_session();

CREATE FUNCTION public.g5_project_legacy_execution_attempt() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog', 'public'
    AS $$
DECLARE
    projected_rows bigint;
BEGIN
    IF TG_OP = 'DELETE' OR (
        TG_OP = 'UPDATE' AND NEW.started_at IS DISTINCT FROM OLD.started_at
    ) THEN
        DELETE FROM public.task_attempts AS projected
        WHERE projected.org_id = OLD.org_id
          AND projected.task_id = OLD.task_id
          AND projected.attempt = OLD.attempt_number
          AND projected.vm_id IS NULL
          AND projected.epoch IS NULL
          AND NOT projected.start_confirmed
          AND projected.policy_snapshot_ref IS NULL
          AND projected.session_files_state = 'absent'
          AND projected.session_manifest_generation = 0
          AND projected.acked_event_seq = 0
          AND (
              projected.outcome ->> 'legacy_attempt_id' = OLD.id::text
              OR (projected.outcome IS NULL AND OLD.ended_at IS NULL)
          );

        IF TG_OP = 'DELETE' THEN
            RETURN NULL;
        END IF;
    END IF;

    INSERT INTO public.task_attempts(
        org_id, task_id, attempt, started_at, ended_at, outcome, reason
    ) VALUES (
        NEW.org_id,
        NEW.task_id,
        NEW.attempt_number,
        NEW.started_at,
        NEW.ended_at,
        CASE WHEN NEW.ended_at IS NULL THEN NULL ELSE jsonb_strip_nulls(jsonb_build_object(
            'legacy_attempt_id', NEW.id,
            'disposition', NEW.disposition,
            'reason', NEW.reason,
            'close_evidence', NEW.close_evidence
        )) END,
        CASE
            WHEN NEW.ended_at IS NULL THEN NULL
            WHEN NEW.disposition = 'completed' THEN 'exit_code'
            WHEN NEW.disposition = 'cancelled' THEN 'cancelled'
            WHEN NEW.disposition = 'interrupted' THEN 'interrupted'
            ELSE 'provider_error'
        END
    )
    ON CONFLICT (task_id, attempt) DO UPDATE
    SET ended_at = EXCLUDED.ended_at,
        outcome = EXCLUDED.outcome,
        reason = EXCLUDED.reason
    WHERE task_attempts.org_id = EXCLUDED.org_id
      AND task_attempts.vm_id IS NULL
      AND task_attempts.epoch IS NULL
      AND NOT task_attempts.start_confirmed
      AND task_attempts.policy_snapshot_ref IS NULL
      AND task_attempts.session_files_state = 'absent'
      AND task_attempts.session_manifest_generation = 0
      AND task_attempts.acked_event_seq = 0
      AND (
          task_attempts.outcome ->> 'legacy_attempt_id' = NEW.id::text
          OR task_attempts.outcome IS NULL
      );

    GET DIAGNOSTICS projected_rows = ROW_COUNT;
    IF projected_rows <> 1 THEN
        RAISE EXCEPTION 'legacy execution attempt conflicts with a target-native attempt'
            USING ERRCODE = '55000';
    END IF;

    UPDATE public.tasks
    SET attempt = GREATEST(attempt, NEW.attempt_number)
    WHERE id = NEW.task_id AND org_id = NEW.org_id;
    RETURN NULL;
END;
$$;

CREATE TRIGGER task_execution_attempts_project_v9_attempt
    AFTER INSERT OR DELETE OR UPDATE OF disposition, reason, started_at, ended_at, close_evidence
    ON public.task_execution_attempts
    FOR EACH ROW EXECUTE FUNCTION public.g5_project_legacy_execution_attempt();

CREATE FUNCTION public.g5_validate_runtime_intent_binding() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog', 'public'
    AS $$
DECLARE
    workspace_owner uuid;
    workspace_generation bigint;
BEGIN
    SELECT owner_user_id, lifecycle_generation
    INTO workspace_owner, workspace_generation
    FROM public.workspaces
    WHERE org_id = NEW.org_id AND id = NEW.workspace_id;

    IF NEW.start_intent IS NOT NULL
       AND NEW.start_intent ->> 'status' IN ('pending', 'queued_for_capacity', 'starting')
       AND (
           workspace_owner IS NULL
           OR (NEW.start_intent ->> 'owner_user_id')::uuid <> workspace_owner
           OR (NEW.start_intent ->> 'lifecycle_generation')::bigint <> workspace_generation
           OR (NEW.start_intent ->> 'expires_at')::timestamptz <= now()
           OR (NEW.start_intent ->> 'created_at')::timestamptz >=
              (NEW.start_intent ->> 'expires_at')::timestamptz
       )
    THEN
        RAISE EXCEPTION 'active start intent must bind the current owner and lifecycle generation'
            USING ERRCODE = '23514';
    END IF;
    RETURN NULL;
END;
$$;

CREATE CONSTRAINT TRIGGER workspace_vms_validate_start_intent
    AFTER INSERT OR UPDATE OF org_id, workspace_id, start_intent ON public.workspace_vms
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION public.g5_validate_runtime_intent_binding();

CREATE FUNCTION public.g5_workspace_vm_identity_is_fenced() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog'
    AS $$
BEGIN
    IF OLD.vm_id IS NOT NULL AND NEW.vm_id = OLD.vm_id AND NEW.epoch = OLD.epoch THEN
        IF OLD.boot_spec IS NOT NULL AND (
            NEW.boot_spec IS DISTINCT FROM OLD.boot_spec
            OR NEW.source_manifest_digest IS DISTINCT FROM OLD.source_manifest_digest
        ) THEN
            RAISE EXCEPTION 'boot source is immutable within an incarnation' USING ERRCODE = '23514';
        END IF;
        IF OLD.stop_operation_id IS NOT NULL
           AND NEW.stop_operation_id IS DISTINCT FROM OLD.stop_operation_id THEN
            RAISE EXCEPTION 'unreleased stop operation cannot be overwritten' USING ERRCODE = '23514';
        END IF;
    END IF;

    IF OLD.start_intent IS NOT NULL
       AND OLD.start_intent ->> 'status' IN ('pending', 'queued_for_capacity', 'starting')
       AND NEW.start_intent IS NOT NULL
       AND NEW.start_intent ->> 'id' IS DISTINCT FROM OLD.start_intent ->> 'id'
       AND NEW.start_intent ->> 'supersedes_intent_id' IS DISTINCT FROM OLD.start_intent ->> 'id'
    THEN
        RAISE EXCEPTION 'active start intent requires explicit fenced supersession'
            USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER workspace_vms_fence_identity
    BEFORE UPDATE ON public.workspace_vms
    FOR EACH ROW EXECUTE FUNCTION public.g5_workspace_vm_identity_is_fenced();

CREATE FUNCTION public.g5_validate_flush_repositories() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog', 'public'
    AS $$
DECLARE
    required_repo_id uuid;
BEGIN
    FOREACH required_repo_id IN ARRAY NEW.required_repo_ids
    LOOP
        IF NOT EXISTS (
            SELECT 1 FROM public.workspace_repos AS mapped
            WHERE mapped.org_id = NEW.org_id
              AND mapped.workspace_id = NEW.workspace_id
              AND mapped.repo_id = required_repo_id
        ) THEN
            RAISE EXCEPTION 'flush repository is not a same-org workspace member'
                USING ERRCODE = '23503';
        END IF;
    END LOOP;
    RETURN NULL;
END;
$$;

CREATE CONSTRAINT TRIGGER workspace_flushes_same_org_members
    AFTER INSERT OR UPDATE OF org_id, workspace_id, required_repo_ids ON public.workspace_flushes
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION public.g5_validate_flush_repositories();

CREATE FUNCTION public.g5_reject_direct_compat_projection_write() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog'
    AS $$
BEGIN
    IF pg_trigger_depth() <= 1 THEN
        RAISE EXCEPTION '% is a legacy compatibility projection until its producer cutover', TG_TABLE_NAME
            USING ERRCODE = '55000';
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER workspace_repos_legacy_authority_guard
    BEFORE INSERT OR UPDATE OR DELETE ON public.workspace_repos
    FOR EACH ROW EXECUTE FUNCTION public.g5_reject_direct_compat_projection_write();

CREATE TRIGGER task_dependencies_legacy_authority_guard
    BEFORE INSERT OR UPDATE OR DELETE ON public.task_dependencies
    FOR EACH ROW EXECUTE FUNCTION public.g5_reject_direct_compat_projection_write();

CREATE VIEW public.p4_1_workspace_migration_preflight WITH (security_invoker = true) AS
WITH legacy_branch_summary AS (
    SELECT workspace_id,
           count(*) AS branch_rows,
           count(DISTINCT repo_id) AS repo_count,
           count(DISTINCT branch_name) AS branch_name_count,
           jsonb_agg(DISTINCT branch_name ORDER BY branch_name) AS branch_names,
           bool_and(public.g5_git_branch_name_is_valid(branch_name)) AS names_valid
    FROM public.workspace_branches
    WHERE authority_scope_kind IS NULL OR authority_scope_kind = 'workspace'
    GROUP BY workspace_id
), missing_projection AS (
    SELECT branch.workspace_id,
           jsonb_agg(DISTINCT branch.repo_id ORDER BY branch.repo_id) AS repo_ids
    FROM public.workspace_branches AS branch
    LEFT JOIN public.workspace_repos AS mapped
      ON mapped.workspace_id = branch.workspace_id AND mapped.repo_id = branch.repo_id
    WHERE (branch.authority_scope_kind IS NULL OR branch.authority_scope_kind = 'workspace')
      AND mapped.repo_id IS NULL
    GROUP BY branch.workspace_id
)
SELECT 'owner_unresolved'::text AS issue_kind,
       'blocker'::text AS severity,
       workspace.org_id,
       workspace.id AS workspace_id,
       workspace.id AS record_id,
       jsonb_build_object('created_by', workspace.created_by) AS details
FROM public.workspaces AS workspace
WHERE workspace.deleted_at IS NULL AND workspace.owner_user_id IS NULL
UNION ALL
SELECT 'branch_mapping_unresolved',
       'blocker',
       workspace.org_id,
       workspace.id,
       workspace.id,
       jsonb_build_object(
           'branch_names', summary.branch_names,
           'branch_rows', summary.branch_rows,
           'repo_count', summary.repo_count,
           'names_valid', summary.names_valid
       )
FROM public.workspaces AS workspace
LEFT JOIN legacy_branch_summary AS summary ON summary.workspace_id = workspace.id
WHERE workspace.deleted_at IS NULL
  AND NOT workspace.is_base
  AND (summary.workspace_id IS NULL OR summary.branch_name_count <> 1 OR NOT summary.names_valid)
UNION ALL
SELECT 'display_branch_name_difference',
       'review',
       workspace.org_id,
       workspace.id,
       workspace.id,
       jsonb_build_object('display_name', workspace.name, 'branch_name', workspace.branch_name)
FROM public.workspaces AS workspace
WHERE workspace.deleted_at IS NULL
  AND NOT workspace.is_base
  AND workspace.branch_name IS NOT NULL
  AND workspace.name <> workspace.branch_name
UNION ALL
SELECT 'workspace_repo_projection_missing',
       'blocker',
       workspace.org_id,
       workspace.id,
       workspace.id,
       jsonb_build_object('repo_ids', missing.repo_ids)
FROM missing_projection AS missing
JOIN public.workspaces AS workspace ON workspace.id = missing.workspace_id
UNION ALL
SELECT 'workspace_repo_root_invalid',
       'blocker',
       mapped.org_id,
       mapped.workspace_id,
       mapped.repo_id,
       jsonb_build_object('root_path', mapped.root_path)
FROM public.workspace_repos AS mapped
WHERE mapped.root_path ~ '(^|/)\\.\\.?(/|$)'
   OR mapped.root_path ~ '^/'
   OR mapped.root_path ~ '/$';

CREATE VIEW public.p4_1_task_migration_preflight WITH (security_invoker = true) AS
WITH RECURSIVE legacy_edges AS (
    SELECT task.org_id,
           task.workspace_id,
           task.id AS task_id,
           dependency_id AS depends_on_task_id
    FROM public.tasks AS task
    CROSS JOIN LATERAL unnest(task.depends_on) AS dependency_id
), reach(org_id, workspace_id, source_id, target_id) AS (
    SELECT org_id, workspace_id, task_id, depends_on_task_id FROM legacy_edges
    UNION
    SELECT reach.org_id, reach.workspace_id, reach.source_id, edge.depends_on_task_id
    FROM reach
    JOIN legacy_edges AS edge
      ON edge.org_id = reach.org_id
     AND edge.workspace_id IS NOT DISTINCT FROM reach.workspace_id
     AND edge.task_id = reach.target_id
)
SELECT 'task_workspace_unresolved'::text AS issue_kind,
       'blocker'::text AS severity,
       task.org_id,
       task.workspace_id,
       task.id AS task_id,
       jsonb_build_object('job_id', task.job_id) AS details
FROM public.tasks AS task
WHERE task.workspace_id IS NULL
UNION ALL
SELECT 'task_kind_unmapped',
       'blocker',
       task.org_id,
       task.workspace_id,
       task.id,
       jsonb_build_object('task_type', task.task_type)
FROM public.tasks AS task
WHERE task.kind IS NULL AND task.task_type <> 'merge'
UNION ALL
SELECT 'agent_session_unresolved',
       'blocker',
       task.org_id,
       task.workspace_id,
       task.id,
       jsonb_build_object('task_type', task.task_type)
FROM public.tasks AS task
WHERE task.kind = 'agent' AND task.chat_session_id IS NULL
UNION ALL
SELECT 'permission_policy_unresolved',
       'blocker',
       task.org_id,
       task.workspace_id,
       task.id,
       jsonb_build_object('legacy_async_decision_policy', task.async_decision_policy)
FROM public.tasks AS task
WHERE task.kind = 'agent' AND NOT (task.spec ? 'permission_policy')
UNION ALL
SELECT 'legacy_dependency_invalid',
       'blocker',
       edge.org_id,
       edge.workspace_id,
       edge.task_id,
       jsonb_build_object('depends_on_task_id', edge.depends_on_task_id)
FROM legacy_edges AS edge
LEFT JOIN public.tasks AS dependency ON dependency.id = edge.depends_on_task_id
WHERE edge.depends_on_task_id = edge.task_id
   OR dependency.id IS NULL
   OR dependency.org_id <> edge.org_id
   OR dependency.workspace_id IS DISTINCT FROM edge.workspace_id
UNION ALL
SELECT 'legacy_dependency_cycle',
       'blocker',
       task.org_id,
       task.workspace_id,
       task.id,
       jsonb_build_object('task_id', task.id)
FROM public.tasks AS task
WHERE EXISTS (
    SELECT 1 FROM reach
    WHERE reach.org_id = task.org_id
      AND reach.workspace_id IS NOT DISTINCT FROM task.workspace_id
      AND reach.source_id = task.id
      AND reach.target_id = task.id
)
UNION ALL
SELECT 'multiple_legacy_sessions_for_task',
       'blocker',
       task.org_id,
       task.workspace_id,
       task.id,
       jsonb_build_object(
           'session_ids', jsonb_agg(session.id ORDER BY session.id),
           'selected_session_id', (array_agg(session.id ORDER BY session.id))[1]
       )
FROM public.tasks AS task
JOIN public.chat_sessions AS session ON session.task_id = task.id
GROUP BY task.org_id, task.workspace_id, task.id
HAVING count(*) > 1
UNION ALL
SELECT 'legacy_question_successor_reference',
       'review',
       task.org_id,
       task.workspace_id,
       task.id,
       jsonb_build_object('question_id', question.id, 'successor_task_id', question.successor_task_id)
FROM public.task_questions AS question
JOIN public.tasks AS task ON task.id = question.task_id
WHERE question.successor_task_id IS NOT NULL;

CREATE VIEW public.p4_1_config_migration_preflight WITH (security_invoker = true) AS
SELECT 'org_settings_require_mapping'::text AS issue_kind,
       org_id,
       org_id AS scope_id,
       jsonb_object_keys(settings_json) AS key
FROM public.org_configs
WHERE settings_json <> '{}'::jsonb
UNION ALL
SELECT 'user_settings_require_mapping', org_id, user_id, jsonb_object_keys(settings_json)
FROM public.user_configs
WHERE settings_json <> '{}'::jsonb
UNION ALL
SELECT 'workspace_settings_require_typed_mapping', org_id, workspace_id, jsonb_object_keys(settings_json)
FROM public.workspace_configs
WHERE settings_json <> '{}'::jsonb;

-- -------------------------------------------------------------------------
-- Tenant isolation and grants for every new tenant-owned relation
-- -------------------------------------------------------------------------

ALTER TABLE public.orgs ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.orgs FORCE ROW LEVEL SECURITY;
ALTER TABLE public.configs ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.configs FORCE ROW LEVEL SECURITY;
ALTER TABLE public.workspace_repos ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.workspace_repos FORCE ROW LEVEL SECURITY;
ALTER TABLE public.workspace_ownership_transfers ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.workspace_ownership_transfers FORCE ROW LEVEL SECURITY;
ALTER TABLE public.workspace_forks ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.workspace_forks FORCE ROW LEVEL SECURITY;
ALTER TABLE public.workspace_vms ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.workspace_vms FORCE ROW LEVEL SECURITY;
ALTER TABLE public.workspace_flushes ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.workspace_flushes FORCE ROW LEVEL SECURITY;
ALTER TABLE public.repo_mirror_refs ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.repo_mirror_refs FORCE ROW LEVEL SECURITY;
ALTER TABLE public.worker_vm_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.worker_vm_events FORCE ROW LEVEL SECURITY;
ALTER TABLE public.task_attempts ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.task_attempts FORCE ROW LEVEL SECURITY;
ALTER TABLE public.task_dependencies ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.task_dependencies FORCE ROW LEVEL SECURITY;
ALTER TABLE public.provider_requests ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.provider_requests FORCE ROW LEVEL SECURITY;
ALTER TABLE public.session_inputs ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.session_inputs FORCE ROW LEVEL SECURITY;

CREATE POLICY orgs_tenant_isolation ON public.orgs
    TO g5_tenant
    USING (id = ANY (authz.current_org_ids()))
    WITH CHECK (id = ANY (authz.current_org_ids()));
CREATE POLICY orgs_system_access ON public.orgs
    TO g5_app
    USING (true)
    WITH CHECK (true);
CREATE POLICY tenant_isolation ON public.configs
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
CREATE POLICY tenant_isolation ON public.workspace_repos
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
CREATE POLICY tenant_isolation ON public.workspace_ownership_transfers
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
CREATE POLICY tenant_isolation ON public.workspace_forks
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
CREATE POLICY tenant_isolation ON public.workspace_vms
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
CREATE POLICY tenant_isolation ON public.workspace_flushes
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
CREATE POLICY tenant_isolation ON public.repo_mirror_refs
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
CREATE POLICY tenant_isolation ON public.worker_vm_events
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
CREATE POLICY tenant_isolation ON public.task_attempts
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
CREATE POLICY tenant_isolation ON public.task_dependencies
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
CREATE POLICY tenant_isolation ON public.provider_requests
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
CREATE POLICY tenant_isolation ON public.session_inputs
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));

CREATE POLICY system_access ON public.configs TO g5_app USING (true) WITH CHECK (true);
CREATE POLICY system_access ON public.workspace_repos TO g5_app USING (true) WITH CHECK (true);
CREATE POLICY system_access ON public.workspace_ownership_transfers TO g5_app USING (true) WITH CHECK (true);
CREATE POLICY system_access ON public.workspace_forks TO g5_app USING (true) WITH CHECK (true);
CREATE POLICY system_access ON public.workspace_vms TO g5_app USING (true) WITH CHECK (true);
CREATE POLICY system_access ON public.workspace_flushes TO g5_app USING (true) WITH CHECK (true);
CREATE POLICY system_access ON public.repo_mirror_refs TO g5_app USING (true) WITH CHECK (true);
CREATE POLICY system_access ON public.worker_vm_events TO g5_app USING (true) WITH CHECK (true);
CREATE POLICY system_access ON public.task_attempts TO g5_app USING (true) WITH CHECK (true);
CREATE POLICY system_access ON public.task_dependencies TO g5_app USING (true) WITH CHECK (true);
CREATE POLICY system_access ON public.provider_requests TO g5_app USING (true) WITH CHECK (true);
CREATE POLICY system_access ON public.session_inputs TO g5_app USING (true) WITH CHECK (true);

GRANT SELECT (id, access_generation), UPDATE (access_generation)
    ON public.orgs TO g5_tenant;
GRANT SELECT, INSERT, UPDATE, DELETE ON public.configs TO g5_tenant, g5_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON public.workspace_repos TO g5_tenant, g5_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON public.workspace_ownership_transfers TO g5_tenant, g5_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON public.workspace_forks TO g5_tenant, g5_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON public.workspace_vms TO g5_tenant, g5_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON public.workspace_flushes TO g5_tenant, g5_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON public.repo_mirror_refs TO g5_tenant, g5_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON public.worker_vm_events TO g5_tenant, g5_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON public.task_attempts TO g5_tenant, g5_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON public.task_dependencies TO g5_tenant, g5_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON public.provider_requests TO g5_tenant, g5_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON public.session_inputs TO g5_tenant, g5_app;
GRANT SELECT ON public.p4_1_workspace_migration_preflight TO g5_tenant, g5_app;
GRANT SELECT ON public.p4_1_task_migration_preflight TO g5_tenant, g5_app;
GRANT SELECT ON public.p4_1_config_migration_preflight TO g5_tenant, g5_app;
