-- Personal workspaces (g5.personal.identity, g5.workspace.kind, G5-Lean#624).
--
-- * workspaces.personal_user_id: a non-null value makes the workspace's registry
--   kind personal, bound to that (organization, user). The bound user is always
--   the owner, and a personal workspace is never Base. At most one live personal
--   workspace exists per (organization, user); tombstoned ones keep their binding.
-- * A new trigger beside 108's identity trigger fixes personal_user_id.
-- * default_personal_idle_timeout_secs (60-86400; absent = 1800) becomes an
--   eighth registered org configuration key (g5.personal.capacity). The
--   validator body is 086's with one branch added; no rows are seeded.
-- * owner_removed joins the workspace VM stop reasons (g5.personal.retirement),
--   beside 099's member_deactivated.
-- * session_inputs.start_consent_id binds a Start-and-send input to the Start
--   consent it was queued with (g5.personal.start-and-send). It is the `id` of
--   workspace_vms.start_intent, a JSONB value, so it carries no foreign key; a
--   trigger settles bound inputs whenever that consent changes.
-- * The reservation of `me`, `users` and `users/...` names is enforced by name
--   admission. Existing workspaces and pending fork claims holding such names
--   stay unchanged. A migration session cannot see tenant rows under FORCE
--   ROW LEVEL SECURITY, so the personal workspace reconciler reports them per
--   organization instead (g5.personal.names).
ALTER TABLE public.workspaces
    ADD COLUMN personal_user_id uuid REFERENCES public.users(id),
    ADD CONSTRAINT workspaces_personal_owner_check CHECK (
        personal_user_id IS NULL
        OR (personal_user_id = owner_user_id AND NOT is_base AND cloned_from_id IS NULL)
    );

-- Membership changes run on the system pool, whose role reads but never
-- updated workspaces. Retiring the owner's personal workspace in that same
-- transaction (g5.personal.retirement) needs exactly the tombstone columns.
GRANT UPDATE (deleted_at, deleted_by, deletion_operation_id, lifecycle_generation, updated_at)
    ON public.workspaces TO g5_app;

CREATE UNIQUE INDEX workspaces_live_personal_unique
    ON public.workspaces (org_id, personal_user_id)
    WHERE personal_user_id IS NOT NULL AND deleted_at IS NULL;

-- 108's identity trigger keeps is_base and branch_name immutable; the binding
-- gets its own trigger rather than another replaced function.
CREATE FUNCTION public.g5_workspace_personal_kind_immutable() RETURNS trigger
LANGUAGE plpgsql SET search_path TO pg_catalog AS $$
BEGIN
    IF NEW.personal_user_id IS DISTINCT FROM OLD.personal_user_id THEN
        RAISE EXCEPTION 'workspace kind is immutable' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER workspace_personal_kind_immutable
    BEFORE UPDATE OF personal_user_id ON public.workspaces
    FOR EACH ROW EXECUTE FUNCTION public.g5_workspace_personal_kind_immutable();

CREATE OR REPLACE FUNCTION public.g5_config_value_is_registered(
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
             AND (config_value #>> '{}')::numeric BETWEEN 0 AND 100
    WHEN config_scope = 'org' AND config_key IN ('default_idle_timeout_secs', 'default_personal_idle_timeout_secs')
        THEN jsonb_typeof(config_value) = 'number'
             AND (config_value #>> '{}') ~ '^[0-9]+$'
             AND (config_value #>> '{}')::numeric BETWEEN 60 AND 86400
    WHEN config_scope = 'org' AND config_key IN ('auto_add_hosts', 'auto_retire_idle_hosts')
        THEN jsonb_typeof(config_value) = 'boolean'
    WHEN config_scope = 'org' AND config_key = 'auto_host_instance_type'
        THEN jsonb_typeof(config_value) = 'string'
             AND (config_value #>> '{}') ~ '^(n2|c2|c3)-standard-[0-9]{1,3}$'
    WHEN config_scope = 'org' AND config_key = 'idle_host_retire_secs'
        THEN jsonb_typeof(config_value) = 'number'
             AND (config_value #>> '{}') ~ '^[0-9]+$'
             AND (config_value #>> '{}')::numeric BETWEEN 900 AND 86400
    WHEN config_scope = 'org' AND config_key = 'min_free_slots'
        THEN jsonb_typeof(config_value) = 'number'
             AND (config_value #>> '{}') ~ '^[0-9]+$'
             AND (config_value #>> '{}')::numeric BETWEEN 0 AND 100
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

ALTER TABLE public.workspace_vms
    DROP CONSTRAINT workspace_vms_stop_reason_check,
    ADD CONSTRAINT workspace_vms_stop_reason_check CHECK (
        stop_reason IS NULL OR stop_reason IN (
            'user', 'transfer', 'idle', 'drain', 'rollout', 'credential', 'capacity', 'delete',
            'member_deactivated', 'owner_removed'
        )
    ),
    DROP CONSTRAINT workspace_vms_pending_stop_reason_check,
    ADD CONSTRAINT workspace_vms_pending_stop_reason_check CHECK (
        pending_stop_reason IS NULL OR pending_stop_reason IN (
            'user', 'transfer', 'idle', 'drain', 'rollout', 'credential', 'capacity', 'delete',
            'member_deactivated', 'owner_removed'
        )
    );

ALTER TABLE public.session_inputs
    ADD COLUMN start_consent_id uuid;
CREATE INDEX session_inputs_start_consent_idx
    ON public.session_inputs (start_consent_id)
    WHERE start_consent_id IS NOT NULL;

-- Settle Start-and-send inputs in the same transition that ends their Start
-- consent. A consent that reaches ready ('consumed') releases its inputs to
-- ordinary queueing; one cancelled, expired, failed, superseded by another
-- intent or removed with its row withdraws the inputs still queued, so a later
-- unrelated Start never delivers them. The agent task reconciler cancels a task
-- left with no input.
CREATE FUNCTION public.g5_settle_start_bound_inputs() RETURNS trigger
LANGUAGE plpgsql SET search_path TO pg_catalog, public AS $$
DECLARE
    bound uuid;
    successor jsonb;
BEGIN
    IF OLD.start_intent IS NULL THEN
        RETURN NULL;
    END IF;
    bound := (OLD.start_intent ->> 'id')::uuid;
    successor := CASE WHEN TG_OP = 'DELETE' THEN NULL ELSE NEW.start_intent END;
    IF successor IS NOT NULL AND (successor ->> 'id')::uuid = bound THEN
        IF successor ->> 'status' = 'consumed' THEN
            UPDATE public.session_inputs SET start_consent_id = NULL
            WHERE org_id = OLD.org_id AND start_consent_id = bound;
            RETURN NULL;
        END IF;
        IF successor ->> 'status' NOT IN ('expired', 'cancelled', 'failed') THEN
            RETURN NULL;
        END IF;
    END IF;
    UPDATE public.session_inputs
    SET delivery = 'withdrawn', withdrawn_at = now(), start_consent_id = NULL,
        delivery_audit = delivery_audit || jsonb_build_array(
            jsonb_build_object('event', 'start_ended', 'start_intent_id', bound))
    WHERE org_id = OLD.org_id AND start_consent_id = bound AND delivery = 'queued';
    UPDATE public.session_inputs SET start_consent_id = NULL
    WHERE org_id = OLD.org_id AND start_consent_id = bound;
    RETURN NULL;
END $$;

CREATE TRIGGER workspace_vms_settle_start_bound_inputs
    AFTER UPDATE OF start_intent ON public.workspace_vms
    FOR EACH ROW WHEN (OLD.start_intent IS DISTINCT FROM NEW.start_intent)
    EXECUTE FUNCTION public.g5_settle_start_bound_inputs();
CREATE TRIGGER workspace_vms_settle_start_bound_inputs_on_delete
    AFTER DELETE ON public.workspace_vms
    FOR EACH ROW EXECUTE FUNCTION public.g5_settle_start_bound_inputs();
