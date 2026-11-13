-- Generated from backend/g5-contracts/agent-approval-limits.json.
-- Forward snapshot: never regenerate a published migration.
-- Artifact storage only. The review coordinator and agent-status writer remain absent.
CREATE FUNCTION public.agent_approval_frontmatter_bytes(document text) RETURNS integer
LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE AS $$
    SELECT CASE
      WHEN left(document, 4) = E'---\n'
       AND strpos(substring(document FROM 5), E'\n---\n') > 0
      THEN octet_length(split_part(substring(document FROM 5), E'\n---\n', 1))
      ELSE -1 END
$$;

CREATE TABLE public.agent_approval_artifacts (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id uuid NOT NULL REFERENCES public.orgs(id) ON DELETE CASCADE,
    source_workspace_id uuid NOT NULL,
    approval_request_id uuid NOT NULL,
    task_id uuid NOT NULL,
    result text NOT NULL CHECK (result IN ('approved', 'rejected')),
    document text NOT NULL,
    content_sha256 text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (org_id, source_workspace_id, approval_request_id)
        REFERENCES public.approval_requests(org_id, source_workspace_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (org_id, task_id) REFERENCES public.tasks(org_id, id) ON DELETE RESTRICT,
    CONSTRAINT agent_approval_artifacts_output_size
        CHECK (octet_length(document) BETWEEN 1 AND 262144),
    CONSTRAINT agent_approval_artifacts_frontmatter_size
        CHECK (public.agent_approval_frontmatter_bytes(document) BETWEEN 1 AND 32768),
    CONSTRAINT agent_approval_artifacts_digest
        CHECK (content_sha256 = encode(sha256(convert_to(document, 'UTF8')), 'hex'))
);
CREATE INDEX agent_approval_artifacts_request
    ON public.agent_approval_artifacts(org_id, approval_request_id);
ALTER TABLE public.agent_approval_artifacts ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.agent_approval_artifacts FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.agent_approval_artifacts
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
GRANT SELECT, INSERT ON public.agent_approval_artifacts TO g5_tenant;
GRANT SELECT ON public.agent_approval_artifacts TO g5_app;
