-- Review copy paths remain stable identities for preview, discard and history.
-- Only a published Project Doc proposal has a distinct owner-side destination.
ALTER TABLE public.review_suggestions
    ADD COLUMN target_path text,
    ADD COLUMN target_base text,
    ADD COLUMN applied_content text,
    ADD CONSTRAINT review_document_target_shape CHECK (
        target_path IS NULL OR (kind = 'new_file' AND repo_id IS NOT NULL AND path IS NOT NULL)
    ) NOT VALID;

COMMENT ON COLUMN public.review_suggestions.target_base IS
    'Complete previously accepted owner-side document, captured at publication; NULL means create without overwrite.';
COMMENT ON COLUMN public.review_suggestions.applied_content IS
    'Complete accepted document including owner edits. The original reviewer proposal remains in after_lines.';

-- Existing review rows and files retain their history. Legacy proposals must
-- be republished with an explicit destination before they can write a document.
