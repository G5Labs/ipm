-- Retain operation aliases on the admitted run, including aliases of reused
-- active runs. Terminal history retains these receipts after settlement.
ALTER TABLE public.agent_approval_reviews ADD COLUMN client_operations jsonb NOT NULL
    DEFAULT '{}'::jsonb CHECK (jsonb_typeof(client_operations) = 'object');
CREATE INDEX agent_approval_review_operations ON public.agent_approval_reviews USING gin(client_operations);

CREATE FUNCTION public.guard_agent_approval_operations() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NOT NEW.client_operations @> OLD.client_operations THEN
        RAISE EXCEPTION 'agent review admission receipts are immutable' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER agent_approval_operations_immutable BEFORE UPDATE ON public.agent_approval_reviews
    FOR EACH ROW EXECUTE FUNCTION public.guard_agent_approval_operations();
