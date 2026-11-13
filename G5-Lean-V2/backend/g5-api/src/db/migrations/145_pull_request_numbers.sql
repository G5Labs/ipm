-- G5 pull request numbers and pull requests of deleted workspaces (G5-Lean#830).
--
-- 1. Every G5 pull request gets a random number from 1 to 0xffffff, unique
--    in its organization, shown as six zero-padded hex digits (for example
--    `PR3fa9c1`) so it never reads as a GitHub number
--    (g5.git.pr-local-number). Taking a number holds no lock and uses no
--    sequence. Existing pull requests get random numbers the same way.
-- 2. Deletion now closes a workspace's open pull requests
--    (g5.workspace.delete.pull-requests). Pull requests left open by
--    workspaces deleted earlier are closed once the same way, each with a
--    closed event that names the deletion, so they stay off every surface
--    that acts on open pull requests now that reads include them. The close
--    is dated by the deletion, not by this migration.
ALTER TABLE public.workspace_pull_requests ADD COLUMN number bigint;

-- Mixed DDL/data requires the documented definer-role migration protocol.
-- Preserve the migrator's exact role membership; no runtime grants remain.
SELECT set_config(
    'g5.pr_numbers_membership_snapshot',
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
    'g5.pr_numbers_had_set',
    CASE WHEN pg_has_role(SESSION_USER, 'authz_lookup_definer', 'SET')
         THEN 'true' ELSE 'false' END,
    false
);
DO $$
BEGIN
    IF current_setting('g5.pr_numbers_had_set', true)
        IS DISTINCT FROM 'true'
    THEN
        EXECUTE format(
            'GRANT authz_lookup_definer TO %I WITH SET TRUE',
            SESSION_USER
        );
    END IF;
END $$;
-- The backfill must see every organization's rows (docs/database-migrations.md):
-- run it as the BYPASSRLS definer role, with only the privileges it needs.
GRANT SELECT, UPDATE (number, status, local_merge_status, updated_at)
    ON public.workspace_pull_requests TO authz_lookup_definer;
GRANT INSERT ON public.workspace_pull_request_events TO authz_lookup_definer;
SET ROLE authz_lookup_definer;
DO $$
DECLARE
    pull record;
    candidate bigint;
BEGIN
    FOR pull IN SELECT id, org_id FROM public.workspace_pull_requests LOOP
        LOOP
            candidate := 1 + floor(random() * 16777215)::bigint;
            EXIT WHEN NOT EXISTS (
                SELECT 1 FROM public.workspace_pull_requests
                 WHERE org_id = pull.org_id AND number = candidate);
        END LOOP;
        UPDATE public.workspace_pull_requests SET number = candidate WHERE id = pull.id;
    END LOOP;
END $$;
WITH closed AS (
    UPDATE public.workspace_pull_requests AS pr
       SET status = 'closed', local_merge_status = 'closed',
           updated_at = GREATEST(pr.updated_at, COALESCE(
               (SELECT w.deleted_at FROM public.workspaces AS w
                 WHERE w.id = pr.source_workspace_id),
               (SELECT w.deleted_at FROM public.workspaces AS w
                 WHERE w.id = pr.target_workspace_id)))
     WHERE pr.status = 'open'
       AND EXISTS (SELECT 1 FROM public.workspaces AS w
                    WHERE w.id IN (pr.source_workspace_id, pr.target_workspace_id)
                      AND w.deleted_at IS NOT NULL)
    RETURNING pr.org_id, pr.id, pr.source_workspace_id, pr.target_workspace_id
)
INSERT INTO public.workspace_pull_request_events
    (org_id, pull_request_id, event_type, actor_id, message, created_at)
SELECT closed.org_id, closed.id, 'closed',
       CASE WHEN sw.deleted_at IS NOT NULL THEN sw.deleted_by ELSE tw.deleted_by END,
       CASE WHEN sw.deleted_at IS NOT NULL
            THEN 'Closed because its source workspace was deleted'
            ELSE 'Closed because its target workspace was deleted' END,
       COALESCE(sw.deleted_at, tw.deleted_at)
  FROM closed
  JOIN public.workspaces AS sw ON sw.id = closed.source_workspace_id
  JOIN public.workspaces AS tw ON tw.id = closed.target_workspace_id;
RESET ROLE;
REVOKE INSERT ON public.workspace_pull_request_events FROM authz_lookup_definer;
REVOKE SELECT, UPDATE (number, status, local_merge_status, updated_at)
    ON public.workspace_pull_requests FROM authz_lookup_definer;

ALTER TABLE public.workspace_pull_requests
    ALTER COLUMN number SET NOT NULL,
    ADD CONSTRAINT workspace_pull_requests_number_check CHECK (number BETWEEN 1 AND 16777215),
    ADD CONSTRAINT workspace_pull_requests_org_number_key UNIQUE (org_id, number);

-- A new pull request takes a random number its organization has not used:
-- no lock and no sequence, so a creating transaction that waits on GitHub
-- blocks no other creation. Only two creations that pick the same number at
-- once (about one in 17 million) meet at the unique key: the later waits for
-- the earlier to end and, if it committed, the creation path retries. Runs
-- after trg_wpr_set_org (trigger names fire alphabetically).
CREATE FUNCTION public.workspace_pull_request_take_number() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'public'
    AS $$
DECLARE
    candidate bigint;
BEGIN
    IF NEW.number IS NULL THEN
        FOR attempt IN 1..32 LOOP
            candidate := 1 + floor(random() * 16777215)::bigint;
            IF NOT EXISTS (SELECT 1 FROM public.workspace_pull_requests
                            WHERE org_id = NEW.org_id AND number = candidate) THEN
                NEW.number := candidate;
                RETURN NEW;
            END IF;
        END LOOP;
        RAISE EXCEPTION 'no free G5 pull request number in organization %', NEW.org_id;
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER trg_wpr_take_number BEFORE INSERT ON public.workspace_pull_requests
    FOR EACH ROW EXECUTE FUNCTION public.workspace_pull_request_take_number();

-- Give back the temporary SET-capable membership exactly as it was.
DO $$
DECLARE
    membership_snapshot jsonb :=
        COALESCE(
            current_setting('g5.pr_numbers_membership_snapshot', true),
            '[]'
        )::jsonb;
    membership_row record;
    prior_row jsonb;
BEGIN
    IF current_setting('g5.pr_numbers_had_set', true)
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
