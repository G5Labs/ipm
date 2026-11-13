-- P4.7: serialize the shared agent-session lifecycle at its durable owner.
-- P4.1 introduced the additive task/session/inbox columns; this constraint is
-- enabled only after the typed-input producer writes the new six-state model.

CREATE UNIQUE INDEX tasks_one_active_agent_per_session_idx
    ON public.tasks(chat_session_id)
    WHERE kind = 'agent'
      AND chat_session_id IS NOT NULL
      AND status IN ('queued', 'running', 'waiting');
