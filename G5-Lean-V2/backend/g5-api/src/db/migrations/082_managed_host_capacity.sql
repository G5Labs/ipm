-- Managed capacity owns only host supply; the organization VM quota stays ten.
ALTER TABLE public.worker_hosts
    ADD COLUMN idle_since timestamptz,
    ADD COLUMN automatic_capacity boolean NOT NULL DEFAULT false,
    ADD COLUMN capacity_start_intent_id uuid,
    ADD COLUMN cloud_deleted_at timestamptz,
    ADD COLUMN cloud_delete_attempted_at timestamptz;
CREATE UNIQUE INDEX worker_hosts_capacity_start_intent
    ON public.worker_hosts (org_id, capacity_start_intent_id)
    WHERE capacity_start_intent_id IS NOT NULL;

-- All API versions lose idle evidence on legacy/unknown control writes. Current
-- heartbeat code may restore the locked prior timer after merging inventory.
CREATE FUNCTION public.g5_guard_host_capacity() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    -- Fence replacement selections from an older API replica too: its INSERT
    -- and pairing write share a transaction, so refusing the pair rolls back
    -- the would-be successor before it can launch cloud work.
    IF TG_OP = 'UPDATE' AND OLD.status = 'terminated'
       AND OLD.drain_reason = 'automatic_idle'
       AND NEW.rotation_replaced_by IS NOT NULL
       AND NEW.rotation_replaced_by IS DISTINCT FROM OLD.rotation_replaced_by THEN
        RAISE EXCEPTION 'idle-retired host cannot acquire a replacement' USING ERRCODE = '23514';
    END IF;
    IF NEW.status <> 'ready' OR NOT NEW.slot_size_reported
       OR NEW.reported_live_vm_ids IS NULL
       OR cardinality(NEW.reported_live_vm_ids) <> 0
       OR NEW.last_heartbeat IS NULL
       OR NEW.last_heartbeat < now() - interval '30 seconds' THEN
        NEW.idle_since := NULL;
    END IF;
    -- Net-growth retries are demand-conditioned. Older retry workers must not
    -- consume them or retarget them through the manual cross-region policy.
    IF NEW.capacity_start_intent_id IS NOT NULL THEN
        NEW.provision_next_retry_at := NULL;
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER zz_worker_hosts_capacity_guard
BEFORE INSERT OR UPDATE ON public.worker_hosts
FOR EACH ROW EXECUTE FUNCTION public.g5_guard_host_capacity();

-- Rotation's active pairing marker is the authority even when an older
-- replica creates the successor without knowing about automatic origin.
CREATE FUNCTION public.g5_copy_automatic_host_origin() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.automatic_capacity AND NEW.rotation_replaced_by IS NOT NULL THEN
        UPDATE public.worker_hosts SET automatic_capacity = true
        WHERE org_id = NEW.org_id AND host_id = NEW.rotation_replaced_by
          AND NOT automatic_capacity;
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER worker_hosts_copy_automatic_origin
AFTER UPDATE OF rotation_replaced_by ON public.worker_hosts
FOR EACH ROW EXECUTE FUNCTION public.g5_copy_automatic_host_origin();

-- A VM that starts and finishes between capacity sweeps still resets the hour.
CREATE FUNCTION public.g5_reservation_resets_host_idle() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.host_id IS NOT NULL AND NEW.status IN ('starting','running','stopping') THEN
        UPDATE public.worker_hosts SET idle_since = NULL
        WHERE org_id = NEW.org_id AND host_id = NEW.host_id AND idle_since IS NOT NULL;
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER workspace_vm_reservation_resets_host_idle
AFTER INSERT OR UPDATE OF host_id, status ON public.workspace_vms
FOR EACH ROW EXECUTE FUNCTION public.g5_reservation_resets_host_idle();
