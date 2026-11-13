-- The guest-confirmed journal acknowledgement of an attempt (`g5.chat.journal`,
-- #966). `acked_event_seq` is the prefix the platform database holds; the
-- guest keeps its journal until it confirms that prefix, so a failed
-- acknowledgement is retried from here, including for attempts that already
-- ended, while their VM incarnation lives.
--
-- The column, the backfill and the index take ACCESS EXCLUSIVE and row locks
-- on a live table whose rows every history batch and reconciler claim lock;
-- a blocked migration times out and the migrate Job retries it
-- (docs/database-migrations.md).
SET LOCAL lock_timeout = '5s';

ALTER TABLE public.task_attempts
    ADD COLUMN journal_acked_seq bigint NOT NULL DEFAULT 0
        CHECK (journal_acked_seq >= 0);

-- Mixed DDL/data requires the documented definer-role migration protocol
-- (docs/database-migrations.md). Preserve the migrator's exact role
-- membership; no runtime grants remain.
SELECT set_config(
    'g5.journal_ack_membership_snapshot',
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
    'g5.journal_ack_had_set',
    CASE WHEN pg_has_role(SESSION_USER, 'authz_lookup_definer', 'SET')
         THEN 'true' ELSE 'false' END,
    false
);
DO $$
BEGIN
    IF current_setting('g5.journal_ack_had_set', true)
        IS DISTINCT FROM 'true'
    THEN
        EXECUTE format(
            'GRANT authz_lookup_definer TO %I WITH SET TRUE',
            SESSION_USER
        );
    END IF;
END $$;
-- Attempts that ended before this column existed were acknowledged by the
-- reconciler of their day or their VM is gone: treat their prefix as
-- confirmed, so the retry scan's partial index holds only genuine arrears.
GRANT UPDATE (journal_acked_seq) ON public.task_attempts TO authz_lookup_definer;
GRANT SELECT (ended_at, acked_event_seq, journal_acked_seq)
    ON public.task_attempts TO authz_lookup_definer;
SET ROLE authz_lookup_definer;
UPDATE public.task_attempts
   SET journal_acked_seq = acked_event_seq
 WHERE ended_at IS NOT NULL AND journal_acked_seq < acked_event_seq;
RESET ROLE;
REVOKE UPDATE (journal_acked_seq) ON public.task_attempts FROM authz_lookup_definer;
REVOKE SELECT (ended_at, acked_event_seq, journal_acked_seq)
    ON public.task_attempts FROM authz_lookup_definer;

-- Give back the temporary SET-capable membership exactly as it was.
DO $$
DECLARE
    membership_snapshot jsonb :=
        COALESCE(
            current_setting('g5.journal_ack_membership_snapshot', true),
            '[]'
        )::jsonb;
    membership_row record;
    prior_row jsonb;
BEGIN
    IF current_setting('g5.journal_ack_had_set', true)
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

-- The retry scan reads only attempts whose confirmed prefix lags the
-- persisted one (`g5.chat.journal`).
CREATE INDEX task_attempts_journal_ack_pending
    ON public.task_attempts (org_id, last_reconciled_at)
    WHERE journal_acked_seq < acked_event_seq;
