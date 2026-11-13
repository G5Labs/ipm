-- Who created a worker host, for the Workers page "Created by" column. Set
-- from the acting user on POST .../worker-hosts; a rotation replacement
-- copies its predecessor's creator. NULL for hosts created before this
-- column existed.
-- Additive DDL only: no row-level data section runs under FORCE ROW LEVEL
-- SECURITY (docs/database-migrations.md).
-- The foreign key is added NOT VALID so the ALTER skips the existing-row
-- scan; migration 062 validates it without blocking writes.
ALTER TABLE public.worker_hosts
    ADD COLUMN created_by_user_id uuid;
ALTER TABLE public.worker_hosts
    ADD CONSTRAINT worker_hosts_created_by_user_id_fkey
        FOREIGN KEY (created_by_user_id) REFERENCES public.users(id) NOT VALID;
