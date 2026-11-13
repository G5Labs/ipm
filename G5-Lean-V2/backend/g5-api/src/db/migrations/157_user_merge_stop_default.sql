-- The personal default is available to server-side merges in every organization.
ALTER TABLE public.users
    ADD COLUMN stop_vm_after_merge_default boolean NOT NULL DEFAULT false;

-- NULL follows the current source owner's default until merge. Existing values
-- remain explicit choices: the old boolean cannot distinguish a saved false
-- from the old initial default, so do not silently replace either.
ALTER TABLE public.workspace_pull_requests
    ALTER COLUMN stop_vm_after_merge DROP NOT NULL,
    ALTER COLUMN stop_vm_after_merge DROP DEFAULT;
