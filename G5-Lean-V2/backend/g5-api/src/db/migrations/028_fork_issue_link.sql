-- Link an existing external issue to the workspace a fork creates
-- (docs/external-issue-integration-design.md, "Forking and starting work").
-- The issue is resolved when the fork is admitted; its identity and the
-- requested role are part of the accepted request and are frozen on the
-- fork. The association is created in the fork's completion transaction,
-- and issue_link_id records it.

ALTER TABLE public.workspace_forks
    ADD COLUMN issue_tracker_id uuid,
    -- GitHub REST issue id as a decimal string, as on workspace_issue_links.
    ADD COLUMN issue_external_id text,
    ADD COLUMN issue_display_key text,
    ADD COLUMN issue_canonical_url text,
    ADD COLUMN issue_locator jsonb,
    -- The role the requester asked for. A primary claim that another
    -- workspace won while the fork ran is linked as related instead.
    ADD COLUMN issue_role text,
    ADD COLUMN issue_link_id uuid,
    ADD CONSTRAINT workspace_forks_issue_intent_check CHECK (
        (issue_tracker_id IS NULL AND issue_external_id IS NULL
         AND issue_display_key IS NULL AND issue_canonical_url IS NULL
         AND issue_locator IS NULL AND issue_role IS NULL)
        OR (issue_tracker_id IS NOT NULL
            AND issue_external_id ~ '^[1-9][0-9]*$'
            AND btrim(issue_display_key) <> ''
            AND btrim(issue_canonical_url) <> ''
            AND jsonb_typeof(issue_locator) = 'object'
            AND issue_role IN ('primary', 'related'))
    ),
    ADD CONSTRAINT workspace_forks_issue_link_check CHECK (
        issue_link_id IS NULL OR (issue_tracker_id IS NOT NULL AND status = 'completed')
    ),
    ADD CONSTRAINT workspace_forks_issue_tracker_fk
        FOREIGN KEY (org_id, issue_tracker_id)
        REFERENCES public.issue_trackers(org_id, id),
    ADD CONSTRAINT workspace_forks_issue_link_fk
        FOREIGN KEY (issue_link_id)
        REFERENCES public.workspace_issue_links(id);

-- The requested issue is part of the accepted request: it never changes,
-- and the resulting association is recorded once.
CREATE FUNCTION public.g5_workspace_fork_issue_is_immutable() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog'
    AS $$
BEGIN
    IF NEW.issue_tracker_id IS DISTINCT FROM OLD.issue_tracker_id
       OR NEW.issue_external_id IS DISTINCT FROM OLD.issue_external_id
       OR NEW.issue_display_key IS DISTINCT FROM OLD.issue_display_key
       OR NEW.issue_canonical_url IS DISTINCT FROM OLD.issue_canonical_url
       OR NEW.issue_locator IS DISTINCT FROM OLD.issue_locator
       OR NEW.issue_role IS DISTINCT FROM OLD.issue_role
       OR (OLD.issue_link_id IS NOT NULL
           AND NEW.issue_link_id IS DISTINCT FROM OLD.issue_link_id)
    THEN
        RAISE EXCEPTION 'workspace fork issue request is immutable' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER workspace_forks_immutable_issue
    BEFORE UPDATE ON public.workspace_forks
    FOR EACH ROW EXECUTE FUNCTION public.g5_workspace_fork_issue_is_immutable();
