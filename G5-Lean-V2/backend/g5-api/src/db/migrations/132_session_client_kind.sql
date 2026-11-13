-- Person sessions record how they were issued (G5-Lean#873,
-- g5.security.browser-session-actions): `browser` for the web sign-in,
-- `cli` for CLI device login. Refresh keeps the kind on the same row, and the
-- access JWT carries it as the `client` claim, so an action reserved for the
-- web UI can refuse a CLI session of the same person.
--
-- Rows issued before this migration stay NULL, which means unknown: they are
-- never treated as browser sessions, so a person signs in once more before a
-- browser-only action.
ALTER TABLE public.refresh_tokens
    ADD COLUMN client_kind text NULL
    CONSTRAINT refresh_tokens_client_kind_check CHECK (client_kind IN ('browser', 'cli'));

COMMENT ON COLUMN public.refresh_tokens.client_kind IS
    'How the session was issued: browser (web sign-in) or cli (CLI device login); NULL for sessions issued before G5-Lean#873.';
