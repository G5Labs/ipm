-- Review suggestions: the deliverable of a review workspace becomes rows in
-- the database instead of a pull request. A reviewer's Push review records a
-- publication (one round of their review of a source workspace) holding
-- findings (the organising unit of a fix-first review), suggestions (a hunk,
-- a new file, a pure comment, an artifact, or an assessment) and their
-- threads (existing comment_index rows with a new scope). The owner acts on
-- suggestions from the source workspace: accept writes into the owner's
-- running working tree (no commit is created), reject and reply record the
-- decision in the thread. Nothing here moves a Git ref.

CREATE TABLE public.review_publications (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    source_workspace_id uuid NOT NULL,
    -- NULL for a comments-only publication made without a review workspace.
    review_workspace_id uuid,
    reviewer_user_id uuid NOT NULL,
    author_kind text DEFAULT 'human'::text NOT NULL,
    author_label text NOT NULL,
    round integer NOT NULL,
    base_commit_vector jsonb DEFAULT '[]'::jsonb NOT NULL,
    summary text DEFAULT ''::text NOT NULL,
    verdict text,
    scores jsonb,
    superseded_by uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT review_publications_pkey PRIMARY KEY (id),
    CONSTRAINT review_publications_author_kind_check
        CHECK (author_kind = ANY (ARRAY['human'::text, 'agent'::text, 'platform'::text])),
    CONSTRAINT review_publications_verdict_check
        CHECK (verdict IS NULL OR verdict = ANY (ARRAY['approve'::text, 'request_changes'::text, 'comment_only'::text])),
    CONSTRAINT review_publications_round_positive CHECK (round >= 1),
    CONSTRAINT review_publications_round_unique
        UNIQUE (source_workspace_id, reviewer_user_id, author_kind, round),
    CONSTRAINT review_publications_source_fkey
        FOREIGN KEY (org_id, source_workspace_id) REFERENCES public.workspaces(org_id, id) ON DELETE CASCADE,
    -- A deleted review workspace leaves its publications in place as
    -- comments-only history; supersession then no longer targets them.
    CONSTRAINT review_publications_review_workspace_fkey
        FOREIGN KEY (review_workspace_id) REFERENCES public.workspaces(id) ON DELETE SET NULL,
    CONSTRAINT review_publications_superseded_by_fkey
        FOREIGN KEY (superseded_by) REFERENCES public.review_publications(id) ON DELETE SET NULL
);
COMMENT ON TABLE public.review_publications IS
    'One round of one reviewer''s review of a source workspace (Push review). Rows are immutable except superseded_by.';
CREATE INDEX review_publications_source_idx
    ON public.review_publications (source_workspace_id, created_at DESC);
CREATE INDEX review_publications_review_workspace_idx
    ON public.review_publications (review_workspace_id) WHERE review_workspace_id IS NOT NULL;

CREATE TABLE public.review_findings (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    publication_id uuid NOT NULL,
    title text NOT NULL,
    severity text DEFAULT 'P2'::text NOT NULL,
    dimension text DEFAULT ''::text NOT NULL,
    evidence text DEFAULT ''::text NOT NULL,
    -- Structured grounding review actions (Answer/Fix/Implement/Clear); never a ledger diff.
    actions jsonb DEFAULT '[]'::jsonb NOT NULL,
    status text DEFAULT 'open'::text NOT NULL,
    position integer DEFAULT 0 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT review_findings_pkey PRIMARY KEY (id),
    CONSTRAINT review_findings_severity_check
        CHECK (severity = ANY (ARRAY['P0'::text, 'P1'::text, 'P2'::text, 'P3'::text])),
    CONSTRAINT review_findings_status_check
        CHECK (status = ANY (ARRAY['open'::text, 'accepted'::text, 'rejected'::text, 'superseded'::text])),
    CONSTRAINT review_findings_publication_fkey
        FOREIGN KEY (publication_id) REFERENCES public.review_publications(id) ON DELETE CASCADE
);
CREATE INDEX review_findings_publication_idx ON public.review_findings (publication_id, position);

CREATE TABLE public.review_suggestions (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    publication_id uuid NOT NULL,
    finding_id uuid,
    repo_id uuid,
    path text,
    kind text NOT NULL,
    -- hunk / new_file: exact text the reviewer replaces and proposes.
    before_lines text,
    after_lines text,
    -- Anchor on the source's content (not on a commit): 1-based line the
    -- hunk starts at when published, SHA-256 of the whole file it was
    -- computed against, and a bounded context window for relocation.
    anchor_start integer,
    content_hash text,
    anchor_context text,
    -- artifact: bytes live in the blob store.
    blob_id uuid,
    blob_sha256 text,
    bytes bigint,
    -- assessment: block review metadata / scores / report, never a hunk.
    assessment jsonb,
    -- Authorship: the reviewer (permissions, attribution) and whether their
    -- agent wrote it (label such as "kaihuiz-agent").
    author_kind text DEFAULT 'human'::text NOT NULL,
    author_label text NOT NULL,
    agent_task_id uuid,
    note text DEFAULT ''::text NOT NULL,
    -- Thread: an existing comment_index row (scope 'suggestion'); replies are comment_replies.
    comment_id uuid,
    status text DEFAULT 'open'::text NOT NULL,
    status_by uuid,
    status_at timestamp with time zone,
    supersedes uuid,
    adopted_from uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT review_suggestions_pkey PRIMARY KEY (id),
    CONSTRAINT review_suggestions_kind_check
        CHECK (kind = ANY (ARRAY['hunk'::text, 'new_file'::text, 'comment'::text, 'artifact'::text, 'assessment'::text])),
    CONSTRAINT review_suggestions_status_check
        CHECK (status = ANY (ARRAY['open'::text, 'accepted'::text, 'rejected'::text, 'withdrawn'::text, 'superseded'::text, 'obsolete'::text, 'resolved'::text])),
    CONSTRAINT review_suggestions_author_kind_check
        CHECK (author_kind = ANY (ARRAY['human'::text, 'agent'::text, 'platform'::text])),
    CONSTRAINT review_suggestions_hunk_shape
        CHECK (kind <> 'hunk' OR (repo_id IS NOT NULL AND path IS NOT NULL AND before_lines IS NOT NULL AND after_lines IS NOT NULL)),
    CONSTRAINT review_suggestions_new_file_shape
        CHECK (kind <> 'new_file' OR (repo_id IS NOT NULL AND path IS NOT NULL AND after_lines IS NOT NULL)),
    CONSTRAINT review_suggestions_comment_shape
        CHECK (kind <> 'comment' OR (repo_id IS NOT NULL AND path IS NOT NULL)),
    CONSTRAINT review_suggestions_content_hash_shape
        CHECK (content_hash IS NULL OR content_hash ~ '^[0-9a-f]{64}$'),
    CONSTRAINT review_suggestions_anchor_context_size
        CHECK (anchor_context IS NULL OR octet_length(anchor_context) <= 4096),
    CONSTRAINT review_suggestions_publication_fkey
        FOREIGN KEY (publication_id) REFERENCES public.review_publications(id) ON DELETE CASCADE,
    CONSTRAINT review_suggestions_finding_fkey
        FOREIGN KEY (finding_id) REFERENCES public.review_findings(id) ON DELETE SET NULL,
    CONSTRAINT review_suggestions_comment_fkey
        FOREIGN KEY (comment_id) REFERENCES public.comment_index(id) ON DELETE SET NULL,
    CONSTRAINT review_suggestions_supersedes_fkey
        FOREIGN KEY (supersedes) REFERENCES public.review_suggestions(id) ON DELETE SET NULL,
    CONSTRAINT review_suggestions_adopted_from_fkey
        FOREIGN KEY (adopted_from) REFERENCES public.review_suggestions(id) ON DELETE SET NULL
);
CREATE INDEX review_suggestions_publication_idx ON public.review_suggestions (publication_id);
CREATE INDEX review_suggestions_path_idx ON public.review_suggestions (repo_id, path) WHERE path IS NOT NULL;
CREATE INDEX review_suggestions_finding_idx ON public.review_suggestions (finding_id) WHERE finding_id IS NOT NULL;

-- Threads of suggestions live in comment_index under a new scope.
ALTER TABLE public.comment_index DROP CONSTRAINT comment_index_scope_check;
ALTER TABLE public.comment_index
    ADD CONSTRAINT comment_index_scope_check
    CHECK (scope = ANY (ARRAY['line'::text, 'node'::text, 'suggestion'::text]));

-- Tenant isolation and grants follow comment_index.
ALTER TABLE public.review_publications ENABLE ROW LEVEL SECURITY;
ALTER TABLE ONLY public.review_publications FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.review_publications USING ((org_id = ANY (authz.current_org_ids())));
ALTER TABLE public.review_findings ENABLE ROW LEVEL SECURITY;
ALTER TABLE ONLY public.review_findings FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.review_findings USING ((org_id = ANY (authz.current_org_ids())));
ALTER TABLE public.review_suggestions ENABLE ROW LEVEL SECURITY;
ALTER TABLE ONLY public.review_suggestions FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.review_suggestions USING ((org_id = ANY (authz.current_org_ids())));
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.review_publications TO g5_tenant;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.review_findings TO g5_tenant;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.review_suggestions TO g5_tenant;
