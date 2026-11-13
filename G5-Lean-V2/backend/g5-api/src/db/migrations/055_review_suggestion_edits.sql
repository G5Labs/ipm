-- Individual source edits remain owned by their published review parent.
-- Existing rows normalize lazily under the review workspace decision lock.
ALTER TABLE public.review_suggestions ADD COLUMN edits jsonb NOT NULL DEFAULT '[]'::jsonb;
ALTER TABLE public.review_suggestions ADD CONSTRAINT review_suggestions_edits_array CHECK (jsonb_typeof(edits) = 'array');
ALTER TABLE public.review_suggestions DROP CONSTRAINT review_suggestions_status_check;
ALTER TABLE public.review_suggestions ADD CONSTRAINT review_suggestions_status_check CHECK (status = ANY (ARRAY['open','accepted','rejected','partial','withdrawn','superseded','obsolete','resolved']::text[]));
