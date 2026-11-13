-- g5.runtime.host.cloud-recovery / g5.runtime.host.cloud-linkage (issue #627):
-- 1. The orphan pass leaves managed-capacity claims to their capacity owner
--    (S3: launch-queue delay is not abandonment) and additionally covers live
--    unlinked rows stranded with a stale stockout classification (S18) so a
--    found instance re-attaches instead of leaking.
-- 2. Attaching a found instance clears the stale classification the same way
--    the 4-arg success setter does, so a recovered host never keeps a
--    terminal badge.

-- The definer role reads worker_hosts through column-level grants; the claim
-- ownership predicate needs the two claim columns (as migration 046 did for
-- slot_size_reported).
GRANT SELECT (capacity_start_intent_id, capacity_floor_claim)
    ON public.worker_hosts TO authz_lookup_definer;

-- Temporarily assume the existing function owner, as in migrations 039/073.
SELECT set_config(
    'g5.issue627_authz_lookup_membership_snapshot',
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
    'g5.issue627_authz_lookup_had_set',
    CASE WHEN pg_has_role(SESSION_USER, 'authz_lookup_definer', 'SET')
         THEN 'true' ELSE 'false' END,
    false
);
SELECT set_config(
    'g5.issue627_authz_lookup_had_authz_create',
    has_schema_privilege('authz_lookup_definer', 'authz', 'CREATE')::text,
    false
);
DO $$
BEGIN
    IF current_setting('g5.issue627_authz_lookup_had_set', true)
        IS DISTINCT FROM 'true'
    THEN
        EXECUTE format(
            'GRANT authz_lookup_definer TO %I WITH SET TRUE',
            SESSION_USER
        );
    END IF;
END $$;
GRANT USAGE, CREATE ON SCHEMA authz TO authz_lookup_definer;

SET ROLE authz_lookup_definer;

-- The projection is unchanged (classification governs selection inside the
-- WHERE clause only); recreated for the new predicates.
DROP FUNCTION authz.list_orphan_bringup_worker_hosts();

CREATE FUNCTION authz.list_orphan_bringup_worker_hosts()
RETURNS TABLE (
    host_id uuid,
    org_id uuid,
    region text,
    created_at timestamptz,
    orphan_check_misses integer,
    status text
)
LANGUAGE sql STABLE SECURITY DEFINER PARALLEL RESTRICTED
SET search_path TO 'public', 'authz', 'pg_catalog'
AS $$
    SELECT wh.host_id, wh.org_id, wh.region, wh.created_at,
           wh.orphan_check_misses, wh.status
    FROM public.worker_hosts wh
    WHERE wh.status <> 'terminated'
      AND wh.cloud_instance_id IS NULL
      -- Managed-capacity claims are reconciled and abandoned only by their
      -- capacity owner WHILE they are provisioning rows
      -- (g5.runtime.host.cloud-recovery, issue #627 S3). A claim host that
      -- REGISTERED while unlinked has left the owner's recovery scope
      -- (drive_claim handles provisioning rows only) and stays
      -- attach-eligible like any live row (issue #627 Codex review P1).
      AND NOT (wh.status = 'provisioning'
               AND (wh.capacity_start_intent_id IS NOT NULL
                    OR wh.capacity_floor_claim))
      -- Unclassified rows, plus rows stranded with a stale stockout
      -- classification (consumed schedule, retained code — issue #627 S18).
      -- Classified rows are bounded by the MISS COUNTER, not wall clock
      -- (CodeRabbit on PR #637, both rounds): each gets at most the orphan
      -- threshold of confirmed-missing lookups before aging out of this set,
      -- so a pre-migration strand of ANY age is checked and reattached when
      -- its instance lives, while a budget-frozen row costs a bounded number
      -- of lookups in total — never per-tick forever. The terminate setter
      -- keeps refusing classified rows, so the [D66] freeze badge survives;
      -- 10 mirrors ORPHAN_TERMINATE_MISS_THRESHOLD.
      AND (wh.provision_error_code IS NULL
           OR (wh.provision_error_code = 'zone_capacity_exhausted'
               AND wh.provision_next_retry_at IS NULL
               AND wh.orphan_check_misses < 10))
      AND wh.created_at < now() - interval '60 seconds'
      AND (wh.bringup_heartbeat_at IS NULL
           OR wh.bringup_heartbeat_at < now() - interval '90 seconds')
$$;

ALTER FUNCTION authz.list_orphan_bringup_worker_hosts() OWNER TO authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.list_orphan_bringup_worker_hosts() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION authz.list_orphan_bringup_worker_hosts() TO g5_app;
COMMENT ON FUNCTION authz.list_orphan_bringup_worker_hosts() IS
    'Live unlinked worker hosts past creation grace without a fresh bring-up lease, excluding capacity claims and including stale stockout classifications (g5.runtime.host.cloud-recovery, issue #627). SECURITY DEFINER + BYPASSRLS owner for cross-org repair.';

CREATE OR REPLACE FUNCTION authz.set_worker_host_orphan_check_miss(p_host_id uuid)
RETURNS integer
LANGUAGE plpgsql SECURITY DEFINER
SET search_path TO 'public', 'authz', 'pg_catalog'
AS $$
DECLARE
    v_misses INT;
BEGIN
    UPDATE public.worker_hosts
    SET orphan_check_misses = orphan_check_misses + 1
    WHERE host_id = p_host_id
      AND status = 'provisioning'
      AND cloud_instance_id IS NULL
      -- A stale stockout classification counts misses too (issue #627 S18):
      -- the counter ages it out of the orphan selector after a bounded number
      -- of confirmed-missing lookups; the terminate setter still refuses
      -- classified rows, so the badge is never converted to abandonment.
      AND (provision_error_code IS NULL
           OR provision_error_code = 'zone_capacity_exhausted')
      -- Capacity claims are never miss-counted; their owner reconciles and
      -- abandons them (issue #627 S3).
      AND capacity_start_intent_id IS NULL
      AND NOT capacity_floor_claim
      -- [D60] lease fence: an in-flight bring-up is never counted.
      AND (bringup_heartbeat_at IS NULL
           OR bringup_heartbeat_at < now() - interval '90 seconds')
    RETURNING orphan_check_misses INTO v_misses;
    RETURN v_misses;
END;
$$;

COMMENT ON FUNCTION authz.set_worker_host_orphan_check_miss(uuid) IS
    'Reconciler orphan-pass miss-counter increment ([D39]), lease-fenced ([D60]), claim-excluded (issue #627 S3) and counting stale stockout classifications toward their bounded lookup budget (issue #627 S18). Returns NULL when the row no longer qualifies. SECURITY DEFINER + BYPASSRLS owner.';

CREATE OR REPLACE FUNCTION authz.reset_worker_host_orphan_check_miss(p_host_id uuid)
RETURNS integer
LANGUAGE plpgsql SECURITY DEFINER
SET search_path TO 'public', 'authz', 'pg_catalog'
AS $$
DECLARE
    v_misses INT;
BEGIN
    UPDATE public.worker_hosts
    SET orphan_check_misses = 0
    WHERE host_id = p_host_id
      AND status = 'provisioning'
      AND cloud_instance_id IS NULL
      -- Mirror the increment setter (issue #627 S18): a transient
      -- observation must break a classified row's missing streak too.
      AND (provision_error_code IS NULL
           OR provision_error_code = 'zone_capacity_exhausted')
      -- Only emit a write when there's something to reset. Avoids dead-row
      -- churn when the transient observation came in BEFORE any miss was
      -- accumulated (the common case for a Stopping-on-first-tick scenario).
      AND orphan_check_misses > 0
    RETURNING orphan_check_misses INTO v_misses;
    RETURN v_misses;
END;
$$;

COMMENT ON FUNCTION authz.reset_worker_host_orphan_check_miss(uuid) IS
    'Reconciler orphan-pass miss-counter RESET: a live-but-transient observation breaks the missing streak so the threshold keeps meaning consecutive missing observations ([D39]); since issue #627 S18 it also covers stale stockout classifications, mirroring the increment setter. Returns NULL when the row no longer qualifies. SECURITY DEFINER + BYPASSRLS owner.';

CREATE OR REPLACE FUNCTION authz.terminate_abandoned_worker_host(
    p_host_id uuid,
    p_miss_threshold integer
) RETURNS uuid
LANGUAGE sql SECURITY DEFINER
SET search_path TO 'public', 'authz', 'pg_catalog'
AS $$
    UPDATE public.worker_hosts
    SET status = 'terminated',
        last_heartbeat = NULL,
        provision_error_code = 'abandoned_bringup'
    WHERE host_id = p_host_id
      AND status = 'provisioning'
      AND cloud_instance_id IS NULL
      AND provision_error_code IS NULL
      -- Capacity claims are abandoned only by their capacity owner
      -- (issue #627 S3).
      AND capacity_start_intent_id IS NULL
      AND NOT capacity_floor_claim
      -- [D60] lease fence + atomic threshold re-check.
      AND (bringup_heartbeat_at IS NULL
           OR bringup_heartbeat_at < now() - interval '90 seconds')
      AND orphan_check_misses >= p_miss_threshold
    RETURNING host_id
$$;

COMMENT ON FUNCTION authz.terminate_abandoned_worker_host(uuid, integer) IS
    'Reconciler orphan-pass terminate after threshold confirmed-missing observations ([D39]), lease-fenced ([D60]) and claim-excluded (issue #627 S3). SECURITY DEFINER + BYPASSRLS owner.';

CREATE OR REPLACE FUNCTION authz.attach_orphan_worker_host_cloud_refs(
    p_host_id uuid,
    p_vpc_id text,
    p_cloud_instance_id text
) RETURNS uuid
LANGUAGE sql SECURITY DEFINER
SET search_path TO 'public', 'authz', 'pg_catalog'
AS $$
    UPDATE public.worker_hosts
    SET vpc_id = p_vpc_id,
        cloud_instance_id = p_cloud_instance_id,
        -- A recovered, linked host sheds any stale retry classification the
        -- same way the 4-arg success setter does (issue #627 S18).
        provision_error_code = NULL,
        provision_next_retry_at = NULL,
        cloud_gone_since = NULL
    WHERE host_id = p_host_id
      AND status <> 'terminated'
      AND cloud_instance_id IS NULL
      AND (bringup_heartbeat_at IS NULL
           OR bringup_heartbeat_at < now() - interval '90 seconds')
    RETURNING host_id
$$;

COMMENT ON FUNCTION authz.attach_orphan_worker_host_cloud_refs(uuid, text, text) IS
    'Attach a found-alive instance to any live unlinked host, rechecking the bring-up lease and termination/linkage guards atomically and clearing any stale retry classification (g5.runtime.host.cloud-recovery, issue #627). SECURITY DEFINER + BYPASSRLS owner.';

RESET ROLE;

-- Restore the exact membership and schema CREATE privileges.
DO $$
DECLARE
    membership_snapshot jsonb :=
        COALESCE(
            current_setting('g5.issue627_authz_lookup_membership_snapshot', true),
            '[]'
        )::jsonb;
    membership_row record;
    prior_row jsonb;
BEGIN
    IF current_setting('g5.issue627_authz_lookup_had_set', true)
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

DO $$
BEGIN
    IF current_setting('g5.issue627_authz_lookup_had_authz_create', true)
        IS DISTINCT FROM 'true'
    THEN
        REVOKE CREATE ON SCHEMA authz FROM authz_lookup_definer;
    END IF;
END $$;
