-- Distinguish platform-authored explanations from genuine partial agent output.
-- Historical unmarked content is deliberately not classified by its wording.
ALTER TABLE public.chat_messages
    ADD COLUMN is_error_notice boolean NOT NULL DEFAULT false;

ALTER TABLE public.chat_messages
    ADD CONSTRAINT chat_messages_error_notice_role_check
    CHECK (NOT is_error_notice OR role = 'assistant');
