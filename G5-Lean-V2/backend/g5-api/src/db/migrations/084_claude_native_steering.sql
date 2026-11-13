-- One initial input and additional flushed Claude inputs share an attempt.
ALTER TABLE public.session_inputs ADD COLUMN steered boolean NOT NULL DEFAULT false;
ALTER TABLE public.session_inputs DROP CONSTRAINT session_inputs_reservation_shape;
ALTER TABLE public.session_inputs ADD CONSTRAINT session_inputs_reservation_shape CHECK (
    (delivery = 'queued' AND delivered_at IS NULL AND withdrawn_at IS NULL AND (
        (reservation_id IS NULL AND reservation_revision IS NULL AND NOT steered)
        OR (steered AND reservation_id IS NOT NULL AND reservation_revision IS NOT NULL
            AND reservation_revision = revision
            AND task_id IS NOT NULL AND attempt IS NOT NULL)))
    OR (delivery IN ('reserved','delivery_unknown') AND reservation_id IS NOT NULL
        AND reservation_revision IS NOT NULL AND task_id IS NOT NULL AND attempt IS NOT NULL
        AND delivered_at IS NULL AND withdrawn_at IS NULL AND NOT steered)
    OR (delivery = 'delivered' AND reservation_id IS NOT NULL AND reservation_revision IS NOT NULL
        AND delivered_at IS NOT NULL AND withdrawn_at IS NULL)
    OR (delivery = 'withdrawn' AND withdrawn_at IS NOT NULL AND delivered_at IS NULL)
) NOT VALID;
DROP INDEX public.session_inputs_one_input_per_attempt_idx;
CREATE UNIQUE INDEX session_inputs_one_input_per_attempt_idx
    ON public.session_inputs(task_id, attempt) WHERE task_id IS NOT NULL AND NOT steered;
-- Every live output event resolves its current assistant segment.
CREATE INDEX chat_messages_assistant_attempt_idx
    ON public.chat_messages(task_id, attempt, message_index DESC) WHERE role = 'assistant';
