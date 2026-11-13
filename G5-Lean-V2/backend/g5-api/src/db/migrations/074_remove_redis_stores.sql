-- 074: Postgres stores that replace the Redis uses removed by G5Labs/G5-Lean#405.
--
-- Additive only: two new tables, no data section, no existing table touched.
--
-- 1. remote_tip_observations — the non-authoritative remote-tip hint that the
--    advisory pinger (maintenance role) and the sync worker write and the API
--    reads (fork-source advisories, branch tracking summary). One row per
--    remote integration, overwritten on every observation, cascading with the
--    policy row. Readers ignore rows older than the 300 s hint TTL. Tenant data:
--    same RLS shape as remote_integrations.
--
-- 2. rate_limit_windows — the shared fixed-window counters behind the platform
--    bug-report quota (reporter and platform keys). One row per key; the window
--    resets in place when a new bucket starts. Not tenant data: same grant shape
--    as platform_issue_outbox, written by the API through the system pool.

CREATE TABLE public.remote_tip_observations (
    org_id        uuid NOT NULL,
    workspace_id  uuid NOT NULL,
    repo_id       uuid NOT NULL,
    git_url       text NOT NULL,
    remote_branch text NOT NULL,
    oid           text NOT NULL,
    observed_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, repo_id),
    CONSTRAINT remote_tip_observations_integration_fk
        FOREIGN KEY (org_id, workspace_id, repo_id)
        REFERENCES public.remote_integrations (org_id, workspace_id, repo_id)
        ON DELETE CASCADE
);
ALTER TABLE public.remote_tip_observations ENABLE ROW LEVEL SECURITY;
ALTER TABLE ONLY public.remote_tip_observations FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.remote_tip_observations
    USING (org_id = ANY(authz.current_org_ids()))
    WITH CHECK (org_id = ANY(authz.current_org_ids()));
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE public.remote_tip_observations TO g5_tenant;
GRANT SELECT ON TABLE public.remote_tip_observations TO g5_app;

CREATE TABLE public.rate_limit_windows (
    key          text PRIMARY KEY,
    window_start timestamptz NOT NULL,
    count        integer NOT NULL CHECK (count >= 0)
);
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE public.rate_limit_windows TO g5_app;
