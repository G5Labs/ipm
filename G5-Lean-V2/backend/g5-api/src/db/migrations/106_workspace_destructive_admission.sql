-- A policy drain has an identity independent of its UI marker. Legacy stranded
-- markers may have no identity and remain recoverable by restoring editing.
ALTER TABLE public.workspaces ADD COLUMN mutation_policy_transition_id uuid;
ALTER TABLE public.workspaces ADD CONSTRAINT workspace_policy_transition_identity
    CHECK (mutation_policy_transition_id IS NULL OR mutation_policy_transition IS NOT DISTINCT FROM 'freezing');

-- deletion_operation_id reserves a live workspace before VM stop and remains
-- on the tombstone. All producers of deletion blockers must serialize on the
-- same workspace row, including writers that do not use the public routes.
CREATE FUNCTION public.g5_require_workspace_not_deleting(p_workspace uuid)
RETURNS void LANGUAGE plpgsql VOLATILE SET search_path = pg_catalog, public AS $$
BEGIN
    IF p_workspace IS NULL THEN RETURN; END IF;
    PERFORM id FROM public.workspaces WHERE id = p_workspace FOR SHARE;
    -- A separate statement observes a reservation committed while the lock
    -- above waited; a subquery in that statement could use the old snapshot.
    IF EXISTS (SELECT 1 FROM public.workspaces
               WHERE id = p_workspace AND deletion_operation_id IS NOT NULL) THEN
        RAISE EXCEPTION 'Workspace deletion is in progress'
            USING ERRCODE = 'P0001', CONSTRAINT = 'workspace_deletion_reserved';
    END IF;
END $$;

CREATE FUNCTION public.g5_guard_workspace_deletion_admission()
RETURNS trigger LANGUAGE plpgsql SET search_path = pg_catalog, public AS $$
DECLARE workspace uuid;
BEGIN
    IF TG_TABLE_NAME = 'workspaces' THEN
        IF TG_OP = 'UPDATE' AND OLD.deletion_operation_id IS NOT NULL
           AND NEW.owner_user_id IS DISTINCT FROM OLD.owner_user_id THEN
            RAISE EXCEPTION 'Workspace deletion is in progress'
                USING ERRCODE = 'P0001', CONSTRAINT = 'workspace_deletion_reserved';
        END IF;
        IF NEW.deleted_at IS NULL THEN
            PERFORM public.g5_require_workspace_not_deleting(NEW.cloned_from_id);
        END IF;
    ELSIF TG_TABLE_NAME = 'workspace_forks' THEN
        IF NEW.status IN ('pending', 'aborting') THEN
            PERFORM public.g5_require_workspace_not_deleting(NEW.source_workspace_id);
        END IF;
    ELSIF TG_TABLE_NAME = 'workspace_merge_runs' THEN
        IF NEW.status IN ('pending', 'running', 'waiting_for_human', 'rolling_back') THEN
            FOR workspace IN SELECT DISTINCT id FROM unnest(ARRAY[
                NEW.workspace_id, NEW.source_workspace_id, NEW.target_workspace_id
            ]) AS ids(id) WHERE id IS NOT NULL ORDER BY id LOOP
                PERFORM public.g5_require_workspace_not_deleting(workspace);
            END LOOP;
        END IF;
    ELSIF TG_TABLE_NAME = 'workspace_ownership_transfers' THEN
        IF NEW.status = 'pending' THEN
            PERFORM public.g5_require_workspace_not_deleting(NEW.workspace_id);
        END IF;
    ELSIF TG_TABLE_NAME = 'workspace_repos' THEN
        PERFORM public.g5_require_workspace_not_deleting(NEW.workspace_id);
    END IF;
    RETURN NEW;
END $$;

CREATE TRIGGER workspaces_deletion_admission
    BEFORE INSERT OR UPDATE OF cloned_from_id, owner_user_id ON public.workspaces
    FOR EACH ROW EXECUTE FUNCTION public.g5_guard_workspace_deletion_admission();
CREATE TRIGGER workspace_forks_deletion_admission
    BEFORE INSERT OR UPDATE OF source_workspace_id, status ON public.workspace_forks
    FOR EACH ROW EXECUTE FUNCTION public.g5_guard_workspace_deletion_admission();
CREATE TRIGGER workspace_merges_deletion_admission
    BEFORE INSERT OR UPDATE OF workspace_id, source_workspace_id, target_workspace_id, status
    ON public.workspace_merge_runs
    FOR EACH ROW EXECUTE FUNCTION public.g5_guard_workspace_deletion_admission();
CREATE TRIGGER workspace_transfers_deletion_admission
    BEFORE INSERT OR UPDATE OF workspace_id, status ON public.workspace_ownership_transfers
    FOR EACH ROW EXECUTE FUNCTION public.g5_guard_workspace_deletion_admission();
CREATE TRIGGER workspace_repos_deletion_admission
    BEFORE INSERT OR UPDATE OF workspace_id, repo_id ON public.workspace_repos
    FOR EACH ROW EXECUTE FUNCTION public.g5_guard_workspace_deletion_admission();
