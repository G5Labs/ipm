-- A publication admitted under the workspace fence must drain before handoff.
-- The pair lease owns this marker even if a newer queue request replaces its row.
ALTER TABLE public.branch_integration_remote_pair_leases
    ADD COLUMN publication_workspace_id uuid,
    ADD CONSTRAINT branch_integration_publication_workspace_org_fk
        FOREIGN KEY (org_id, publication_workspace_id)
        REFERENCES public.workspaces(org_id, id) ON DELETE CASCADE;

CREATE INDEX idx_branch_integration_publication_workspace
    ON public.branch_integration_remote_pair_leases (publication_workspace_id)
    WHERE publication_workspace_id IS NOT NULL;
