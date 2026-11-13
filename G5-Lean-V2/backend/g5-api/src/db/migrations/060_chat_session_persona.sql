-- Chat persona (g5.workbench.persona): a chat session records the persona it
-- started with ('be' | 'se' | 'ie') and the persona instructions resolved
-- once, after the request key was claimed, from the workbench configuration
-- snapshot of that moment. The instructions are frozen on the session so a
-- withdrawn or edited first input can still be bootstrapped with them at
-- delivery, and a fork inherits them without re-reading Git. NULL for every
-- session started outside the workbench.
-- Additive DDL only: existing rows stay NULL, so no row-level data section
-- runs under FORCE ROW LEVEL SECURITY (docs/database-migrations.md).
ALTER TABLE public.chat_sessions
    ADD COLUMN persona text
    CONSTRAINT chat_sessions_persona_check CHECK (persona IS NULL OR persona IN ('be', 'se', 'ie')),
    ADD COLUMN persona_instructions text
    CONSTRAINT chat_sessions_persona_instructions_pair
        CHECK ((persona IS NULL) = (persona_instructions IS NULL));
