-- Give existing mention edges their workspace's organization (see 052).
-- Table-owner migrations run without BYPASSRLS on Cloud SQL: read the
-- workspace organizations under the temporary owner exemption, which holds an
-- exclusive lock until commit while tenant policies stay enabled.
ALTER TABLE public.workspaces NO FORCE ROW LEVEL SECURITY;

UPDATE public.comment_mentions m
SET org_id = w.org_id
FROM public.workspaces w
WHERE w.id = m.workspace_id;

-- Drain deferred tenant checks before restoring the table policy flag.
SET CONSTRAINTS ALL IMMEDIATE;

ALTER TABLE public.workspaces FORCE ROW LEVEL SECURITY;
