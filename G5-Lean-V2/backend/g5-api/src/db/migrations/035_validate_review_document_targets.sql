-- SQLx commits each migration separately. Validate after 034 releases its
-- ACCESS EXCLUSIVE lock so the historical-row scan permits reads and writes.
-- The constraint already enforces new/changed rows after 034 commits.
ALTER TABLE public.review_suggestions
    VALIDATE CONSTRAINT review_document_target_shape;
