-- Retire agent review handoffs and replies while retaining shared human reviews.
-- Use the documented definer-role data scope so a non-superuser migration sees
-- every tenant. Restore membership and table grants before committing.
SELECT set_config(
    'g5.agent_review_cleanup_membership_snapshot',
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
    'g5.agent_review_cleanup_had_set',
    CASE WHEN pg_has_role(SESSION_USER, 'authz_lookup_definer', 'SET')
         THEN 'true' ELSE 'false' END,
    false
);
DO $$
BEGIN
    IF current_setting('g5.agent_review_cleanup_had_set', true)
        IS DISTINCT FROM 'true'
    THEN
        EXECUTE format(
            'GRANT authz_lookup_definer TO %I WITH SET TRUE',
            SESSION_USER
        );
    END IF;
END $$;
SELECT set_config(
    'g5.agent_review_cleanup_grants',
    COALESCE((
        SELECT jsonb_agg(jsonb_build_object('table', t.name, 'privilege', p.name))
        FROM (VALUES ('review_publications'), ('review_findings'),
                     ('review_suggestions'), ('comment_index'), ('comment_replies')) t(name)
        CROSS JOIN (VALUES ('SELECT'), ('UPDATE'), ('DELETE')) p(name)
        WHERE NOT has_table_privilege('authz_lookup_definer', 'public.' || t.name, p.name)
    ), '[]'::jsonb)::text,
    false
);
DO $$
DECLARE item jsonb;
BEGIN
    FOR item IN SELECT value FROM jsonb_array_elements(current_setting('g5.agent_review_cleanup_grants')::jsonb)
    LOOP
        EXECUTE format('GRANT %s ON TABLE public.%I TO authz_lookup_definer', item->>'privilege', item->>'table');
    END LOOP;
END $$;

CREATE TEMP TABLE removed_agent_review_threads (id uuid PRIMARY KEY) ON COMMIT DROP;
GRANT SELECT, INSERT ON removed_agent_review_threads TO authz_lookup_definer;
SET ROLE authz_lookup_definer;
INSERT INTO removed_agent_review_threads (id)
WITH removed_suggestions AS (
    SELECT s.comment_id, s.edits
    FROM public.review_suggestions s
    JOIN public.review_publications p ON p.id = s.publication_id
    WHERE p.author_kind = 'agent' OR s.author_kind = 'agent'
       OR s.agent_task_id IS NOT NULL OR s.kind = 'assessment'
)
SELECT comment_id AS id FROM removed_suggestions WHERE comment_id IS NOT NULL
UNION
SELECT (edit->>'commentId')::uuid
FROM removed_suggestions, jsonb_array_elements(edits) AS edit
WHERE edit->>'commentId' IS NOT NULL;

DELETE FROM public.review_suggestions
WHERE author_kind = 'agent' OR agent_task_id IS NOT NULL OR kind = 'assessment';
-- Cascades to the remaining suggestions and findings of agent publications.
DELETE FROM public.review_publications WHERE author_kind = 'agent';

-- A retained edit must not point to a thread removed with an agent proposal.
UPDATE public.review_suggestions s
SET edits = (
    SELECT jsonb_agg(
        CASE WHEN t.id IS NOT NULL THEN jsonb_set(e.edit, '{commentId}', 'null'::jsonb)
             ELSE e.edit END ORDER BY e.position
    )
    FROM jsonb_array_elements(s.edits) WITH ORDINALITY AS e(edit, position)
    LEFT JOIN removed_agent_review_threads t ON t.id::text = e.edit->>'commentId'
)
WHERE EXISTS (
    SELECT 1 FROM jsonb_array_elements(s.edits) e
    JOIN removed_agent_review_threads t ON t.id::text = e->>'commentId'
);
-- Thread deletion also removes its replies and mentions. The root suggestion
-- reference is cleared by its existing ON DELETE SET NULL foreign key.
DELETE FROM public.comment_index WHERE id IN (SELECT id FROM removed_agent_review_threads);
DELETE FROM public.comment_replies WHERE author_kind = 'agent';

RESET ROLE;
DO $$
DECLARE item jsonb;
BEGIN
    FOR item IN SELECT value FROM jsonb_array_elements(current_setting('g5.agent_review_cleanup_grants')::jsonb)
    LOOP
        EXECUTE format('REVOKE %s ON TABLE public.%I FROM authz_lookup_definer', item->>'privilege', item->>'table');
    END LOOP;
END $$;

ALTER TABLE public.comment_replies DROP COLUMN author_kind;
ALTER TABLE public.review_suggestions DROP COLUMN agent_task_id;
ALTER TABLE public.review_suggestions DROP COLUMN assessment;
ALTER TABLE public.review_suggestions DROP CONSTRAINT review_suggestions_kind_check;
ALTER TABLE public.review_suggestions ADD CONSTRAINT review_suggestions_kind_check
    CHECK (kind IN ('hunk', 'new_file', 'comment', 'artifact'));
ALTER TABLE public.review_publications DROP CONSTRAINT review_publications_author_kind_check;
ALTER TABLE public.review_publications ADD CONSTRAINT review_publications_author_kind_check
    CHECK (author_kind IN ('human', 'platform'));
ALTER TABLE public.review_suggestions DROP CONSTRAINT review_suggestions_author_kind_check;
ALTER TABLE public.review_suggestions ADD CONSTRAINT review_suggestions_author_kind_check
    CHECK (author_kind IN ('human', 'platform'));


-- Give back the temporary SET-capable membership exactly as it was.
DO $$
DECLARE
    membership_snapshot jsonb :=
        COALESCE(
            current_setting('g5.agent_review_cleanup_membership_snapshot', true),
            '[]'
        )::jsonb;
    membership_row record;
    prior_row jsonb;
BEGIN
    IF current_setting('g5.agent_review_cleanup_had_set', true)
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
