-- Native device login keeps tokens in the owner-bound VM. Only the short-lived
-- device challenge is encrypted here so the owner can reconnect to the chat.
ALTER TABLE public.task_attempts
    ADD COLUMN codex_auth_state jsonb,
    ADD COLUMN codex_auth_challenge_enc bytea,
    ADD CONSTRAINT task_attempts_codex_auth_state CHECK (
        codex_auth_state IS NULL OR (
            jsonb_typeof(codex_auth_state) = 'object'
            AND codex_auth_state ->> 'phase' IN ('checking','waiting','ready','superseded','ended')
        )
    ),
    ADD CONSTRAINT task_attempts_codex_auth_secret_lifetime CHECK (
        codex_auth_challenge_enc IS NULL OR codex_auth_state ->> 'phase' = 'waiting'
    );
