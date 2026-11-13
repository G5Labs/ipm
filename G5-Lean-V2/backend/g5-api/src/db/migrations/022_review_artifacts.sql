-- Review artifacts: a reviewer's new documents (an HTML mockup, a report,
-- an image) travel as artifacts, not as repository files. The bytes live in
-- the source repository's content-addressed blob store (`.blobs/ab/cd/<oid>`
-- under the repository storage, the same layout the blob capability reader
-- serves); the row keeps the Git blob id, the raw SHA-256, the size and the
-- media type. A reviewer may instead ask for a new file to enter the
-- repository ("suggest adding to repo"), which stays a `new_file` suggestion.

ALTER TABLE public.review_suggestions
    ADD COLUMN blob_oid text,
    ADD COLUMN content_type text,
    ADD CONSTRAINT review_suggestions_blob_oid_shape
        CHECK (blob_oid IS NULL OR blob_oid ~ '^[0-9a-f]{40}$'),
    ADD CONSTRAINT review_suggestions_artifact_shape
        CHECK (kind <> 'artifact'
               OR (repo_id IS NOT NULL AND path IS NOT NULL AND blob_oid IS NOT NULL
                   AND blob_sha256 IS NOT NULL AND bytes IS NOT NULL));
COMMENT ON COLUMN public.review_suggestions.blob_oid IS
    'artifact: Git blob id of the bytes in the source repository''s .blobs store';
COMMENT ON COLUMN public.review_suggestions.content_type IS
    'artifact: media type for download (guessed from the file name when the reviewer gave none)';
