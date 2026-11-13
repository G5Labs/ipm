-- Retire the legacy evaluation ledger after the owner decision in #903 / #908.
-- The retention sweep has no reader or writer; its synthetic fixture is retired.
-- DROP TABLE removes the owned identity sequence, indexes, constraints, policy
-- and triggers. Keep the shared set_approvals_governance_created_at() and
-- set_org_id_from_session() functions and their other trigger consumers.
-- Rollout/rollback prerequisites: docs/db-audit-903.md. No CASCADE.
SET LOCAL lock_timeout = '5s';
DROP TABLE public.merge_evaluations;
