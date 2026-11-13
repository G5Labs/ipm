-- Platform operator organization (g5.organization.platform-operator.identity,
-- G5-Lean#776).
--
-- The organization kind, not its tenant-chosen slug, identifies the operator
-- organization whose memberships are the platform-operator roster. Existing
-- organizations are tenants. The partial unique index keeps at most one
-- operator organization; bootstrap relies on it to resolve concurrent first
-- sign-ins. Pure DDL: no rows are read or rewritten.
ALTER TABLE public.orgs
    ADD COLUMN kind text NOT NULL DEFAULT 'tenant',
    ADD CONSTRAINT orgs_kind_check CHECK (kind = ANY (ARRAY['tenant'::text, 'operator'::text])),
    ADD CONSTRAINT orgs_operator_slug CHECK (kind <> 'operator' OR slug = 'operator');

CREATE UNIQUE INDEX orgs_single_operator ON public.orgs (kind) WHERE kind = 'operator';

COMMENT ON COLUMN public.orgs.kind IS 'tenant, or operator for the one platform-operator organization (g5.organization.platform-operator.identity).';
