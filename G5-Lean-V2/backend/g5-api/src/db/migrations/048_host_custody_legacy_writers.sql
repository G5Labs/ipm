-- Old API replicas omit inventory both when inserting and when accepting
-- control messages. Only current provisioning explicitly establishes empty.
ALTER TABLE public.worker_hosts
    ALTER COLUMN reported_live_vm_ids DROP DEFAULT;

-- Rows written between migrations 047 and 048 may still contain untracked
-- inventory accepted by an old replica. Require a current complete hello.
UPDATE public.worker_hosts SET reported_live_vm_ids = NULL
WHERE last_heartbeat IS NOT NULL AND status <> 'terminated';

CREATE FUNCTION public.g5_host_liveness_invalidates_inventory() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    NEW.reported_live_vm_ids := NULL;
    RETURN NEW;
END;
$$;

-- Current control transactions restore their inventory after this update,
-- while old writers leave it unknown. Both writes hold the same host row lock.
CREATE TRIGGER worker_hosts_liveness_invalidates_inventory
BEFORE UPDATE OF last_heartbeat ON public.worker_hosts
FOR EACH ROW EXECUTE FUNCTION public.g5_host_liveness_invalidates_inventory();
