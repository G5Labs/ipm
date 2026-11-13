-- A provider request can outlive its response and the local transaction.
-- Keep its target selection on the existing provider record until reconciled.
ALTER TABLE public.workspace_pull_request_provider_repos
    ADD COLUMN merge_attempt_id uuid,
    ADD COLUMN merge_execution_id uuid,
    ADD COLUMN merge_target_head_sha text,
    ADD COLUMN merge_pending boolean NOT NULL DEFAULT false,
    ADD COLUMN merge_uncertain boolean NOT NULL DEFAULT false,
    ADD COLUMN merge_accepted boolean NOT NULL DEFAULT false,
    ADD CONSTRAINT wpr_provider_merge_attempt CHECK (
        (merge_attempt_id IS NULL AND merge_execution_id IS NULL
         AND merge_target_head_sha IS NULL AND NOT merge_pending AND NOT merge_uncertain AND NOT merge_accepted)
        OR
        (merge_attempt_id IS NOT NULL AND merge_execution_id IS NOT NULL
         AND merge_target_head_sha IS NOT NULL AND NOT (merge_pending AND merge_accepted)
         AND merge_target_head_sha ~ '^([0-9a-f]{40}|[0-9a-f]{64})$')
    );

CREATE UNIQUE INDEX wpr_provider_merge_target_pending
    ON public.workspace_pull_request_provider_repos (repo_id, target_remote_branch)
    WHERE merge_pending;

-- Refresh/replacement and deletion cascades must not discard an operation
-- whose provider effect may still complete. Terminal writes retain its evidence.
CREATE FUNCTION public.protect_pending_provider_merge() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.merge_pending THEN
        IF TG_OP = 'DELETE' THEN
            RAISE EXCEPTION 'Reconcile the pending provider merge before deleting its records'
                USING ERRCODE = '23514';
        END IF;
        IF (NEW.id, NEW.org_id, NEW.pull_request_id, NEW.repo_id, NEW.provider,
            NEW.provider_repo_full_name, NEW.provider_pr_number,
            NEW.source_remote_branch, NEW.target_remote_branch, NEW.source_head_sha,
            NEW.merge_attempt_id, NEW.merge_target_head_sha)
           IS DISTINCT FROM
           (OLD.id, OLD.org_id, OLD.pull_request_id, OLD.repo_id, OLD.provider,
            OLD.provider_repo_full_name, OLD.provider_pr_number,
            OLD.source_remote_branch, OLD.target_remote_branch, OLD.source_head_sha,
            OLD.merge_attempt_id, OLD.merge_target_head_sha) THEN
            RAISE EXCEPTION 'A pending provider merge retains its provider, source and target identity'
                USING ERRCODE = '23514';
        END IF;
    END IF;
    IF TG_OP = 'DELETE' THEN RETURN OLD; END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER protect_pending_provider_merge
BEFORE UPDATE OR DELETE ON public.workspace_pull_request_provider_repos
FOR EACH ROW EXECUTE FUNCTION public.protect_pending_provider_merge();

-- The provider record's repository identity must also survive reconfiguration
-- between the committed reservation and its execution/reconciliation.
CREATE FUNCTION public.protect_pending_provider_repository() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'UPDATE' AND
       (NEW.git_url, NEW.provider, NEW.provider_repo_id, NEW.provider_full_name)
       IS NOT DISTINCT FROM
       (OLD.git_url, OLD.provider, OLD.provider_repo_id, OLD.provider_full_name) THEN
        RETURN NEW;
    END IF;
    IF EXISTS (SELECT 1 FROM public.workspace_pull_request_provider_repos
               WHERE repo_id=OLD.id AND merge_pending) THEN
        RAISE EXCEPTION 'Reconcile the pending provider merge before changing its repository identity'
            USING ERRCODE = '23514';
    END IF;
    IF TG_OP = 'DELETE' THEN RETURN OLD; END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER protect_pending_provider_repository
BEFORE UPDATE OF git_url, provider, provider_repo_id, provider_full_name OR DELETE ON public.repos
FOR EACH ROW EXECUTE FUNCTION public.protect_pending_provider_repository();
