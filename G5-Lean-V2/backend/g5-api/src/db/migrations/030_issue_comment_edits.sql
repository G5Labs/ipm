-- Edits of a person's own comments on a workspace's primary issue, made
-- from G5 under their GitHub identity. An edit is recorded like a posted
-- comment: its destination and exact replacement text are frozen before it
-- is sent. Replacing a comment's text is safe to repeat, so an edit is never
-- left with an unknown outcome.

ALTER TABLE public.issue_comment_publications
    ADD COLUMN kind text DEFAULT 'comment' NOT NULL,
    -- The GitHub comment an edit replaces.
    ADD COLUMN target_comment_id text,
    ADD CONSTRAINT issue_comment_publications_kind_check CHECK (
        (kind = 'comment' AND target_comment_id IS NULL)
        OR (kind = 'edit'
            AND target_comment_id ~ '^[1-9][0-9]*$'
            AND state <> 'outcome_unknown'
            AND (external_comment_id IS NULL OR external_comment_id = target_comment_id))
    );

-- Kind and target join the frozen destination and payload.
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
       OR (OLD.state = 'published' AND NEW.state IS DISTINCT FROM OLD.state)
    THEN
        RAISE EXCEPTION 'issue comment publication is frozen' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;
