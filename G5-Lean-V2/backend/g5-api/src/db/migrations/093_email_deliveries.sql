-- Outbound email delivery requests (.g5/ontology/software_architecture/email.g5.md).
-- One row is one message identity: its id is the provider idempotency key and its
-- rendered content is fixed when the row is recorded. Requests are written inside
-- the requesting domain's transaction and drained by the email worker.
--
-- System data reachable only through the email module (g5.email.data): granted to
-- g5_app and denied to g5_tenant, like platform_issue_outbox. Additive DDL only.
CREATE TABLE public.email_deliveries (
    id uuid NOT NULL,
    org_id uuid NOT NULL,
    -- The purpose identifies what the message is for; there is at most one live
    -- request per purpose. Kinds are validated in code so a new kind needs no
    -- migration.
    purpose_kind text NOT NULL,
    purpose_key text NOT NULL,
    sender text NOT NULL,
    recipient text NOT NULL,
    -- Rendered once; cleared when the request ends.
    subject text,
    text_body text,
    html_body text,
    status text NOT NULL,
    attempts integer DEFAULT 0 NOT NULL,
    next_attempt_at timestamp with time zone DEFAULT now() NOT NULL,
    lease_expires_at timestamp with time zone,
    first_attempt_at timestamp with time zone,
    -- An attempt may have reached the provider without a definite answer.
    outcome_uncertain boolean DEFAULT false NOT NULL,
    last_error text,
    terminal_reason text,
    provider_message_id text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    completed_at timestamp with time zone,
    CONSTRAINT email_deliveries_pkey PRIMARY KEY (id),
    CONSTRAINT email_deliveries_purpose_kind_check
        CHECK (purpose_kind ~ '^[a-z][a-z0-9_]{0,63}$'),
    CONSTRAINT email_deliveries_purpose_key_check
        CHECK (length(purpose_key) BETWEEN 1 AND 256),
    CONSTRAINT email_deliveries_status_check
        CHECK (status = ANY (ARRAY['queued'::text, 'sending'::text, 'sent'::text, 'failed'::text, 'cancelled'::text])),
    CONSTRAINT email_deliveries_completed_check
        CHECK ((status = ANY (ARRAY['queued'::text, 'sending'::text])) = (completed_at IS NULL))
);
ALTER TABLE ONLY public.email_deliveries
    ADD CONSTRAINT email_deliveries_org_id_fkey
    FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;
-- One live request per purpose; the request upsert names this predicate.
CREATE UNIQUE INDEX email_deliveries_live_purpose_idx
    ON public.email_deliveries USING btree (purpose_kind, purpose_key)
    WHERE (status = ANY (ARRAY['queued'::text, 'sending'::text]));
CREATE INDEX email_deliveries_due_idx
    ON public.email_deliveries USING btree (next_attempt_at)
    WHERE (status = 'queued'::text);
CREATE INDEX email_deliveries_lease_idx
    ON public.email_deliveries USING btree (lease_expires_at)
    WHERE (status = 'sending'::text);
-- Per-organization volume limits count recent requests of one kind.
CREATE INDEX email_deliveries_volume_idx
    ON public.email_deliveries USING btree (org_id, purpose_kind, created_at);
-- Latest status per purpose, and retention of ended requests.
CREATE INDEX email_deliveries_purpose_idx
    ON public.email_deliveries USING btree (purpose_kind, purpose_key, created_at DESC);
CREATE INDEX email_deliveries_completed_idx
    ON public.email_deliveries USING btree (completed_at)
    WHERE (completed_at IS NOT NULL);
REVOKE ALL ON TABLE public.email_deliveries FROM g5_tenant;
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE public.email_deliveries TO g5_app;
