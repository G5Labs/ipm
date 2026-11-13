-- 063_drop_unreferenced_tables.sql — P8.5 slice 3a: drop tables whose last
-- code consumer is gone (issue #342 "Removals still in the code").
--
-- Each table below has no reader or writer in the application after this
-- change's code deletions, verified by a repository-wide search of Rust, SQL,
-- the sqlx offline cache and scripts:
--   waitlist_signups           public landing-page waitlist; route deleted
--   task_checkpoints           task lifecycle kernel leftover (P7.1)
--   task_completion_constraints task lifecycle kernel leftover (P7.1)
--   job_merge_artifacts        job-merge execution (P4.8)
--   job_submission_receipts    job-merge execution (P4.8)
--   repo_mirror_refs           protected-ref mirror ledger, dead since #281
--   ontology_grounding         intent-store projection; grounding lives in
--   intent_record              JSONL ledgers under Git (design v9 §9)
--
-- Deliberately retained here: worker_tokens (the admin routes are gone, but
-- seed-prod-org.sh and reset-dev-environment.sh still mint the bare-host
-- registration token into it; the table goes with that provisioning in the
-- P8.4 infra cleanup), merge_evaluations (governance legacy, P6.4 decision
-- pending) and the human_escalations pair (readers still reach the home page;
-- removed with the frontend slice).
--
-- Dropping a table removes its triggers, policies, indexes and grants. Trigger
-- functions are not table-owned, so the one that served only
-- job_submission_receipts is dropped explicitly after its table. On a
-- populated upgrade the restricted migrator owns the tables (migration 045)
-- but not functions created by the original owner, so the drop is skipped
-- when the migrator cannot own the function; the orphan is inert without its
-- table and a fresh install, where the migrator created it, removes it.

DROP TABLE IF EXISTS public.waitlist_signups;
DROP TABLE IF EXISTS public.task_checkpoints;
DROP TABLE IF EXISTS public.task_completion_constraints;
DROP TABLE IF EXISTS public.job_merge_artifacts;
DROP TABLE IF EXISTS public.job_submission_receipts;
DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM pg_proc p
        JOIN pg_namespace n ON n.oid = p.pronamespace
        WHERE n.nspname = 'public'
          AND p.proname = 'job_submission_receipts_touch_updated_at'
          AND pg_has_role(current_user, p.proowner, 'USAGE')
    ) THEN
        DROP FUNCTION public.job_submission_receipts_touch_updated_at();
    END IF;
END $$;
DROP TABLE IF EXISTS public.repo_mirror_refs;
DROP TABLE IF EXISTS public.ontology_grounding;
DROP TABLE IF EXISTS public.intent_record;
