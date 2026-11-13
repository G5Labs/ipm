-- Raise workspaces still on the previous default of 2 bounded execution slots
-- to the new default of 16 (see 070, g5.agents.admission.default-slots).
-- No UI ever sets this column; only a direct client of the settings API could
-- have chosen exactly 2, and nothing recorded whether a row's 2 was chosen or
-- took the column default. The owner decided on 2026-10-08 to raise every row
-- on 2. Any other value is an explicit choice and is kept.
-- Forks copy their source's value, so without this backfill no existing
-- workspace or later fork of one would reach the new default.
--
-- Deploy only beside an API that already caps VMs on a four-slot catalog at
-- two turns (g5.agents.admission.vm-slots): this hook runs while the previous
-- API still serves, and a rolled-back deploy keeps the raised rows.
--
-- Table-owner migrations run without BYPASSRLS on Cloud SQL: write under the
-- temporary owner exemption, which holds an exclusive lock until commit while
-- tenant policies stay enabled (docs/database-migrations.md). This file is
-- straight-line NO FORCE / UPDATE / FORCE only; the DDL lives in 070.
ALTER TABLE public.workspace_settings NO FORCE ROW LEVEL SECURITY;

UPDATE public.workspace_settings
SET bounded_execution_slots = 16
WHERE bounded_execution_slots = 2;

-- Drain deferred tenant checks before restoring the table policy flag.
SET CONSTRAINTS ALL IMMEDIATE;

ALTER TABLE public.workspace_settings FORCE ROW LEVEL SECURITY;
