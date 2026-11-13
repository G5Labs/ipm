-- Comments on a provider pull request's conversation, posted from G5 under the
-- acting person's GitHub identity (g5.governance.pr.conversation, issue #387):
-- `g5d pr comment` posts to a PR's discussion, `--reply-to THREAD` replies in a
-- review thread. They reuse the issue-comment publication ledger: the exact
-- payload with its hidden marker is recorded before sending, and an unclear
-- outcome is reconciled by finding the marker, never by sending again. GitHub
-- remains the store of the conversation.
--
-- A PR destination has no issue domain or tracker. `external_issue_id` holds
-- the provider PR's number and `locator` its repository name and number.
--
-- Additive DDL only: existing rows take destination 'issue' and keep their
-- domain and tracker, so no row-level data section runs under FORCE ROW LEVEL
-- SECURITY (docs/database-migrations.md).

ALTER TABLE public.issue_comment_publications
    ADD COLUMN destination text DEFAULT 'issue' NOT NULL,
    -- The review thread (GitHub node ID) a reply is posted in.
    ADD COLUMN target_thread_id text,
    ALTER COLUMN domain_id DROP NOT NULL,
    ALTER COLUMN tracker_id DROP NOT NULL,
    ADD CONSTRAINT issue_comment_publications_destination_check CHECK (
        (destination = 'issue'
            AND domain_id IS NOT NULL AND tracker_id IS NOT NULL
            AND target_thread_id IS NULL)
        OR (destination = 'pull_request'
            AND domain_id IS NULL AND tracker_id IS NULL
            AND kind = 'comment' AND target_thread_id IS NULL)
        OR (destination = 'review_thread'
            AND domain_id IS NULL AND tracker_id IS NULL
            AND kind = 'comment'
            AND target_thread_id ~ '^[A-Za-z0-9_=-]{1,200}$')
    );

-- Destination and thread join the frozen destination and payload. A separate
-- trigger, not a replacement of g5_issue_comment_publication_is_frozen: this
-- migration owns what it creates, while that function may belong to the role
-- that built the pre-045 schema.
CREATE FUNCTION public.g5_comment_publication_destination_is_frozen() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog'
    AS $$
BEGIN
    IF NEW.destination IS DISTINCT FROM OLD.destination
       OR NEW.target_thread_id IS DISTINCT FROM OLD.target_thread_id
    THEN
        RAISE EXCEPTION 'issue comment publication is frozen' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER issue_comment_publications_destination_frozen
    BEFORE UPDATE OF destination, target_thread_id ON public.issue_comment_publications
    FOR EACH ROW EXECUTE FUNCTION public.g5_comment_publication_destination_is_frozen();
