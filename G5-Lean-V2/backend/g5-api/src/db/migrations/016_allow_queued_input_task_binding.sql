ALTER TABLE public.session_inputs
    DROP CONSTRAINT session_inputs_attempt_pair,
    ADD CONSTRAINT session_inputs_attempt_pair CHECK (
        attempt IS NULL OR task_id IS NOT NULL
    );
