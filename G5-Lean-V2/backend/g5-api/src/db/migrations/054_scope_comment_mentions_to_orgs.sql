-- g5.security.tenant.rls: every mention edge now names its organization
-- (053); require it, fill it from the session like other tenant rows, and
-- force row-level security as on comment_index.
ALTER TABLE public.comment_mentions ALTER COLUMN org_id SET NOT NULL;
CREATE INDEX idx_comment_mentions_org ON public.comment_mentions USING btree (org_id);

CREATE TRIGGER trg_comment_mentions_set_org BEFORE INSERT ON public.comment_mentions
    FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

ALTER TABLE public.comment_mentions ENABLE ROW LEVEL SECURITY;
ALTER TABLE ONLY public.comment_mentions FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.comment_mentions
    USING ((org_id = ANY (authz.current_org_ids())));
