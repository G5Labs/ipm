-- Agent approval reviews are ordinary conversation tasks. This relationship
-- associates their transcripts/results with requests without another job or
-- another execution state machine. Existing review rows remain historical.
ALTER TABLE public.approval_requests ADD CONSTRAINT approval_requests_org_id_id_key UNIQUE (org_id, source_workspace_id, id);
CREATE TABLE public.approval_request_tasks (
    org_id uuid NOT NULL,
    approval_request_id uuid NOT NULL,
    source_workspace_id uuid NOT NULL,
    FOREIGN KEY (org_id, source_workspace_id, approval_request_id) REFERENCES public.approval_requests(org_id, source_workspace_id, id) ON DELETE RESTRICT,
    task_id uuid PRIMARY KEY,
    FOREIGN KEY (org_id, source_workspace_id, task_id) REFERENCES public.tasks(org_id, workspace_id, id) ON DELETE CASCADE
);
CREATE INDEX approval_request_tasks_request ON public.approval_request_tasks (org_id, approval_request_id);
ALTER TABLE public.approval_request_tasks ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.approval_request_tasks FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.approval_request_tasks
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
GRANT SELECT, INSERT, UPDATE, DELETE ON public.approval_request_tasks TO g5_tenant;
GRANT SELECT ON public.approval_request_tasks TO g5_app;
