-- The launch preferences a fork's first message runs with: the provider,
-- model, account, permission mode and effort the requester chose, resolved
-- for the new workspace when the chat is queued. Part of the accepted request
-- and frozen on the fork. NULL inherits the configured defaults.

ALTER TABLE public.workspace_forks
    ADD COLUMN start_chat_agent_launch jsonb,
    ADD CONSTRAINT workspace_forks_start_chat_agent_launch_check CHECK (
        start_chat_agent_launch IS NULL
        OR (start_chat_content IS NOT NULL
            AND jsonb_typeof(start_chat_agent_launch) = 'object')
    );

CREATE OR REPLACE FUNCTION public.g5_workspace_fork_start_chat_is_immutable() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog'
    AS $$
BEGIN
    IF NEW.start_chat_content IS DISTINCT FROM OLD.start_chat_content
       OR NEW.start_chat_agent_launch IS DISTINCT FROM OLD.start_chat_agent_launch
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
