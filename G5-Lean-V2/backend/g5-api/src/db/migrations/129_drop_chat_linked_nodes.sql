-- Projection retirement (f0db5e29d). Session models, reads, creates and forks
-- no longer use this array; seed-timsystem-workspace.sh was updated first.
-- Document YAML linked_nodes is a separate live feature.
-- Rollout/rollback prerequisites: docs/db-audit-903.md (#903).
SET LOCAL lock_timeout = '5s';
ALTER TABLE public.chat_sessions DROP COLUMN linked_nodes;
