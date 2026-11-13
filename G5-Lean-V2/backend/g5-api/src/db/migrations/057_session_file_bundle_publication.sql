-- A provider session is resumable only after the complete harvested file set
-- is committed. Starting a turn fences the previous set before provider launch.
CREATE TABLE public.chat_session_file_bundles (
    session_id uuid PRIMARY KEY,
    org_id uuid NOT NULL REFERENCES public.orgs(id),
    generation uuid NOT NULL,
    state text NOT NULL CHECK (state IN ('pending', 'complete')),
    provider text,
    provider_session_id text,
    fenced_task_id uuid,
    fenced_attempt integer,
    fenced_reservation_id uuid,
    fenced_vm_id text,
    fenced_epoch bigint,
    cwd_relative_path text NOT NULL DEFAULT '',
    files jsonb NOT NULL DEFAULT '[]'::jsonb CHECK (jsonb_typeof(files) = 'array'),
    prior_publication jsonb,
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT chat_session_file_bundles_complete_shape CHECK (
        state = 'pending' OR (provider IN ('claude_code', 'codex') AND provider_session_id IS NOT NULL AND jsonb_array_length(files) > 0)
    ),
    CONSTRAINT chat_session_file_bundles_session_org_fk
        FOREIGN KEY (org_id, session_id) REFERENCES public.chat_sessions(org_id, id) ON DELETE CASCADE
);
CREATE INDEX idx_chat_session_file_bundles_org ON public.chat_session_file_bundles(org_id);
CREATE TRIGGER trg_chat_session_file_bundles_set_org BEFORE INSERT ON public.chat_session_file_bundles
    FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();
ALTER TABLE public.chat_session_file_bundles ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.chat_session_file_bundles FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.chat_session_file_bundles
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
GRANT SELECT, INSERT, UPDATE ON public.chat_session_file_bundles TO g5_tenant;
GRANT SELECT ON public.chat_session_file_bundles TO g5_app;
