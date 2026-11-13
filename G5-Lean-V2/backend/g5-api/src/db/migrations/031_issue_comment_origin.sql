-- Who wrote a comment G5 posted: the person themselves, or the workspace's
-- agent posting under the workspace owner's connected GitHub identity
-- (`g5d issue comment`). Agents only post new comments; they do not edit.

ALTER TABLE public.issue_comment_publications
    ADD COLUMN origin text DEFAULT 'person' NOT NULL,
    ADD CONSTRAINT issue_comment_publications_origin_check CHECK (
        origin = 'person' OR (origin = 'agent' AND kind = 'comment')
    );

-- Origin joins the frozen destination and payload.
CREATE OR REPLACE FUNCTION public.g5_issue_comment_publication_is_frozen() RETURNS trigger
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
       OR NEW.kind IS DISTINCT FROM OLD.kind
       OR NEW.target_comment_id IS DISTINCT FROM OLD.target_comment_id
       OR NEW.origin IS DISTINCT FROM OLD.origin
       OR (OLD.state = 'published' AND NEW.state IS DISTINCT FROM OLD.state)
    THEN
        RAISE EXCEPTION 'issue comment publication is frozen' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;
