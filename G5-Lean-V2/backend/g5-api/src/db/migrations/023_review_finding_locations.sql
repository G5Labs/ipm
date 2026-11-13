-- Where a finding points. Push review and the agent hand-off submit the
-- repository-relative paths a finding concerns (and the repository when the
-- workspace holds several); the paths were only used to attach generated
-- suggestions and were then dropped, which left a standalone agent finding
-- (no suggestion of its own) with no location at all.

ALTER TABLE public.review_findings
    ADD COLUMN repo_id uuid,
    ADD COLUMN paths jsonb DEFAULT '[]'::jsonb NOT NULL,
    ADD CONSTRAINT review_findings_paths_shape CHECK (jsonb_typeof(paths) = 'array');
COMMENT ON COLUMN public.review_findings.repo_id IS
    'repository the paths live in; NULL when the finding named paths without one';
COMMENT ON COLUMN public.review_findings.paths IS
    'repository-relative paths the finding concerns, as submitted (JSON array of strings)';
