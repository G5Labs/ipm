-- Comment threads (ontology plan `.g5/ontology-plan-comments.md`).
--
-- g5.governance.review-thread.attribution: a thread message written by an
-- agent names the agent and the person it acted for, so a reply records
-- whether a person or that person's agent wrote it. Existing replies were
-- written by people.
ALTER TABLE public.comment_replies
    ADD COLUMN author_kind text DEFAULT 'person'::text NOT NULL,
    ADD CONSTRAINT comment_replies_author_kind_check
        CHECK (author_kind = ANY (ARRAY['person'::text, 'agent'::text]));

COMMENT ON COLUMN public.comment_replies.author_kind IS
    'person, or agent when the author''s agent wrote the message on their behalf';

-- g5.security.tenant.rls: mention edges are tenant data. Each row gets its
-- workspace's organization (backfilled by 053) before 054 requires it and
-- forces row-level security as on comment_index.
ALTER TABLE public.comment_mentions ADD COLUMN org_id uuid;
