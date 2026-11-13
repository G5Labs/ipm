ALTER TABLE public.task_attempts
    ADD COLUMN last_reconciled_at TIMESTAMPTZ NOT NULL DEFAULT '-infinity';

CREATE INDEX task_attempts_active_reconciliation_order
    ON public.task_attempts (last_reconciled_at, task_id, attempt)
    WHERE ended_at IS NULL;
