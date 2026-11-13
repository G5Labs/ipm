ALTER TABLE public.task_attempts
    DROP CONSTRAINT task_attempts_reason_check,
    ADD CONSTRAINT task_attempts_reason_check CHECK (reason IS NULL OR reason IN (
        'exit_code', 'provider_error', 'vm_lost', 'timed_out',
        'cancelled', 'cancelled_before_start', 'interrupted', 'suspended'
    ));
