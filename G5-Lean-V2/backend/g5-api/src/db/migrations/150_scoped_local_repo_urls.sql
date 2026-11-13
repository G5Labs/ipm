-- Scoped URLs for G5-hosted repositories (g5.repo.storage-tenancy, G5-Lean#1079).
--
-- A G5-hosted repository's recorded URL is `g5-local://<org-slug>/<repo-slug>`,
-- docs and code repositories alike. It names no disk path: storage is keyed by
-- organization and repository id and never reads it. Rows written before this
-- release carry `file://` paths (docs repositories, seeds), `g5-local://workspaces/…`
-- or `g5-local://personal/…`; rewrite exactly the provider-less rows Git policy
-- treats as local. Provider repositories, provider-less ones with a real
-- remote URL, and provider-less ones a workspace tracks as a remote (a local
-- `file://` remote used by development fixtures) keep theirs. Repeating the
-- update is a no-op.
--
-- Table-owner migrations run without BYPASSRLS on Cloud SQL; the ALTERs hold
-- exclusive locks until commit, and tenant policies stay enabled throughout
-- (docs/database-migrations.md).
ALTER TABLE public.repos NO FORCE ROW LEVEL SECURITY;
ALTER TABLE public.orgs NO FORCE ROW LEVEL SECURITY;
ALTER TABLE public.remote_integrations NO FORCE ROW LEVEL SECURITY;

UPDATE public.repos AS r
SET git_url = 'g5-local://' || o.slug || '/' || r.slug
FROM public.orgs AS o
WHERE o.id = r.org_id
  AND r.provider IS NULL
  AND (r.git_url LIKE 'g5-local://%' OR r.git_url LIKE 'file:%' OR r.git_url LIKE '/%')
  AND r.git_url IS DISTINCT FROM 'g5-local://' || o.slug || '/' || r.slug
  AND NOT EXISTS (
      SELECT 1 FROM public.remote_integrations AS i
       WHERE i.org_id = r.org_id AND i.repo_id = r.id
  );

SET CONSTRAINTS ALL IMMEDIATE;

ALTER TABLE public.repos FORCE ROW LEVEL SECURITY;
ALTER TABLE public.orgs FORCE ROW LEVEL SECURITY;
ALTER TABLE public.remote_integrations FORCE ROW LEVEL SECURITY;
