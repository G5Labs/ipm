-- Document view metadata comes from repository files, not these retired
-- database fields. Retain content and commit markers: UUID document reads
-- still use them. No document or discussion row is deleted.
ALTER TABLE public.docs
    DROP CONSTRAINT docs_persona_check,
    DROP COLUMN persona,
    DROP COLUMN instruction,
    DROP COLUMN is_future,
    DROP COLUMN review_iterations,
    DROP COLUMN custom_persona_label,
    DROP COLUMN source_files,
    DROP COLUMN mapping_content;
