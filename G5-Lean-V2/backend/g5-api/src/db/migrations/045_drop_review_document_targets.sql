-- Reverting PR #222 (PR #258) removed every reader and writer of the review
-- Project Doc destination that 034/035 added. Return the rows written while it
-- was live to the shape the review service reads, then drop the columns.
-- Table-owner migrations run without BYPASSRLS on Cloud SQL, so FORCE is lifted
-- only inside this transaction (as in 037); tenant policies stay enabled.
ALTER TABLE public.review_suggestions NO FORCE ROW LEVEL SECURITY;

-- Accepted text, including the owner's edits, lives in after_lines again. A
-- new-file proposal has no before text: the reviewer-side merge base kept for
-- Discard has no reader after the revert.
UPDATE public.review_suggestions
SET after_lines = COALESCE(applied_content, after_lines),
    before_lines = CASE WHEN target_path IS NOT NULL AND kind = 'new_file'
                        THEN NULL ELSE before_lines END
WHERE applied_content IS NOT NULL OR target_path IS NOT NULL;

-- Drain deferred checks before altering the table.
SET CONSTRAINTS ALL IMMEDIATE;

ALTER TABLE public.review_suggestions
    DROP CONSTRAINT review_document_target_shape,
    DROP COLUMN target_path,
    DROP COLUMN target_base,
    DROP COLUMN applied_content;

ALTER TABLE public.review_suggestions FORCE ROW LEVEL SECURITY;
