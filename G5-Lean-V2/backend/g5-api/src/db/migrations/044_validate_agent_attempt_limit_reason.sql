-- SQLx commits each migration separately. Validate after 043 releases its
-- ACCESS EXCLUSIVE lock; the constraint already governs new and changed rows.
ALTER TABLE public.task_attempts
    VALIDATE CONSTRAINT task_attempts_reason_check;
