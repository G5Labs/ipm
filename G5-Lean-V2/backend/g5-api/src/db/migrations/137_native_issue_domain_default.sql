-- 137: G5 issues for every tenant organization by default
-- (issues g5.issue.native.domain, owner instruction 2026-10-29).
--
-- Issues are on unless an organization's switch is set to `false` (the
-- service reads the setting that way from this release), so an organization
-- that never configured a tracker needs one to track issues with. Every
-- tenant organization without any issue domain gets the native G5 tracker as
-- its default creation domain, keyed from its slug as the Admin page would
-- suggest, with trusted content, configured by its first owner. New
-- organizations get the same domain when they are created. Organizations
-- that already configured a domain (GitHub, or G5 issues under their own key)
-- are untouched.

-- Organization creation runs as g5_app, which writes the new organization's
-- rows under the capability minted for it (orgs, memberships, repos and
-- workspaces carry policies for every role). The issue tables were granted
-- and policed for g5_tenant only; g5_app gets the same org-scoped access so
-- it can add the native tracker and domain of a new organization.
GRANT SELECT, INSERT ON TABLE public.issue_trackers TO g5_app;
GRANT SELECT, INSERT ON TABLE public.issue_domains TO g5_app;
CREATE POLICY org_creation ON public.issue_trackers
    TO g5_app
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
CREATE POLICY org_creation ON public.issue_domains
    TO g5_app
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));

-- The backfill touches tenant rows: it runs as the BYPASSRLS definer role
-- (docs/database-migrations.md). Preserve the migrator's exact role
-- membership; no runtime grants on the issue tables remain.
SELECT set_config(
    'g5.native_issue_domain_membership_snapshot',
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
    'g5.native_issue_domain_had_set',
    CASE WHEN pg_has_role(SESSION_USER, 'authz_lookup_definer', 'SET')
         THEN 'true' ELSE 'false' END,
    false
);
DO $$
BEGIN
    IF current_setting('g5.native_issue_domain_had_set', true)
        IS DISTINCT FROM 'true'
    THEN
        EXECUTE format(
            'GRANT authz_lookup_definer TO %I WITH SET TRUE',
            SESSION_USER
        );
    END IF;
END $$;
-- The definer's authorization lookups already read organizations and
-- memberships; those grants stay. The issue tables are granted for this
-- backfill only.
GRANT SELECT ON TABLE public.orgs, public.org_memberships TO authz_lookup_definer;
GRANT SELECT, INSERT ON TABLE public.issue_trackers, public.issue_domains TO authz_lookup_definer;
SET ROLE authz_lookup_definer;
WITH candidates AS (
    SELECT o.id AS org_id,
           (SELECT m.user_id FROM public.org_memberships m
             WHERE m.org_id = o.id AND m.role = 'owner'
             ORDER BY m.created_at LIMIT 1) AS owner_id,
           left(regexp_replace(regexp_replace(upper(o.slug), '[^A-Z0-9]', '', 'g'), '^[0-9]+', ''), 10) AS slug_key
      FROM public.orgs o
     WHERE o.kind = 'tenant'
       AND NOT EXISTS (SELECT 1 FROM public.issue_domains d WHERE d.org_id = o.id)
),
inserted_trackers AS (
    INSERT INTO public.issue_trackers (org_id, provider, instance_key, display_name, web_base_url, api_base_url)
    SELECT c.org_id, 'g5', 'g5', 'G5', '', ''
      FROM candidates c
     WHERE c.owner_id IS NOT NULL
    ON CONFLICT ON CONSTRAINT issue_trackers_instance_key DO NOTHING
    RETURNING id, org_id
)
INSERT INTO public.issue_domains
    (org_id, tracker_id, provider, container_kind, external_container_id, display_key,
     locator, is_default_create_target, content_trusted, configured_by)
SELECT c.org_id,
       COALESCE(t.id, existing.id),
       'g5', 'g5_project', '1',
       CASE WHEN length(c.slug_key) >= 2 THEN c.slug_key ELSE 'G5' END,
       '{}'::jsonb, true, true, c.owner_id
  FROM candidates c
  LEFT JOIN inserted_trackers t ON t.org_id = c.org_id
  LEFT JOIN public.issue_trackers existing
         ON existing.org_id = c.org_id AND existing.provider = 'g5' AND existing.instance_key = 'g5'
 WHERE c.owner_id IS NOT NULL
   AND COALESCE(t.id, existing.id) IS NOT NULL;
RESET ROLE;
REVOKE SELECT, INSERT ON TABLE public.issue_trackers, public.issue_domains FROM authz_lookup_definer;
DO $$
DECLARE
    membership_snapshot jsonb :=
        COALESCE(
            current_setting('g5.native_issue_domain_membership_snapshot', true),
            '[]'
        )::jsonb;
    membership_row record;
    prior_row jsonb;
BEGIN
    IF current_setting('g5.native_issue_domain_had_set', true)
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
