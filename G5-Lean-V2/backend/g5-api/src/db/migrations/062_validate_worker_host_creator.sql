-- Validate the creator foreign key added NOT VALID in migration 061.
-- VALIDATE CONSTRAINT scans existing rows without blocking concurrent
-- writes (docs/database-migrations.md).
ALTER TABLE public.worker_hosts
    VALIDATE CONSTRAINT worker_hosts_created_by_user_id_fkey;
