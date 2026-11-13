-- Validate after 084 commits so the existing-row scan does not retain its
-- ACCESS EXCLUSIVE schema lock. New writes already enforce the constraint.
ALTER TABLE public.session_inputs
    VALIDATE CONSTRAINT session_inputs_reservation_shape;
