-- Bound the amount of stored data rewritten for a native-session append.
-- Existing rows keep their complete bytes and a NULL digest state until a
-- whole upload replaces them; no historical file bytes are migrated.
ALTER TABLE public.chat_session_files ADD COLUMN sha256_state bytea;
ALTER TABLE public.chat_session_files DROP CONSTRAINT chat_session_files_size_check;
ALTER TABLE public.chat_session_files ADD CONSTRAINT chat_session_files_size_check
    CHECK (size >= 0 AND size <= 134217728 AND octet_length(bytes) <= size);
ALTER TABLE public.chat_session_files ADD CONSTRAINT chat_session_files_org_file_key
    UNIQUE (org_id, session_id, provider_session_id, relative_path);

CREATE TABLE public.chat_session_file_segments (
    session_id uuid NOT NULL,
    org_id uuid NOT NULL REFERENCES public.orgs(id),
    provider_session_id text NOT NULL,
    relative_path text NOT NULL,
    segment_index integer NOT NULL CHECK (segment_index >= 1),
    bytes bytea NOT NULL,
    size integer NOT NULL CHECK (size > 0 AND size <= 8388608 AND octet_length(bytes) = size),
    CONSTRAINT chat_session_file_segments_pkey
        PRIMARY KEY (session_id, provider_session_id, relative_path, segment_index),
    CONSTRAINT chat_session_file_segments_same_org_fk
        FOREIGN KEY (org_id, session_id, provider_session_id, relative_path)
        REFERENCES public.chat_session_files(org_id, session_id, provider_session_id, relative_path)
        ON DELETE CASCADE
);
CREATE INDEX idx_chat_session_file_segments_org ON public.chat_session_file_segments(org_id);
CREATE TRIGGER trg_chat_session_file_segments_set_org
    BEFORE INSERT ON public.chat_session_file_segments
    FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();
ALTER TABLE public.chat_session_file_segments ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.chat_session_file_segments FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.chat_session_file_segments
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
GRANT SELECT, INSERT, UPDATE, DELETE ON public.chat_session_file_segments TO g5_tenant;
GRANT SELECT ON public.chat_session_file_segments TO g5_app;

-- A prior API version writes a whole file into the parent bytea. Clear any
-- segments if that happens during a rolling deployment or rollback, so a
-- later new API cannot concatenate stale child bytes onto that rewrite.
CREATE FUNCTION public.clear_session_file_segments_on_parent_rewrite()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    DELETE FROM public.chat_session_file_segments
     WHERE session_id = NEW.session_id
       AND provider_session_id = NEW.provider_session_id
       AND relative_path = NEW.relative_path;
    RETURN NEW;
END;
$$;
CREATE TRIGGER trg_chat_session_files_clear_segments_on_rewrite
    AFTER UPDATE OF bytes ON public.chat_session_files
    FOR EACH ROW EXECUTE FUNCTION public.clear_session_file_segments_on_parent_rewrite();
