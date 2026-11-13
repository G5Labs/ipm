-- Rows admitted by migration 018, covering an in-flight stop, a pending nudge,
-- an admitted start, a capacity-queued start, and a retained non-restart reason.
DO $$
DECLARE
    owner_id uuid := '10000000-0000-4000-8000-000000000019';
    org_id uuid := '20000000-0000-4000-8000-000000000019';
    host_id uuid := gen_random_uuid();
    workspace_id uuid;
    session_id uuid;
    vm_status text;
BEGIN
    INSERT INTO public.users(id, email, display_name)
        VALUES (owner_id, 'restart-upgrade@example.test', 'Owner');
    INSERT INTO public.orgs(id, name, slug) VALUES (org_id, 'Restart upgrade', 'restart-upgrade');
    INSERT INTO public.org_memberships(org_id, user_id, role) VALUES (org_id, owner_id, 'owner');
    INSERT INTO public.worker_hosts(host_id, org_id, region, status, last_heartbeat)
        VALUES (host_id, org_id, 'us-central1', 'ready', now());
    FOREACH vm_status IN ARRAY ARRAY['stopping', 'running', 'starting', 'stopped', 'failed'] LOOP
        workspace_id := gen_random_uuid();
        session_id := gen_random_uuid();
        INSERT INTO public.workspaces(id, org_id, name, branch_name, owner_user_id, created_by, lifecycle_generation)
            VALUES (workspace_id, org_id, 'feature/' || vm_status, 'feature/' || vm_status, owner_id, owner_id, 7);
        INSERT INTO public.workspace_vms(
            org_id, workspace_id, status, host_id, vm_id, epoch,
            stop_reason, stop_operation_id, stop_generation, stop_deadline_at,
            pending_stop_reason, pending_stop_deadline_at, nudged_at, start_intent
        ) VALUES (
            org_id, workspace_id, vm_status,
            CASE WHEN vm_status IN ('stopped', 'failed') THEN NULL ELSE host_id END,
            CASE WHEN vm_status IN ('stopped', 'failed') THEN NULL ELSE gen_random_uuid()::text END,
            11, CASE WHEN vm_status = 'failed' THEN 'drain' ELSE 'restart' END,
            gen_random_uuid(), 7, now() + interval '5 minutes',
            CASE WHEN vm_status = 'running' THEN 'restart' ELSE NULL END,
            CASE WHEN vm_status = 'running' THEN now() + interval '5 minutes' ELSE NULL END,
            CASE WHEN vm_status = 'running' THEN now() ELSE NULL END,
            jsonb_build_object(
                'id', gen_random_uuid(),
                'kind', CASE WHEN vm_status = 'failed' THEN 'resume' ELSE 'restart' END,
                'status', CASE vm_status WHEN 'starting' THEN 'starting'
                    WHEN 'stopped' THEN 'queued_for_capacity' ELSE 'consumed' END,
                'owner_user_id', owner_id, 'lifecycle_generation', 7,
                'attempts', 1, 'created_at', now(), 'expires_at', now() + interval '1 hour'
            )
        );
        INSERT INTO public.chat_sessions(id, org_id, workspace_id, scope_type, title, created_by)
            VALUES (session_id, org_id, workspace_id, 'workspace', vm_status, owner_id);
        INSERT INTO public.tasks(org_id, workspace_id, name, kind, status, spec, chat_session_id, created_by)
            VALUES (org_id, workspace_id, vm_status, 'agent', 'queued',
                jsonb_build_object('lifecycle', jsonb_build_object(
                    'interrupt_reason', CASE WHEN vm_status = 'failed' THEN 'drain' ELSE 'restart' END,
                    'interrupt_operation_id', gen_random_uuid(), 'interrupt_requested_at', now()
                )), session_id, owner_id);
    END LOOP;
END $$;

-- Model time passing before the reconciler expires an admitted capacity intent.
-- The original admission was valid; changing only its kind must not re-admit it.
SET CONSTRAINTS ALL IMMEDIATE;
ALTER TABLE public.workspace_vms DISABLE TRIGGER workspace_vms_validate_start_intent;
UPDATE public.workspace_vms
SET start_intent = jsonb_set(
        jsonb_set(start_intent, '{created_at}', to_jsonb(now() - interval '2 hours')),
        '{expires_at}', to_jsonb(now() - interval '1 hour'))
WHERE status = 'stopped';
ALTER TABLE public.workspace_vms ENABLE TRIGGER workspace_vms_validate_start_intent;
