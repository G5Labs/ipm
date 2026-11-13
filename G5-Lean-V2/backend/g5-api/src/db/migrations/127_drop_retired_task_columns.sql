-- Retire the lifecycle fields removed in 995228629 and execution fields
-- replaced by task_attempts in 008 / be37f45df (#903).
-- 008 already removed the active_execution_attempt_id foreign key and the
-- sticky-admission trigger/function; 088 removed the job/checkpoint index.
-- Keep tasks.attempt, task_attempts, task_dependencies and their live trigger.
-- Rollout/rollback prerequisites: docs/db-audit-903.md. DDL only.
SET LOCAL lock_timeout = '5s';
ALTER TABLE public.tasks
    DROP CONSTRAINT tasks_execution_provider_check,
    DROP COLUMN max_retries,
    DROP COLUMN pruned,
    DROP COLUMN is_checkpoint,
    DROP COLUMN agent_execution_receipt,
    DROP COLUMN active_execution_attempt_id;
