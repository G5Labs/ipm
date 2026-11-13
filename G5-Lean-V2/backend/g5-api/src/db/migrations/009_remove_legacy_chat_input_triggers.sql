-- Migration 008 removed the legacy chat-session discriminator. These
-- projections belonged only to the compatibility write path and must not run
-- against the retained shared-session schema.
DROP TRIGGER IF EXISTS chat_messages_project_legacy_input_insert
    ON public.chat_messages;
DROP TRIGGER IF EXISTS chat_messages_settle_legacy_pending_input
    ON public.chat_messages;

DROP FUNCTION IF EXISTS public.g5_project_legacy_chat_message_input();
DROP FUNCTION IF EXISTS public.g5_settle_legacy_pending_chat_input();
