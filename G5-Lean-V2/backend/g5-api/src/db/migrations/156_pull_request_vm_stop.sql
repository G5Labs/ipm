-- The source owner's optional stop after an automatic PR merge.
ALTER TABLE public.workspace_pull_requests
    ADD COLUMN stop_vm_after_merge boolean NOT NULL DEFAULT false;
