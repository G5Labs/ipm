-- Preserve reported VM custody even when its workspace_vms row is absent.
ALTER TABLE public.worker_hosts
    ADD COLUMN reported_live_vm_ids uuid[] DEFAULT '{}';

-- Earlier control-plane versions did not retain reported inventory. Preserve
-- that uncertainty until a complete authenticated hello establishes custody.
UPDATE public.worker_hosts SET reported_live_vm_ids = NULL
WHERE last_heartbeat IS NOT NULL AND status <> 'terminated';
