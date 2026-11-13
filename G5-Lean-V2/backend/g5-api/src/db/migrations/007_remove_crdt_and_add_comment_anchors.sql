-- P2.2/P2.4: explicit-Save comment identity and CRDT storage retirement.
--
-- Existing comments remain readable. New line comments require the exact
-- line/content hashes at the service boundary; nullable columns preserve
-- historical rows whose source bytes are no longer reconstructable.

ALTER TABLE public.comment_index
    ADD COLUMN line_hash text,
    ADD COLUMN content_hash text,
    ADD COLUMN anchor_context text,
    ADD CONSTRAINT comment_index_line_hash_shape CHECK (
        line_hash IS NULL OR line_hash ~ '^[0-9a-f]{64}$'
    ),
    ADD CONSTRAINT comment_index_content_hash_shape CHECK (
        content_hash IS NULL OR content_hash ~ '^[0-9a-f]{64}$'
    ),
    ADD CONSTRAINT comment_index_anchor_context_size CHECK (
        anchor_context IS NULL OR octet_length(anchor_context) <= 4096
    );

COMMENT ON COLUMN public.comment_index.line_hash IS
    'SHA-256 of the exact UTF-8 line bytes at comment creation; null only for legacy rows';
COMMENT ON COLUMN public.comment_index.content_hash IS
    'SHA-256 of the exact file bytes displayed at comment creation; null only for legacy rows';
COMMENT ON COLUMN public.comment_index.anchor_context IS
    'Bounded surrounding text used only for hash-confirmed or unique contextual remapping';

DROP TABLE IF EXISTS public.draft_participants CASCADE;
DROP TABLE IF EXISTS public.draft_updates CASCADE;
DROP TABLE IF EXISTS public.draft_sessions CASCADE;
DROP TABLE IF EXISTS public.task_commit_frontier_receipts CASCADE;
DROP TABLE IF EXISTS public.task_commit_frontier_cuts CASCADE;
DROP TABLE IF EXISTS public.task_commit_requests CASCADE;
