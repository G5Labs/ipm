-- Contract follow-up to #908: deploy only after its explicit projections are
-- serving, old prepared-query connections are drained, and rollback releases
-- meet the same prerequisite. See docs/db-audit-903.md.
-- 022 replaced the initial artifact UUID with Git blob identity. Named
-- ReviewSuggestionRecord decoding ignores this UUID, and publication now
-- returns an explicit projection. Keep blob_oid, blob_sha256, bytes/content_type.
-- Rollout/rollback prerequisites: docs/db-audit-903.md (#903).
SET LOCAL lock_timeout = '5s';
ALTER TABLE public.review_suggestions DROP COLUMN blob_id;
