-- Image attachments of G5's native issues (g5.issue.native.attachments,
-- G5-Lean#1013 and #726). An attachment belongs to its issue; text shows
-- it by its address under the issue page, and where it appears is derived
-- from the text, never stored. Its bytes live in the attachment store
-- (g5.issue.native.attachment-store) as `<org_id>/<storage_id>`, written
-- before this row and deleted after its removal commits.

CREATE TABLE public.native_issue_attachments (
    id bigint GENERATED ALWAYS AS IDENTITY,
    org_id uuid NOT NULL,
    issue_id bigint NOT NULL,
    -- The random id naming its bytes in the attachment store.
    storage_id uuid NOT NULL,
    uploader_user_id uuid NOT NULL,
    -- `person`, or `agent` when a workspace's agent uploaded it as its owner.
    origin text NOT NULL,
    idempotency_key text NOT NULL,
    file_name text NOT NULL,
    -- Recognized from the content, never taken from the upload.
    content_type text NOT NULL,
    size_bytes integer NOT NULL,
    sha256 text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT native_issue_attachments_pkey PRIMARY KEY (id),
    CONSTRAINT native_issue_attachments_idempotency_key
        UNIQUE (org_id, uploader_user_id, idempotency_key),
    CONSTRAINT native_issue_attachments_storage_id UNIQUE (storage_id),
    CONSTRAINT native_issue_attachments_origin_check CHECK (origin IN ('person', 'agent')),
    CONSTRAINT native_issue_attachments_key_check CHECK (btrim(idempotency_key) <> ''),
    CONSTRAINT native_issue_attachments_file_name_check
        CHECK (btrim(file_name) <> '' AND char_length(file_name) <= 255),
    CONSTRAINT native_issue_attachments_type_check
        CHECK (content_type IN ('image/png', 'image/jpeg', 'image/gif', 'image/webp')),
    -- 10 MiB, GitHub's own limit for images.
    CONSTRAINT native_issue_attachments_size_check
        CHECK (size_bytes > 0 AND size_bytes <= 10485760),
    CONSTRAINT native_issue_attachments_sha256_check CHECK (sha256 ~ '^[0-9a-f]{64}$'),
    CONSTRAINT native_issue_attachments_issue_fk
        FOREIGN KEY (org_id, issue_id) REFERENCES public.native_issues(org_id, id)
        ON DELETE CASCADE,
    CONSTRAINT native_issue_attachments_uploader_fk
        FOREIGN KEY (uploader_user_id) REFERENCES public.users(id)
);
CREATE INDEX native_issue_attachments_issue
    ON public.native_issue_attachments (issue_id, created_at, id);

CREATE TRIGGER trg_native_issue_attachments_set_org
    BEFORE INSERT ON public.native_issue_attachments
    FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

ALTER TABLE public.native_issue_attachments ENABLE ROW LEVEL SECURITY;
ALTER TABLE ONLY public.native_issue_attachments FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.native_issue_attachments
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));

-- Attachments are never edited: added, read, or removed.
GRANT SELECT, INSERT, DELETE ON TABLE public.native_issue_attachments TO g5_tenant;
