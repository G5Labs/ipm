-- Workspace-wide approval requests; legacy phases remain immutable history.
-- Target requirements and current decisions are evaluated without a phase axis.
ALTER TABLE public.approval_requests ADD COLUMN note text
    CONSTRAINT approval_request_note_size CHECK (note IS NULL OR octet_length(note) <= 8192);

-- Mixed DDL/data requires the documented definer-role migration protocol.
-- Preserve the migrator's exact role membership; no runtime grants remain.
SELECT set_config(
    'g5.workspace_approval_lookup_membership_snapshot',
    COALESCE((
        SELECT jsonb_agg(
                   jsonb_build_object(
                       'grantor', grantor_role.rolname,
                       'admin_option', membership.admin_option,
                       'inherit_option', membership.inherit_option,
                       'set_option', membership.set_option
                   )
                   ORDER BY grantor_role.rolname
               )
        FROM pg_auth_members membership
        JOIN pg_roles granted_role ON granted_role.oid = membership.roleid
        JOIN pg_roles member_role ON member_role.oid = membership.member
        JOIN pg_roles grantor_role ON grantor_role.oid = membership.grantor
        WHERE granted_role.rolname = 'authz_lookup_definer'
          AND member_role.rolname = SESSION_USER
    ), '[]'::jsonb)::text,
    false
);
SELECT set_config(
    'g5.workspace_approval_lookup_had_set',
    CASE WHEN pg_has_role(SESSION_USER, 'authz_lookup_definer', 'SET')
         THEN 'true' ELSE 'false' END,
    false
);
DO $$
BEGIN
    IF current_setting('g5.workspace_approval_lookup_had_set', true)
        IS DISTINCT FROM 'true'
    THEN
        EXECUTE format(
            'GRANT authz_lookup_definer TO %I WITH SET TRUE',
            SESSION_USER
        );
    END IF;
END $$;
GRANT SELECT, UPDATE ON public.approval_requests TO authz_lookup_definer;
SET ROLE authz_lookup_definer;
WITH ranked AS (
    SELECT id, row_number() OVER (
        PARTITION BY org_id, source_workspace_id, pull_request_id,
                     approval_definition_id, approval_definition_version,
                     actor_type, actor_user_id
        ORDER BY seq DESC
    ) AS position
    FROM public.approval_requests WHERE state = 'pending'
)
UPDATE public.approval_requests request
SET state = 'superseded', closed_at = transaction_timestamp(), updated_at = transaction_timestamp()
FROM ranked WHERE request.id = ranked.id AND ranked.position > 1;
-- The formal review coordinator observes nonpending requests and owns cancellation/cleanup.
SET CONSTRAINTS ALL IMMEDIATE;
RESET ROLE;
REVOKE SELECT, UPDATE ON public.approval_requests FROM authz_lookup_definer;
DO $$
DECLARE
    membership_snapshot jsonb :=
        COALESCE(
            current_setting('g5.workspace_approval_lookup_membership_snapshot', true),
            '[]'
        )::jsonb;
    membership_row record;
    prior_row jsonb;
BEGIN
    IF current_setting('g5.workspace_approval_lookup_had_set', true)
        IS DISTINCT FROM 'true'
    THEN
        FOR membership_row IN
            SELECT
                grantor_role.rolname AS grantor_name,
                membership.admin_option,
                membership.inherit_option,
                membership.set_option
            FROM pg_auth_members membership
            JOIN pg_roles granted_role ON granted_role.oid = membership.roleid
            JOIN pg_roles member_role ON member_role.oid = membership.member
            JOIN pg_roles grantor_role ON grantor_role.oid = membership.grantor
            WHERE granted_role.rolname = 'authz_lookup_definer'
              AND member_role.rolname = SESSION_USER
        LOOP
            SELECT snapshot_row
              INTO prior_row
              FROM jsonb_array_elements(membership_snapshot) AS snapshot_row
             WHERE snapshot_row->>'grantor' = membership_row.grantor_name;

            IF prior_row IS NULL THEN
                EXECUTE format(
                    'REVOKE authz_lookup_definer FROM %I',
                    SESSION_USER
                );
            ELSIF membership_row.admin_option IS DISTINCT FROM
                      (prior_row->>'admin_option')::boolean
                OR membership_row.inherit_option IS DISTINCT FROM
                      (prior_row->>'inherit_option')::boolean
                OR membership_row.set_option IS DISTINCT FROM
                      (prior_row->>'set_option')::boolean
            THEN
                EXECUTE format(
                    'GRANT authz_lookup_definer TO %I WITH ADMIN %s, INHERIT %s, SET %s',
                    SESSION_USER,
                    prior_row->>'admin_option',
                    prior_row->>'inherit_option',
                    prior_row->>'set_option'
                );
            END IF;
        END LOOP;
    END IF;
END $$;


CREATE UNIQUE INDEX approval_requests_workspace_pending_slot
    ON public.approval_requests (org_id, source_workspace_id, pull_request_id,
        approval_definition_id, approval_definition_version, actor_type, actor_user_id)
    NULLS NOT DISTINCT WHERE state = 'pending';

CREATE OR REPLACE FUNCTION public.guard_approval_request_update() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF NEW.id IS DISTINCT FROM OLD.id
       OR NEW.seq IS DISTINCT FROM OLD.seq
       OR NEW.org_id IS DISTINCT FROM OLD.org_id
       OR NEW.source_workspace_id IS DISTINCT FROM OLD.source_workspace_id
       OR NEW.approval_definition_id IS DISTINCT FROM OLD.approval_definition_id
       OR NEW.approval_definition_version IS DISTINCT FROM OLD.approval_definition_version
       OR NEW.request_key IS DISTINCT FROM OLD.request_key
       OR NEW.actor_type IS DISTINCT FROM OLD.actor_type
       OR NEW.actor_user_id IS DISTINCT FROM OLD.actor_user_id
       OR NEW.requested_by IS DISTINCT FROM OLD.requested_by
       OR NEW.expires_at IS DISTINCT FROM OLD.expires_at
       OR NEW.note IS DISTINCT FROM OLD.note
       OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
        RAISE EXCEPTION 'approval_requests identity fields are immutable'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'approval_requests_immutable';
    END IF;

    IF OLD.state <> 'pending'
       OR NEW.state NOT IN ('settled', 'cancelled', 'expired', 'superseded') THEN
        RAISE EXCEPTION 'approval_requests permits one pending-to-terminal transition'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'approval_requests_transition';
    END IF;

    IF NEW.closed_at IS DISTINCT FROM transaction_timestamp()
       OR NEW.updated_at IS DISTINCT FROM transaction_timestamp() THEN
        RAISE EXCEPTION 'approval_requests close timestamps must use transaction time'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'approval_requests_server_timestamps';
    END IF;

    NEW.closed_at := transaction_timestamp();
    NEW.updated_at := transaction_timestamp();

    RETURN NEW;
END;
$$;
