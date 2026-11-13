-- P4.8: immutable workflow profile input and trusted-bootstrap receipt.
-- Clients never write bootstrap_graph_receipt; the graph admission service
-- appends it in the same transaction that creates child sessions and tasks.

ALTER TABLE public.jobs
    ADD COLUMN spec jsonb NOT NULL DEFAULT '{}'::jsonb
        CHECK (jsonb_typeof(spec) = 'object');

COMMENT ON COLUMN public.jobs.spec IS
    'Frozen workflow profile, parameters, template/policy selection, and server-owned bootstrap graph receipt';
