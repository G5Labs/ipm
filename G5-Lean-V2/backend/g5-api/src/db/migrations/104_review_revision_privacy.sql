-- The publication journal was added after 095's one-time resource inventory.
-- Apply the same private-review boundary without changing ordinary tenant
-- isolation, infrastructure authority, or the journal's SELECT/INSERT grants.
CREATE POLICY formal_review_private_select ON public.workspace_revision_events
    AS RESTRICTIVE FOR SELECT TO g5_tenant
    USING (authz.formal_review_allows(workspace_id, false));
CREATE POLICY formal_review_private_insert ON public.workspace_revision_events
    AS RESTRICTIVE FOR INSERT TO g5_tenant
    WITH CHECK (authz.formal_review_allows(workspace_id, true));
CREATE POLICY formal_review_private_update ON public.workspace_revision_events
    AS RESTRICTIVE FOR UPDATE TO g5_tenant
    USING (authz.formal_review_allows(workspace_id, true))
    WITH CHECK (authz.formal_review_allows(workspace_id, true));
CREATE POLICY formal_review_private_delete ON public.workspace_revision_events
    AS RESTRICTIVE FOR DELETE TO g5_tenant
    USING (authz.formal_review_allows(workspace_id, true));
