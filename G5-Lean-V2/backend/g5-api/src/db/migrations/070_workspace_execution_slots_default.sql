-- Default bounded execution slots per workspace (g5.agents.admission.default-slots).
-- A workspace admits 16 concurrent agent and command turns unless its owner
-- sets another value through workspace settings (1-64). The VM module catalog
-- (backend/g5d-dispatch/modules/catalog.json) admits 16 agent turns and 20
-- bounded executions in total, so the default is reachable in the VM and
-- task-less Changes and diagnostics operations keep headroom.
--
-- DDL only: 071 raises existing rows still on the previous default under the
-- table-owner exemption (docs/database-migrations.md).
ALTER TABLE public.workspace_settings
    ALTER COLUMN bounded_execution_slots SET DEFAULT 16;
