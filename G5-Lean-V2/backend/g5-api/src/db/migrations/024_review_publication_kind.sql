-- A publication is either a review round (Push review, an agent's recorded
-- round) or the annotation publication that stand-alone comments and
-- uploaded artifacts belong to. Only rounds supersede the author's earlier
-- rounds; the verdict alone could not tell them apart (an annotation
-- publication records `comment_only`, and an agent round may carry findings
-- without a verdict or scores).

ALTER TABLE public.review_publications
    ADD COLUMN kind text DEFAULT 'round'::text NOT NULL,
    ADD CONSTRAINT review_publications_kind_check
        CHECK (kind = ANY (ARRAY['round'::text, 'annotation'::text]));
COMMENT ON COLUMN public.review_publications.kind IS
    'round: a review round that supersedes the author''s previous round; annotation: home of stand-alone comments and uploads';
