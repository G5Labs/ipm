-- Sending a chat message to a workspace for its agent to work on
-- (docs/external-issue-integration-design.md, "Forking and starting work").
--
-- A message is queued as a session input in one transaction; the agent task
-- it creates runs through the normal dispatch path. A caller that may retry
-- (a lost response, a double submit) names the request with a key: the
-- first request's chat and input are returned instead of sending again.

CREATE TABLE public.workspace_message_requests (
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    requested_by uuid NOT NULL,
    request_key text NOT NULL,
    -- SHA-256 of the message and its target: a key reused for a different
    -- message is refused.
    request_digest text NOT NULL,
    -- Set in the same transaction that claims the key.
    session_id uuid,
    input_id uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT workspace_message_requests_pkey
        PRIMARY KEY (org_id, requested_by, request_key),
    CONSTRAINT workspace_message_requests_key_check CHECK (
        request_key = btrim(request_key) AND length(request_key) BETWEEN 1 AND 200
    ),
    CONSTRAINT workspace_message_requests_digest_check
        CHECK (request_digest ~ '^[0-9a-f]{64}$'),
    CONSTRAINT workspace_message_requests_workspace_same_org_fk
        FOREIGN KEY (org_id, workspace_id)
        REFERENCES public.workspaces(org_id, id) ON DELETE CASCADE,
    CONSTRAINT workspace_message_requests_session_fk
        FOREIGN KEY (session_id) REFERENCES public.chat_sessions(id) ON DELETE CASCADE,
    CONSTRAINT workspace_message_requests_input_fk
        FOREIGN KEY (input_id) REFERENCES public.session_inputs(id) ON DELETE CASCADE,
    CONSTRAINT workspace_message_requests_requested_by_fk
        FOREIGN KEY (requested_by) REFERENCES public.users(id) ON DELETE CASCADE
);

CREATE TRIGGER trg_workspace_message_requests_set_org
    BEFORE INSERT ON public.workspace_message_requests
    FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

ALTER TABLE public.workspace_message_requests ENABLE ROW LEVEL SECURITY;
ALTER TABLE ONLY public.workspace_message_requests FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON public.workspace_message_requests
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));

GRANT SELECT, INSERT, UPDATE ON TABLE public.workspace_message_requests TO g5_tenant;

-- A fork can start a chat in the new workspace. The first message is part
-- of the accepted request and is frozen on the fork; the fork's completion
-- transaction queues it. Starting the chat never undoes a completed fork:
-- if it cannot start, the reason is recorded and the workspace is ready.
ALTER TABLE public.workspace_forks
    ADD COLUMN start_chat_content text,
    ADD COLUMN started_session_id uuid,
    ADD COLUMN start_chat_error text,
    ADD CONSTRAINT workspace_forks_start_chat_check CHECK (
        (start_chat_content IS NULL AND started_session_id IS NULL
         AND start_chat_error IS NULL)
        OR (btrim(start_chat_content) <> ''
            AND (started_session_id IS NULL OR start_chat_error IS NULL)
            AND ((started_session_id IS NULL AND start_chat_error IS NULL)
                 OR status = 'completed'))
    ),
    ADD CONSTRAINT workspace_forks_started_session_fk
        FOREIGN KEY (started_session_id) REFERENCES public.chat_sessions(id) ON DELETE SET NULL;

-- The first message is part of the accepted request, and its outcome is
-- recorded once.
CREATE FUNCTION public.g5_workspace_fork_start_chat_is_immutable() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog'
    AS $$
BEGIN
    IF NEW.start_chat_content IS DISTINCT FROM OLD.start_chat_content
       OR (OLD.start_chat_error IS NOT NULL
           AND NEW.start_chat_error IS DISTINCT FROM OLD.start_chat_error)
       OR (OLD.started_session_id IS NOT NULL
           AND NEW.started_session_id IS NOT NULL
           AND NEW.started_session_id IS DISTINCT FROM OLD.started_session_id)
    THEN
        RAISE EXCEPTION 'workspace fork chat request is immutable' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER workspace_forks_immutable_start_chat
    BEFORE UPDATE ON public.workspace_forks
    FOR EACH ROW EXECUTE FUNCTION public.g5_workspace_fork_start_chat_is_immutable();
